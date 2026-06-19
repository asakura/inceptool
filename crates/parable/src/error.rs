//! The crate's rich error type — see [`ParseError`].

use crate::types::Token;

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
