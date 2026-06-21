//! Subshells (`(...)`) and brace groups (`{ ...; }`) — [`parse_subshell`], [`parse_brace_group`].

use super::{
    command::parse_list_until,
    {ParserStream, at_keyword},
};

use crate::types::{Statement, Token};

use winnow::{
    ModalResult, Parser as _, combinator::cut_err, error::StrContext, stream::Stream as _,
    token::any,
};

/// The reserved word closing a brace group — see [`parse_brace_group`]'s doc for why it needs
/// its own keyword guard, unlike [`parse_subshell`].
const KW_RBRACE: &str = "}";

#[must_use = "parses a subshell; discarding ignores syntax structures"]
pub(super) fn parse_subshell<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    any.verify(|t| matches!(t, Token::LParen))
        .void()
        .parse_next(input)?;

    cut_err(|rest: &mut ParserStream<'a>| {
        let body = parse_list_until(rest, |inp| matches!(inp.peek_token(), Some(Token::RParen)))?;

        any.verify(|t| matches!(t, Token::RParen))
            .void()
            .parse_next(rest)?;

        Ok(Statement::Subshell {
            body: Box::new(body),
        })
    })
    .context(StrContext::Label("subshell"))
    .parse_next(input)
}

/// Parses a brace group.
///
/// Unlike [`parse_subshell`], this can't reuse `parse_statement`-in-a-loop-then-check-for-the-
/// closer: `)` lexes as its own dedicated [`Token::RParen`], so a stray `)` simply can't be
/// mistaken for a command name and `parse_base_command` cleanly rejects it. `{`/`}` lex as plain
/// [`Token::Word`]s (see this module's `parse_brace_group`'s opening-token check, which already
/// has to accept a bare `Word("{")`), so [`parse_list_until`] is given an `at_keyword` guard — the
/// same pattern `control_flow` uses for `done`/`fi`/`else` — to stop before trying to fold one
/// `and_or` too many and handing `}` to `parse_base_command` as a bogus command name.
#[must_use = "parses a brace group; discarding ignores syntax structures"]
pub(super) fn parse_brace_group<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    any.verify(|t| matches!(t, Token::LBrace) || matches!(t, Token::Word(w) if w.as_ref() == "{"))
        .void()
        .parse_next(input)?;

    cut_err(|rest: &mut ParserStream<'a>| {
        let body = parse_list_until(rest, |inp| at_keyword(inp, KW_RBRACE))?;

        any.verify(|t| {
            matches!(t, Token::RBrace) || matches!(t, Token::Word(w) if w.as_ref() == "}")
        })
        .void()
        .parse_next(rest)?;

        Ok(Statement::BraceGroup {
            body: Box::new(body),
        })
    })
    .context(StrContext::Label("brace group"))
    .parse_next(input)
}

// `parse_subshell`'s rejection of an empty `()` and propagation of a malformed nested compound
// command's error are covered by the "Syntax errors" group in `corpus/06_compound.tests`,
// rather than duplicated here.
