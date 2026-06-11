//! Implementation of the Driver trait for Claude.

use crate::error::ClaudeDriverError;
use crate::types::{ClaudeHookSpecificOutput, ClaudeMeta, ClaudeOutputWire};

use inceptool_protocol::{
    Conn, Decision, Driver, HookInputEvent, HookOutputEvent, ProtocolError, SessionMeta, types,
};

/// Implements `Driver` for Claude Code.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeDriver;

impl Driver for ClaudeDriver {
    type Error = ClaudeDriverError;
    type InputWire<'a> = &'a serde_json::value::RawValue;
    type OutputWire<'a> = ClaudeOutputWire<'a>;

    fn map_input<'a>(&self, wire: Self::InputWire<'a>) -> Result<Conn<'a>, Self::Error> {
        let raw_json = wire.get();
        let meta: ClaudeMeta<'a> = serde_json::from_str(raw_json)?;
        let raw_value: &'a serde_json::value::RawValue = serde_json::from_str(raw_json)?;

        let event = match meta.hook_event_name.as_ref() {
            "PreToolUse" => HookInputEvent::BeforeTool(serde_json::from_str(raw_json)?),
            "PostToolUse" => HookInputEvent::AfterTool(serde_json::from_str(raw_json)?),
            "UserPromptSubmit" => HookInputEvent::UserPromptSubmit(serde_json::from_str(raw_json)?),
            "SessionStart" => HookInputEvent::SessionStart(serde_json::from_str(raw_json)?),
            "CwdChanged" => HookInputEvent::CwdChanged(serde_json::from_str(raw_json)?),
            "FileChanged" => HookInputEvent::FileChanged(serde_json::from_str(raw_json)?),
            "InstructionsLoaded" => {
                HookInputEvent::InstructionsLoaded(serde_json::from_str(raw_json)?)
            }
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
                timestamp: None, // Claude Code does not provide a timestamp
                driver: "Claude".into(),
                driver_meta: Some(types::RawJson(raw_value)),
            },
            event,
        })
    }

    fn map_output<'a>(
        &self,
        _event_name: &'a str,
        output: &'a HookOutputEvent,
    ) -> Result<Self::OutputWire<'a>, Self::Error> {
        let hook_specific_output = ClaudeHookSpecificOutput::try_from(output).ok();

        let wire = ClaudeOutputWire {
            continue_flag: output.halt().map(|h| !h),
            suppress_output: output.suppress_output(),
            stop_reason: None,
            decision: match output.decision() {
                Some(Decision::Deny) | Some(Decision::Block) => Some("block"),
                _ => None,
            },
            reason: output.reason(),
            system_message: output.system_message(),
            permission_decision: None,
            hook_specific_output,
        };

        Ok(wire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use inceptool_protocol::BeforeToolOutput;

    use rstest::rstest;

    #[derive(thiserror::Error, Debug)]
    pub enum TestError {
        #[error(transparent)]
        Protocol(#[from] ProtocolError),
        #[error(transparent)]
        Driver(#[from] ClaudeDriverError),
        #[error(transparent)]
        Json(#[from] serde_json::Error),
        #[error("Missing key: {0}")]
        MissingKey(&'static str),
    }

    #[rstest]
    #[case::pre_tool_use(r#"{"session_id": "1", "hook_event_name": "PreToolUse", "tool_name": "test", "tool_input": {}}"#)]
    #[case::post_tool_use(r#"{"session_id": "1", "hook_event_name": "PostToolUse", "tool_name": "test", "tool_input": {}, "tool_response": "test"}"#)]
    #[case::user_prompt_submit(
        r#"{"session_id": "1", "hook_event_name": "UserPromptSubmit", "prompt": "test"}"#
    )]
    #[case::session_start(r#"{"session_id": "1", "hook_event_name": "SessionStart"}"#)]
    #[case::cwd_changed(r#"{"session_id": "1", "hook_event_name": "CwdChanged", "new_cwd": "test", "previous_cwd": "old"}"#)]
    #[case::file_changed(r#"{"session_id": "1", "hook_event_name": "FileChanged", "file_path": "test", "content": "test"}"#)]
    #[case::instructions_loaded(r#"{"session_id": "1", "hook_event_name": "InstructionsLoaded", "instructions": "test", "file_path": "test", "memory_type": "project", "load_reason": "startup"}"#)]
    fn test_parse_valid_events(#[case] payload: &str) -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let conn = inceptool_protocol::from_wire(&driver, payload)?;

        assert_eq!(conn.session.session_id, "1");
        assert_eq!(conn.session.driver, "Claude");

        Ok(())
    }

    #[rstest]
    fn test_parse_invalid_event() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let result = inceptool_protocol::from_wire(
            &driver,
            r#"{"session_id": "1", "hook_event_name": "Unknown"}"#,
        );

        assert!(result.is_err());

        Ok(())
    }

    #[rstest]
    fn test_format_output_decision_block() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::BeforeTool(BeforeToolOutput {
            decision: Some(Decision::Block),
            ..Default::default()
        });

        let formatted = inceptool_protocol::to_wire(&driver, "PreToolUse", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;

        assert_eq!(
            parsed.get("decision").and_then(|v| v.as_str()),
            Some("block")
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_halt() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::BeforeTool(BeforeToolOutput {
            halt: Some(true),
            ..Default::default()
        });

        let formatted = inceptool_protocol::to_wire(&driver, "PreToolUse", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;

        assert_eq!(
            parsed.get("continue").and_then(|v| v.as_bool()),
            Some(false)
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_before_tool() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let mut input_map = serde_json::Map::new();

        input_map.insert("key".to_string(), serde_json::json!("val"));

        let updated_input = serde_json::Value::Object(input_map);

        let output = HookOutputEvent::BeforeTool(BeforeToolOutput {
            decision: Some(Decision::Allow),
            reason: Some("Allowed reason".to_string()),
            updated_input: Some(updated_input),
            ..Default::default()
        });

        let formatted = inceptool_protocol::to_wire(&driver, "PreToolUse", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("hookEventName")
                .ok_or(TestError::MissingKey("hookEventName"))?,
            "PreToolUse"
        );
        assert_eq!(
            hook_specific
                .get("permissionDecision")
                .ok_or(TestError::MissingKey("permissionDecision"))?,
            "allow"
        );
        assert_eq!(
            hook_specific
                .get("permissionDecisionReason")
                .ok_or(TestError::MissingKey("permissionDecisionReason"))?,
            "Allowed reason"
        );
        assert_eq!(
            hook_specific
                .get("updatedInput")
                .and_then(|v| v.get("key"))
                .ok_or(TestError::MissingKey("updatedInput key"))?,
            "val"
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_after_tool() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::AfterTool(inceptool_protocol::AfterToolOutput {
            additional_context: Some("ctx".to_string()),
            ..Default::default()
        });

        let formatted = inceptool_protocol::to_wire(&driver, "PostToolUse", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;

        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("hookEventName")
                .ok_or(TestError::MissingKey("hookEventName"))?,
            "PostToolUse"
        );
        assert_eq!(
            hook_specific
                .get("additionalContext")
                .ok_or(TestError::MissingKey("additionalContext"))?,
            "ctx"
        );

        Ok(())
    }
}
