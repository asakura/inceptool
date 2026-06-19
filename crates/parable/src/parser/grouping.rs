//! Subshells (`(...)`) and brace groups (`{ ...; }`) — [`parse_subshell`], [`parse_brace_group`].

use super::{ParserStream, at_keyword, parse_command, parse_statement, skip_optional_separator};

use crate::types::{Statement, Token};

use winnow::ModalResult;
use winnow::Parser as _;
use winnow::token::any;

/// The reserved word closing a brace group — see [`parse_brace_group`]'s doc for why it needs
/// its own keyword guard, unlike [`parse_subshell`].
const KW_RBRACE: &str = "}";

#[must_use = "parses a subshell; discarding ignores syntax structures"]
pub(super) fn parse_subshell<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::LParen))
        .parse_next(input)?;

    let mut body = Vec::new();

    while let Ok(stmt) = parse_statement(input) {
        body.push(stmt);

        if let Ok(_rparen) = {
            let res: ModalResult<Token<'_>> =
                any.verify(|t| matches!(t, Token::RParen)).parse_next(input);

            res
        } {
            break;
        }
    }

    Ok(Statement::Subshell { body })
}

/// Parses a brace group.
///
/// Unlike [`parse_subshell`], this can't reuse `parse_statement`-in-a-loop-then-check-for-the-
/// closer: `)` lexes as its own dedicated [`Token::RParen`], so a stray `)` simply can't be
/// mistaken for a command name and `parse_base_command` cleanly rejects it. `{`/`}` lex as plain
/// [`Token::Word`]s (see this module's `parse_brace_group`'s opening-token check, which already
/// has to accept a bare `Word("{")`), so without an explicit `at_keyword` guard — the same
/// pattern `control_flow` uses for `done`/`fi`/`else` — a `;` before the closing `}` would let
/// `parse_command` swallow `}` itself as a bogus command name.
#[must_use = "parses a brace group; discarding ignores syntax structures"]
pub(super) fn parse_brace_group<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::LBrace) || matches!(t, Token::Word(w) if w.as_ref() == "{"))
        .parse_next(input)?;

    let mut body = Vec::new();

    while !at_keyword(input, KW_RBRACE) {
        let Ok(stmt) = parse_command(input) else {
            break;
        };

        body.push(stmt);
        skip_optional_separator(input);
    }

    let _: Token<'_> = any
        .verify(|t| matches!(t, Token::RBrace) || matches!(t, Token::Word(w) if w.as_ref() == "}"))
        .parse_next(input)?;

    Ok(Statement::BraceGroup { body })
}
