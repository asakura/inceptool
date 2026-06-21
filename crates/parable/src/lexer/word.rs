use super::LexerStream;
use crate::types::Token;

use std::borrow::Cow;
use winnow::{
    ModalResult, Parser as _,
    error::{ContextError, ErrMode},
    token::take,
};

impl<'a> LexerStream<'a> {
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
    pub(super) fn parse_word(&mut self) -> ModalResult<Token<'a>> {
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
}
