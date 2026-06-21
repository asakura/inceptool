//! `for`/`if`/`while`/`until` — [`parse_for_loop`], [`parse_if`], [`parse_while`],
//! [`parse_until`].

use super::{
    command::parse_list_until,
    word::parse_literal,
    {ParserStream, at_keyword, consume_keyword, skip_newlines},
};

use crate::types::{Expr, Statement, Token};

use winnow::{
    ModalResult, Parser as _,
    combinator::{cut_err, opt},
    error::StrContext,
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
///
/// Single call site each in [`parse_for_loop`] (its `in`-clause path) and [`parse_loop_clause`],
/// not two — but kept as its own named function rather than inlined, since both call sites also
/// reach it conditionally through [`opt`]/plain `?`, and naming the operation here documents the
/// shared grammar rule (`;`/newline is the only valid separator) once instead of letting the
/// `Token::Semi`/`Token::Newline` check drift between call sites.
fn consume_separator(input: &mut ParserStream<'_>) -> ModalResult<()> {
    any.verify(|t: &Token<'_>| matches!(t, Token::Semi) || matches!(t, Token::Newline))
        .void()
        .parse_next(input)
}

/// Consumes a `{`, lexed as a plain [`Token::Word`] rather than its own token kind — see
/// `grouping::parse_brace_group`'s doc for why both [`Token::LBrace`] and `Word("{")` are checked.
///
/// Single call site, like [`consume_rbrace`] — kept separate from [`parse_for_loop`] anyway
/// because it pairs the two token shapes `{` can take, which is a fact about the lexer, not
/// about the for-loop grammar calling it.
fn consume_lbrace(input: &mut ParserStream<'_>) -> ModalResult<()> {
    any.verify(|t: &Token<'_>| {
        matches!(t, Token::LBrace) || matches!(t, Token::Word(w) if w.as_ref() == KW_LBRACE)
    })
    .void()
    .parse_next(input)
}

/// Consumes a `}` — see [`consume_lbrace`].
fn consume_rbrace(input: &mut ParserStream<'_>) -> ModalResult<()> {
    any.verify(|t: &Token<'_>| {
        matches!(t, Token::RBrace) || matches!(t, Token::Word(w) if w.as_ref() == KW_RBRACE)
    })
    .void()
    .parse_next(input)
}

#[must_use = "parses a loop statement; discarding ignores syntax structures"]
pub(super) fn parse_for_loop<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    consume_keyword(input, KW_FOR)?;

    cut_err(|rest: &mut ParserStream<'a>| {
        let variable = parse_literal(rest)?;

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
            // No `in` clause: Bash defaults to iterating the positional parameters.
            vec![Expr::Literal(Cow::Borrowed("\"$@\""))]
        };

        if has_in_clause {
            consume_separator(rest)?;
        } else {
            opt(consume_separator).parse_next(rest)?;
        }

        skip_newlines(rest);

        let body = if at_keyword(rest, KW_DO) {
            consume_keyword(rest, KW_DO)?;
            let body = parse_list_until(rest, |inp| at_keyword(inp, KW_DONE))?;
            consume_keyword(rest, KW_DONE)?;
            body
        } else {
            // Bash extension: `{ ...; }` can substitute for `do ... done` directly. The braces
            // are just alternate delimiters here, not a real brace-group statement, so `body`
            // isn't wrapped in `Statement::BraceGroup`.
            consume_lbrace(rest)?;
            let body = parse_list_until(rest, |inp| at_keyword(inp, KW_RBRACE))?;
            consume_rbrace(rest)?;
            body
        };

        Ok(Statement::ForLoop {
            variable,
            iterable,
            body: Box::new(body),
        })
    })
    .context(StrContext::Label("for loop"))
    .parse_next(input)
}

/// Parses the `<condition>; then <then_branch>` shared by `if` and each `elif` clause — the
/// leading `if`/`elif` keyword itself is the caller's responsibility, since callers need to peek
/// it before deciding to call this at all.
fn parse_if_head<'a>(input: &mut ParserStream<'a>) -> ModalResult<(Statement<'a>, Statement<'a>)> {
    let condition = parse_list_until(input, |inp| at_keyword(inp, KW_THEN))?;
    consume_keyword(input, KW_THEN)?;
    let then_branch = parse_list_until(input, |inp| {
        at_keyword(inp, KW_ELIF) || at_keyword(inp, KW_ELSE) || at_keyword(inp, KW_FI)
    })?;

    Ok((condition, then_branch))
}

/// Parses the `elif`/`else` tail following an `if`'s (or another `elif`'s) `then` branch, if
/// present. An `elif` becomes a nested [`Statement::If`] in the slot `else_branch` fills,
/// recursing for any further `elif`/`else` — see [`crate::types::Statement::If`] for why that's
/// the right shape (it's also exactly what a literal `else if ...; fi` produces, so the two
/// forms are AST-indistinguishable).
fn parse_else_clause<'a>(input: &mut ParserStream<'a>) -> ModalResult<Option<Box<Statement<'a>>>> {
    if at_keyword(input, KW_ELIF) {
        consume_keyword(input, KW_ELIF)?;
        let (condition, then_branch) = parse_if_head(input)?;
        let else_branch = parse_else_clause(input)?;

        Ok(Some(Box::new(Statement::If {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch,
        })))
    } else if at_keyword(input, KW_ELSE) {
        consume_keyword(input, KW_ELSE)?;
        let branch = parse_list_until(input, |inp| at_keyword(inp, KW_FI))?;

        Ok(Some(Box::new(branch)))
    } else {
        Ok(None)
    }
}

#[must_use = "parses an if statement; discarding ignores syntax structures"]
pub(super) fn parse_if<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    consume_keyword(input, KW_IF)?;

    cut_err(|rest: &mut ParserStream<'a>| {
        let (condition, then_branch) = parse_if_head(rest)?;
        let else_branch = parse_else_clause(rest)?;

        consume_keyword(rest, KW_FI)?;

        Ok(Statement::If {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch,
        })
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
) -> ModalResult<(Statement<'a>, Statement<'a>)> {
    consume_keyword(input, head_keyword)?;

    cut_err(|rest: &mut ParserStream<'a>| {
        let condition = parse_list_until(rest, |inp| at_keyword(inp, KW_DO))?;
        consume_keyword(rest, KW_DO)?;
        let body = parse_list_until(rest, |inp| at_keyword(inp, KW_DONE))?;
        consume_keyword(rest, KW_DONE)?;

        Ok((condition, body))
    })
    .context(StrContext::Label(label))
    .parse_next(input)
}

#[must_use = "parses a while statement; discarding ignores syntax structures"]
pub(super) fn parse_while<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let (condition, body) = parse_loop_clause(input, KW_WHILE, "while loop")?;

    Ok(Statement::While {
        condition: Box::new(condition),
        body: Box::new(body),
    })
}

#[must_use = "parses an until statement; discarding ignores syntax structures"]
pub(super) fn parse_until<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let (condition, body) = parse_loop_clause(input, KW_UNTIL, "until loop")?;

    Ok(Statement::Until {
        condition: Box::new(condition),
        body: Box::new(body),
    })
}

// `parse_for_loop`'s conditional separator and every construct's empty-body rejection (plus
// error propagation through `&&` and `;`) are covered by the corpus's "Syntax errors" groups in
// `corpus/04_lists.tests`, `corpus/07_if_expr.tests`, and `corpus/08_loops.tests`, rather than
// duplicated here.
