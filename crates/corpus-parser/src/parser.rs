//! # Corpus Parser Architecture
//!
//! Parsing logic for corpus `.tests` files.
//!
//! ## Core Design
//!
//! A state-machine parser that walks file content as a `&str` slice, dispatching
//! on line prefixes to identify headers, comments, and test case blocks.
//!
//! ## Flow
//!
//! 1. **Normalization**: CRLF line endings are normalized to LF. When no CRLF is
//!    present, the input is borrowed zero-copy.
//! 2. **Header parse**: The `#! Suite N: Name` first line is consumed.
//! 3. **Group dispatch loop**: Remaining content is consumed line-by-line,
//!    dispatching to group-header, comment, or test-case sub-parsers.
//! 4. **Validation**: The completed group list is checked for emptiness.
//!
//! ## Edge Cases
//!
//! - Files with CRLF line endings are normalized but produce owned (`Cow::Owned`)
//!   fields rather than zero-copy borrows.
//! - Lone `\r` characters (classic Mac line endings) are treated as whitespace at
//!   the top-level dispatch but are NOT normalized within test case content.
//! - A case's `input`/`expected` text can contain a literal `---` or `===` line by
//!   escaping it as `\---` or `\===` (see `unescape_delimiter_lines`); like CRLF
//!   normalization, this falls back to an owned copy only when an escape is
//!   actually present.
//! - A case may use `--- <error>` instead of the bare `---` separator
//!   (`ERROR_INPUT_SEPARATOR`) to mark itself negative: `input` must fail to
//!   parse, and the `expected` section holds the exact error message it must
//!   produce instead of an AST (see [`crate::types::CaseExpectation`]).

use crate::error::{CorpusParseError, CorpusParseErrorKind};
use crate::types::{CaseExpectation, CorpusCase, TestGroup, TestSuite};

use std::borrow::Cow;

/// Magic prefix on the first line of every `.tests` file.
const SUITE_HEADER_PREFIX: &str = "#! Suite ";

/// Prefix marking a group header line.
const GROUP_HEADER_PREFIX: &str = "# === ";

/// Suffix that must close a group header line.
const GROUP_HEADER_SUFFIX: &str = " ===";

/// Prefix marking a test case header.
const CASE_HEADER_PREFIX: &str = "=== ";

/// Separator between the input section and the expected section.
const INPUT_SEPARATOR: &str = "\n---\n";

/// Separator marking the case as negative: `input` must fail to parse, and the section
/// that follows is the exact error message it must produce, rather than an expected AST.
const ERROR_INPUT_SEPARATOR: &str = "\n--- <error>\n";

/// Terminator after the expected section (with preceding newline).
const EXPECTED_TERMINATOR: &str = "\n---\n\n\n";

/// Terminator when the expected section is empty (no preceding newline).
const EMPTY_EXPECTED_PREFIX: &str = "---\n\n\n";

/// Prefix for regular comment lines.
const COMMENT_PREFIX: &str = "# ";

/// An empty comment followed by a newline.
const COMMENT_NEWLINE: &str = "#\n";

/// An empty comment at end-of-file (no trailing newline).
const COMMENT_EOF: &str = "#";

/// Escaped form of a literal `---` line inside a case's `input`/`expected` text.
const ESCAPED_SEPARATOR_LINE: &str = "\\---";

/// Escaped form of a literal `===` line inside a case's `input`/`expected` text.
const ESCAPED_GROUP_HEADER_LINE: &str = "\\===";

/// Intermediate result from parsing the suite header line.
#[derive(Debug, Clone)]
struct SuiteHeader<'a> {
    number: u32,
    name: Cow<'a, str>,
}

impl<'a> TestSuite<'a> {
    /// Parses a `.tests` file into a [`TestSuite`].
    ///
    /// `file_stem` is the filename without the `.tests` extension, used in
    /// error messages.
    ///
    /// When `raw_content` contains no `\r\n` sequences, all string fields borrow
    /// directly from the input (zero-copy). Otherwise, fields are owned copies
    /// with normalized line endings.
    ///
    /// # Errors
    ///
    /// Returns [`CorpusParseError`] when the file content is malformed: missing or
    /// invalid suite header, empty groups, orphan test cases, or malformed comments.
    #[must_use = "returns the parsed suite or an error"]
    pub fn parse(file_stem: &str, raw_content: &'a str) -> Result<Self, CorpusParseError> {
        if raw_content.contains("\r\n") {
            let normalized = raw_content.replace("\r\n", "\n");
            let suite = parse_content(file_stem, &normalized)?;

            Ok(suite.into_owned())
        } else {
            parse_content(file_stem, raw_content)
        }
    }
}

/// Parses normalized content (no CRLF) into a [`TestSuite`] borrowing from `content`.
fn parse_content<'a>(file_stem: &str, content: &'a str) -> Result<TestSuite<'a>, CorpusParseError> {
    let (header, rest) = parse_header(file_stem, content)?;
    let groups = parse_groups(file_stem, rest)?;

    validate_groups(file_stem, &groups)?;

    Ok(TestSuite {
        number: header.number,
        name: header.name,
        groups,
    })
}

/// Parses the `#! Suite N: Name` header on the first line.
///
/// Returns the parsed header and the remaining content after the first line.
fn parse_header<'a>(
    file_stem: &str,
    content: &'a str,
) -> Result<(SuiteHeader<'a>, &'a str), CorpusParseError> {
    let (first_line, rest) = content
        .split_once('\n')
        .ok_or_else(|| CorpusParseError::new(file_stem, CorpusParseErrorKind::EmptyFile))?;

    let after_magic = first_line
        .strip_prefix(SUITE_HEADER_PREFIX)
        .ok_or_else(|| {
            CorpusParseError::new(file_stem, CorpusParseErrorKind::MissingSuiteHeader)
        })?;

    let (number_str, suite_name) = after_magic.split_once(':').ok_or_else(|| {
        CorpusParseError::new(file_stem, CorpusParseErrorKind::MalformedSuiteHeader)
    })?;

    let number_str = number_str.trim();
    let suite_name = suite_name.trim();

    if number_str.is_empty() {
        return Err(CorpusParseError::new(
            file_stem,
            CorpusParseErrorKind::EmptySuiteNumber,
        ));
    }

    let suite_number = number_str.parse::<u32>().map_err(|e| {
        CorpusParseError::new(
            file_stem,
            CorpusParseErrorKind::InvalidSuiteNumber {
                number: number_str.into(),
                source: e,
            },
        )
    })?;

    if suite_name.is_empty() {
        return Err(CorpusParseError::new(
            file_stem,
            CorpusParseErrorKind::EmptySuiteName,
        ));
    }

    Ok((
        SuiteHeader {
            number: suite_number,
            name: Cow::Borrowed(suite_name),
        },
        rest,
    ))
}

/// Parses all groups, cases, and comments from the content after the header.
fn parse_groups<'a>(
    file_stem: &str,
    content: &'a str,
) -> Result<Vec<TestGroup<'a>>, CorpusParseError> {
    let mut groups: Vec<TestGroup<'a>> = Vec::new();
    let mut remaining = content;

    while !remaining.is_empty() {
        remaining = skip_whitespace(remaining);

        if remaining.is_empty() {
            break;
        }

        if remaining.starts_with(GROUP_HEADER_PREFIX) {
            let (rest, group_name) = parse_group_header(file_stem, remaining)?;

            groups.push(TestGroup {
                name: Cow::Borrowed(group_name),
                cases: Vec::new(),
            });

            remaining = rest;

            continue;
        }

        if let Some(rest) = skip_comment(file_stem, remaining)? {
            remaining = rest;

            continue;
        }

        if remaining.starts_with('#') {
            return Err(CorpusParseError::new(
                file_stem,
                CorpusParseErrorKind::MalformedComment,
            ));
        }

        if remaining.starts_with(CASE_HEADER_PREFIX) {
            remaining = parse_case(file_stem, remaining, &mut groups)?;

            continue;
        }

        let next_line = remaining.split_once('\n').map_or(remaining, |(l, _)| l);

        return Err(CorpusParseError::new(
            file_stem,
            CorpusParseErrorKind::UnexpectedContent {
                line: next_line.into(),
            },
        ));
    }

    Ok(groups)
}

/// Strips leading newlines and carriage returns from the input.
#[must_use = "returns the trimmed slice; original is unchanged"]
fn skip_whitespace(remaining: &str) -> &str {
    remaining.trim_start_matches(&['\n', '\r'][..])
}

/// Parses a `# === Name ===` group header line.
///
/// Returns the remaining content after the line and the extracted group name.
fn parse_group_header<'a>(
    file_stem: &str,
    remaining: &'a str,
) -> Result<(&'a str, &'a str), CorpusParseError> {
    let (line, rest) = remaining.split_once('\n').ok_or_else(|| {
        CorpusParseError::new(file_stem, CorpusParseErrorKind::TruncatedGroupHeader)
    })?;

    if !line.ends_with(GROUP_HEADER_SUFFIX) {
        return Err(CorpusParseError::new(
            file_stem,
            CorpusParseErrorKind::MalformedGroupHeader,
        ));
    }

    let group_name = line
        .strip_prefix(GROUP_HEADER_PREFIX)
        .and_then(|s| s.strip_suffix(GROUP_HEADER_SUFFIX))
        .ok_or_else(|| {
            CorpusParseError::new(file_stem, CorpusParseErrorKind::MalformedGroupHeader)
        })?
        .trim();

    if group_name.is_empty() {
        return Err(CorpusParseError::new(
            file_stem,
            CorpusParseErrorKind::EmptyGroupName,
        ));
    }

    Ok((rest, group_name))
}

/// Attempts to skip a comment line.
///
/// Returns `Ok(Some(rest))` if a valid comment was consumed, `Ok(None)` if the
/// current position does not start with a comment prefix.
///
/// # Errors
///
/// Returns [`CorpusParseErrorKind::TruncatedComment`] when a `# ` prefixed comment
/// line has no trailing newline.
fn skip_comment<'a>(
    file_stem: &str,
    remaining: &'a str,
) -> Result<Option<&'a str>, CorpusParseError> {
    if remaining.starts_with(COMMENT_PREFIX) {
        let (_, rest) = remaining.split_once('\n').ok_or_else(|| {
            CorpusParseError::new(file_stem, CorpusParseErrorKind::TruncatedComment)
        })?;

        return Ok(Some(rest));
    }

    if let Some(rest) = remaining.strip_prefix(COMMENT_NEWLINE) {
        return Ok(Some(rest));
    }

    if remaining == COMMENT_EOF {
        return Ok(Some(""));
    }

    Ok(None)
}

/// The prefix [`ERROR_INPUT_SEPARATOR`] and [`INPUT_SEPARATOR`] share, exploited by
/// [`find_case_separator`] to scan for either in a single pass.
const SEPARATOR_COMMON_PREFIX: &str = "\n---";

/// Finds this case's separator in `text` — whichever of [`ERROR_INPUT_SEPARATOR`]/
/// [`INPUT_SEPARATOR`] occurs first — and which [`CaseExpectation`] it marks. Checking one
/// pattern unconditionally before the other would let a `--- <error>` belonging to some later
/// case in the file outrun a nearer bare `---` and swallow everything in between, so whichever
/// is *first* must win.
///
/// Both separators start with [`SEPARATOR_COMMON_PREFIX`], so this scans for that shared prefix
/// once, classifying each occurrence found (and resuming the scan past it if it's neither
/// separator verbatim), rather than running two independent full scans of `text` — one per
/// separator — the way two separate [`str::find`] calls would.
///
/// Returns the separator's start offset, its byte length, and the [`CaseExpectation`] it marks.
#[must_use = "finding the separator has no effect unless the caller uses the result"]
fn find_case_separator(text: &str) -> Option<(usize, usize, CaseExpectation)> {
    let mut search_from = 0;

    loop {
        let found_at = text.get(search_from..)?.find(SEPARATOR_COMMON_PREFIX)?;
        let candidate = search_from.saturating_add(found_at);
        let rest = text.get(candidate..)?;

        if rest.starts_with(ERROR_INPUT_SEPARATOR) {
            return Some((
                candidate,
                ERROR_INPUT_SEPARATOR.len(),
                CaseExpectation::FailsToParse,
            ));
        }

        if rest.starts_with(INPUT_SEPARATOR) {
            return Some((candidate, INPUT_SEPARATOR.len(), CaseExpectation::Parses));
        }

        search_from = candidate.saturating_add(SEPARATOR_COMMON_PREFIX.len());
    }
}

/// Parses a single `=== name` test case block, pushing it to the last group.
///
/// Returns the remaining content after the parsed case.
fn parse_case<'a>(
    file_stem: &str,
    remaining: &'a str,
    groups: &mut [TestGroup<'a>],
) -> Result<&'a str, CorpusParseError> {
    let (case_header, after_header) = remaining.split_once('\n').ok_or_else(|| {
        CorpusParseError::new(file_stem, CorpusParseErrorKind::MalformedCaseHeader)
    })?;

    let case_name = case_header
        .strip_prefix(CASE_HEADER_PREFIX)
        .ok_or_else(|| CorpusParseError::new(file_stem, CorpusParseErrorKind::MalformedCaseHeader))?
        .trim();

    if case_name.is_empty() {
        return Err(CorpusParseError::new(
            file_stem,
            CorpusParseErrorKind::EmptyCaseDescription,
        ));
    }

    let current_group = groups.last_mut().ok_or_else(|| {
        CorpusParseError::new(
            file_stem,
            CorpusParseErrorKind::OrphanTestCase {
                case: case_name.into(),
            },
        )
    })?;

    let (separator_pos, separator_len, expectation) = find_case_separator(after_header)
        .ok_or_else(|| {
            CorpusParseError::new(
                file_stem,
                CorpusParseErrorKind::MissingInputSeparator {
                    case: case_name.into(),
                },
            )
        })?;

    let (input, after_separator) = after_header.split_at(separator_pos);
    let (_, after_input) = after_separator.split_at(separator_len);

    let (expected, after_expected) =
        if let Some(stripped) = after_input.strip_prefix(EMPTY_EXPECTED_PREFIX) {
            ("", stripped)
        } else {
            after_input.split_once(EXPECTED_TERMINATOR).ok_or_else(|| {
                CorpusParseError::new(
                    file_stem,
                    CorpusParseErrorKind::MissingExpectedSeparator {
                        case: case_name.into(),
                    },
                )
            })?
        };

    current_group.cases.push(CorpusCase {
        name: Cow::Borrowed(case_name),
        input: unescape_delimiter_lines(input),
        expected: unescape_delimiter_lines(expected),
        expectation,
    });

    Ok(after_expected)
}

/// Unescapes literal `---`/`===` lines (written as `\---`/`\===`) within a case's
/// `input` or `expected` text.
///
/// A leading backslash never collides with the separator/terminator search in
/// [`parse_case`], since the extra byte shifts the line out of alignment with the
/// `"\n---\n"`-shaped patterns being matched; this only needs to undo the escape in
/// the extracted text.
///
/// Returns a borrowed slice when no escaped lines are present (the common case);
/// otherwise returns an owned, unescaped copy.
#[must_use = "returns the unescaped text; original is unchanged"]
fn unescape_delimiter_lines(content: &str) -> Cow<'_, str> {
    let needs_unescaping = content
        .split('\n')
        .any(|line| line == ESCAPED_SEPARATOR_LINE || line == ESCAPED_GROUP_HEADER_LINE);

    if !needs_unescaping {
        return Cow::Borrowed(content);
    }

    let unescaped = content
        .split('\n')
        .map(|line| match line {
            ESCAPED_SEPARATOR_LINE => "---",
            ESCAPED_GROUP_HEADER_LINE => "===",
            other => other,
        })
        .collect::<Vec<_>>()
        .join("\n");

    Cow::Owned(unescaped)
}

/// Validates that the parsed groups are non-empty and each contains at least one case.
fn validate_groups(file_stem: &str, groups: &[TestGroup<'_>]) -> Result<(), CorpusParseError> {
    if groups.is_empty() {
        return Err(CorpusParseError::new(
            file_stem,
            CorpusParseErrorKind::NoGroups,
        ));
    }

    for group in groups {
        if group.cases.is_empty() {
            return Err(CorpusParseError::new(
                file_stem,
                CorpusParseErrorKind::EmptyGroup {
                    group: group.name.as_ref().into(),
                },
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    use core::assert_matches;
    use std::{borrow::Cow, fmt};

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error(transparent)]
        Parse(#[from] CorpusParseError),
        #[error("Test failure: {0}")]
        Failure(String),
    }

    /// Extracts the error from a `Result` that is expected to be `Err`.
    fn expect_err<T>(result: Result<T, CorpusParseError>) -> Result<CorpusParseError, TestError>
    where
        T: fmt::Debug,
    {
        result
            .err()
            .ok_or_else(|| TestError::Failure("expected parse to fail".into()))
    }

    const VALID_MINIMAL: &str = "#! Suite 1: Minimal Test\n# === Basic ===\n=== Simple case\ninput text\n---\nexpected text\n---\n\n\n";

    const VALID_MULTI_GROUP: &str = "#! Suite 42: Multi Group\n\
        # === First ===\n\
        === case A\n\
        input A\n\
        ---\n\
        expected A\n\
        ---\n\n\n\
        # === Second ===\n\
        === case B\n\
        input B\n\
        ---\n\
        expected B\n\
        ---\n\n\n";

    const VALID_EMPTY_EXPECTED: &str =
        "#! Suite 5: Empty Exp\n# === Group ===\n=== empty out\ninput only\n---\n---\n\n\n";

    const VALID_WITH_COMMENTS: &str = "#! Suite 7: Comments\n\
        # This is a comment\n\
        # === Group ===\n\
        # Another comment\n\
        === test\n\
        in\n\
        ---\n\
        out\n\
        ---\n\n\n";

    const VALID_BARE_HASH_NEWLINE: &str =
        "#! Suite 8: Hash\n#\n# === G ===\n=== t\ni\n---\no\n---\n\n\n";

    const VALID_BARE_HASH_EOF: &str =
        "#! Suite 9: HashEOF\n# === G ===\n=== t\ni\n---\no\n---\n\n\n#";

    mod parse {
        use super::*;

        mod valid_input {
            use super::*;

            #[rstest]
            #[case::minimal(VALID_MINIMAL, 1, "Minimal Test", 1, &[1])]
            #[case::multi_group(VALID_MULTI_GROUP, 42, "Multi Group", 2, &[1, 1])]
            #[case::with_comments(VALID_WITH_COMMENTS, 7, "Comments", 1, &[1])]
            #[case::bare_hash_newline(VALID_BARE_HASH_NEWLINE, 8, "Hash", 1, &[1])]
            #[case::bare_hash_eof(VALID_BARE_HASH_EOF, 9, "HashEOF", 1, &[1])]
            fn parses_suite_structure(
                #[case] input: &str,
                #[case] expected_number: u32,
                #[case] expected_name: &str,
                #[case] expected_groups: usize,
                #[case] cases_per_group: &[usize],
            ) -> Result<(), TestError> {
                let suite = TestSuite::parse("test", input)?;

                assert_eq!(suite.number, expected_number);
                assert_eq!(suite.name.as_ref(), expected_name);
                assert_eq!(suite.groups.len(), expected_groups);

                for (group, &expected_cases) in suite.groups.iter().zip(cases_per_group) {
                    assert_eq!(group.cases.len(), expected_cases);
                }

                Ok(())
            }

            #[rstest]
            #[case::minimal(VALID_MINIMAL)]
            fn borrows_from_input(#[case] input: &str) -> Result<(), TestError> {
                let suite = TestSuite::parse("test", input)?;

                assert!(
                    matches!(suite.name, Cow::Borrowed(_)),
                    "suite name should be borrowed"
                );

                let case = suite
                    .groups
                    .first()
                    .and_then(|g| g.cases.first())
                    .ok_or_else(|| TestError::Failure("no cases found".into()))?;

                assert!(
                    matches!(case.name, Cow::Borrowed(_)),
                    "case name should be borrowed"
                );

                assert!(
                    matches!(case.input, Cow::Borrowed(_)),
                    "case input should be borrowed"
                );

                assert!(
                    matches!(case.expected, Cow::Borrowed(_)),
                    "case expected should be borrowed"
                );

                Ok(())
            }

            #[rstest]
            #[case::empty_expected(VALID_EMPTY_EXPECTED)]
            fn parses_empty_expected(#[case] input: &str) -> Result<(), TestError> {
                let suite = TestSuite::parse("test", input)?;
                let case = suite
                    .groups
                    .first()
                    .and_then(|g| g.cases.first())
                    .ok_or_else(|| TestError::Failure("no cases found".into()))?;

                assert_eq!(case.expected.as_ref(), "");

                Ok(())
            }

            #[rstest]
            #[case::case_content(VALID_MINIMAL)]
            fn extracts_case_fields(#[case] input: &str) -> Result<(), TestError> {
                let suite = TestSuite::parse("test", input)?;
                let case = suite
                    .groups
                    .first()
                    .and_then(|g| g.cases.first())
                    .ok_or_else(|| TestError::Failure("no cases found".into()))?;

                assert_eq!(case.name.as_ref(), "Simple case");
                assert_eq!(case.input.as_ref(), "input text");
                assert_eq!(case.expected.as_ref(), "expected text");

                Ok(())
            }
        }

        mod crlf_normalization {
            use super::*;

            #[rstest]
            #[case::crlf_minimal(
                "#! Suite 1: CRLF\r\n# === G ===\r\n=== t\r\ninput\r\n---\r\nout\r\n---\r\n\r\n\r\n"
            )]
            fn normalizes_crlf_to_owned(#[case] input: &str) -> Result<(), TestError> {
                let suite = TestSuite::parse("test", input)?;

                assert_eq!(suite.number, 1);
                assert_eq!(suite.name.as_ref(), "CRLF");

                assert!(
                    matches!(suite.name, Cow::Owned(_)),
                    "suite name should be owned after CRLF normalization"
                );

                Ok(())
            }
        }

        mod escaped_delimiters {
            use super::*;

            const ESCAPED_SEPARATOR_IN_INPUT: &str = "#! Suite 1: T\n# === G ===\n=== escaped input\nbefore\n\\---\nafter\n---\nexpected text\n---\n\n\n";

            const ESCAPED_GROUP_HEADER_IN_EXPECTED: &str = "#! Suite 1: T\n# === G ===\n=== escaped expected\ninput text\n---\nbefore\n\\===\nafter\n---\n\n\n";

            const ESCAPED_LINE_AT_START_OF_EXPECTED: &str = "#! Suite 1: T\n# === G ===\n=== escaped first expected line\ninput text\n---\n\\---\n---\n\n\n";

            fn first_case<'a>(suite: &'a TestSuite<'a>) -> Result<&'a CorpusCase<'a>, TestError> {
                suite
                    .groups
                    .first()
                    .and_then(|g| g.cases.first())
                    .ok_or_else(|| TestError::Failure("no cases found".into()))
            }

            #[rstest]
            fn unescapes_separator_line_in_input() -> Result<(), TestError> {
                let suite = TestSuite::parse("test", ESCAPED_SEPARATOR_IN_INPUT)?;
                let case = first_case(&suite)?;

                assert_eq!(case.input.as_ref(), "before\n---\nafter");

                assert!(
                    matches!(case.input, Cow::Owned(_)),
                    "escaped input should be an owned, unescaped copy"
                );

                Ok(())
            }

            #[rstest]
            fn unescapes_group_header_line_in_expected() -> Result<(), TestError> {
                let suite = TestSuite::parse("test", ESCAPED_GROUP_HEADER_IN_EXPECTED)?;
                let case = first_case(&suite)?;

                assert_eq!(case.expected.as_ref(), "before\n===\nafter");

                Ok(())
            }

            #[rstest]
            fn escaped_line_at_start_of_expected_is_not_mistaken_for_empty_expected()
            -> Result<(), TestError> {
                let suite = TestSuite::parse("test", ESCAPED_LINE_AT_START_OF_EXPECTED)?;
                let case = first_case(&suite)?;

                assert_eq!(case.expected.as_ref(), "---");

                Ok(())
            }
        }

        mod case_expectation {
            use super::*;

            const NEGATIVE_CASE: &str = "#! Suite 1: T\n# === G ===\n=== dangling pipe is a syntax error\necho a |\n--- <error>\nParse error: invalid syntax\n---\n\n\n";

            const POSITIVE_CASE: &str = "#! Suite 1: T\n# === G ===\n=== simple command\necho a\n---\n(command (word \"echo\") (word \"a\"))\n---\n\n\n";

            fn first_case<'a>(suite: &'a TestSuite<'a>) -> Result<&'a CorpusCase<'a>, TestError> {
                suite
                    .groups
                    .first()
                    .and_then(|g| g.cases.first())
                    .ok_or_else(|| TestError::Failure("no cases found".into()))
            }

            #[rstest]
            fn error_marker_yields_fails_to_parse_with_message() -> Result<(), TestError> {
                let suite = TestSuite::parse("test", NEGATIVE_CASE)?;
                let case = first_case(&suite)?;

                assert_matches!(case.expectation, CaseExpectation::FailsToParse);
                assert_eq!(case.expected.as_ref(), "Parse error: invalid syntax");

                Ok(())
            }

            #[rstest]
            fn bare_separator_yields_parses() -> Result<(), TestError> {
                let suite = TestSuite::parse("test", POSITIVE_CASE)?;
                let case = first_case(&suite)?;

                assert_matches!(case.expectation, CaseExpectation::Parses);

                Ok(())
            }
        }

        mod error_cases {
            use super::*;

            #[rstest]
            #[case::empty_file("")]
            #[case::no_newline("#! Suite 1: Test")]
            fn rejects_empty_file(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::EmptyFile);
                assert_eq!(err.file.as_ref(), "test");

                Ok(())
            }

            #[rstest]
            #[case::missing_magic("not a suite\n")]
            fn rejects_missing_suite_header(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::MissingSuiteHeader);

                Ok(())
            }

            #[rstest]
            #[case::no_colon("#! Suite 1 Test Name\n")]
            fn rejects_malformed_suite_header(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::MalformedSuiteHeader);

                Ok(())
            }

            #[rstest]
            #[case::blank_number("#! Suite : Name\n")]
            fn rejects_empty_suite_number(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::EmptySuiteNumber);

                Ok(())
            }

            #[rstest]
            #[case::non_numeric("#! Suite abc: Name\n")]
            fn rejects_invalid_suite_number(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::InvalidSuiteNumber { .. });

                Ok(())
            }

            #[rstest]
            #[case::blank_name("#! Suite 1:\n")]
            #[case::whitespace_name("#! Suite 1:   \n")]
            fn rejects_empty_suite_name(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::EmptySuiteName);

                Ok(())
            }

            #[rstest]
            #[case::no_groups("#! Suite 1: Test\n")]
            fn rejects_no_groups(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::NoGroups);

                Ok(())
            }

            #[rstest]
            #[case::empty_group("#! Suite 1: Test\n# === Empty ===\n")]
            fn rejects_empty_group(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::EmptyGroup { .. });

                Ok(())
            }

            #[rstest]
            #[case::orphan("#! Suite 1: Test\n=== orphan case\ninput\n---\nout\n---\n\n\n")]
            fn rejects_orphan_test_case(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::OrphanTestCase { .. });

                Ok(())
            }

            #[rstest]
            #[case::bad_comment("#! Suite 1: Test\n#bad comment\n")]
            fn rejects_malformed_comment(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::MalformedComment);

                Ok(())
            }

            #[rstest]
            #[case::no_suffix("#! Suite 1: Test\n# === NoSuffix\n")]
            fn rejects_malformed_group_header(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::MalformedGroupHeader);

                Ok(())
            }

            #[rstest]
            #[case::blank_group_name("#! Suite 1: Test\n# ===  ===\n")]
            fn rejects_empty_group_name(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::EmptyGroupName);

                Ok(())
            }

            #[rstest]
            #[case::no_input_sep("#! Suite 1: T\n# === G ===\n=== case\njust input no separator\n")]
            fn rejects_missing_input_separator(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::MissingInputSeparator { .. });

                Ok(())
            }

            #[rstest]
            #[case::no_expected_sep("#! Suite 1: T\n# === G ===\n=== case\ni\n---\nout\n---\n")]
            fn rejects_missing_expected_separator(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(
                    err.kind,
                    CorpusParseErrorKind::MissingExpectedSeparator { .. }
                );

                Ok(())
            }

            #[rstest]
            #[case::unexpected("#! Suite 1: T\n# === G ===\nrandom line\n")]
            fn rejects_unexpected_content(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::UnexpectedContent { .. });

                Ok(())
            }

            #[rstest]
            #[case::blank_desc("#! Suite 1: T\n# === G ===\n===   \ni\n---\no\n---\n\n\n")]
            fn rejects_empty_case_description(#[case] input: &str) -> Result<(), TestError> {
                let err = expect_err(TestSuite::parse("test", input))?;

                assert_matches!(err.kind, CorpusParseErrorKind::EmptyCaseDescription);

                Ok(())
            }
        }
    }

    mod skip_whitespace_fn {
        use super::*;

        #[rstest]
        #[case::empty("", "")]
        #[case::no_whitespace("hello", "hello")]
        #[case::leading_newlines("\n\nhello", "hello")]
        #[case::leading_cr("\r\rhello", "hello")]
        #[case::mixed("\r\n\n\rhello", "hello")]
        #[case::only_whitespace("\n\n\n", "")]
        fn strips_leading_whitespace(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(skip_whitespace(input), expected);
        }
    }

    mod skip_comment_fn {
        use super::*;

        #[rstest]
        #[case::regular("# a comment\nrest", Some("rest"))]
        #[case::hash_newline("#\nrest", Some("rest"))]
        #[case::hash_eof("#", Some(""))]
        #[case::not_comment("=== case\n", None)]
        #[case::group_header_not_comment("# === Group ===\nrest", Some("rest"))]
        fn handles_comment_variants(
            #[case] input: &str,
            #[case] expected: Option<&str>,
        ) -> Result<(), TestError> {
            let result = skip_comment("test", input)?;

            assert_eq!(result, expected);

            Ok(())
        }

        #[rstest]
        #[case::truncated("# no newline")]
        fn rejects_truncated_comment(#[case] input: &str) -> Result<(), TestError> {
            let err = expect_err(skip_comment("test", input))?;

            assert_matches!(err.kind, CorpusParseErrorKind::TruncatedComment);

            Ok(())
        }
    }

    mod find_case_separator_fn {
        use super::*;

        #[rstest]
        #[case::bare_only("a\n---\nb", 1, 5, CaseExpectation::Parses)]
        #[case::error_only("a\n--- <error>\nb", 1, 13, CaseExpectation::FailsToParse)]
        #[case::bare_before_error("a\n---\nb\n--- <error>\nc", 1, 5, CaseExpectation::Parses)]
        #[case::error_before_bare(
            "a\n--- <error>\nb\n---\nc",
            1,
            13,
            CaseExpectation::FailsToParse
        )]
        #[case::near_miss_prefix_is_skipped("a\n----\nb\n---\nc", 8, 5, CaseExpectation::Parses)]
        fn finds_the_earliest_separator(
            #[case] input: &str,
            #[case] expected_pos: usize,
            #[case] expected_len: usize,
            #[case] expected_expectation: CaseExpectation,
        ) -> Result<(), TestError> {
            let (pos, len, expectation) = find_case_separator(input)
                .ok_or_else(|| TestError::Failure("expected a separator".into()))?;

            assert_eq!(pos, expected_pos);
            assert_eq!(len, expected_len);
            assert_eq!(expectation, expected_expectation);

            Ok(())
        }

        #[rstest]
        #[case::no_separator("just text, no separator at all")]
        #[case::near_miss_only("a\n----\nb")]
        fn returns_none_when_absent(#[case] input: &str) {
            assert!(find_case_separator(input).is_none());
        }
    }

    mod into_owned_fn {
        use super::*;

        #[rstest]
        #[case::borrowed_suite(VALID_MINIMAL)]
        fn produces_static_lifetime(#[case] input: &str) -> Result<(), TestError> {
            let suite = TestSuite::parse("test", input)?;
            let owned: TestSuite<'static> = suite.into_owned();

            assert!(
                matches!(owned.name, Cow::Owned(_)),
                "name should be owned after into_owned"
            );

            Ok(())
        }
    }
}
