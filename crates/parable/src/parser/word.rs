//! Resolves a lexed [`Token::Word`] into an [`Expr`], splitting out `$NAME`/`${NAME}` references
//! — see [`interpolation_segments`].

use super::ParserStream;

use crate::types::{Expr, Token};

use winnow::ModalResult;
use winnow::Parser as _;
use winnow::token::any;

use std::borrow::Cow;

/// One segment of a word split by [`interpolation_segments`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment<'a> {
    /// Plain text, not subject to expansion.
    Literal(&'a str),
    /// A `$NAME`/`${NAME}` reference.
    VarRef(&'a str),
}

/// What follows a `$` while scanning: either a recognized variable reference, or something
/// that should be folded back into the surrounding literal run — an ordinary `$`, a command
/// substitution or arithmetic expansion (`` $(...) ``/`` `...` ``/`$((...))`), or a `${...}`
/// form with a parameter-expansion operator (`${var:-default}`, `${arr[@]}`, ...) this
/// analysis doesn't decompose. None of these are misparsed as a variable reference, but they
/// also don't get split out as their own segment: there's nothing to resolve in them, so
/// breaking the surrounding literal text in two would only lose information for no benefit.
enum ReferenceOutcome<'a> {
    VarRef(&'a str),
    NotAReference,
}

/// Splits `text` into literal and variable-reference segments, honoring single-quote
/// literalness (Bash doesn't expand `$x` inside `'...'`) and `\$` escaping.
#[must_use = "splitting text has no effect unless the caller uses the resulting segments"]
pub fn interpolation_segments(text: &str) -> Vec<Segment<'_>> {
    let mut segments = Vec::new();
    let mut literal_start = 0_usize;
    let mut in_single = false;
    let mut escaped = false;
    let mut cursor = 0_usize;

    while let Some(c) = text.get(cursor..).and_then(|s| s.chars().next()) {
        let char_len = c.len_utf8();

        if escaped {
            escaped = false;
            cursor = cursor.saturating_add(char_len);
            continue;
        }

        if in_single {
            if c == '\'' {
                in_single = false;
            }
            cursor = cursor.saturating_add(char_len);
            continue;
        }

        match c {
            '\\' => {
                escaped = true;
                cursor = cursor.saturating_add(char_len);
            }
            '\'' => {
                in_single = true;
                cursor = cursor.saturating_add(char_len);
            }
            '$' => {
                let after_dollar = cursor.saturating_add(char_len);
                let (outcome, next_cursor) = scan_reference(text, after_dollar);

                if let ReferenceOutcome::VarRef(name) = outcome {
                    push_literal_segment(&mut segments, text, literal_start, cursor);
                    segments.push(Segment::VarRef(name));
                    literal_start = next_cursor;
                }

                cursor = next_cursor;
            }
            _ => {
                cursor = cursor.saturating_add(char_len);
            }
        }
    }

    push_literal_segment(&mut segments, text, literal_start, text.len());

    segments
}

/// Pushes `text[start..end]` as a literal segment, unless it's empty.
fn push_literal_segment<'a>(
    segments: &mut Vec<Segment<'a>>,
    text: &'a str,
    start: usize,
    end: usize,
) {
    if let Some(literal) = text.get(start..end).filter(|s| !s.is_empty()) {
        segments.push(Segment::Literal(literal));
    }
}

/// Determines what follows a `$` (with `after_dollar` pointing just past it) and returns the
/// outcome plus the cursor position after consuming whatever was scanned.
#[must_use = "scanning a reference has no effect unless the caller uses the result"]
fn scan_reference(text: &str, after_dollar: usize) -> (ReferenceOutcome<'_>, usize) {
    let Some(next) = text.get(after_dollar..).and_then(|s| s.chars().next()) else {
        return (ReferenceOutcome::NotAReference, after_dollar);
    };

    match next {
        '{' => scan_braced(text, after_dollar),
        '(' => scan_command_or_arithmetic(text, after_dollar),
        '0'..='9' | '@' | '*' | '#' | '?' | '$' | '!' | '-' => {
            let end = after_dollar.saturating_add(next.len_utf8());
            let name = text.get(after_dollar..end).unwrap_or_default();
            (ReferenceOutcome::VarRef(name), end)
        }
        c if c == '_' || c.is_alphabetic() => scan_identifier(text, after_dollar),
        _ => (ReferenceOutcome::NotAReference, after_dollar),
    }
}

/// Scans a `[A-Za-z0-9_]+` identifier starting at `start` (whose first character is already
/// known to be a valid identifier-start character).
#[must_use = "scanning an identifier has no effect unless the caller uses the result"]
fn scan_identifier(text: &str, start: usize) -> (ReferenceOutcome<'_>, usize) {
    let mut end = start;

    while let Some(c) = text.get(end..).and_then(|s| s.chars().next()) {
        if c.is_alphanumeric() || c == '_' {
            end = end.saturating_add(c.len_utf8());
        } else {
            break;
        }
    }

    (
        ReferenceOutcome::VarRef(text.get(start..end).unwrap_or_default()),
        end,
    )
}

/// Scans a `${...}` reference starting at the `{` (`after_dollar`). Only plain `${NAME}` forms
/// resolve to a reference; anything with a parameter-expansion operator inside the braces
/// (`${var:-default}`, `${arr[@]}`, ...) isn't decomposed, since taking just a leading
/// identifier out of it would silently change what the expression means rather than leave it
/// as-is.
#[must_use = "scanning a braced reference has no effect unless the caller uses the result"]
fn scan_braced(text: &str, after_dollar: usize) -> (ReferenceOutcome<'_>, usize) {
    let after_brace = after_dollar.saturating_add(1);

    let Some(close_offset) = text.get(after_brace..).and_then(|rest| rest.find('}')) else {
        return (ReferenceOutcome::NotAReference, text.len());
    };

    let name_end = after_brace.saturating_add(close_offset);
    let end = name_end.saturating_add(1);
    let content = text.get(after_brace..name_end).unwrap_or_default();

    if is_plain_parameter_name(content) {
        (ReferenceOutcome::VarRef(content), end)
    } else {
        (ReferenceOutcome::NotAReference, end)
    }
}

/// Whether `s` is *only* a valid Bash parameter name — a plain identifier, a positional
/// parameter's digits, or one of the single-character special parameters — with nothing else
/// (no parameter-expansion operator) alongside it.
#[must_use = "checking the parameter name has no effect unless the caller uses the result"]
fn is_plain_parameter_name(s: &str) -> bool {
    let mut chars = s.chars();

    match chars.next() {
        Some(first) if first.is_alphabetic() || first == '_' => {
            chars.all(|rest| rest.is_alphanumeric() || rest == '_')
        }
        Some(first) if first.is_ascii_digit() => chars.all(|rest| rest.is_ascii_digit()),
        Some('@' | '*' | '#' | '?' | '$' | '!' | '-') => chars.next().is_none(),
        _ => false,
    }
}

/// Scans past a `$(...)` command substitution or `$((...))` arithmetic expansion (both are
/// paren-balanced, so the same depth count handles either) starting at the `(` (`after_dollar`).
/// The contents are not parsed.
#[must_use = "scanning a substitution has no effect unless the caller uses the result"]
fn scan_command_or_arithmetic(text: &str, after_dollar: usize) -> (ReferenceOutcome<'_>, usize) {
    let mut depth = 0_usize;
    let mut cursor = after_dollar;

    while let Some(c) = text.get(cursor..).and_then(|s| s.chars().next()) {
        cursor = cursor.saturating_add(c.len_utf8());

        match c {
            '(' => depth = depth.saturating_add(1),
            ')' => {
                depth = depth.saturating_sub(1);

                if depth == 0 {
                    return (ReferenceOutcome::NotAReference, cursor);
                }
            }
            _ => {}
        }
    }

    (ReferenceOutcome::NotAReference, text.len())
}

/// Converts a lexed word into an [`Expr`], splitting out `$NAME`/`${NAME}` references (see
/// [`interpolation_segments`]) into [`Expr::VarRef`]s when the word isn't entirely literal.
#[must_use = "interpolates the word; discarding it drops the parsed structure"]
fn interpolate(word: Cow<'_, str>) -> Expr<'_> {
    let Cow::Borrowed(text) = word else {
        // Never produced by the current lexer (which only ever borrows), but a `Cow::Owned`
        // word can't be split into `'a`-lived segments, so fall back to one opaque literal.
        return Expr::Literal(word);
    };

    let mut parts = interpolation_segments(text)
        .into_iter()
        .map(|segment| match segment {
            Segment::Literal(s) => Expr::Literal(Cow::Borrowed(s)),
            Segment::VarRef(name) => Expr::VarRef(name),
        });

    match (parts.next(), parts.next()) {
        (None, _) => Expr::Literal(Cow::Borrowed("")),
        (Some(only), None) => only,
        (Some(first), Some(second)) => {
            let mut all = vec![first, second];
            all.extend(parts);
            Expr::Interpolated(all)
        }
    }
}

#[must_use = "parses a literal; discarding ignores syntax structures"]
pub(super) fn parse_literal<'a>(input: &mut ParserStream<'a>) -> ModalResult<Expr<'a>> {
    any.verify_map(|t| match t {
        Token::Word(word) => Some(interpolate(word)),
        _ => None,
    })
    .parse_next(input)
}
