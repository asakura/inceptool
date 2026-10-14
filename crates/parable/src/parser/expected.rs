use super::ParserStream;

use winnow::{
    error::{AddContext, ContextError, ErrMode, StrContext, StrContextValue},
    stream::Stream as _,
};

/// What [`expect_command`]/[`parse_list_until`](super::command::parse_list_until) failed to find — a closed,
/// compiler-checked choice every call site makes explicitly, read back by
/// [`crate::error::ParseError::from_winnow`] to decide how to phrase the failure. Replaces
/// message shape being guessed downstream from a bare `desc` string's shape (an `== "command"`
/// equality check or an `ends_with("body"/"condition")` suffix check).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected {
    /// At least one command was expected; combined downstream with the enclosing label as
    /// "`<label>` should contain at least one command", or "expected a command" with no label.
    Command,
    /// A bare construct name (e.g. `"if body"`); rendered downstream as `"missing {name}"`,
    /// ignoring any enclosing label. The "missing " phrasing lives once in
    /// `crate::error::ExpectedMessage`, not duplicated at every call site.
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
