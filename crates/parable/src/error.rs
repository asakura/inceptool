//! The crate's rich error type — see [`ParseError`].

use crate::types::Token;

use winnow::error::{ContextError, ErrMode, StrContext, StrContextValue};

use std::borrow::Cow;
use std::fmt;

/// A rich, structured parse failure.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ParseError<'a> {
    /// The lexer couldn't tokenize the input at `offset`.
    #[error(
        "Lexical error at line {line}, column {column}{}",
        Self::format_lexer_expected(expected.as_deref())
    )]
    Lexer {
        /// The line number where lexing failed (1-indexed).
        line: usize,
        /// The column number where lexing failed (1-indexed).
        column: usize,
        /// Context winnow attached to the lex failure, if any. No lexer code path attaches one
        /// today (every lex failure is a context-free `Backtrack`), but a future one might, and
        /// this carries it through rather than [`crate::stream::TokenStream::take_lex_error`]'s
        /// recovered error being inspected and discarded.
        expected: Option<Cow<'a, str>>,
    },
    /// The parser expected one grammar construct but found another.
    #[error(
        "Parse error at line {line}, column {column}: {expected}, but found {}",
        FoundToken(found.as_ref())
    )]
    Syntax {
        /// A human-readable description of what the grammar expected at this position.
        expected: Cow<'a, str>,
        /// The line number where parsing failed (1-indexed).
        line: usize,
        /// The column number where parsing failed (1-indexed).
        column: usize,
        /// The token actually found, or `None` if input was exhausted first.
        found: Option<Token<'a>>,
    },
}

/// Renders the token found at a parse failure, or `"end of file"` if none — one self-contained
/// [`fmt::Display`] surface for that combination, rather than [`Token`]'s own `Display` (which
/// renders just the token's own text) being backtick-wrapped through a separate helper.
struct FoundToken<'a>(Option<&'a Token<'a>>);

impl fmt::Display for FoundToken<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            // Not backtick-wrapped, like `None`'s "end of file": `Token`'s own `Display` renders
            // this as a literal "\n" (correct for reconstructing source text), but splicing a raw
            // newline into the middle of a one-line "Parse error at ..." message would break it
            // across two lines instead of naming the token.
            Some(Token::Newline) => write!(f, "newline"),
            Some(t) => write!(f, "`{t}`"),
            None => write!(f, "end of file"),
        }
    }
}

/// The human-readable phrase for a parse failure's "expected" component, classified once from
/// winnow's `Expected`/`Label` context in [`ParseError::from_winnow`] — replacing message shape
/// being guessed downstream from `desc`'s text (an `== "command"` equality check or an
/// `ends_with("body"/"condition")` suffix check) with an exhaustive match on this enum.
enum ExpectedMessage {
    /// A [`crate::parser::Expected::Standalone`] construct name (e.g. `"if body"`) — rendered as
    /// `"missing {name}"`, the one place that phrasing convention is applied, rather than every
    /// call site spelling out "missing " itself.
    Standalone(&'static str),
    /// At least one command was expected inside the named enclosing construct.
    CommandIn(&'static str),
    /// At least one command was expected with no enclosing construct known.
    Command,
    /// A specific grammar item (a literal keyword or punctuation mark) was expected, optionally
    /// inside a named enclosing construct.
    Item {
        value: StrContextValue,
        label: Option<&'static str>,
    },
    /// Only the enclosing construct's name is known.
    Malformed(&'static str),
}

impl fmt::Display for ExpectedMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Standalone(s) => write!(f, "missing {s}"),
            Self::CommandIn(l) => write!(f, "{l} should contain at least one command"),
            Self::Command => write!(f, "expected a command"),
            Self::Item {
                value,
                label: Some(l),
            } => write!(f, "expected {value} for the {l}"),
            Self::Item { value, label: None } => write!(f, "expected {value}"),
            Self::Malformed(l) => write!(f, "malformed {l}"),
        }
    }
}

/// Reads back the human-readable phrase winnow's context machinery attached to a `Cut`/
/// `Backtrack` failure, if any. `None` for a context-free failure (every lexer failure today)
/// or an `Incomplete` one (which carries no `ContextError` at all) — callers supply their own
/// fallback text for those cases rather than this picking one on their behalf.
fn describe_expected(err: &ErrMode<ContextError>) -> Option<Cow<'static, str>> {
    let inner = match err {
        ErrMode::Cut(inner) | ErrMode::Backtrack(inner) => inner,
        ErrMode::Incomplete(_) => return None,
    };

    let mut expected_value = None;
    let mut label_desc = None;

    for context in inner.context() {
        match context {
            StrContext::Expected(value) if expected_value.is_none() => {
                expected_value = Some(value.clone());
            }
            StrContext::Label(l) if label_desc.is_none() => {
                label_desc = Some(*l);
            }
            _ => {}
        }
    }

    let message = match (expected_value, label_desc) {
        (Some(StrContextValue::Description("command")), Some(l)) => ExpectedMessage::CommandIn(l),
        (Some(StrContextValue::Description("command")), None) => ExpectedMessage::Command,
        (Some(StrContextValue::Description(s)), _) => ExpectedMessage::Standalone(s),
        (Some(value), label) => ExpectedMessage::Item { value, label },
        (None, Some(l)) => ExpectedMessage::Malformed(l),
        (None, None) => return None,
    };

    Some(match message {
        ExpectedMessage::Command => Cow::Borrowed("expected a command"),
        other => Cow::Owned(other.to_string()),
    })
}

impl<'a> ParseError<'a> {
    fn format_lexer_expected(expected: Option<&str>) -> Cow<'static, str> {
        expected.map_or(Cow::Borrowed(""), |e| Cow::Owned(format!(": {e}")))
    }
    #[expect(
        clippy::string_slice,
        reason = "offset is from winnow, guaranteed char boundary"
    )]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "line and col counts are strictly bounded by input length"
    )]
    pub(crate) fn get_line_column(input: &str, offset: usize) -> (usize, usize) {
        let prefix = &input[..offset.min(input.len())];
        let line = prefix.matches('\n').count() + 1;
        let column = prefix.chars().rev().take_while(|&c| c != '\n').count() + 1;
        (line, column)
    }

    pub(crate) fn from_winnow(
        input: &str,
        err: &ErrMode<ContextError>,
        offset: usize,
        found: Option<Token<'a>>,
    ) -> Self {
        let (line, column) = Self::get_line_column(input, offset);
        let expected = if matches!(err, ErrMode::Incomplete(_)) {
            Cow::Borrowed("incomplete input")
        } else {
            describe_expected(err).unwrap_or(Cow::Borrowed("syntax error"))
        };

        ParseError::Syntax {
            expected,
            line,
            column,
            found,
        }
    }

    /// Builds a [`ParseError::Lexer`] from `lex_error`, the recovered failure
    /// [`crate::stream::TokenStream::take_lex_error`] reports — folding in whatever context
    /// winnow attached to it, if any, rather than discarding the recovered value unread.
    pub(crate) fn from_lex_error(
        input: &str,
        lex_error: &ErrMode<ContextError>,
        offset: usize,
    ) -> Self {
        let (line, column) = Self::get_line_column(input, offset);
        ParseError::Lexer {
            line,
            column,
            expected: describe_expected(lex_error),
        }
    }
}
