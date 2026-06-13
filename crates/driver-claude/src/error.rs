//! Error types for the Claude driver.
//!
//! [`ClaudeDriverError`] is the `Driver::Error` associated type for
//! [`ClaudeDriver`](crate::driver::ClaudeDriver), returned by `map_input` /
//! `map_output` and thus by [`inceptool_protocol::from_wire`] /
//! [`inceptool_protocol::to_wire`] on failure.
//!
//! [`ConversionError`] is produced when converting a `HookOutputEvent` into a
//! [`ClaudeHookSpecificOutput`](crate::ClaudeHookSpecificOutput); it is
//! wrapped by [`ClaudeDriverError::Conversion`], but
//! [`ClaudeDriver::map_output`](crate::driver::ClaudeDriver) catches it with
//! `.ok()` rather than propagating it.

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
#[derive(Debug, Clone, Copy, Error)]
pub enum ConversionError {
    /// An unsupported hook output event was encountered.
    #[error("Unsupported event variant for Claude: {0}")]
    UnsupportedEvent(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_formatting() {
        let err = ClaudeDriverError::Protocol(inceptool_protocol::ProtocolError::UnsupportedEvent(
            "foo".into(),
        ));
        assert!(err.to_string().contains("foo"));
    }
}
