//! [`ClaudeDriver`] implements [`inceptool_protocol::Driver`] for Claude Code.
//!
//! - `map_input` reads a small `ClaudeMeta` envelope to determine
//!   `hook_event_name`, retains the full raw JSON as a zero-copy
//!   [`inceptool_protocol::types::RawJson`] `driver_meta`, then
//!   re-deserializes the same JSON into the matching
//!   [`inceptool_protocol::HookInputEvent`] variant. Every `hook_event_name`
//!   Claude Code documents is handled; an unrecognized name yields
//!   [`ProtocolError::UnsupportedEvent`].
//! - `map_output` builds a [`ClaudeOutputWire`] from the
//!   [`inceptool_protocol::HookOutputEvent`]'s generic accessors
//!   (`decision()`, `reason()`, `halt()`, ...), collapsing
//!   `Decision::Deny`/`Decision::Block` to the top-level `"decision": "block"`
//!   and leaving `Allow`/`Ask` to be conveyed via
//!   `hookSpecificOutput.permissionDecision` (see
//!   [`ClaudeHookSpecificOutput`]). `stopReason` and the top-level
//!   `permissionDecision` are always `None`.

use crate::error::ClaudeDriverError;
use crate::types::{ClaudeHookSpecificOutput, ClaudeMeta, ClaudeOutputWire};

use inceptool_protocol::{
    Conn, Decision, Driver, HookInputEvent, HookOutputEvent, ProtocolError, SessionMeta, types,
};

/// Zero-sized [`Driver`] implementation for Claude Code.
///
/// `Self::InputWire<'a> = &'a RawValue` (Claude's hook payload is consumed as
/// borrowed raw JSON), `Self::OutputWire<'a> = ClaudeOutputWire<'a>`, and
/// `Self::Error = ClaudeDriverError`. See the module docs for what
/// `map_input`/`map_output` do.
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
            "PreToolUse" => HookInputEvent::PreToolUse(serde_json::from_str(raw_json)?),
            "PostToolUse" => HookInputEvent::PostToolUse(serde_json::from_str(raw_json)?),
            "UserPromptSubmit" => HookInputEvent::UserPromptSubmit(serde_json::from_str(raw_json)?),
            "SessionStart" => HookInputEvent::SessionStart(serde_json::from_str(raw_json)?),
            "SessionEnd" => HookInputEvent::SessionEnd(serde_json::from_str(raw_json)?),
            "CwdChanged" => HookInputEvent::CwdChanged(serde_json::from_str(raw_json)?),
            "FileChanged" => HookInputEvent::FileChanged(serde_json::from_str(raw_json)?),
            "InstructionsLoaded" => {
                HookInputEvent::InstructionsLoaded(serde_json::from_str(raw_json)?)
            }
            "Setup" => HookInputEvent::Setup(serde_json::from_str(raw_json)?),
            "UserPromptExpansion" => {
                HookInputEvent::UserPromptExpansion(serde_json::from_str(raw_json)?)
            }
            "MessageDisplay" => HookInputEvent::MessageDisplay(serde_json::from_str(raw_json)?),
            "PermissionRequest" => {
                HookInputEvent::PermissionRequest(serde_json::from_str(raw_json)?)
            }
            "PostToolUseFailure" => {
                HookInputEvent::PostToolUseFailure(serde_json::from_str(raw_json)?)
            }
            "PostToolBatch" => HookInputEvent::PostToolBatch(serde_json::from_str(raw_json)?),
            "PermissionDenied" => HookInputEvent::PermissionDenied(serde_json::from_str(raw_json)?),
            "SubagentStart" => HookInputEvent::SubagentStart(serde_json::from_str(raw_json)?),
            "SubagentStop" => HookInputEvent::SubagentStop(serde_json::from_str(raw_json)?),
            "TaskCreated" => HookInputEvent::TaskCreated(serde_json::from_str(raw_json)?),
            "TaskCompleted" => HookInputEvent::TaskCompleted(serde_json::from_str(raw_json)?),
            "Stop" => HookInputEvent::Stop(serde_json::from_str(raw_json)?),
            "StopFailure" => HookInputEvent::StopFailure(serde_json::from_str(raw_json)?),
            "TeammateIdle" => HookInputEvent::TeammateIdle(serde_json::from_str(raw_json)?),
            "ConfigChange" => HookInputEvent::ConfigChange(serde_json::from_str(raw_json)?),
            "PreCompact" => HookInputEvent::PreCompact(serde_json::from_str(raw_json)?),
            "PostCompact" => HookInputEvent::PostCompact(serde_json::from_str(raw_json)?),
            "Elicitation" => HookInputEvent::Elicitation(serde_json::from_str(raw_json)?),
            "ElicitationResult" => {
                HookInputEvent::ElicitationResult(serde_json::from_str(raw_json)?)
            }
            "Notification" => HookInputEvent::Notification(serde_json::from_str(raw_json)?),
            "WorktreeCreate" => HookInputEvent::WorktreeCreate(serde_json::from_str(raw_json)?),
            "WorktreeRemove" => HookInputEvent::WorktreeRemove(serde_json::from_str(raw_json)?),
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
                permission_mode: meta.permission_mode,
                effort: meta.effort,
                agent_id: meta.agent_id,
                agent_type: meta.agent_type,
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

    use inceptool_protocol::PreToolUseOutput;

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
    #[case::post_tool_use(r#"{"session_id": "1", "hook_event_name": "PostToolUse", "tool_name": "test", "tool_input": {}, "tool_output": "test"}"#)]
    #[case::post_tool_use_tool_response_alias(r#"{"session_id": "1", "hook_event_name": "PostToolUse", "tool_name": "test", "tool_input": {}, "tool_response": "test"}"#)]
    #[case::user_prompt_submit(
        r#"{"session_id": "1", "hook_event_name": "UserPromptSubmit", "prompt": "test"}"#
    )]
    #[case::session_start(r#"{"session_id": "1", "hook_event_name": "SessionStart"}"#)]
    #[case::session_end(
        r#"{"session_id": "1", "hook_event_name": "SessionEnd", "reason": "test"}"#
    )]
    #[case::cwd_changed(r#"{"session_id": "1", "hook_event_name": "CwdChanged", "new_cwd": "test", "old_cwd": "old"}"#)]
    #[case::cwd_changed_previous_cwd_alias(r#"{"session_id": "1", "hook_event_name": "CwdChanged", "new_cwd": "test", "previous_cwd": "old"}"#)]
    #[case::file_changed(r#"{"session_id": "1", "hook_event_name": "FileChanged", "file_path": "test", "content": "test"}"#)]
    #[case::instructions_loaded(r#"{"session_id": "1", "hook_event_name": "InstructionsLoaded", "instructions": "test", "file_path": "test", "memory_type": "project", "load_reason": "startup"}"#)]
    #[case::setup(r#"{"session_id": "1", "hook_event_name": "Setup", "trigger": "init"}"#)]
    #[case::user_prompt_expansion(r#"{"session_id": "1", "hook_event_name": "UserPromptExpansion", "command_name": "/review", "prompt": "Review this PR"}"#)]
    #[case::message_display(r#"{"session_id": "1", "hook_event_name": "MessageDisplay", "lines": ["line one", "line two"]}"#)]
    #[case::permission_request(r#"{"session_id": "1", "hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": {"command": "ls"}, "permission_rule_name": "Bash(ls:*)"}"#)]
    #[case::post_tool_use_failure(r#"{"session_id": "1", "hook_event_name": "PostToolUseFailure", "tool_name": "Bash", "tool_input": {"command": "false"}, "tool_error": "exit status 1"}"#)]
    #[case::post_tool_batch(r#"{"session_id": "1", "hook_event_name": "PostToolBatch", "tool_calls": [{"tool_name": "Bash"}]}"#)]
    #[case::permission_denied(r#"{"session_id": "1", "hook_event_name": "PermissionDenied", "tool_name": "Bash", "tool_input": {"command": "curl evil.com"}, "reason": "network access denied"}"#)]
    #[case::subagent_start(r#"{"session_id": "1", "hook_event_name": "SubagentStart", "agent_type": "Explore", "prompt": "Find usages of foo"}"#)]
    #[case::subagent_stop(r#"{"session_id": "1", "hook_event_name": "SubagentStop", "agent_type": "Explore", "result": "Found 3 usages"}"#)]
    #[case::task_created(r#"{"session_id": "1", "hook_event_name": "TaskCreated", "task": {"id": "task-1", "title": "Write tests"}}"#)]
    #[case::task_completed(r#"{"session_id": "1", "hook_event_name": "TaskCompleted", "task": {"id": "task-1", "status": "done"}}"#)]
    #[case::stop(r#"{"session_id": "1", "hook_event_name": "Stop", "message": "All done"}"#)]
    #[case::stop_failure(r#"{"session_id": "1", "hook_event_name": "StopFailure", "error_type": "rate_limit", "error_message": "Too many requests"}"#)]
    #[case::teammate_idle(r#"{"session_id": "1", "hook_event_name": "TeammateIdle", "result": "Implemented feature X"}"#)]
    #[case::config_change(r#"{"session_id": "1", "hook_event_name": "ConfigChange", "config_source": "project_settings", "changed_file": "/repo/.claude/settings.json"}"#)]
    #[case::pre_compact(
        r#"{"session_id": "1", "hook_event_name": "PreCompact", "trigger": "limit"}"#
    )]
    #[case::post_compact(r#"{"session_id": "1", "hook_event_name": "PostCompact", "trigger": "auto", "summary": "Compacted 50 messages"}"#)]
    #[case::elicitation(r#"{"session_id": "1", "hook_event_name": "Elicitation", "server_name": "filesystem", "request": {"prompt": "Confirm overwrite?"}}"#)]
    #[case::elicitation_result(r#"{"session_id": "1", "hook_event_name": "ElicitationResult", "result": {"accepted": true}}"#)]
    #[case::notification(
        r#"{"session_id": "1", "hook_event_name": "Notification", "message": "msg"}"#
    )]
    #[case::worktree_create(r#"{"session_id": "1", "hook_event_name": "WorktreeCreate", "subagent_name": "explorer", "worktree_id": "wt-1", "git_root": "/repo", "parent_path": "/repo/.worktrees/main"}"#)]
    #[case::worktree_remove(
        r#"{"session_id": "1", "hook_event_name": "WorktreeRemove", "worktree_path": "wt"}"#
    )]
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
        let output = HookOutputEvent::PreToolUse(PreToolUseOutput {
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
        let output = HookOutputEvent::PreToolUse(PreToolUseOutput {
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
    fn test_format_output_hook_specific_pre_tool_use() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let mut input_map = serde_json::Map::new();

        input_map.insert("key".to_string(), serde_json::json!("val"));

        let updated_input = serde_json::Value::Object(input_map);

        let output = HookOutputEvent::PreToolUse(PreToolUseOutput {
            decision: Some(Decision::Allow),
            reason: Some("Allowed reason".into()),
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
    fn test_format_output_hook_specific_post_tool_use() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::PostToolUse(inceptool_protocol::PostToolUseOutput {
            additional_context: Some("ctx".into()),
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

    #[rstest]
    fn test_format_output_hook_specific_pre_tool_use_additional_context() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::PreToolUse(PreToolUseOutput {
            additional_context: Some("extra context".into()),
            ..Default::default()
        });

        let formatted = inceptool_protocol::to_wire(&driver, "PreToolUse", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("additionalContext")
                .ok_or(TestError::MissingKey("additionalContext"))?,
            "extra context"
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_post_tool_use_updated_tool_output() -> Result<(), TestError>
    {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::PostToolUse(inceptool_protocol::PostToolUseOutput {
            updated_tool_output: Some(serde_json::json!({"stdout": "ok"})),
            ..Default::default()
        });

        let formatted = inceptool_protocol::to_wire(&driver, "PostToolUse", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("updatedToolOutput")
                .and_then(|v| v.get("stdout"))
                .ok_or(TestError::MissingKey("updatedToolOutput.stdout"))?,
            "ok"
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_session_start() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::SessionStart(inceptool_protocol::SessionStartOutput {
            additional_context: Some("ctx".into()),
            initial_user_message: Some("hello".to_string()),
            session_title: Some("My Session".to_string()),
            watch_paths: Some(vec!["/repo/src".to_string()]),
            reload_skills: Some(true),
            ..Default::default()
        });

        let formatted = inceptool_protocol::to_wire(&driver, "SessionStart", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("hookEventName")
                .ok_or(TestError::MissingKey("hookEventName"))?,
            "SessionStart"
        );
        assert_eq!(
            hook_specific
                .get("initialUserMessage")
                .ok_or(TestError::MissingKey("initialUserMessage"))?,
            "hello"
        );
        assert_eq!(
            hook_specific
                .get("sessionTitle")
                .ok_or(TestError::MissingKey("sessionTitle"))?,
            "My Session"
        );
        assert_eq!(
            hook_specific
                .get("watchPaths")
                .and_then(|v| v.get(0))
                .ok_or(TestError::MissingKey("watchPaths[0]"))?,
            "/repo/src"
        );
        assert_eq!(
            hook_specific
                .get("reloadSkills")
                .ok_or(TestError::MissingKey("reloadSkills"))?,
            true
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_setup() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::Setup(inceptool_protocol::SetupOutput {
            additional_context: Some("setup ctx".into()),
        });

        let formatted = inceptool_protocol::to_wire(&driver, "Setup", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("hookEventName")
                .ok_or(TestError::MissingKey("hookEventName"))?,
            "Setup"
        );
        assert_eq!(
            hook_specific
                .get("additionalContext")
                .ok_or(TestError::MissingKey("additionalContext"))?,
            "setup ctx"
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_permission_request_with_behavior() -> Result<(), TestError>
    {
        let driver = ClaudeDriver;
        let output =
            HookOutputEvent::PermissionRequest(inceptool_protocol::PermissionRequestOutput {
                behavior: Some(inceptool_protocol::PermissionBehavior::Allow),
                ..Default::default()
            });

        let formatted = inceptool_protocol::to_wire(&driver, "PermissionRequest", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("decision")
                .and_then(|v| v.get("behavior"))
                .ok_or(TestError::MissingKey("decision.behavior"))?,
            "allow"
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_permission_request_omits_decision_when_behavior_none()
    -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::PermissionRequest(
            inceptool_protocol::PermissionRequestOutput::default(),
        );

        let formatted = inceptool_protocol::to_wire(&driver, "PermissionRequest", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert!(hook_specific.get("decision").is_none());

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_worktree_create() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::WorktreeCreate(inceptool_protocol::WorktreeCreateOutput {
            worktree_path: Some("/repo/.worktrees/feature".to_string()),
        });

        let formatted = inceptool_protocol::to_wire(&driver, "WorktreeCreate", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("worktreePath")
                .ok_or(TestError::MissingKey("worktreePath"))?,
            "/repo/.worktrees/feature"
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_stop() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::Stop(inceptool_protocol::StopOutput {
            additional_context: Some("keep going".into()),
            ..Default::default()
        });

        let formatted = inceptool_protocol::to_wire(&driver, "Stop", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("additionalContext")
                .ok_or(TestError::MissingKey("additionalContext"))?,
            "keep going"
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_user_prompt_expansion() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output =
            HookOutputEvent::UserPromptExpansion(inceptool_protocol::UserPromptExpansionOutput {
                additional_context: Some("expanded".into()),
                ..Default::default()
            });

        let formatted = inceptool_protocol::to_wire(&driver, "UserPromptExpansion", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("additionalContext")
                .ok_or(TestError::MissingKey("additionalContext"))?,
            "expanded"
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_subagent_start() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::SubagentStart(inceptool_protocol::SubagentStartOutput {
            additional_context: Some("subagent ctx".into()),
        });

        let formatted = inceptool_protocol::to_wire(&driver, "SubagentStart", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("additionalContext")
                .ok_or(TestError::MissingKey("additionalContext"))?,
            "subagent ctx"
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_subagent_stop() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::SubagentStop(inceptool_protocol::SubagentStopOutput {
            additional_context: Some("subagent done".into()),
            ..Default::default()
        });

        let formatted = inceptool_protocol::to_wire(&driver, "SubagentStop", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("additionalContext")
                .ok_or(TestError::MissingKey("additionalContext"))?,
            "subagent done"
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_permission_denied() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output =
            HookOutputEvent::PermissionDenied(inceptool_protocol::PermissionDeniedOutput {
                additional_context: Some("denied ctx".into()),
            });

        let formatted = inceptool_protocol::to_wire(&driver, "PermissionDenied", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("additionalContext")
                .ok_or(TestError::MissingKey("additionalContext"))?,
            "denied ctx"
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_message_display() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::MessageDisplay(inceptool_protocol::MessageDisplayOutput {
            replacement_text: Some("replaced".to_string()),
        });

        let formatted = inceptool_protocol::to_wire(&driver, "MessageDisplay", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("replacementText")
                .ok_or(TestError::MissingKey("replacementText"))?,
            "replaced"
        );

        Ok(())
    }

    #[rstest]
    fn test_format_output_hook_specific_elicitation() -> Result<(), TestError> {
        let driver = ClaudeDriver;
        let output = HookOutputEvent::Elicitation(inceptool_protocol::ElicitationOutput {
            response: Some(serde_json::json!({"accepted": true})),
        });

        let formatted = inceptool_protocol::to_wire(&driver, "Elicitation", &output)?;
        let parsed: serde_json::Value = serde_json::from_str(&formatted)?;
        let hook_specific = parsed
            .get("hookSpecificOutput")
            .ok_or(TestError::MissingKey("hookSpecificOutput"))?;

        assert_eq!(
            hook_specific
                .get("response")
                .and_then(|v| v.get("accepted"))
                .ok_or(TestError::MissingKey("response.accepted"))?,
            true
        );

        Ok(())
    }
}
