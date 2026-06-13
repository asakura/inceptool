//! Defines the driver trait abstractions for agent backends.

use crate::error::ProtocolError;
use crate::input::HookKind;
use crate::output::HookOutputEvent;
use crate::session::Conn;

/// An abstraction over a specific backend driver (e.g., Claude or Gemini).
///
/// The `Driver` trait provides the necessary methods to convert raw JSON strings
/// from the backend into canonical protocol objects (`Conn`), format the hook
/// results back into the JSON format expected by the backend, and map the
/// backend's raw hook event names onto the canonical [`HookKind`] used to
/// select a stage pipeline.
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
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` if the wire input cannot be mapped into a
    /// canonical `Conn`.
    fn map_input<'a>(&self, wire: Self::InputWire<'a>) -> Result<Conn<'a>, Self::Error>;

    /// Maps the canonical hook output into the backend-specific output wrapper.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` if the output cannot be mapped into the
    /// backend-specific wire format.
    fn map_output<'a>(
        &self,
        event_name: &'a str,
        output: &'a HookOutputEvent,
    ) -> Result<Self::OutputWire<'a>, Self::Error>;

    /// Maps a raw hook event name, in this driver's own vocabulary (e.g.
    /// `"PreToolUse"` for Claude, `"BeforeTool"` for Gemini), to the
    /// canonical [`HookKind`] used to select a stage pipeline bucket.
    ///
    /// The raw name is supplied by the CLI invocation (the `<hook>` argument
    /// configured alongside this driver in the agent's hook settings), not
    /// derived from the JSON payload — this is what lets dispatch happen
    /// without any payload-sniffing heuristics.
    ///
    /// # Errors
    ///
    /// Returns `Self::Error` if `raw_name` does not correspond to a known
    /// `HookKind`.
    fn hook_kind(&self, raw_name: &str) -> Result<HookKind, Self::Error>;
}

/// Deserializes a driver's wire-format input from a raw JSON string and maps
/// it into a canonical [`Conn`].
///
/// This is the entry point the CLI uses to turn the raw JSON it reads from
/// stdin into the [`Conn`] / `HookInputEvent` that the engine and stages
/// operate on. It first deserializes `raw_json` into `D::InputWire`, then
/// delegates to [`Driver::map_input`] to produce the canonical representation.
///
/// # Errors
///
/// Returns an error if `raw_json` cannot be deserialized into
/// `D::InputWire`, or if `Driver::map_input` fails.
pub fn from_wire<'a, D: Driver>(driver: &D, raw_json: &'a str) -> Result<Conn<'a>, D::Error> {
    let wire = serde_json::from_str::<D::InputWire<'a>>(raw_json)?;

    driver.map_input(wire)
}

/// Maps a canonical [`HookOutputEvent`] into a driver's wire-format output via
/// [`Driver::map_output`] and serializes it back to a JSON string.
///
/// This is the entry point the CLI uses to turn the canonical
/// [`HookOutputEvent`] produced by the engine and stages back into the raw
/// JSON it writes to stdout.
///
/// # Errors
///
/// Returns an error if `Driver::map_output` fails, or if the resulting
/// wire value cannot be serialized to JSON.
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
    use crate::output::{HookOutputEvent, PreToolUseOutput};
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
                    permission_mode: None,
                    effort: None,
                    agent_id: None,
                    agent_type: None,
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

        fn hook_kind(&self, raw_name: &str) -> Result<HookKind, Self::Error> {
            Ok(HookKind::parse(raw_name)?)
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
        let event = HookOutputEvent::PreToolUse(PreToolUseOutput {
            decision: Some(Decision::Allow),
            ..Default::default()
        });
        let json = to_wire(&driver, "evt", &event)?;

        assert_eq!(json, r#"{"decision":"Allow"}"#);

        Ok(())
    }
}
