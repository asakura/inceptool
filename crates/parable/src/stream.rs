//! # Token Stream Architecture
//!
//! [`TokenStream`] adapts the crate's lazy, on-demand lexer ([`LexerStream::lex_token`]) to winnow's
//! [`Stream`] trait, so [`crate::parser`] can drive parsing token-by-token without
//! pre-collecting the input into a `Vec<Token>` ahead of time.
//!
//! ## Core Design
//!
//! Bash reserved words (`done`, `fi`, `then`, ...) lex as plain `Word` tokens and are only
//! recognized as keywords by grammar position, so the parser routinely peeks the very same
//! upcoming token several times in a row — once per candidate keyword — before anything
//! actually consumes it. [`Stream::peek_token`] takes `&self`, so caching that peeked token
//! across calls (to avoid re-lexing the same bytes on every peek) needs interior mutability:
//! `lexer`, `lookahead`, and `lex_failure` are all wrapped in [`RefCell`] for exactly this
//! reason — not because a `TokenStream` is ever shared across threads.
//!
//! ## Flow
//!
//! 1. **Cache hit**: [`Stream::peek_token`] returns the buffered token directly, no lexing.
//! 2. **Cache miss**: [`Stream::peek_token`] lexes one token ahead — physically advancing
//!    `lexer` — and stashes it, along with the lexer's remaining-input length from just before
//!    that lex, so [`Offset::offset_from`] can still report the parser's true *logical*
//!    position even though `lexer` is now ahead of it.
//! 3. **Consume**: [`Stream::next_token`] drains the buffered token first, with no relexing;
//!    it only drives the lexer for real once the buffer is empty.
//!
//! ## Edge Cases
//!
//! - A lex failure discovered during [`Stream::peek_token`] (not just [`Stream::next_token`])
//!   must still land in `lex_failure` on `self`, never on a throwaway clone — otherwise
//!   [`TokenStream::take_lex_error`] would silently miss it.
//! - [`Offset::offset_from`] must diff the parser's *logical* remaining length on both sides,
//!   not `lexer`'s raw remaining length: either side may hold a buffered lookahead that has
//!   already physically advanced `lexer` past a token the parser hasn't logically consumed.
//! - [`Stream::checkpoint`]/[`Stream::reset`] clone/restore the whole struct, including any
//!   buffered lookahead, so checkpointing right after a peek and resetting later correctly
//!   replays that same peek rather than losing it.
//! - [`Location::current_token_start`] and [`Location::previous_token_end`] diff against
//!   different lengths, not the same one: whitespace before a token is skipped lazily inside
//!   [`LexerStream::lex_token_with_start`], so "end of the previous token" and "start of the
//!   next one" are different positions whenever whitespace separates them. Reporting both from
//!   the same length would place the next token's start back at the previous token's end,
//!   underlining the gap between them instead of the token itself.

use crate::lexer::LexerStream;
use crate::types::Token;

use smallvec::SmallVec;
use winnow::error::{ContextError, ErrMode};
use winnow::stream::{Location, Needed, Offset, Stream, StreamIsPartial};

use std::cell::RefCell;
use std::fmt;

/// Inline capacity of [`Stream::Slice`]'s backing storage.
///
/// Multi-token slicing (`take`, `literal`, ...) isn't used by [`crate::parser`] today, but the
/// `Stream` trait still requires a `Slice` type. `SmallVec` keeps that type's allocation cost at
/// zero for any request up to this size, only falling back to a heap `Vec` if something asks
/// for more tokens at once than this — never losing correctness, just losing the free lunch.
///
/// This is unrelated to [`TokenStream`]'s single-token `lookahead` buffer below: that buffer
/// exists to cache the result of a single [`Stream::peek_token`] call, and `parser` never peeks
/// more than one token ahead, so it stays a plain `Option<Token>` rather than growing to this
/// same capacity. Widening it would buy nothing today and would cost every [`Stream::checkpoint`]
/// (which clones the whole struct) bytes proportional to that unused capacity.
const TOKEN_SLICE_CAPACITY: usize = 4;

/// The token-level [`Stream`] consumed by [`crate::parser`].
///
/// Wraps a [`LexerStream`] and calls [`LexerStream::lex_token`] on demand as the parser asks for tokens,
/// rather than pre-draining the input into a `Vec<Token>` ahead of parsing.
///
/// `Clone` (required by [`Stream::peek_token`]/[`Stream::checkpoint`], which can't mutate the
/// caller's position) is an allocation-free copy of `(&str, LexerState)` in the common case:
/// `LexerState`'s only potentially-owned field, `heredoc_delimiter`, is `Cow::Borrowed` unless
/// the delimiter needed backslash-unquoting. `lex_failure` is boxed rather than inlined so this
/// clone stays one lexer-state-sized copy instead of two: inlined, the field would double
/// `TokenStream`'s size on every clone just to carry a snapshot that's `None` almost always.
///
/// `lexer` and `lex_failure` are wrapped in [`RefCell`] because [`Stream::peek_token`] takes
/// `&self`, yet still needs to drive the lexer forward to fill `lookahead` on a cache miss (and,
/// on a lex failure, to record it the same way [`Stream::next_token`] does). The alternative —
/// `peek_token` falling back to `self.clone().next_token()` whenever `lookahead` is empty —
/// would silently drop a lex failure discovered that way, since it'd be recorded on the
/// throwaway clone instead of `self`.
#[derive(Debug, Clone)]
pub struct TokenStream<'a> {
    lexer: RefCell<LexerStream<'a>>,
    /// Token already lexed by a previous [`Stream::peek_token`] call, not yet consumed by
    /// [`Stream::next_token`].
    ///
    /// Bash's reserved-word-as-plain-`Word` grammar (see `parser::at_keyword`) means a single
    /// parse position is often peeked several times in a row — once per candidate keyword —
    /// before anything actually consumes it. Without this cache, each of those peeks would
    /// re-run the lexer over the same bytes from scratch via a full `self.clone()`.
    lookahead: RefCell<Option<Lookahead<'a>>>,
    /// Snapshot of `lexer` taken when [`LexerStream::lex_token`] last failed.
    ///
    /// `Stream::next_token`'s `Option` return has no slot for an error, so a lex failure is
    /// reported to the parser as ordinary end-of-stream; this snapshot lets the caller re-derive
    /// and surface the real lex error afterward via [`TokenStream::take_lex_error`], instead of
    /// the generic syntax error the parser would otherwise produce.
    lex_failure: RefCell<Option<Box<LexerStream<'a>>>>,
    /// The original input length in bytes, used to compute absolute byte offsets.
    original_len: usize,
}

/// A token lexed ahead of the parser's logical position, cached by [`Stream::peek_token`].
///
/// Lexing `token` necessarily moved `TokenStream::lexer` past it, even though the parser hasn't
/// logically consumed it yet — [`Stream::peek_token`] only borrows `self`, so it has nowhere
/// else to leave the already-lexed token while still avoiding to re-lex it on the next call.
/// `remaining_before` records the lexer's remaining input length as it stood immediately before
/// that lex, i.e. the parser's true logical position; [`Offset::offset_from`] reports progress
/// from this length rather than from `lexer`'s already-advanced one, so a peek alone is never
/// mistaken for consumed progress.
#[derive(Debug, Clone)]
struct Lookahead<'a> {
    token: Token<'a>,
    /// `lexer`'s remaining-input length measured right after this token's leading whitespace
    /// was skipped but before the token itself was lexed — i.e. this token's own start position.
    /// [`Location::current_token_start`] reports from this length.
    token_start_remaining: usize,
    /// `lexer`'s remaining-input length measured before [`LexerStream::lex_token_with_start`]
    /// was called at all, i.e. the end of the *previous* token, before this one's leading
    /// whitespace. [`Offset::offset_from`] and [`Location::previous_token_end`] both report from
    /// this length, never from `token_start_remaining`, so a peek alone is never mistaken for
    /// consumed progress.
    remaining_before: usize,
}

/// Iterator returned by [`TokenStream::iter_offsets`], lexing one token per step from a private
/// clone of the stream and pairing it with its index.
#[derive(Debug, Clone)]
pub struct TokenOffsets<'a> {
    stream: TokenStream<'a>,
    index: usize,
}

impl<'a> TokenStream<'a> {
    /// Builds a token stream that lexes `input` from byte offset 0.
    #[must_use = "constructs the token stream; discarding it parses nothing"]
    pub fn new(input: &'a str) -> Self {
        Self {
            lexer: RefCell::new(LexerStream::new(input)),
            lookahead: RefCell::new(None),
            lex_failure: RefCell::new(None),
            original_len: input.len(),
        }
    }

    /// Re-derives and returns the lex error that caused the stream to report exhaustion, if it
    /// stopped because the lexer failed rather than because input was genuinely exhausted.
    #[must_use = "the recovered lex error must be reported, not discarded"]
    pub fn take_lex_error(&mut self) -> Option<ErrMode<ContextError>> {
        let mut snapshot = *self.lex_failure.get_mut().take()?;
        snapshot.lex_token().err()
    }

    /// The lexer's logical remaining-input length: `lookahead`'s `remaining_before` if a
    /// lookahead is buffered, otherwise `lexer`'s own remaining length.
    ///
    /// `lexer` physically advances past a lookahead token as soon as it's lexed, ahead of the
    /// parser logically consuming it (see [`Lookahead`]); this is the length [`Offset::offset_from`]
    /// must diff against so a peek alone is never mistaken for consumed progress.
    #[must_use = "looking up the remaining length has no effect unless the caller uses it"]
    fn logical_remaining_len(&self) -> usize {
        self.lookahead.borrow().as_ref().map_or_else(
            || self.lexer.borrow().eof_offset(),
            |lookahead| lookahead.remaining_before,
        )
    }

    /// The lexer's logical remaining-input length at the next token's *start*, i.e. right after
    /// its leading whitespace but before the token itself — the length
    /// [`Location::current_token_start`] must diff against, as opposed to
    /// [`TokenStream::logical_remaining_len`]'s *end-of-previous-token* length. Forces a peek if
    /// none is buffered yet, since whitespace is only skipped lazily inside
    /// [`LexerStream::lex_token_with_start`].
    #[must_use = "looking up the remaining length has no effect unless the caller uses it"]
    fn token_start_remaining_len(&self) -> usize {
        drop(self.peek_token());

        self.lookahead.borrow().as_ref().map_or_else(
            || self.lexer.borrow().eof_offset(),
            |lookahead| lookahead.token_start_remaining,
        )
    }
}

#[expect(
    clippy::missing_trait_methods,
    reason = "default-provided Stream methods (the *_unchecked slicers winnow's own slice impls \
              override with unsafe, plus finish/peek_finish) are correct as-is and unsafe_code \
              is forbidden in this crate"
)]
impl<'a> Stream for TokenStream<'a> {
    type Token = Token<'a>;
    type Slice = SmallVec<[Token<'a>; TOKEN_SLICE_CAPACITY]>;
    type IterOffsets = TokenOffsets<'a>;
    type Checkpoint = Self;

    fn iter_offsets(&self) -> Self::IterOffsets {
        TokenOffsets {
            stream: self.clone(),
            index: 0,
        }
    }

    fn eof_offset(&self) -> usize {
        self.iter_offsets().count()
    }

    fn next_token(&mut self) -> Option<Self::Token> {
        if let Some(lookahead) = self.lookahead.get_mut().take() {
            return Some(lookahead.token);
        }

        if self.lex_failure.get_mut().is_some() {
            return None;
        }

        match self.lexer.get_mut().lex_token() {
            Ok(Token::Eof) => None,
            Ok(token) => Some(token),
            Err(_) => {
                *self.lex_failure.get_mut() = Some(Box::new(self.lexer.get_mut().clone()));
                None
            }
        }
    }

    fn peek_token(&self) -> Option<Self::Token> {
        if let Some(lookahead) = self.lookahead.borrow().as_ref() {
            return Some(lookahead.token.clone());
        }

        if self.lex_failure.borrow().is_some() {
            return None;
        }

        let remaining_before = self.lexer.borrow().eof_offset();
        let lexed = self.lexer.borrow_mut().lex_token_with_start();

        match lexed {
            Ok((_, Token::Eof)) => None,
            Ok((token_start_remaining, token)) => {
                *self.lookahead.borrow_mut() = Some(Lookahead {
                    token: token.clone(),
                    token_start_remaining,
                    remaining_before,
                });
                Some(token)
            }
            Err(_) => {
                *self.lex_failure.borrow_mut() = Some(Box::new(self.lexer.borrow().clone()));
                None
            }
        }
    }

    fn offset_for<P>(&self, predicate: P) -> Option<usize>
    where
        P: Fn(Self::Token) -> bool,
    {
        self.iter_offsets()
            .find(|(_, token)| predicate(token.clone()))
            .map(|(index, _)| index)
    }

    fn offset_at(&self, tokens: usize) -> Result<usize, Needed> {
        let mut remaining = self.clone();

        for consumed in 0..tokens {
            if remaining.next_token().is_none() {
                return Err(Needed::new(tokens.saturating_sub(consumed)));
            }
        }

        Ok(tokens)
    }

    fn next_slice(&mut self, offset: usize) -> Self::Slice {
        (0..offset).filter_map(|_| self.next_token()).collect()
    }

    fn peek_slice(&self, offset: usize) -> Self::Slice {
        self.clone().next_slice(offset)
    }

    fn checkpoint(&self) -> Self::Checkpoint {
        self.clone()
    }

    fn reset(&mut self, checkpoint: &Self::Checkpoint) {
        self.clone_from(checkpoint);
    }

    #[expect(
        clippy::use_debug,
        reason = "trace() exists purely to render a debug trace of the stream's state"
    )]
    fn trace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Offset for TokenStream<'_> {
    fn offset_from(&self, start: &Self) -> usize {
        start
            .logical_remaining_len()
            .saturating_sub(self.logical_remaining_len())
    }
}

impl Location for TokenStream<'_> {
    fn previous_token_end(&self) -> usize {
        self.original_len
            .saturating_sub(self.logical_remaining_len())
    }
    fn current_token_start(&self) -> usize {
        self.original_len
            .saturating_sub(self.token_start_remaining_len())
    }
}

impl TokenStream<'_> {
    /// Returns the start byte offset of the next token in the stream.
    #[must_use = "fetching the current span start has no effect unless the caller uses the offset"]
    pub fn current_span_start(&self) -> usize {
        Location::current_token_start(self)
    }

    /// Returns the end byte offset of the previously consumed token.
    #[must_use = "fetching the previous span end has no effect unless the caller uses the offset"]
    pub fn previous_span_end(&self) -> usize {
        Location::previous_token_end(self)
    }
}

#[expect(
    clippy::missing_trait_methods,
    reason = "is_partial's default (Self::is_partial_supported()) is correct as-is"
)]
impl StreamIsPartial for TokenStream<'_> {
    type PartialState = ();

    fn complete(&mut self) -> Self::PartialState {}

    fn restore_partial(&mut self, _state: Self::PartialState) {}

    fn is_partial_supported() -> bool {
        false
    }
}

#[expect(
    clippy::missing_trait_methods,
    reason = "only a minimal lexing-driven next() is needed; Iterator's other ~70 default-provided \
              combinators are correct as-is and reimplementing them all would dwarf this module"
)]
impl<'a> Iterator for TokenOffsets<'a> {
    type Item = (usize, Token<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.stream.next_token()?;
        let index = self.index;
        self.index = self.index.saturating_add(1);
        Some((index, token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::assert_matches;
    use std::borrow::Cow;

    #[derive(Debug, thiserror::Error)]
    enum TestError {}

    mod peek_token {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn repeated_peek_returns_same_token_without_consuming() -> Result<(), TestError> {
            let stream = TokenStream::new("foo bar");

            assert_eq!(stream.peek_token(), stream.peek_token());

            Ok(())
        }

        #[rstest]
        fn peek_then_next_token_consumes_the_buffered_token() -> Result<(), TestError> {
            let mut stream = TokenStream::new("foo bar");

            let peeked = stream.peek_token();
            let consumed = stream.next_token();

            assert_eq!(peeked, consumed);
            assert_eq!(stream.next_token(), Some(Token::Word(Cow::Borrowed("bar"))));

            Ok(())
        }

        #[rstest]
        fn peek_does_not_swallow_a_lex_failure() -> Result<(), TestError> {
            let mut stream = TokenStream::new("\r");

            assert_eq!(stream.peek_token(), None);
            assert_matches!(stream.take_lex_error(), Some(_));

            Ok(())
        }
    }

    mod offset_from {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn peek_alone_reports_zero_progress() -> Result<(), TestError> {
            let stream = TokenStream::new("foo bar");
            let checkpoint = stream.checkpoint();

            drop(stream.peek_token());

            assert_eq!(stream.offset_from(&checkpoint), 0);

            Ok(())
        }

        #[rstest]
        fn consuming_a_peeked_token_reports_its_width() -> Result<(), TestError> {
            let mut stream = TokenStream::new("foo bar");
            let checkpoint = stream.checkpoint();

            drop(stream.peek_token());
            drop(stream.next_token());

            assert_eq!(stream.offset_from(&checkpoint), 3);

            Ok(())
        }
    }

    mod eof_offset {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn unaffected_by_a_prior_peek() -> Result<(), TestError> {
            let stream = TokenStream::new("foo bar");
            let before = stream.eof_offset();

            drop(stream.peek_token());

            assert_eq!(stream.eof_offset(), before);

            Ok(())
        }
    }

    mod location {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn current_token_start_skips_whitespace_the_previous_token_end_does_not()
        -> Result<(), TestError> {
            let mut stream = TokenStream::new("foo   bar");

            drop(stream.next_token());

            assert_eq!(stream.previous_span_end(), 3);
            assert_eq!(stream.current_span_start(), 6);

            Ok(())
        }
    }

    mod checkpoint {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn reset_restores_a_buffered_lookahead() -> Result<(), TestError> {
            let mut stream = TokenStream::new("foo bar");

            drop(stream.peek_token());
            let checkpoint = stream.checkpoint();
            drop(stream.next_token());
            stream.reset(&checkpoint);

            assert_eq!(stream.peek_token(), Some(Token::Word(Cow::Borrowed("foo"))));

            Ok(())
        }
    }
}
