//! # Parser Architecture
//!
//! Builds a [`Statement`] AST from the [`Token`] stream [`crate::lexer`] produces, using winnow
//! combinators driven directly off [`ParserStream`] (a [`TokenStream`] alias) rather than a
//! hand-rolled recursive-descent loop.
//!
//! ## Core Design
//!
//! Bash's grammar is split across five sibling modules: `word` turns a lexed `Word` token into
//! an `Expr` (resolving `$NAME`/`${NAME}` references); `command` covers simple commands,
//! pipelines, and command lists; `control_flow` covers `for`/`if`/`while`/`until`; `grouping`
//! covers subshells and brace groups; `case` covers `case`/`in`/`esac`. `parse_command` is the
//! dispatch point that ties them together — adding a new compound command (e.g. a function
//! definition) means adding its own sibling module and one more arm to this `alt`.
//!
//! Bash reserved words (`done`, `fi`, `then`, ...) lex as plain [`Token::Word`]s rather than
//! their own token kind (see [`crate::lexer`]'s module doc) — `at_keyword` is how every
//! compound-command parser recognizes one at the position the grammar expects it, without the
//! lexer needing to know grammar position itself.
//!
//! `redirect` is a cross-cutting sibling rather than another `alt` arm in [`parse_command`]:
//! every compound command (and `command::parse_base_command`'s own simple command) can have
//! redirects trailing it, so `parse_command` consumes them itself after the `alt` succeeds,
//! wrapping the result in [`crate::types::Statement::Redirected`] rather than each compound
//! parser doing so independently.
//!
//! `command::parse_list` and `command::parse_and_or` mirror POSIX's two-level `list`/`and_or`
//! split: `&&`/`||` (`and_or`) bind tighter than `;`/`&`/newline (`list`), so a `&&`/`||` chain
//! always folds into one [`crate::types::Statement::AndOr`] before `parse_list` sees the next
//! `;`/`&`/newline separator. This isn't just a precedence nicety — `&`'s scope depends on it:
//! `a && b &` backgrounds the whole `a && b` conjunction, not just `b`.
//!
//! ## Flow
//!
//! 1. [`parse_statement`] (the crate's entry point) parses a full `;`/`&`/`&&`/`||`/newline
//!    -separated `command::parse_list`.
//! 2. Each list item is an `and_or` chain (`command::parse_and_or`) of one or more pipelines
//!    (`command::parse_pipeline`) joined by `&&`/`||`; each pipeline is one or more
//!    `parse_command`s joined by `|`.
//! 3. `parse_command` tries each compound-command parser before falling back to `command`'s
//!    base-command parser — order matters only in that a compound command must be recognized by
//!    its leading keyword/punctuation before a plain command parser would otherwise swallow it
//!    as a command name.

mod case;
mod command;
mod control_flow;
mod grouping;
mod redirect;
mod word;

pub(crate) use word::{Segment, interpolation_segments};

use crate::{
    stream::TokenStream,
    types::{Statement, Token},
};

use winnow::{ModalResult, Parser as _, combinator::alt, stream::Stream as _, token::any};

/// The token-level stream every parser function in this module tree consumes.
pub type ParserStream<'a> = TokenStream<'a>;

/// Checks whether the next token is the given reserved word without consuming it.
///
/// Bash reserved words (`done`, `fi`, `then`, ...) lex as plain `Word`s, so a
/// command-collection loop must peek for them before attempting to parse another
/// command — otherwise a base-command parse would happily consume the closing
/// keyword as a command name.
#[must_use = "peeking has no effect unless the caller acts on the result"]
fn at_keyword(input: &ParserStream<'_>, keyword: &str) -> bool {
    matches!(input.peek_token(), Some(Token::Word(w)) if w.as_ref() == keyword)
}

/// Consumes the next token if it's the reserved word `keyword`, failing otherwise. Shared by
/// every sibling module that recognizes a leading keyword (`for`/`if`/`while`/`until`/`case`),
/// rather than each keeping its own byte-for-byte copy.
fn consume_keyword(input: &mut ParserStream<'_>, keyword: &'static str) -> ModalResult<()> {
    any.verify(|t: &Token<'_>| matches!(t, Token::Word(w) if w.as_ref() == keyword))
        .void()
        .parse_next(input)
}

/// Consumes every immediately-following newline, if any. Blank lines carry no meaning beyond
/// the `;`/newline separator a command list already tracks, so runs of them (between
/// `in`/`do`/`then` and the next real token, around `case`'s `;;`, ...) are simply skipped
/// rather than folded into the AST. Shared by every sibling module with such a gap, rather than
/// each keeping its own copy.
fn skip_newlines(input: &mut ParserStream<'_>) {
    loop {
        let result: ModalResult<()> = any
            .verify(|t: &Token<'_>| matches!(t, Token::Newline))
            .void()
            .parse_next(input);

        if result.is_err() {
            break;
        }
    }
}

#[must_use = "parses a compound command or base command; discarding ignores syntax structures"]
fn parse_command<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let stmt = alt((
        control_flow::parse_for_loop,
        control_flow::parse_if,
        control_flow::parse_while,
        control_flow::parse_until,
        case::parse_case,
        grouping::parse_subshell,
        grouping::parse_brace_group,
        command::parse_base_command,
    ))
    .parse_next(input)?;

    let mut trailing = Vec::new();

    while let Ok(r) = redirect::parse_redirect(input) {
        trailing.push(r);
    }

    if trailing.is_empty() {
        return Ok(stmt);
    }

    match stmt {
        Statement::Redirected {
            inner,
            mut redirects,
        } => {
            redirects.extend(trailing);
            Ok(Statement::Redirected { inner, redirects })
        }
        other => Ok(Statement::Redirected {
            inner: Box::new(other),
            redirects: trailing,
        }),
    }
}

/// Parses the statement.
///
/// # Errors
/// Returns an error if the statement cannot be parsed.
#[must_use = "parses a statement; discarding ignores syntax structures"]
pub fn parse_statement<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    command::parse_list(input)
}
