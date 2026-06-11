//! Error types for the protocol crate.

use thiserror::Error;

/// Errors that can occur during protocol operations, such as parsing or formatting data.
#[derive(Error, Debug)]
pub enum ProtocolError {
    /// Occurs when a JSON payload fails to parse.
    #[error("Failed to parse JSON: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Occurs when an unrecognized or unsupported hook event is encountered.
    #[error("Unsupported hook event: {0}")]
    UnsupportedEvent(String),

    /// Occurs when a required field is missing from a payload.
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::assert_matches;
    use rstest::rstest;

    #[derive(thiserror::Error, Debug)]
    pub enum TestError {
        #[error(transparent)]
        Protocol(#[from] ProtocolError),
        #[error(transparent)]
        Json(#[from] serde_json::Error),
        #[error("Test failure: {0}")]
        Failure(String),
    }

    #[rstest]
    fn test_protocol_error_formatting_unsupported() -> Result<(), TestError> {
        let err1 = ProtocolError::UnsupportedEvent("OnHover".to_string());
        assert_eq!(err1.to_string(), "Unsupported hook event: OnHover");
        Ok(())
    }

    #[rstest]
    fn test_protocol_error_formatting_missing() -> Result<(), TestError> {
        let err2 = ProtocolError::MissingField("event_type");
        assert_eq!(err2.to_string(), "Missing required field: event_type");
        Ok(())
    }

    #[rstest]
    fn test_protocol_error_formatting_json() -> Result<(), TestError> {
        let res: Result<serde_json::Value, _> = serde_json::from_str("invalid");
        assert_matches!(res, Err(_));

        let Err(json_err) = res else {
            return Err(TestError::Failure("expected Err".into()));
        };

        let err3 = ProtocolError::JsonParse(json_err);

        assert_matches!(err3, ProtocolError::JsonParse(_));
        assert!(err3.to_string().starts_with("Failed to parse JSON: "));

        Ok(())
    }
}
