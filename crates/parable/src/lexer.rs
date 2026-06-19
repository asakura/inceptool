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
//!    find the end of the next word, then slices it off in one shot with [`take`].
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

use crate::types::{LexerState, Token};

use winnow::{
    ModalResult, Parser as _,
    combinator::alt,
    error::{ContextError, ErrMode},
    stream::{Compare, CompareResult, Needed, Offset, Stateful, Stream, StreamIsPartial},
    token::{take, take_while},
};

use std::borrow::Cow;
use std::fmt;

/// The character-level [`Stream`] consumed by [`LexerStream::lex_token`], pairing the lexer's raw `&str`
/// cursor with [`LexerState`] so context (e.g. heredoc delimiters) can travel alongside it.
///
/// Wraps a winnow [`Stateful`] rather than aliasing it directly, so the crate can hang its own
/// constructor and trait impls off this type — not possible through a type alias to a foreign
/// type. Every [`Stream`]/[`StreamIsPartial`]/[`Compare`]/[`Offset`] method is a thin forward to
/// the wrapped [`Stateful`], which already implements them correctly.
#[derive(Debug, Clone)]
pub struct LexerStream<'a>(Stateful<&'a str, LexerState<'a>>);

impl AsRef<str> for LexerStream<'_> {
    fn as_ref(&self) -> &str {
        self.0.input
    }
}

#[expect(
    clippy::missing_trait_methods,
    reason = "default-provided Stream methods (the *_unchecked slicers winnow's own slice impls \
              override with unsafe, plus finish/peek_finish) are correct as-is and unsafe_code \
              is forbidden in this crate"
)]
impl<'a> Stream for LexerStream<'a> {
    type Token = <Stateful<&'a str, LexerState<'a>> as Stream>::Token;
    type Slice = <Stateful<&'a str, LexerState<'a>> as Stream>::Slice;
    type IterOffsets = <Stateful<&'a str, LexerState<'a>> as Stream>::IterOffsets;
    type Checkpoint = Self;

    fn iter_offsets(&self) -> Self::IterOffsets {
        self.0.iter_offsets()
    }

    fn eof_offset(&self) -> usize {
        self.0.eof_offset()
    }

    fn next_token(&mut self) -> Option<Self::Token> {
        self.0.next_token()
    }

    fn peek_token(&self) -> Option<Self::Token> {
        self.0.peek_token()
    }

    fn offset_for<P>(&self, predicate: P) -> Option<usize>
    where
        P: Fn(Self::Token) -> bool,
    {
        self.0.offset_for(predicate)
    }

    fn offset_at(&self, tokens: usize) -> Result<usize, Needed> {
        self.0.offset_at(tokens)
    }

    fn next_slice(&mut self, offset: usize) -> Self::Slice {
        self.0.next_slice(offset)
    }

    fn peek_slice(&self, offset: usize) -> Self::Slice {
        self.0.peek_slice(offset)
    }

    fn checkpoint(&self) -> Self::Checkpoint {
        self.clone()
    }

    fn reset(&mut self, checkpoint: &Self::Checkpoint) {
        self.clone_from(checkpoint);
    }

    fn trace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.trace(f)
    }
}

impl Offset for LexerStream<'_> {
    fn offset_from(&self, start: &Self) -> usize {
        self.0.offset_from(&start.0)
    }
}

#[expect(
    clippy::missing_trait_methods,
    reason = "is_partial's default (Self::is_partial_supported()) and is_partial's default \
              (false) are correct as-is"
)]
impl<'a> StreamIsPartial for LexerStream<'a> {
    type PartialState = <Stateful<&'a str, LexerState<'a>> as StreamIsPartial>::PartialState;

    #[expect(
        clippy::semicolon_if_nothing_returned,
        reason = "the trailing expression is this fn's must_use return value, not a discarded \
                  statement; a trailing `;` would make it one and trip unused_must_use"
    )]
    fn complete(&mut self) -> Self::PartialState {
        self.0.complete()
    }

    fn restore_partial(&mut self, state: Self::PartialState) {
        self.0.restore_partial(state);
    }

    fn is_partial_supported() -> bool {
        <Stateful<&'a str, LexerState<'a>> as StreamIsPartial>::is_partial_supported()
    }
}

impl<'a, U> Compare<U> for LexerStream<'a>
where
    &'a str: Compare<U>,
{
    fn compare(&self, t: U) -> CompareResult {
        self.0.compare(t)
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

    /// Matches the fixed-punctuation operator token at the cursor, if any.
    ///
    /// Branches are ordered longest-prefix-first (e.g. `;;&` before `;;` before `;`); see the
    /// module-level Core Design note for why that order is load-bearing.
    #[must_use = "constructs the operator token; failure to use drops the parsed stream state"]
    fn parse_operator(&mut self) -> ModalResult<Token<'a>> {
        alt((
            alt((
                ";;&".value(Token::SemiSemiAmp),
                "<<-".value(Token::LessLessMinus),
                "<<<".value(Token::LessLessLess),
                "&>>".value(Token::AmpGreaterGreater),
            )),
            alt((
                "&&".value(Token::AndAnd),
                "||".value(Token::OrOr),
                ";;".value(Token::SemiSemi),
                ";&".value(Token::SemiAmp),
                "<<".value(Token::LessLess),
                ">>".value(Token::GreaterGreater),
            )),
            alt((
                "<&".value(Token::LessAmp),
                ">&".value(Token::GreaterAmp),
                "<>".value(Token::LessGreater),
                ">|".value(Token::GreaterPipe),
                "&>".value(Token::AmpGreater),
                "|&".value(Token::PipeAmp),
            )),
            alt((
                ";".value(Token::Semi),
                "|".value(Token::Pipe),
                "&".value(Token::Amp),
                "(".value(Token::LParen),
                ")".value(Token::RParen),
                "<".value(Token::Less),
                ">".value(Token::Greater),
                "\n".value(Token::Newline),
            )),
        ))
        .parse_next(self)
    }

    /// Scans a maximal run of word content starting at the cursor and slices it off as a
    /// [`Token::Word`], without classifying it as a reserved word (see module-level Core Design).
    ///
    /// Quoting, escaping, and nested `$(...)`/`${...}`/`` `...` `` expansions are treated as
    /// transparent to the scan — see the module-level Edge Cases for exactly how each is tracked.
    #[must_use = "constructs the word token; failure to use drops the parsed stream state"]
    #[expect(
        clippy::too_many_lines,
        reason = "lexer state machine is inherently long"
    )]
    #[expect(
        clippy::else_if_without_else,
        reason = "state machine branches are independent"
    )]
    fn parse_word(&mut self) -> ModalResult<Token<'a>> {
        let s: &str = self.as_ref();

        let mut chars = s.chars().peekable();
        let mut len: usize = 0;
        let mut in_single = false;
        let mut in_double = false;
        let mut paren_depth: usize = 0;
        let mut brace_depth: usize = 0;
        let mut backtick_depth: usize = 0;
        let mut escaped = false;

        while let Some(&c) = chars.peek() {
            if escaped {
                escaped = false;
                len = len.saturating_add(c.len_utf8());
                chars.next();
                continue;
            }

            if c == '\\' {
                escaped = true;
                len = len.saturating_add(c.len_utf8());
                chars.next();
                continue;
            }

            if in_single {
                if c == '\'' {
                    in_single = false;
                }
                len = len.saturating_add(c.len_utf8());
                chars.next();
                continue;
            }

            if c == '\'' {
                in_single = true;
                len = len.saturating_add(c.len_utf8());
                chars.next();
                continue;
            }

            if in_double {
                if c == '"' {
                    in_double = false;
                } else if c == '`' {
                    backtick_depth = backtick_depth.saturating_add(1);
                } else if c == '$' {
                    let mut lookahead = chars.clone();

                    lookahead.next();

                    if let Some(&n) = lookahead.peek() {
                        if n == '(' {
                            paren_depth = paren_depth.saturating_add(1);
                            len = len
                                .saturating_add(c.len_utf8())
                                .saturating_add(n.len_utf8());
                            chars.next(); // consume $
                            chars.next(); // consume (

                            continue;
                        } else if n == '{' {
                            brace_depth = brace_depth.saturating_add(1);
                            len = len
                                .saturating_add(c.len_utf8())
                                .saturating_add(n.len_utf8());
                            chars.next(); // consume $
                            chars.next(); // consume {

                            continue;
                        }
                    }
                } else if paren_depth > 0 && c == ')' {
                    paren_depth = paren_depth.saturating_sub(1);
                } else if brace_depth > 0 && c == '}' {
                    brace_depth = brace_depth.saturating_sub(1);
                }

                len = len.saturating_add(c.len_utf8());
                chars.next();

                continue;
            }

            // We are unquoted
            if backtick_depth > 0 {
                if c == '`' {
                    backtick_depth = backtick_depth.saturating_sub(1);
                }

                len = len.saturating_add(c.len_utf8());
                chars.next();

                continue;
            }

            if brace_depth > 0 {
                if c == '}' {
                    brace_depth = brace_depth.saturating_sub(1);
                }

                len = len.saturating_add(c.len_utf8());
                chars.next();

                continue;
            }

            if paren_depth > 0 {
                if c == ')' {
                    paren_depth = paren_depth.saturating_sub(1);
                } else if c == '(' {
                    paren_depth = paren_depth.saturating_add(1);
                }

                len = len.saturating_add(c.len_utf8());
                chars.next();

                continue;
            }

            if c == '`' {
                backtick_depth = backtick_depth.saturating_add(1);
                len = len.saturating_add(c.len_utf8());
                chars.next();

                continue;
            }

            if c == '$' {
                let mut lookahead = chars.clone();

                lookahead.next();

                if let Some(&n) = lookahead.peek() {
                    if n == '(' {
                        paren_depth = paren_depth.saturating_add(1);
                        len = len
                            .saturating_add(c.len_utf8())
                            .saturating_add(n.len_utf8());
                        chars.next(); // consume $
                        chars.next(); // consume (

                        continue;
                    } else if n == '{' {
                        brace_depth = brace_depth.saturating_add(1);
                        len = len
                            .saturating_add(c.len_utf8())
                            .saturating_add(n.len_utf8());
                        chars.next(); // consume $
                        chars.next(); // consume {

                        continue;
                    }
                }
            }

            if c == '"' {
                in_double = true;
                len = len.saturating_add(c.len_utf8());
                chars.next();

                continue;
            }

            // If we are unquoted and not in any expansion, check for metachars
            if c.is_whitespace()
                || c == ';'
                || c == '|'
                || c == '&'
                || c == '('
                || c == ')'
                || c == '<'
                || c == '>'
            {
                break;
            }

            len = len.saturating_add(c.len_utf8());
            chars.next();
        }

        if len == 0 {
            return Err(ErrMode::Backtrack(ContextError::new()));
        }

        let word = take(len).parse_next(self)?;

        Ok(Token::Word(Cow::Borrowed(word)))
    }

    /// Consumes a run of inline spaces/tabs. Newlines are deliberately excluded — see the
    /// module-level Flow note on why they're a token rather than whitespace.
    #[must_use = "consumes whitespace; failure to use leaves whitespace unparsed"]
    fn skip_whitespace(&mut self) -> ModalResult<()> {
        take_while(0.., (' ', '\t')).void().parse_next(self)
    }

    /// Lexes the next token from the stream.
    ///
    /// Named `lex_token` rather than `next_token` to avoid colliding with this type's own
    /// [`Stream::next_token`] impl above, which advances by a single character rather than a
    /// whole lexed token.
    ///
    /// # Errors
    /// Returns an error if the token cannot be parsed.
    #[must_use = "lexes the next token; failure to use leaves stream state unchanged"]
    pub fn lex_token(&mut self) -> ModalResult<Token<'a>> {
        self.skip_whitespace()?;

        if self.as_ref().is_empty() {
            return Ok(Token::Eof);
        }

        alt((Self::parse_operator, Self::parse_word)).parse_next(self)
    }
}
