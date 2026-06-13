//! Error types for the stage execution engine.

use thiserror::Error;

/// Errors that can occur while executing the stage pipeline.
#[derive(Error, Debug)]
pub enum EngineError {
    /// A stage failed during execution.
    ///
    /// The contained string describes the underlying failure and is forwarded
    /// to the caller as-is.
    #[error("Stage execution failed: {0}")]
    StageExecution(String),

    /// A stage failed to parse a JSON payload (e.g. tool input/output).
    #[error("Failed to parse JSON: {0}")]
    JsonParse(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::assert_matches;
    use rstest::rstest;

    #[derive(thiserror::Error, Debug)]
    pub enum TestError {
        #[error(transparent)]
        Engine(#[from] EngineError),
        #[error("Test failure: {0}")]
        Failure(String),
    }

    #[rstest]
    fn engine_error_stage_execution_display() {
        let err = EngineError::StageExecution("boom".to_owned());
        assert_eq!(err.to_string(), "Stage execution failed: boom");
    }

    #[rstest]
    fn engine_error_json_parse_from() -> Result<(), TestError> {
        let res: Result<serde_json::Value, _> = serde_json::from_str("invalid");
        assert_matches!(res, Err(_));

        let Err(json_err) = res else {
            return Err(TestError::Failure("expected Err".into()));
        };

        let err = EngineError::JsonParse(json_err);

        assert_matches!(err, EngineError::JsonParse(_));
        assert!(err.to_string().starts_with("Failed to parse JSON: "));

        Ok(())
    }
}
