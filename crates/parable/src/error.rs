//! The crate's rich error type and parse-error display helper — see [`ParseError`],
//! [`ParseErrorDisplay`].

use crate::types::Token;

use winnow::error::{ContextError, ErrMode, StrContext};

use std::fmt;

/// Wraps a [`crate::ModalResult`] error for human-readable display.
///
/// `{:?}` on the raw error exposes only the `Backtrack`/`Cut` plumbing winnow uses to decide
/// whether to try another grammar alternative — it says nothing about *why* parsing failed.
/// This instead renders every [`StrContext::Label`] each compound-command parser attaches via
/// `.context(...)` once it commits past its own leading keyword, as a breadcrumb from the
/// outermost construct down to the innermost one still active when the failure happened (e.g.
/// `"if statement: while loop"` for a malformed `while` nested in an `if`'s `then`-body). Falls
/// back to `"invalid syntax"` when nothing committed to any construct at all (e.g. a stray `)`
/// where no statement can start).
#[derive(Debug, Clone, Copy)]
pub struct ParseErrorDisplay<'a>(pub &'a ErrMode<ContextError>);

impl fmt::Display for ParseErrorDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = match self.0 {
            ErrMode::Cut(inner) | ErrMode::Backtrack(inner) => inner,
            ErrMode::Incomplete(_) => return write!(f, "incomplete input"),
        };

        // `.context(...)` pushes onto the end of the list as the error bubbles outward, so the
        // construct closest to the actual failure (pushed first, while still deep in the call
        // stack) sits at index 0 and the outermost one sits last; reversing prints outermost
        // first, which reads as a breadcrumb toward the precise cause rather than away from it.
        let mut labels: Vec<&str> = inner
            .context()
            .filter_map(|context| match context {
                StrContext::Label(label) => Some(*label),
                _ => None,
            })
            .collect();

        labels.reverse();

        if labels.is_empty() {
            return write!(f, "invalid syntax");
        }

        write!(f, "{}", labels.join(": "))
    }
}

/// A rich, structured parse failure.
///
/// Not yet constructed anywhere: [`crate::parse_program`] currently surfaces winnow's own
/// `ModalResult`/`ErrMode<ContextError>` directly. This type is the eventual replacement, once
/// the crate's call sites translate those into one of the variants below instead of forwarding
/// winnow's generic backtrack error.
#[derive(thiserror::Error, Debug)]
pub enum ParseError<'a> {
    /// The lexer couldn't tokenize the input at `offset`.
    #[error("Lexical error at byte offset {offset}")]
    Lexer {
        /// The byte offset into the source at which lexing failed.
        offset: usize,
    },
    /// The parser expected one grammar construct but found another.
    #[error("Syntax error: expected {expected}, found {found:?}")]
    Syntax {
        /// A human-readable description of what the grammar expected at this position.
        expected: &'static str,
        /// The token actually found, or `None` if input was exhausted first.
        found: Option<Token<'a>>,
    },
}
