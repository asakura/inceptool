//! Error types for the Gemini driver.

use inceptool_protocol::ProtocolError;

use thiserror::Error;

/// Errors that can occur within the Gemini driver.
#[derive(Debug, Error)]
pub enum GeminiDriverError {
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
    /// Missing updated_input.
    #[error("Missing updated_input")]
    MissingUpdatedInput,

    /// Missing updated_tool_output.
    #[error("Missing updated_tool_output")]
    MissingUpdatedToolOutput,

    /// Missing additional_context.
    #[error("Missing additional_context")]
    MissingAdditionalContext,

    /// clear_context is not true.
    #[error("clear_context is not true")]
    ClearContextNotTrue,

    /// Missing llm_request and llm_response.
    #[error("Missing llm_request and llm_response")]
    MissingLlmRequestAndResponse,

    /// Missing llm_response.
    #[error("Missing llm_response")]
    MissingLlmResponse,

    /// Missing tool_config.
    #[error("Missing tool_config")]
    MissingToolConfig,

    /// An unsupported hook output event was encountered.
    #[error("Unsupported event variant for Gemini: {0}")]
    UnsupportedEvent(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_formatting() {
        let err = GeminiDriverError::Protocol(inceptool_protocol::ProtocolError::UnsupportedEvent(
            "foo".into(),
        ));
        assert!(err.to_string().contains("foo"));
    }
}
