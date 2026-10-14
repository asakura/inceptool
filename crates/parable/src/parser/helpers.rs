use crate::{
    stream::TokenStream,
    types::{Redirect, Spanned, Statement, Token},
};

use winnow::{
    ModalResult, Parser as _,
    error::{StrContext, StrContextValue},
    stream::Stream as _,
    token::any,
};

/// The token-level stream every parser function in this module tree consumes.
pub type ParserStream<'a> = TokenStream<'a>;

/// Checks whether the next token is the given reserved word without consuming it.
///
/// Bash reserved words (`done`, `fi`, `then`, ...) lex as plain `Word`s, so a
/// command-collection loop must peek for them before attempting to parse another
/// command — otherwise a base-command parse would happily consume the closing
/// keyword as a command name.
#[must_use = "peeking has no effect unless the caller acts on the result"]
pub(super) fn at_keyword(input: &ParserStream<'_>, keyword: &str) -> bool {
    matches!(input.peek_token(), Some(Token::Word(w)) if w.as_ref() == keyword)
}

/// Consumes the next token if it's the reserved word `keyword`, failing otherwise. Shared by
/// every sibling module that recognizes a leading keyword (`for`/`if`/`while`/`until`/`case`),
/// rather than each keeping its own byte-for-byte copy.
pub(super) fn consume_keyword(
    input: &mut ParserStream<'_>,
    keyword: &'static str,
) -> ModalResult<()> {
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
pub(super) fn skip_newlines(input: &mut ParserStream<'_>) {
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
pub(super) fn spanned<T>(start: usize, input: &ParserStream<'_>, inner: T) -> Spanned<T> {
    Spanned {
        inner,
        span: start..input.previous_span_end(),
    }
}

/// Attaches `trailing` redirects to `stmt`, run from `start`: extends `stmt`'s own redirect list
/// if it's already a [`Statement::Redirected`], otherwise wraps it in a fresh one. Returns `stmt`
/// unchanged (span included) when `trailing` is empty.
///
/// Shared by [`parse_command`](super::statement::parse_command) (redirects trailing a compound command, or any a base command
/// didn't already consume) and `command::parse_base_command` (a simple command's own
/// interleaved/trailing redirects) so this merge-or-wrap rule lives in exactly one place —
/// independent copies could otherwise silently diverge on what `cmd <a >b` and
/// `if ...; fi <a >b` each produce.
#[must_use = "attaches the redirects; discarding the result drops them"]
pub(super) fn attach_redirects<'a>(
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
