//! # Parser Architecture
//!
//! Builds a [`Statement`](crate::types::Statement) AST from the [`Token`](crate::types::Token) stream [`crate::lexer`] produces, using winnow
//! combinators driven directly off [`ParserStream`] (a [`TokenStream`](crate::stream::TokenStream) alias) rather than a
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
//! Bash reserved words (`done`, `fi`, `then`, ...) lex as plain [`Token::Word`](crate::types::Token::Word)s rather than
//! their own token kind (see [`crate::lexer`]'s module doc) — `at_keyword` is how every
//! compound-command parser recognizes one at the position the grammar expects it, without the
//! lexer needing to know grammar position itself.
//!
//! `redirect` is a cross-cutting sibling rather than another `alt` arm in `parse_command`:
//! every compound command (and `command::parse_base_command`'s own simple command) can have
//! redirects trailing it, so `parse_command` consumes them itself after the `alt` succeeds,
//! wrapping the result in [`Statement::Redirected`](crate::types::Statement::Redirected) rather than each compound
//! parser doing so independently. `attach_redirects` is the single place that decides whether
//! to extend an already-`Redirected` statement or wrap a fresh one, shared by `parse_command`
//! and `command::parse_base_command`'s own redirect handling, so the two can't silently diverge
//! on what e.g. `cmd <a >b` and `if ...; fi <a >b` each produce. `spanned` is the shared
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
mod expected;
mod grouping;
mod helpers;
mod redirect;
mod statement;
mod word;

pub(crate) use expected::Expected;
pub use helpers::ParserStream;
pub use statement::parse_statement;
pub(crate) use word::{Segment, interpolation_segments};

use expected::expect_command;
use helpers::{at_keyword, attach_redirects, consume_keyword, skip_newlines, spanned};
use statement::parse_command;

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

        use crate::{stream::TokenStream, types::Statement};

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
