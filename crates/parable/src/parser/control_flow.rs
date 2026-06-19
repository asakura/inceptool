//! `for`/`if`/`while`/`until` — see [`super`]'s Edge Cases for `until`'s and `if`'s known gaps.

use super::word::parse_literal;
use super::{ParserStream, at_keyword, parse_command, parse_statement, skip_optional_separator};

use crate::types::{Statement, Token};

use winnow::ModalResult;
use winnow::Parser as _;
use winnow::token::any;

use std::borrow::Cow;

const KW_FOR: &str = "for";
const KW_IN: &str = "in";
const KW_DO: &str = "do";
const KW_DONE: &str = "done";
const KW_IF: &str = "if";
const KW_THEN: &str = "then";
const KW_ELSE: &str = "else";
const KW_FI: &str = "fi";
const KW_WHILE: &str = "while";
const KW_UNTIL: &str = "until";

#[must_use = "parses a loop statement; discarding ignores syntax structures"]
pub(super) fn parse_for_loop<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_FOR))
        .parse_next(input)?;

    let variable = any
        .verify_map(|t| match t {
            Token::Word(Cow::Borrowed(s)) => Some(s),
            _ => None,
        })
        .parse_next(input)?;

    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_IN))
        .parse_next(input)?;

    let mut iterable = Vec::new();

    while let Ok(expr) = parse_literal(input) {
        iterable.push(expr);
    }

    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Semi) || matches!(t, Token::Newline))
        .parse_next(input)?;

    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_DO))
        .parse_next(input)?;

    let mut body = Vec::new();

    while !at_keyword(input, KW_DONE) {
        let Ok(stmt) = parse_command(input) else {
            break;
        };

        body.push(stmt);
        skip_optional_separator(input);
    }

    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_DONE))
        .parse_next(input)?;

    Ok(Statement::ForLoop {
        variable,
        iterable,
        body,
    })
}

#[must_use = "parses an if statement; discarding ignores syntax structures"]
pub(super) fn parse_if<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_IF))
        .parse_next(input)?;

    let mut condition = Vec::new();

    while !at_keyword(input, KW_THEN) {
        let Ok(stmt) = parse_command(input) else {
            break;
        };

        condition.push(stmt);
        skip_optional_separator(input);
    }

    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_THEN))
        .parse_next(input)?;

    let mut then_branch = Vec::new();

    while !at_keyword(input, KW_ELSE) && !at_keyword(input, KW_FI) {
        let Ok(stmt) = parse_command(input) else {
            break;
        };

        then_branch.push(stmt);
        skip_optional_separator(input);
    }

    // Check for else (elif is not yet supported by this parser)
    let mut else_branch = None;

    let else_result: ModalResult<Token<'_>> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_ELSE))
        .parse_next(input);

    if let Ok(_token) = else_result {
        let mut branch = Vec::new();

        while !at_keyword(input, KW_FI) {
            let Ok(stmt) = parse_command(input) else {
                break;
            };

            branch.push(stmt);
            skip_optional_separator(input);
        }

        else_branch = Some(branch);
    }

    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_FI))
        .parse_next(input)?;

    Ok(Statement::If {
        condition,
        then_branch,
        else_branch,
    })
}

#[must_use = "parses a while statement; discarding ignores syntax structures"]
pub(super) fn parse_while<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_WHILE))
        .parse_next(input)?;

    let mut condition = Vec::new();

    while !at_keyword(input, KW_DO) {
        let Ok(stmt) = parse_command(input) else {
            break;
        };

        condition.push(stmt);
        skip_optional_separator(input);
    }

    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_DO))
        .parse_next(input)?;

    let mut body = Vec::new();

    while !at_keyword(input, KW_DONE) {
        let Ok(stmt) = parse_command(input) else {
            break;
        };

        body.push(stmt);
        skip_optional_separator(input);
    }

    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_DONE))
        .parse_next(input)?;

    Ok(Statement::While { condition, body })
}

#[must_use = "parses an until statement; discarding ignores syntax structures"]
pub(super) fn parse_until<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_UNTIL))
        .parse_next(input)?;

    let mut condition = Vec::new();

    while !at_keyword(input, KW_DO) {
        let Ok(stmt) = parse_statement(input) else {
            break;
        };

        condition.push(stmt);
    }

    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_DO))
        .parse_next(input)?;

    let mut body = Vec::new();

    while !at_keyword(input, KW_DONE) {
        let Ok(stmt) = parse_statement(input) else {
            break;
        };

        body.push(stmt);
    }

    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::Word(w) if w.as_ref() == KW_DONE))
        .parse_next(input)?;

    Ok(Statement::Until { condition, body })
}
