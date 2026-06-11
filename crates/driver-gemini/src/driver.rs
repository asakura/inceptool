//! Implementation of the Driver trait for Gemini.

use crate::error::GeminiDriverError;
use crate::types::{GeminiHookSpecificOutput, GeminiMeta, GeminiOutputWire};

use inceptool_protocol::{
    Conn, Driver, HookInputEvent, HookOutputEvent, ProtocolError, SessionMeta,
};

/// Implements `Driver` for Gemini API.
#[derive(Debug, Clone, Copy, Default)]
pub struct GeminiDriver;

impl Driver for GeminiDriver {
    type Error = GeminiDriverError;
    type InputWire<'a> = &'a serde_json::value::RawValue;
    type OutputWire<'a> = GeminiOutputWire<'a>;

    fn map_input<'a>(&self, wire: Self::InputWire<'a>) -> Result<Conn<'a>, Self::Error> {
        let raw_json = wire.get();
        let meta: GeminiMeta<'a> = serde_json::from_str(raw_json)?;

        let event = match meta.hook_event_name.as_ref() {
            "BeforeTool" => HookInputEvent::PreToolUse(serde_json::from_str(raw_json)?),
            "AfterTool" => HookInputEvent::PostToolUse(serde_json::from_str(raw_json)?),
            "BeforeAgent" => HookInputEvent::BeforeAgent(serde_json::from_str(raw_json)?),
            "AfterAgent" => HookInputEvent::AfterAgent(serde_json::from_str(raw_json)?),
            "BeforeModel" => HookInputEvent::BeforeModel(serde_json::from_str(raw_json)?),
            "AfterModel" => HookInputEvent::AfterModel(serde_json::from_str(raw_json)?),
            "BeforeToolSelection" => {
                HookInputEvent::BeforeToolSelection(serde_json::from_str(raw_json)?)
            }
            "SessionStart" => HookInputEvent::SessionStart(serde_json::from_str(raw_json)?),
            "SessionEnd" => HookInputEvent::SessionEnd(serde_json::from_str(raw_json)?),
            "Notification" => HookInputEvent::Notification(serde_json::from_str(raw_json)?),
            "PreCompress" => HookInputEvent::PreCompact(serde_json::from_str(raw_json)?),
            _ => {
                return Err(
                    ProtocolError::UnsupportedEvent(meta.hook_event_name.into_owned()).into(),
                );
            }
        };

        Ok(Conn {
            session: SessionMeta {
                session_id: meta.session_id,
                transcript_path: meta.transcript_path,
                cwd: meta.cwd,
                timestamp: meta.timestamp,
                driver: "Gemini".into(),
                driver_meta: None,
                permission_mode: None,
                effort: None,
                agent_id: None,
                agent_type: None,
            },
            event,
        })
    }

    fn map_output<'a>(
        &self,
        _event_name: &'a str,
        output: &'a HookOutputEvent,
    ) -> Result<Self::OutputWire<'a>, Self::Error> {
        let hook_specific_output = GeminiHookSpecificOutput::try_from(output).ok();

        let wire = GeminiOutputWire {
            decision: output.decision(),
            reason: output.reason(),
            continue_flag: output.halt().map(|h| !h),
            suppress_output: output.suppress_output(),
            system_message: output.system_message(),
            hook_specific_output,
        };

        Ok(wire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use inceptool_protocol::{Decision, PreToolUseOutput};

    use rstest::rstest;

    #[derive(thiserror::Error, Debug)]
    pub enum TestError {
        #[error(transparent)]
        Protocol(#[from] ProtocolError),
        #[error(transparent)]
        Driver(#[from] GeminiDriverError),
        #[error(transparent)]
        Json(#[from] serde_json::Error),
        #[error("Missing key: {0}")]
        MissingKey(&'static str),
    }

    #[rstest]
    #[case::before_tool(r#"{"session_id": "1", "hook_event_name": "BeforeTool", "tool_name": "test", "tool_input": {}}"#)]
    #[case::after_tool(r#"{"session_id": "1", "hook_event_name": "AfterTool", "tool_name": "test", "tool_input": {}, "tool_response": "response"}"#)]
    #[case::before_agent(r#"{"session_id": "1", "hook_event_name": "BeforeAgent", "agent_name": "a", "prompt": "p"}"#)]
    #[case::after_agent(r#"{"session_id": "1", "hook_event_name": "AfterAgent", "agent_name": "a", "prompt": "p", "result": "r", "prompt_response": "ok"}"#)]
    #[case::before_model(
        r#"{"session_id": "1", "hook_event_name": "BeforeModel", "model": "m", "llm_request": {}}"#
    )]
    #[case::after_model(
        r#"{"session_id": "1", "hook_event_name": "AfterModel", "model": "m", "llm_response": {}}"#
    )]
    #[case::before_tool_selection(
        r#"{"session_id": "1", "hook_event_name": "BeforeToolSelection", "tools": []}"#
    )]
    #[case::session_start(r#"{"session_id": "1", "hook_event_name": "SessionStart"}"#)]
    #[case::session_end(
        r#"{"session_id": "1", "hook_event_name": "SessionEnd", "reason": "done"}"#
    )]
    #[case::notification(
        r#"{"session_id": "1", "hook_event_name": "Notification", "message": "msg"}"#
    )]
    #[case::pre_compress(
        r#"{"session_id": "1", "hook_event_name": "PreCompress", "prompt": "msg", "trigger": "t"}"#
    )]
    fn test_parse_valid_events(#[case] payload: &str) -> Result<(), TestError> {
        let driver = GeminiDriver;
        let conn = inceptool_protocol::from_wire(&driver, payload)?;

        assert_eq!(conn.session.session_id, "1");
        assert_eq!(conn.session.driver, "Gemini");

        Ok(())
    }

    #[rstest]
    fn test_parse_invalid_event() -> Result<(), TestError> {
        let driver = GeminiDriver;
        let result = inceptool_protocol::from_wire(
            &driver,
            r#"{"session_id": "1", "hook_event_name": "Unknown"}"#,
        );

        assert!(result.is_err());

        Ok(())
    }

    #[rstest]
    fn test_format_output_decision() -> Result<(), TestError> {
        let driver = GeminiDriver;
        let output = HookOutputEvent::PreToolUse(PreToolUseOutput {
            decision: Some(Decision::Block),
            ..Default::default()
        });

        let formatted = inceptool_protocol::to_wire(&driver, "BeforeTool", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;

        assert_eq!(
            parsed.get("decision").and_then(|v| v.as_str()),
            Some("block")
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_halt() -> Result<(), TestError> {
        let driver = GeminiDriver;
        let output = HookOutputEvent::PreToolUse(PreToolUseOutput {
            halt: Some(true),
            ..Default::default()
        });

        let formatted = inceptool_protocol::to_wire(&driver, "BeforeTool", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;

        assert_eq!(
            parsed.get("continue").and_then(|v| v.as_bool()),
            Some(false)
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_before_tool() -> Result<(), TestError> {
        let driver = GeminiDriver;
        let mut input_map = serde_json::Map::new();

        input_map.insert("key".to_string(), serde_json::json!("val"));

        let updated_input = serde_json::Value::Object(input_map);
        let output = HookOutputEvent::PreToolUse(PreToolUseOutput {
            updated_input: Some(updated_input),
            ..Default::default()
        });

        let formatted = inceptool_protocol::to_wire(&driver, "BeforeTool", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;

        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("tool_input")
                .and_then(|v| v.get("key"))
                .ok_or(TestError::MissingKey("tool_input key"))?,
            "val"
        );

        Ok(())
    }
}
