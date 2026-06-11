//! Defines the driver trait abstractions for agent backends.

use crate::error::ProtocolError;
use crate::output::HookOutputEvent;
use crate::session::Conn;

/// An abstraction over a specific backend driver (e.g., Claude or Gemini).
///
/// The `Driver` trait provides the necessary methods to convert raw JSON strings
/// from the backend into canonical protocol objects (`Conn`), and format the hook
/// results back into the JSON format expected by the backend.
pub trait Driver {
    /// The specific error type returned by this driver.
    type Error: std::error::Error
        + Send
        + Sync
        + From<ProtocolError>
        + From<serde_json::Error>
        + 'static;

    /// The wire format representing the incoming data for this driver.
    type InputWire<'a>: serde::Deserialize<'a>;

    /// The wire format representing the outgoing data for this driver.
    type OutputWire<'a>: serde::Serialize;

    /// Maps the parsed backend-specific input format into a standardized `Conn` object.
    fn map_input<'a>(&self, wire: Self::InputWire<'a>) -> Result<Conn<'a>, Self::Error>;

    /// Maps the canonical hook output into the backend-specific output wrapper.
    fn map_output<'a>(
        &self,
        event_name: &'a str,
        output: &'a HookOutputEvent,
    ) -> Result<Self::OutputWire<'a>, Self::Error>;
}

/// Generic entry point to parse a raw JSON payload using a specific driver.
pub fn from_wire<'a, D: Driver>(driver: &D, raw_json: &'a str) -> Result<Conn<'a>, D::Error> {
    let wire = serde_json::from_str::<D::InputWire<'a>>(raw_json)?;

    driver.map_input(wire)
}

/// Generic entry point to format a protocol output event to JSON using a specific driver.
pub fn to_wire<'a, D: Driver>(
    driver: &'a D,
    event_name: &'a str,
    output: &'a HookOutputEvent,
) -> Result<String, D::Error> {
    let wire = driver.map_output(event_name, output)?;
    let serialized = serde_json::to_string(&wire)?;

    Ok(serialized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{HookInputEvent, SessionEndInput};
    use crate::output::{BeforeToolOutput, HookOutputEvent};
    use crate::session::SessionMeta;
    use crate::types::Decision;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Copy)]
    struct MockDriver;

    #[derive(Debug, thiserror::Error)]
    enum MockError {
        #[error(transparent)]
        Protocol(#[from] ProtocolError),
        #[error(transparent)]
        Json(#[from] serde_json::Error),
    }

    #[derive(Deserialize)]
    struct MockInput<'a> {
        _ignore: Option<&'a str>,
    }

    #[derive(Serialize)]
    struct MockOutput {
        decision: Option<String>,
    }

    impl Driver for MockDriver {
        type Error = MockError;
        type InputWire<'a> = MockInput<'a>;
        type OutputWire<'a> = MockOutput;

        fn map_input<'a>(&self, _wire: Self::InputWire<'a>) -> Result<Conn<'a>, Self::Error> {
            Ok(Conn {
                session: SessionMeta {
                    session_id: "test".into(),
                    transcript_path: None,
                    cwd: None,
                    timestamp: None,
                    driver: "Mock".into(),
                    driver_meta: None,
                },
                event: HookInputEvent::SessionEnd(SessionEndInput {
                    reason: "test".into(),
                }),
            })
        }

        fn map_output<'a>(
            &self,
            _event_name: &'a str,
            output: &'a HookOutputEvent,
        ) -> Result<Self::OutputWire<'a>, Self::Error> {
            Ok(MockOutput {
                decision: output.decision().map(|d| format!("{:?}", d)),
            })
        }
    }

    #[test]
    fn test_from_wire() -> Result<(), MockError> {
        let driver = MockDriver;
        let conn = from_wire(&driver, r#"{"_ignore":"val"}"#)?;
        assert_eq!(conn.session.driver, "Mock");
        Ok(())
    }

    #[test]
    fn test_to_wire() -> Result<(), MockError> {
        let driver = MockDriver;
        let event = HookOutputEvent::BeforeTool(BeforeToolOutput {
            decision: Some(Decision::Allow),
            ..Default::default()
        });
        let json = to_wire(&driver, "evt", &event)?;
        assert_eq!(json, r#"{"decision":"Allow"}"#);
        Ok(())
    }
}
