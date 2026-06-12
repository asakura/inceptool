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
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    #[rstest]
    fn test_engine_error_stage_execution_display() {
        let err = EngineError::StageExecution("boom".to_string());
        assert_eq!(err.to_string(), "Stage execution failed: boom");
    }
}
