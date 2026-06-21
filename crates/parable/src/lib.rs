//! # Parable Architecture
//!
//! A zero-copy, context-aware Bash parser: [`parse_program`] turns a `&str` script into a tree
//! of [`Statement`]s without ever copying the source text, beyond the handful of expansion
//! forms (`$(...)`, `${...}`, ...) the lexer can't slice in place.
//!
//! ## Core Design
//!
//! The pipeline is three layers, each its own module: [`lexer`] turns the raw `&str` into
//! [`Token`]s on demand; [`stream`] adapts that into a [`TokenStream`] winnow can drive
//! token-by-token, caching one token of lookahead so the parser can peek a keyword without
//! re-lexing it; [`parser`] builds the [`Statement`] tree, deciding *grammar* questions (is this
//! `Word` a keyword here, or a plain argument?) that the lexer deliberately leaves unresolved —
//! see each module's own doc for why the split sits where it does.
//!
//! [`taint`] and [`rules`] sit downstream of parsing: [`taint::Environment`] approximates which
//! variables a parsed script's caller could influence, and [`rules::Engine`] runs [`rules::Rule`]
//! implementations over the AST using that approximation to flag dangerous patterns (e.g.
//! tainted input reaching `eval`).
//!
//! ## Flow
//!
//! 1. [`parse_program`] builds a [`TokenStream`] over the input and repeatedly calls
//!    [`parser::parse_statement`] until the stream is exhausted.
//! 2. [`render_program_ast`] is a thin wrapper used by the corpus test suite: it renders the
//!    resulting AST in the `{:?}`-debug form corpus fixtures compare against.
//!
//! ## Edge Cases
//!
//! - A lex failure surfaces as ordinary stream exhaustion to the parser (winnow's `Stream` trait
//!   has no error slot); [`parse_program`] recovers the real error via
//!   [`TokenStream::take_lex_error`] afterward rather than returning a generic syntax error.
#![cfg_attr(
    test,
    expect(
        clippy::panic_in_result_fn,
        clippy::unnecessary_wraps,
        reason = "rstest cases return Result<(), TestError> per project convention even when a \
                  given case has no fallible setup, and use assert_eq!/assert! for assertions"
    )
)]

pub mod error;
pub mod lexer;
pub mod parser;
pub mod rules;
pub mod stream;
pub mod taint;
pub mod types;

pub use error::{ParseError, ParseErrorDisplay};
pub use stream::TokenStream;
pub use types::{
    Expr, LexerState, LogicalOp, Redirect, RedirectKind, RedirectTarget, Statement, Token,
};

use parser::parse_statement;

use winnow::ModalResult;
use winnow::stream::Stream as _;

/// Parses a full token `stream` into its top-level statements.
///
/// # Errors
///
/// Returns an error if parsing fails before the token stream is exhausted.
#[must_use = "parses the token stream; discarding ignores syntax structures or errors"]
fn parse_statements<'a>(stream: &mut TokenStream<'a>) -> ModalResult<Vec<Statement<'a>>> {
    let mut statements = Vec::new();

    while stream.peek_token().is_some() {
        statements.push(parse_statement(stream)?);
    }

    Ok(statements)
}

/// Renders `statements` in the `{:?}`-debug form used to compare against corpus-test
/// expectations, one statement per line.
#[must_use = "builds the rendered AST; discarding it loses the rendered output"]
fn render_statements(statements: &[Statement<'_>]) -> String {
    statements
        .iter()
        .map(|s| format!("{s:?}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Lexes and parses `input` into its top-level statements.
///
/// Tokens are pulled lazily from a [`TokenStream`] as the parser consumes them, instead of
/// being pre-collected into a buffer ahead of parsing.
///
/// # Errors
///
/// Returns an error if lexing or parsing fails before end of input is reached.
#[must_use = "parses the program; discarding ignores syntax structures or errors"]
pub fn parse_program(input: &str) -> ModalResult<Vec<Statement<'_>>> {
    let mut stream = TokenStream::new(input);
    let parsed = parse_statements(&mut stream);

    if let Some(lex_error) = stream.take_lex_error() {
        return Err(lex_error);
    }

    parsed
}

/// Lexes and parses `input` into its top-level statements, rendering the resulting AST
/// in the `{:?}`-debug form used to compare against corpus-test expectations.
///
/// # Errors
///
/// Returns an error if lexing or parsing fails before end of input is reached.
#[must_use = "parses the program; discarding ignores syntax structures or errors"]
pub fn render_program_ast(input: &str) -> ModalResult<String> {
    Ok(render_statements(&parse_program(input)?))
}
