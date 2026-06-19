//! # Parser Architecture
//!
//! Builds a [`Statement`] AST from the [`Token`] stream [`crate::lexer`] produces, using winnow
//! combinators driven directly off [`ParserStream`] (a [`TokenStream`] alias) rather than a
//! hand-rolled recursive-descent loop.
//!
//! ## Core Design
//!
//! Bash's grammar is split across four sibling modules: `word` turns a lexed `Word` token into
//! an `Expr` (resolving `$NAME`/`${NAME}` references); `command` covers simple commands,
//! pipelines, and command lists; `control_flow` covers `for`/`if`/`while`/`until`; `grouping`
//! covers subshells and brace groups. `parse_command` is the dispatch point that ties them
//! together — adding a new compound command (e.g. `case`, a function definition) means adding
//! its own sibling module and one more arm to this `alt`.
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
//!
//! ## Edge Cases
//!
//! - **`until`'s body doesn't skip separators between statements** (`control_flow`): every other
//!   compound command's body loop calls `skip_optional_separator` after each statement;
//!   `parse_until` doesn't, and parses its condition/body with [`parse_statement`] rather than
//!   `parse_command` like its siblings. This predates this module's file split and is left as-is
//!   — fixing it is a parser-behavior change, not a structural one.
//! - **`elif` is not yet supported**: `control_flow`'s `if` parser only recognizes a single
//!   `else`; chained `elif` parses as a syntax error today.

mod command;
mod control_flow;
mod grouping;
mod redirect;
mod word;

pub(crate) use word::{Segment, interpolation_segments};

use crate::stream::TokenStream;
use crate::types::{Statement, Token};

use winnow::ModalResult;
use winnow::Parser as _;
use winnow::combinator::alt;
use winnow::stream::Stream as _;
use winnow::token::any;

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

/// Consumes a `;` or newline between two commands in a compound-statement body, if present.
///
/// Bash requires this separator before a closing keyword (`done`/`fi`/...), but a body's last
/// statement may also be followed directly by EOF or, in this lexer's case, nothing at all —
/// so the separator is optional here rather than required.
fn skip_optional_separator(input: &mut ParserStream<'_>) {
    let result: ModalResult<Token<'_>> = any
        .verify(|t: &Token<'_>| matches!(t, Token::Semi) || matches!(t, Token::Newline))
        .parse_next(input);

    drop(result);
}

#[must_use = "parses a compound command or base command; discarding ignores syntax structures"]
fn parse_command<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let stmt = alt((
        control_flow::parse_for_loop,
        control_flow::parse_if,
        control_flow::parse_while,
        control_flow::parse_until,
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
