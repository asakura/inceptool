//! Error types for corpus `.tests` file parsing.

use std::num::ParseIntError;

/// Errors produced when parsing corpus `.tests` files.
///
/// Wraps a [`CorpusParseErrorKind`] with the file stem that triggered the error,
/// providing a consistent `"{file}.tests: {kind}"` message format.
#[derive(Debug, thiserror::Error)]
#[error("{file}.tests: {kind}")]
pub struct CorpusParseError {
    /// File stem (e.g. `"01_words"`).
    pub file: Box<str>,
    /// The specific error kind.
    #[source]
    pub kind: CorpusParseErrorKind,
}

impl CorpusParseError {
    /// Creates a new parse error for the given file and error kind.
    #[must_use = "returns a new error value; does not have side effects"]
    pub(crate) fn new<S>(file: S, kind: CorpusParseErrorKind) -> Self
    where
        S: Into<Box<str>>,
    {
        Self {
            file: file.into(),
            kind,
        }
    }
}

/// Specific kinds of corpus parse errors.
#[derive(Debug, thiserror::Error)]
pub enum CorpusParseErrorKind {
    /// The file has no first line to parse.
    #[error("file is empty")]
    EmptyFile,

    /// First line lacks the `#! Suite ` magic prefix.
    #[error("must start with '#! Suite '")]
    MissingSuiteHeader,

    /// Suite header line has no `:` separating number from name.
    #[error("missing ':' in suite header")]
    MalformedSuiteHeader,

    /// Suite number field is blank.
    #[error("suite number is empty")]
    EmptySuiteNumber,

    /// Suite number is not a valid `u32`.
    #[error("invalid suite number '{number}': {source}")]
    InvalidSuiteNumber {
        /// The text that could not be parsed.
        number: Box<str>,
        /// Underlying parse failure.
        source: ParseIntError,
    },

    /// Suite name field is blank.
    #[error("suite name is empty")]
    EmptySuiteName,

    /// A group header line is missing its trailing newline (truncated file).
    #[error("truncated group header line")]
    TruncatedGroupHeader,

    /// A group header (`# === Name ===`) does not end with ` ===`.
    #[error("malformed group header: must end with ' ==='")]
    MalformedGroupHeader,

    /// The name inside a group header is blank.
    #[error("empty group name")]
    EmptyGroupName,

    /// A test case header line is syntactically broken.
    #[error("malformed test case header")]
    MalformedCaseHeader,

    /// A test case's description text is blank.
    #[error("case description is empty")]
    EmptyCaseDescription,

    /// A test case appears before any group header.
    #[error("test case '{case}' must be under a test group")]
    OrphanTestCase {
        /// The orphaned case description.
        case: Box<str>,
    },

    /// The `\n---\n` separator after the input section is missing.
    #[error("case '{case}' is missing a closing '\\n---\\n' for its input section")]
    MissingInputSeparator {
        /// Case description.
        case: Box<str>,
    },

    /// The `\n---\n\n\n` terminator after the expected section is missing.
    #[error("case '{case}' expected section must be followed by two newlines (\\n---\\n\\n\\n)")]
    MissingExpectedSeparator {
        /// Case description.
        case: Box<str>,
    },

    /// The file contains no `# === … ===` group headers.
    #[error("must contain at least one test group")]
    NoGroups,

    /// A group header has no test cases beneath it.
    #[error("group '{group}' has no test cases")]
    EmptyGroup {
        /// Name of the empty group.
        group: Box<str>,
    },

    /// A comment line does not start with `# ` (space mandatory).
    #[error("comments outside test cases must start with '# ' (space is mandatory)")]
    MalformedComment,

    /// A line that is not a comment, group header, test case, or blank.
    #[error("unexpected content outside test cases: {line}")]
    UnexpectedContent {
        /// The offending line.
        line: Box<str>,
    },

    /// A comment line is missing its trailing newline (truncated file).
    #[error("comment line missing newline")]
    TruncatedComment,
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    use core::assert_matches;
    use std::error::Error as StdError;

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error("Test failure: {0}")]
        Failure(String),
    }

    mod display {
        use super::*;

        #[rstest]
        fn formats_fieldless_kind_as_file_dot_tests_colon_kind() {
            let err = CorpusParseError::new("test", CorpusParseErrorKind::EmptyFile);
            assert_eq!(err.to_string(), "test.tests: file is empty");
        }

        #[rstest]
        fn formats_kind_with_field() {
            let err = CorpusParseError::new(
                "test",
                CorpusParseErrorKind::EmptyGroup {
                    group: "Group".into(),
                },
            );

            assert_eq!(
                err.to_string(),
                "test.tests: group 'Group' has no test cases"
            );
        }
    }

    mod source {
        use super::*;

        #[rstest]
        fn corpus_parse_error_source_is_always_the_kind() {
            let err = CorpusParseError::new("test", CorpusParseErrorKind::EmptyFile);
            assert_matches!(StdError::source(&err), Some(_));
        }

        #[rstest]
        fn fieldless_kind_has_no_source() {
            assert_matches!(StdError::source(&CorpusParseErrorKind::EmptyFile), None);
        }

        #[rstest]
        fn invalid_suite_number_source_downcasts_to_parse_int_error() -> Result<(), TestError> {
            let parse_error = "abc"
                .parse::<u32>()
                .err()
                .ok_or_else(|| TestError::Failure("expected parse to fail".to_owned()))?;

            let kind = CorpusParseErrorKind::InvalidSuiteNumber {
                number: "abc".into(),
                source: parse_error,
            };

            let source = StdError::source(&kind)
                .ok_or_else(|| TestError::Failure("expected a source error".to_owned()))?;

            assert_matches!(source.downcast_ref::<ParseIntError>(), Some(_));

            Ok(())
        }
    }
}
