#![cfg_attr(
    test,
    expect(
        clippy::panic_in_result_fn,
        reason = "rstest cases return Result for ?-based setup but use assert_matches! for assertions"
    )
)]
//! End-to-end tests against real and hand-written `.tests` fixtures.

use inceptool_corpus_parser::{CorpusParseErrorKind, TestSuite};

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    use core::assert_matches;

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error("Test failure: {0}")]
        Failure(String),
    }

    /// Extracts a value from an `Option`, failing the test with `msg` if absent.
    fn require<T>(value: Option<T>, msg: &str) -> Result<T, TestError> {
        value.ok_or_else(|| TestError::Failure(msg.to_owned()))
    }

    mod valid_fixtures {
        use super::*;

        const EMPTY_CASE: &str = include_str!("fixtures/empty_case/00_empty.tests");
        const MULTI_GROUP_PIPELINE: &str =
            include_str!("fixtures/multi_group_pipeline/03_pipelines.tests");

        #[rstest]
        #[case::empty_case("00_empty", EMPTY_CASE, 0, "Empty", &[1])]
        #[case::multi_group_pipeline(
            "03_pipelines",
            MULTI_GROUP_PIPELINE,
            3,
            "Pipelines",
            &[2, 1]
        )]
        fn parses_suite_structure(
            #[case] file_stem: &str,
            #[case] content: &str,
            #[case] expected_number: u32,
            #[case] expected_name: &str,
            #[case] cases_per_group: &[usize],
        ) -> Result<(), TestError> {
            let suite = TestSuite::parse(file_stem, content)
                .map_err(|e| TestError::Failure(format!("parse failed: {e}")))?;

            assert_eq!(suite.number, expected_number);
            assert_eq!(suite.name.as_ref(), expected_name);
            assert_eq!(suite.groups.len(), cases_per_group.len());

            for (group, &expected_cases) in suite.groups.iter().zip(cases_per_group) {
                assert_eq!(group.cases.len(), expected_cases);
            }

            Ok(())
        }

        #[rstest]
        fn parses_empty_case_with_blank_sections() -> Result<(), TestError> {
            let suite = TestSuite::parse("00_empty", EMPTY_CASE)
                .map_err(|e| TestError::Failure(format!("parse failed: {e}")))?;

            let group = require(suite.groups.first(), "missing group")?;

            assert_eq!(group.name.as_ref(), "Input");

            let case = require(group.cases.first(), "missing case")?;

            assert_eq!(case.name.as_ref(), "no data");
            assert_eq!(case.input.as_ref(), "");
            assert_eq!(case.expected.as_ref(), "");

            Ok(())
        }

        #[rstest]
        fn parses_pipeline_case_content() -> Result<(), TestError> {
            let suite = TestSuite::parse("03_pipelines", MULTI_GROUP_PIPELINE)
                .map_err(|e| TestError::Failure(format!("parse failed: {e}")))?;

            let group = require(suite.groups.first(), "missing group")?;
            let case = require(group.cases.first(), "missing case")?;

            assert_eq!(case.name.as_ref(), "simple pipeline");
            assert_eq!(case.input.as_ref(), "echo foo | cat");

            assert_eq!(
                case.expected.as_ref(),
                "(pipeline (command (word \"echo\") (word \"foo\")) (command (word \"cat\")))"
            );

            Ok(())
        }
    }

    mod escaped_delimiters {
        use super::*;

        const ESCAPED_CASE: &str = include_str!("fixtures/escaped_delimiters/case.tests");

        #[rstest]
        fn unescapes_literal_separator_inside_heredoc_input() -> Result<(), TestError> {
            let suite = TestSuite::parse("case", ESCAPED_CASE)
                .map_err(|e| TestError::Failure(format!("parse failed: {e}")))?;

            let group = require(suite.groups.first(), "missing group")?;
            let case = require(group.cases.first(), "missing case")?;

            assert_eq!(case.input.as_ref(), "cat <<EOF\n---\nEOF");
            assert_eq!(
                case.expected.as_ref(),
                "(command (word \"cat\") (redirect_heredoc \"EOF\"))"
            );

            Ok(())
        }
    }

    mod malformed_fixtures {
        use super::*;

        const TRUNCATED_CASE: &str = include_str!("fixtures/truncated_case/case.tests");

        #[rstest]
        fn rejects_truncated_case() -> Result<(), TestError> {
            let err = TestSuite::parse("case", TRUNCATED_CASE)
                .err()
                .ok_or_else(|| TestError::Failure("expected parse to fail".to_owned()))?;

            assert_matches!(
                err.kind,
                CorpusParseErrorKind::MissingExpectedSeparator { .. }
            );

            Ok(())
        }
    }
}
