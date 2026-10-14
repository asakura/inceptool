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
