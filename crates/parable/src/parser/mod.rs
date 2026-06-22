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
//! `redirect` is a cross-cutting sibling rather than another `alt` arm in `parse_command`:
//! every compound command (and `command::parse_base_command`'s own simple command) can have
//! redirects trailing it, so `parse_command` consumes them itself after the `alt` succeeds,
//! wrapping the result in [`Statement::Redirected`] rather than each compound
//! parser doing so independently. [`attach_redirects`] is the single place that decides whether
//! to extend an already-`Redirected` statement or wrap a fresh one, shared by [`parse_command`]
//! and `command::parse_base_command`'s own redirect handling, so the two can't silently diverge
//! on what e.g. `cmd <a >b` and `if ...; fi <a >b` each produce. [`spanned`] is the shared
//! `start..input.previous_span_end()` constructor every sibling module's `Spanned` nodes go
//! through, instead of each repeating that arithmetic by hand.
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
    types::{Redirect, Spanned, Statement, Token},
};

use winnow::{
    ModalResult, Parser as _,
    combinator::alt,
    error::{AddContext, ContextError, ErrMode, StrContext, StrContextValue},
    stream::Stream as _,
    token::any,
};

/// The token-level stream every parser function in this module tree consumes.
pub type ParserStream<'a> = TokenStream<'a>;

/// What [`expect_command`]/[`command::parse_list_until`] failed to find — a closed,
/// compiler-checked choice every call site makes explicitly, read back by
/// [`crate::error::ParseError::from_winnow`] to decide how to phrase the failure. Replaces
/// message shape being guessed downstream from a bare `desc` string's shape (an `== "command"`
/// equality check or an `ends_with("body"/"condition")` suffix check).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Expected {
    /// At least one command was expected; combined downstream with the enclosing label as
    /// "`<label>` should contain at least one command", or "expected a command" with no label.
    Command,
    /// A bare construct name (e.g. `"if body"`); rendered downstream as `"missing {name}"`,
    /// ignoring any enclosing label. The "missing " phrasing lives once in
    /// [`crate::error::ExpectedMessage`], not duplicated at every call site.
    Standalone(&'static str),
}

impl Expected {
    /// The text attached to winnow's `StrContext::Expected(StrContextValue::Description(_))`.
    #[must_use = "looks up the desc text; discarding it has no effect"]
    const fn desc(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Standalone(desc) => desc,
        }
    }
}

/// Wraps a parser so a `Backtrack` failure also names `expected` as the expected construct.
///
/// Only `ErrMode::Backtrack` is annotated — `Cut` and `Incomplete` are propagated untouched,
/// since a `Cut` has already committed to a specific grammar branch and should keep the more
/// precise context the inner parser attached, not be overwritten by this wrapper's description.
pub(super) fn expect_command<'a, O, P>(
    expected: Expected,
    mut parser: P,
) -> impl winnow::Parser<ParserStream<'a>, O, ErrMode<ContextError>>
where
    P: winnow::Parser<ParserStream<'a>, O, ErrMode<ContextError>>,
{
    move |input: &mut ParserStream<'a>| {
        let start = input.checkpoint();
        match parser.parse_next(input) {
            Ok(o) => Ok(o),
            Err(ErrMode::Backtrack(mut e)) => {
                e = AddContext::add_context(
                    e,
                    input,
                    &start,
                    StrContext::Expected(StrContextValue::Description(expected.desc())),
                );
                Err(ErrMode::Backtrack(e))
            }
            Err(e) => Err(e),
        }
    }
}

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
        .context(StrContext::Expected(StrContextValue::StringLiteral(
            keyword,
        )))
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

/// Builds a [`Spanned`] from `inner` and the span `start..input.previous_span_end()`. Shared by
/// every parser function in this module tree that wraps a freshly-built node in its own span,
/// rather than each repeating the `start_offset..input.previous_span_end()` arithmetic.
#[must_use = "constructs the Spanned wrapper; discarding it loses the parsed node"]
fn spanned<T>(start: usize, input: &ParserStream<'_>, inner: T) -> Spanned<T> {
    Spanned {
        inner,
        span: start..input.previous_span_end(),
    }
}

/// Attaches `trailing` redirects to `stmt`, run from `start`: extends `stmt`'s own redirect list
/// if it's already a [`Statement::Redirected`], otherwise wraps it in a fresh one. Returns `stmt`
/// unchanged (span included) when `trailing` is empty.
///
/// Shared by [`parse_command`] (redirects trailing a compound command, or any a base command
/// didn't already consume) and `command::parse_base_command` (a simple command's own
/// interleaved/trailing redirects) so this merge-or-wrap rule lives in exactly one place —
/// independent copies could otherwise silently diverge on what `cmd <a >b` and
/// `if ...; fi <a >b` each produce.
#[must_use = "attaches the redirects; discarding the result drops them"]
fn attach_redirects<'a>(
    stmt: Spanned<Statement<'a>>,
    trailing: Vec<Redirect<'a>>,
    start: usize,
    input: &ParserStream<'a>,
) -> Spanned<Statement<'a>> {
    if trailing.is_empty() {
        return stmt;
    }

    let inner_span = stmt.span;
    let inner = match stmt.inner {
        Statement::Redirected {
            inner,
            mut redirects,
        } => {
            redirects.extend(trailing);
            Statement::Redirected { inner, redirects }
        }
        other => Statement::Redirected {
            inner: Box::new(Spanned {
                inner: other,
                span: inner_span,
            }),
            redirects: trailing,
        },
    };

    spanned(start, input, inner)
}

#[must_use = "parses a compound command or base command; discarding ignores syntax structures"]
fn parse_command<'a>(input: &mut ParserStream<'a>) -> ModalResult<Spanned<Statement<'a>>> {
    let start_offset = input.current_span_start();
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

    while let Some(r) = redirect::parse_redirect_opt(input)? {
        trailing.push(r);
    }

    Ok(attach_redirects(stmt, trailing, start_offset, input))
}

/// Parses the statement.
///
/// # Errors
/// Returns an error if the statement cannot be parsed.
#[must_use = "parses a statement; discarding ignores syntax structures"]
pub fn parse_statement<'a>(input: &mut ParserStream<'a>) -> ModalResult<Spanned<Statement<'a>>> {
    command::parse_list(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    enum TestError {
        #[error("Test failure: {0}")]
        Failure(String),
    }

    mod parse_statement {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn trailing_redirect_after_a_compound_command_excludes_it_from_the_inner_span()
        -> Result<(), TestError> {
            let mut stream = TokenStream::new("if true; then echo a; fi >file");
            let parsed =
                parse_statement(&mut stream).map_err(|e| TestError::Failure(e.to_string()))?;

            assert_eq!(parsed.span, 0..30);

            let Statement::Redirected { inner: bare, .. } = parsed.inner else {
                return Err(TestError::Failure("expected a Redirected statement".into()));
            };

            assert_eq!(bare.span, 0..24);

            Ok(())
        }
    }
}
