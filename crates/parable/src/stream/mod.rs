//! # Token Stream Architecture
//!
//! [`TokenStream`] adapts the crate's lazy, on-demand lexer ([`crate::lexer::LexerStream::lex_token`]) to winnow's
//! [`winnow::stream::Stream`] trait, so [`crate::parser`] can drive parsing token-by-token without
//! pre-collecting the input into a `Vec<Token>` ahead of time.
//!
//! ## Core Design
//!
//! Bash reserved words (`done`, `fi`, `then`, ...) lex as plain `Word` tokens and are only
//! recognized as keywords by grammar position, so the parser routinely peeks the very same
//! upcoming token several times in a row — once per candidate keyword — before anything
//! actually consumes it. [`winnow::stream::Stream::peek_token`] takes `&self`, so caching that peeked token
//! across calls (to avoid re-lexing the same bytes on every peek) needs interior mutability:
//! `lexer`, `lookahead`, and `lex_failure` are all wrapped in [`std::cell::RefCell`] for exactly this
//! reason — not because a `TokenStream` is ever shared across threads.
//!
//! ## Flow
//!
//! 1. **Cache hit**: [`winnow::stream::Stream::peek_token`] returns the buffered token directly, no lexing.
//! 2. **Cache miss**: [`winnow::stream::Stream::peek_token`] lexes one token ahead — physically advancing
//!    `lexer` — and stashes it, along with the lexer's remaining-input length from just before
//!    that lex, so [`winnow::stream::Offset::offset_from`] can still report the parser's true *logical*
//!    position even though `lexer` is now ahead of it.
//! 3. **Consume**: [`winnow::stream::Stream::next_token`] drains the buffered token first, with no relexing;
//!    it only drives the lexer for real once the buffer is empty.
//!
//! ## Edge Cases
//!
//! - A lex failure discovered during [`winnow::stream::Stream::peek_token`] (not just [`winnow::stream::Stream::next_token`])
//!   must still land in `lex_failure` on `self`, never on a throwaway clone — otherwise
//!   [`TokenStream::take_lex_error`] would silently miss it.
//! - [`winnow::stream::Offset::offset_from`] must diff the parser's *logical* remaining length on both sides,
//!   not `lexer`'s raw remaining length: either side may hold a buffered lookahead that has
//!   already physically advanced `lexer` past a token the parser hasn't logically consumed.
//! - [`winnow::stream::Stream::checkpoint`]/[`winnow::stream::Stream::reset`] clone/restore the whole struct, including any
//!   buffered lookahead, so checkpointing right after a peek and resetting later correctly
//!   replays that same peek rather than losing it.
//! - [`winnow::stream::Location::current_token_start`] and [`winnow::stream::Location::previous_token_end`] diff against
//!   different lengths, not the same one: whitespace before a token is skipped lazily inside
//!   `crate::lexer::LexerStream::lex_token_with_start`, so "end of the previous token" and "start of the
//!   next one" are different positions whenever whitespace separates them. Reporting both from
//!   the same length would place the next token's start back at the previous token's end,
//!   underlining the gap between them instead of the token itself.

#![expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "fields are shared within the module hierarchy"
)]

mod traits;

#[cfg(test)]
mod tests;

use crate::lexer::LexerStream;
use crate::lexer::heredoc;
use crate::types::Token;

use winnow::error::{ContextError, ErrMode};
use winnow::stream::Location;

use std::borrow::Cow;
use std::cell::RefCell;

/// Inline capacity of [`winnow::stream::Stream::Slice`]'s backing storage.
///
/// Multi-token slicing (`take`, `literal`, ...) isn't used by [`crate::parser`] today, but the
/// `Stream` trait still requires a `Slice` type. `SmallVec` keeps that type's allocation cost at
/// zero for any request up to this size, only falling back to a heap `Vec` if something asks
/// for more tokens at once than this — never losing correctness, just losing the free lunch.
///
/// This is unrelated to [`TokenStream`]'s single-token `lookahead` buffer below: that buffer
/// exists to cache the result of a single `Stream::peek_token` call, and `parser` never peeks
/// more than one token ahead, so it stays a plain `Option<Token>` rather than growing to this
/// same capacity. Widening it would buy nothing today and would cost every `Stream::checkpoint`
/// (which clones the whole struct) bytes proportional to that unused capacity.
pub(crate) const TOKEN_SLICE_CAPACITY: usize = 4;

/// The token-level [`winnow::stream::Stream`] consumed by [`crate::parser`].
///
/// Wraps a [`LexerStream`] and calls [`LexerStream::lex_token`] on demand as the parser asks for tokens,
/// rather than pre-draining the input into a `Vec<Token>` ahead of parsing.
///
/// `Clone` (required by `Stream::peek_token`/`Stream::checkpoint`, which can't mutate the
/// caller's position) is an allocation-free copy of `(&str, LexerState)`: `LexerState` is itself
/// `Copy` (a `bool` and a `usize`, nothing owned), so cloning never allocates. `lex_failure` is
/// boxed rather than inlined so this clone stays one lexer-state-sized copy instead of two:
/// inlined, the field would double `TokenStream`'s size on every clone just to carry a snapshot
/// that's `None` almost always.
///
/// `lexer` and `lex_failure` are wrapped in [`RefCell`] because `Stream::peek_token` takes
/// `&self`, yet still needs to drive the lexer forward to fill `lookahead` on a cache miss (and,
/// on a lex failure, to record it the same way `Stream::next_token` does). The alternative —
/// `peek_token` falling back to `self.clone().next_token()` whenever `lookahead` is empty —
/// would silently drop a lex failure discovered that way, since it'd be recorded on the
/// throwaway clone instead of `self`.
#[derive(Debug, Clone)]
#[expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "shared within module hierarchy"
)]
pub struct TokenStream<'a> {
    pub(crate) lexer: RefCell<LexerStream<'a>>,
    /// Token already lexed by a previous `Stream::peek_token` call, not yet consumed by
    /// `Stream::next_token`.
    ///
    /// Bash's reserved-word-as-plain-`Word` grammar (see `parser::at_keyword`) means a single
    /// parse position is often peeked several times in a row — once per candidate keyword —
    /// before anything actually consumes it. Without this cache, each of those peeks would
    /// re-run the lexer over the same bytes from scratch via a full `self.clone()`.
    pub(crate) lookahead: RefCell<Option<Lookahead<'a>>>,
    /// Snapshot of `lexer` taken when [`LexerStream::lex_token`] last failed.
    ///
    /// `Stream::next_token`'s `Option` return has no slot for an error, so a lex failure is
    /// reported to the parser as ordinary end-of-stream; this snapshot lets the caller re-derive
    /// and surface the real lex error afterward via [`TokenStream::take_lex_error`], instead of
    /// the generic syntax error the parser would otherwise produce.
    pub(crate) lex_failure: RefCell<Option<Box<LexerStream<'a>>>>,
    /// The original input length in bytes, used to compute absolute byte offsets.
    pub(crate) original_len: usize,
}

/// A token lexed ahead of the parser's logical position, cached by `Stream::peek_token`.
///
/// Lexing `token` necessarily moved `TokenStream::lexer` past it, even though the parser hasn't
/// logically consumed it yet — `Stream::peek_token` only borrows `self`, so it has nowhere
/// else to leave the already-lexed token while still avoiding to re-lex it on the next call.
/// `remaining_before` records the lexer's remaining input length as it stood immediately before
/// that lex, i.e. the parser's true logical position; `Offset::offset_from` reports progress
/// from this length rather than from `lexer`'s already-advanced one, so a peek alone is never
/// mistaken for consumed progress.
#[derive(Debug, Clone)]
pub(crate) struct Lookahead<'a> {
    pub(crate) token: Token<'a>,
    /// `lexer`'s remaining-input length measured right after this token's leading whitespace
    /// was skipped but before the token itself was lexed — i.e. this token's own start position.
    /// `Location::current_token_start` reports from this length.
    pub(crate) token_start_remaining: usize,
    /// `lexer`'s remaining-input length measured before `LexerStream::lex_token_with_start`
    /// was called at all, i.e. the end of the *previous* token, before this one's leading
    /// whitespace. `Offset::offset_from` and `Location::previous_token_end` both report from
    /// this length, never from `token_start_remaining`, so a peek alone is never mistaken for
    /// consumed progress.
    pub(crate) remaining_before: usize,
}

/// Iterator returned by `TokenStream::iter_offsets`, lexing one token per step from a private
/// clone of the stream and pairing it with its index.
#[derive(Debug, Clone)]
pub struct TokenOffsets<'a> {
    pub(crate) stream: TokenStream<'a>,
    pub(crate) index: usize,
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
    /// parser logically consuming it (see [`Lookahead`]); this is the length `Offset::offset_from`
    /// must diff against so a peek alone is never mistaken for consumed progress.
    #[must_use = "looking up the remaining length has no effect unless the caller uses it"]
    pub(crate) fn logical_remaining_len(&self) -> usize {
        self.lookahead.borrow().as_ref().map_or_else(
            || {
                use winnow::stream::Stream as _;
                self.lexer.borrow().eof_offset()
            },
            |lookahead| lookahead.remaining_before,
        )
    }

    /// The lexer's logical remaining-input length at the next token's *start*, i.e. right after
    /// its leading whitespace but before the token itself — the length
    /// `Location::current_token_start` must diff against, as opposed to
    /// [`TokenStream::logical_remaining_len`]'s *end-of-previous-token* length. Forces a peek if
    /// none is buffered yet, since whitespace is only skipped lazily inside
    /// [`crate::lexer::LexerStream::lex_token_with_start`].
    #[must_use = "looking up the remaining length has no effect unless the caller uses it"]
    pub(crate) fn token_start_remaining_len(&self) -> usize {
        use winnow::stream::Stream as _;
        drop(self.peek_token());

        self.lookahead.borrow().as_ref().map_or_else(
            || self.lexer.borrow().eof_offset(),
            |lookahead| lookahead.token_start_remaining,
        )
    }

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

    /// Captures a heredoc's body, immediately, off the lexer's raw remaining input — see
    /// [`crate::lexer::heredoc`] for the capture algorithm and why it can run eagerly rather than
    /// waiting for the line's terminating newline to actually be lexed.
    ///
    /// Must be called right after consuming the heredoc's delimiter token, with no intervening
    /// peek: it reads [`LexerStream::remaining`] directly, which is only the parser's true
    /// logical position when no token has been lexed ahead of it into [`TokenStream::lookahead`].
    /// Every redirect-parsing combinator between the delimiter and this call consumes via
    /// `Stream::next_token` (which drains any such lookahead first), so that invariant holds in
    /// practice — see `parser::redirect`.
    #[must_use = "capturing a heredoc body has no effect unless the caller uses it"]
    pub(crate) fn capture_heredoc(
        &self,
        delimiter: &str,
        strip_tabs: bool,
        splice: bool,
    ) -> Cow<'a, str> {
        let mut lexer = self.lexer.borrow_mut();
        let text = lexer.remaining();
        let already_claimed = lexer.0.state.heredoc_skip;

        let (body, consumed) =
            heredoc::capture(text, already_claimed, delimiter, strip_tabs, splice);

        lexer.0.state.heredoc_skip = already_claimed.saturating_add(consumed);

        body
    }
}
