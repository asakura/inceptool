//! Error types for the Claude driver.

use inceptool_protocol::ProtocolError;
use thiserror::Error;

/// Errors that can occur within the Claude driver.
#[derive(Debug, Error)]
pub enum ClaudeDriverError {
    /// A protocol-level error occurred.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),

    /// A JSON serialization or deserialization error occurred.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Failed to convert hook output event.
    #[error(transparent)]
    Conversion(#[from] ConversionError),
}

/// Errors that can occur during conversion.
#[derive(Debug, Error)]
pub enum ConversionError {
    /// An unsupported hook output event was encountered.
    #[error("Unsupported event variant for Claude: {0}")]
    UnsupportedEvent(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_formatting() {
        let err = ClaudeDriverError::Protocol(inceptool_protocol::ProtocolError::UnsupportedEvent(
            "foo".into(),
        ));
        assert!(err.to_string().contains("foo"));
    }
}
