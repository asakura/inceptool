//! # Lexer Architecture
//!
//! Tokenizes a bash script's raw `&str` into the [`Token`]s [`crate::parser`] consumes, one
//! token at a time via [`LexerStream::lex_token`] rather than pre-splitting the whole input up front. Lexing
//! is on-demand because bash's grammar is heavily context-sensitive (e.g. whether a reserved word
//! is a keyword or a plain argument depends on grammar position), so [`crate::parser`] is the
//! only thing that can decide what comes after this token before it's even lexed.
//!
//! ## Core Design
//!
//! [`LexerStream::lex_token`] always tries `parse_operator` before `parse_word`: operators are a small,
//! fixed set of multi-character punctuation runs (`;;&`, `<<-`, `&&`, ...) that would otherwise
//! lex as ordinary word characters, so they must win whenever they match. Within
//! `parse_operator` itself, the `alt` branches are ordered longest-prefix-first (`;;&` before
//! `;;` before `;`) for the same reason — `alt` commits to the first alternative that matches, so
//! listing a short operator before a longer one that starts with it would shadow the longer one.
//!
//! `parse_word` does not classify reserved words (`if`, `done`, `in`, ...): whether such a word
//! is a keyword or a plain argument depends on grammar position (e.g. `in` after a `for` NAME vs.
//! `in` as a command argument), which only [`crate::parser`] knows. The lexer always emits
//! [`Token::Word`] and leaves that decision to the parser.
//!
//! ## Flow
//!
//! 1. **Skip whitespace**: `skip_whitespace` consumes runs of inline spaces/tabs. Newlines are
//!    not whitespace here — they are a significant [`Token::Newline`] produced by
//!    `parse_operator`, since bash treats them as statement separators.
//! 2. **Check for EOF**: if nothing remains after skipping whitespace, [`LexerStream::lex_token`] returns
//!    [`Token::Eof`] directly rather than trying either parser.
//! 3. **Try an operator, then a word**: `parse_operator` matches the fixed set of punctuation
//!    tokens; on backtrack, `parse_word` scans forward by hand (not via winnow combinators) to
//!    find the end of the next word, then slices it off in one shot with `take`.
//!
//! ## Edge Cases
//!
//! - **Quoting suspends metacharacter recognition**: inside `'...'` or `"..."`, characters that
//!   would otherwise end a word (whitespace, `;`, `|`, `&`, `(`, `)`, `<`, `>`) are just word
//!   content. `parse_word`'s `in_single`/`in_double` flags track this.
//! - **Nested expansions can contain anything**: `$(...)`, `${...}`, and `` `...` `` can contain
//!   whitespace, operators, even further nested expansions, without ending the word —
//!   `parse_word`'s `paren_depth`/`brace_depth`/`backtick_depth` counters track nesting so the
//!   word only ends once every expansion opened inside it has closed. Depth is tracked even
//!   inside double quotes (where `$(...)`/`${...}` still expand) but not inside single quotes
//!   (where bash performs no expansion at all).
//! - **Escaping consumes exactly one character verbatim**: a backslash and the character after it
//!   are both folded into the word unconditionally, so an escaped metacharacter, quote, or even a
//!   backslash itself never flips quoting state or changes expansion depth.
//! - **An empty match is a parse failure, not an empty word**: if the scan above consumes zero
//!   characters (e.g. the cursor sits on a character `parse_operator` already rejected),
//!   `parse_word` backtracks instead of returning [`Token::Word`] with empty content.

#![expect(
    clippy::multiple_inherent_impl,
    reason = "methods are split across files by category"
)]

pub(crate) mod heredoc;
mod operator;
mod traits;
mod word;

use crate::types::{LexerState, Token};

use winnow::{ModalResult, Parser as _, combinator::alt, stream::Stateful, token::take_while};

use std::mem;

/// The character-level [`winnow::stream::Stream`] consumed by [`LexerStream::lex_token`].
///
/// Pairs the lexer's raw `&str` cursor with [`LexerState`] so context (e.g. heredoc
/// delimiters) can travel alongside it.
///
/// Wraps a winnow [`Stateful`] rather than aliasing it directly, so the crate can hang its own
/// constructor and trait impls off this type — not possible through a type alias to a foreign
/// type. Every `Stream`/`StreamIsPartial`/`Compare`/`Offset` method is a thin forward to
/// the wrapped [`Stateful`], which already implements them correctly.
#[derive(Debug, Clone)]
#[expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "shared within module hierarchy"
)]
pub struct LexerStream<'a>(pub(crate) Stateful<&'a str, LexerState>);

impl AsRef<str> for LexerStream<'_> {
    fn as_ref(&self) -> &str {
        self.0.input
    }
}

impl<'a> LexerStream<'a> {
    /// The lexer's current remaining input, carrying its true `'a` lifetime — unlike
    /// [`AsRef::as_ref`]'s elided signature, which ties the return value's lifetime to `&self`
    /// rather than to the underlying buffer. [`crate::stream::TokenStream::capture_heredoc`]
    /// needs the longer lifetime to build a [`std::borrow::Cow`] body that outlives the borrow
    /// used to read it.
    #[must_use = "looking up the remaining input has no effect unless the caller uses it"]
    pub(crate) const fn remaining(&self) -> &'a str {
        self.0.input
    }
}

impl<'a> LexerStream<'a> {
    /// Builds a lexer stream that reads `input` from byte offset 0, with default lexer state.
    #[must_use = "constructs the lexer stream; discarding it lexes nothing"]
    pub fn new(input: &'a str) -> Self {
        Self(Stateful {
            input,
            state: LexerState::default(),
        })
    }

    /// Consumes a run of inline spaces/tabs. Newlines are deliberately excluded — see the
    /// module-level Flow note on why they're a token rather than whitespace.
    #[must_use = "consumes whitespace; failure to use leaves whitespace unparsed"]
    fn skip_whitespace(&mut self) -> ModalResult<()> {
        take_while(0.., (' ', '\t')).void().parse_next(self)
    }

    /// Lexes the next token, returning it together with `Stream::eof_offset` measured right
    /// after its leading whitespace was skipped but before the token itself was consumed — the
    /// position [`crate::stream::TokenStream`] needs to report a token's true *start*, as
    /// opposed to the position right after the *previous* token (which still includes this
    /// token's upcoming whitespace).
    ///
    /// # Errors
    /// Returns an error if the token cannot be parsed.
    #[must_use = "lexes the next token; failure to use leaves stream state unchanged"]
    pub(crate) fn lex_token_with_start(&mut self) -> ModalResult<(usize, Token<'a>)> {
        use winnow::stream::Stream as _;
        self.skip_whitespace()?;

        let start = self.eof_offset();

        if self.as_ref().is_empty() {
            return Ok((start, Token::Eof));
        }

        let token = alt((Self::parse_operator, Self::parse_word)).parse_next(self)?;

        // A `<<`/`<<-` redirect already captured its body eagerly, the moment its delimiter was
        // parsed (see `crate::stream::TokenStream::capture_heredoc`), by reading past — without
        // consuming — whatever lay beyond the *next* unlexed newline. Now that this lexer has
        // actually reached that exact newline for real, the cursor must jump over the body (and
        // every other heredoc's body queued on the same line) before normal tokenizing resumes.
        if matches!(token, Token::Newline) {
            let skip = mem::take(&mut self.0.state.heredoc_skip);

            if skip > 0 {
                self.0.input = self.0.input.get(skip..).unwrap_or_default();
            }
        }

        Ok((start, token))
    }

    /// Lexes the next token from the stream.
    ///
    /// Named `lex_token` rather than `next_token` to avoid colliding with this type's own
    /// `Stream::next_token` impl above, which advances by a single character rather than a
    /// whole lexed token.
    ///
    /// # Errors
    /// Returns an error if the token cannot be parsed.
    #[must_use = "lexes the next token; failure to use leaves stream state unchanged"]
    pub fn lex_token(&mut self) -> ModalResult<Token<'a>> {
        self.lex_token_with_start().map(|(_, token)| token)
    }
}
