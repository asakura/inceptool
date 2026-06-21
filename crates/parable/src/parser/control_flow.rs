//! `for`/`if`/`while`/`until` — [`parse_for_loop`], [`parse_if`], [`parse_while`],
//! [`parse_until`].

use super::{
    Expected,
    command::parse_list_until,
    word::parse_literal,
    {ParserStream, at_keyword, consume_keyword, skip_newlines, spanned},
};

use crate::types::{Expr, Spanned, SpecialParam, Statement, Token};

use winnow::{
    ModalResult, Parser as _,
    combinator::{cut_err, opt},
    error::{StrContext, StrContextValue},
    token::any,
};

use std::borrow::Cow;

const KW_FOR: &str = "for";
const KW_IN: &str = "in";
const KW_DO: &str = "do";
const KW_DONE: &str = "done";
const KW_IF: &str = "if";
const KW_THEN: &str = "then";
const KW_ELIF: &str = "elif";
const KW_ELSE: &str = "else";
const KW_FI: &str = "fi";
const KW_WHILE: &str = "while";
const KW_UNTIL: &str = "until";
const KW_LBRACE: &str = "{";
const KW_RBRACE: &str = "}";

/// Consumes one `;`/newline separator, failing if neither is next.
fn consume_separator(input: &mut ParserStream<'_>) -> ModalResult<()> {
    any.verify(|t: &Token<'_>| matches!(t, Token::Semi) || matches!(t, Token::Newline))
        .void()
        .parse_next(input)
}

/// Consumes a `{`/`}` brace, accepting either its dedicated [`Token`] variant or the plain
/// [`Token::Word`] shape the lexer produces when it can't yet tell a brace apart from an
/// ordinary word (see `grouping::parse_brace_group`'s doc for why both shapes are checked).
/// Shared by [`consume_lbrace`]/[`consume_rbrace`], which just supply which brace this is.
fn consume_brace(
    input: &mut ParserStream<'_>,
    is_brace_token: fn(&Token<'_>) -> bool,
    word: &'static str,
) -> ModalResult<()> {
    any.verify(|t: &Token<'_>| {
        is_brace_token(t) || matches!(t, Token::Word(w) if w.as_ref() == word)
    })
    .void()
    .context(StrContext::Expected(StrContextValue::StringLiteral(word)))
    .parse_next(input)
}

/// Consumes a `{`, lexed as a plain [`Token::Word`] rather than its own token kind — see
/// [`consume_brace`]. Single call site, in [`parse_for_loop`]'s `{ ... }`-as-`do`/`done` Bash
/// extension.
fn consume_lbrace(input: &mut ParserStream<'_>) -> ModalResult<()> {
    consume_brace(input, |t| matches!(t, Token::LBrace), KW_LBRACE)
}

/// Consumes a `}` — see [`consume_lbrace`].
fn consume_rbrace(input: &mut ParserStream<'_>) -> ModalResult<()> {
    consume_brace(input, |t| matches!(t, Token::RBrace), KW_RBRACE)
}

#[must_use = "parses a loop statement; discarding ignores syntax structures"]
pub(super) fn parse_for_loop<'a>(
    input: &mut ParserStream<'a>,
) -> ModalResult<Spanned<Statement<'a>>> {
    let start_offset = input.current_span_start();
    consume_keyword(input, KW_FOR)?;

    cut_err(|rest: &mut ParserStream<'a>| {
        let variable = parse_literal(rest)?;
        let implicit_iterable_at = rest.previous_span_end();

        skip_newlines(rest);

        // Bash's grammar requires a `;`/newline separator before `do`/`{` only when an `in`
        // clause was present (`for x in a b; do`); without one, the separator is optional
        // (`for x do ...` and `for x; do ...` are both valid) since `do`/`{` alone already
        // disambiguates where the loop head ends.
        let has_in_clause = at_keyword(rest, KW_IN);

        let iterable = if has_in_clause {
            consume_keyword(rest, KW_IN)?;

            let mut items = Vec::new();

            while let Ok(expr) = parse_literal(rest) {
                items.push(expr);
            }

            items
        } else {
            // No `in` clause: Bash defaults to iterating the positional parameters, quoted
            // (`"$@"`) so each one stays its own word. Built as the same `Interpolated` shape
            // real parsing of `"$@"` would produce (rather than one opaque `Literal`) so this
            // synthetic node round-trips identically to parsing its own rendered output. This
            // crate's convention for a synthetic AST node with no corresponding source text is a
            // zero-width span placed where the (absent) construct would have appeared — here,
            // immediately after `variable` — rather than reusing some unrelated token's span, so
            // a diagnostic anchored on it points at the actual gap in the source.
            let zero_width = implicit_iterable_at..implicit_iterable_at;

            vec![Spanned {
                inner: Expr::Interpolated(vec![
                    Spanned {
                        inner: Expr::Literal(Cow::Borrowed("\"")),
                        span: zero_width.clone(),
                    },
                    Spanned {
                        inner: Expr::SpecialParam(SpecialParam::AllArgs),
                        span: zero_width.clone(),
                    },
                    Spanned {
                        inner: Expr::Literal(Cow::Borrowed("\"")),
                        span: zero_width.clone(),
                    },
                ]),
                span: zero_width,
            }]
        };

        if has_in_clause {
            consume_separator(rest)?;
        } else {
            opt(consume_separator).parse_next(rest)?;
        }

        skip_newlines(rest);

        let body = if at_keyword(rest, KW_DO) {
            consume_keyword(rest, KW_DO)?;
            let body = parse_list_until(Expected::Standalone("for loop body"), rest, |inp| {
                at_keyword(inp, KW_DONE)
            })?;
            consume_keyword(rest, KW_DONE)?;
            body
        } else {
            // Bash extension: `{ ...; }` can substitute for `do ... done` directly. The braces
            // are just alternate delimiters here, not a real brace-group statement, so `body`
            // isn't wrapped in `Statement::BraceGroup`.
            consume_lbrace(rest)?;
            let body = parse_list_until(Expected::Standalone("for loop body"), rest, |inp| {
                at_keyword(inp, KW_RBRACE)
            })?;
            consume_rbrace(rest)?;
            body
        };

        Ok(spanned(
            start_offset,
            rest,
            Statement::ForLoop {
                variable,
                iterable,
                body: Box::new(body),
            },
        ))
    })
    .context(StrContext::Label("for loop"))
    .parse_next(input)
}

/// Parses the `<condition>; then <then_branch>` shared by `if` and each `elif` clause — the
/// leading `if`/`elif` keyword itself is the caller's responsibility, since callers need to peek
/// it before deciding to call this at all.
fn parse_if_head<'a>(
    input: &mut ParserStream<'a>,
) -> ModalResult<(Spanned<Statement<'a>>, Spanned<Statement<'a>>)> {
    let condition = parse_list_until(Expected::Standalone("if condition"), input, |inp| {
        at_keyword(inp, KW_THEN)
    })?;
    consume_keyword(input, KW_THEN)?;
    let then_branch = parse_list_until(Expected::Standalone("if body"), input, |inp| {
        at_keyword(inp, KW_ELIF) || at_keyword(inp, KW_ELSE) || at_keyword(inp, KW_FI)
    })?;

    Ok((condition, then_branch))
}

/// Parses the `elif`/`else` tail following an `if`'s (or another `elif`'s) `then` branch, if
/// present. An `elif` becomes a nested [`Statement::If`] in the slot `else_branch` fills,
/// recursing for any further `elif`/`else` — see [`crate::types::Statement::If`] for why that's
/// the right shape (it's also exactly what a literal `else if ...; fi` produces, so the two
/// forms are AST-indistinguishable).
fn parse_else_clause<'a>(
    input: &mut ParserStream<'a>,
) -> ModalResult<Option<Box<Spanned<Statement<'a>>>>> {
    let start_offset = input.current_span_start();
    if at_keyword(input, KW_ELIF) {
        consume_keyword(input, KW_ELIF)?;
        let (condition, then_branch) = parse_if_head(input)?;
        let else_branch = parse_else_clause(input)?;

        Ok(Some(Box::new(spanned(
            start_offset,
            input,
            Statement::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch,
            },
        ))))
    } else if at_keyword(input, KW_ELSE) {
        consume_keyword(input, KW_ELSE)?;
        let branch = parse_list_until(Expected::Standalone("else body"), input, |inp| {
            at_keyword(inp, KW_FI)
        })?;

        Ok(Some(Box::new(branch)))
    } else {
        Ok(None)
    }
}

#[must_use = "parses an if statement; discarding ignores syntax structures"]
pub(super) fn parse_if<'a>(input: &mut ParserStream<'a>) -> ModalResult<Spanned<Statement<'a>>> {
    let start_offset = input.current_span_start();
    consume_keyword(input, KW_IF)?;

    cut_err(|rest: &mut ParserStream<'a>| {
        let (condition, then_branch) = parse_if_head(rest)?;
        let else_branch = parse_else_clause(rest)?;

        consume_keyword(rest, KW_FI)?;

        Ok(spanned(
            start_offset,
            rest,
            Statement::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch,
            },
        ))
    })
    .context(StrContext::Label("if statement"))
    .parse_next(input)
}

/// Parses the shared `<condition>; do <body>; done` clause of `while`/`until` — the leading
/// keyword is the only difference between the two, so each constructs its own [`Statement`]
/// variant from the result rather than duplicating this. `label` names the calling construct
/// (`"while loop"`/`"until loop"`) for the error context attached to any failure past
/// `head_keyword`.
fn parse_loop_clause<'a>(
    input: &mut ParserStream<'a>,
    head_keyword: &'static str,
    label: &'static str,
) -> ModalResult<(Spanned<Statement<'a>>, Spanned<Statement<'a>>)> {
    consume_keyword(input, head_keyword)?;

    cut_err(|rest: &mut ParserStream<'a>| {
        let condition = parse_list_until(Expected::Standalone("loop condition"), rest, |inp| {
            at_keyword(inp, KW_DO)
        })?;
        consume_keyword(rest, KW_DO)?;
        let body = parse_list_until(Expected::Standalone("loop body"), rest, |inp| {
            at_keyword(inp, KW_DONE)
        })?;
        consume_keyword(rest, KW_DONE)?;

        Ok((condition, body))
    })
    .context(StrContext::Label(label))
    .parse_next(input)
}

#[must_use = "parses a while statement; discarding ignores syntax structures"]
pub(super) fn parse_while<'a>(input: &mut ParserStream<'a>) -> ModalResult<Spanned<Statement<'a>>> {
    let start_offset = input.current_span_start();
    let (condition, body) = parse_loop_clause(input, KW_WHILE, "while loop")?;

    Ok(spanned(
        start_offset,
        input,
        Statement::While {
            condition: Box::new(condition),
            body: Box::new(body),
        },
    ))
}

#[must_use = "parses an until statement; discarding ignores syntax structures"]
pub(super) fn parse_until<'a>(input: &mut ParserStream<'a>) -> ModalResult<Spanned<Statement<'a>>> {
    let start_offset = input.current_span_start();
    let (condition, body) = parse_loop_clause(input, KW_UNTIL, "until loop")?;

    Ok(spanned(
        start_offset,
        input,
        Statement::Until {
            condition: Box::new(condition),
            body: Box::new(body),
        },
    ))
}

// `parse_for_loop`'s conditional separator and every construct's empty-body rejection (plus
// error propagation through `&&` and `;`) are covered by the corpus's "Syntax errors" groups in
// `corpus/04_lists.tests`, `corpus/07_if_expr.tests`, and `corpus/08_loops.tests`, rather than
// duplicated here.

#[cfg(test)]
mod tests {
    use super::*;

    use crate::stream::TokenStream;

    #[derive(Debug, thiserror::Error)]
    enum TestError {
        #[error("Test failure: {0}")]
        Failure(String),
    }

    mod parse_for_loop {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn implicit_iterable_span_anchors_after_the_loop_variable() -> Result<(), TestError> {
            let mut stream = TokenStream::new("for x do echo $x; done");
            let parsed = super::parse_for_loop(&mut stream)
                .map_err(|e| TestError::Failure(e.to_string()))?;

            let Statement::ForLoop { iterable, .. } = parsed.inner else {
                return Err(TestError::Failure("expected a ForLoop statement".into()));
            };

            let [iterable] = iterable.as_slice() else {
                return Err(TestError::Failure(
                    "expected exactly one implicit iterable".into(),
                ));
            };

            // Anchored right after `x` (offset 5), not at the `for` keyword (offset 0).
            assert_eq!(iterable.span, 5..5);

            Ok(())
        }
    }
}
