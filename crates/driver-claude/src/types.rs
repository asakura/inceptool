//! Wire types for the Claude driver.
//!
//! These model the JSON Claude Code sends to and expects back from a hook,
//! as distinct from the protocol's normalized [`inceptool_protocol::Conn`] /
//! [`inceptool_protocol::HookInputEvent`] /
//! [`inceptool_protocol::HookOutputEvent`] types:
//!
//! - `ClaudeMeta` is a `pub(crate)` "peek" struct:
//!   [`ClaudeDriver::map_input`](crate::driver::ClaudeDriver) deserializes it
//!   first to read the common envelope fields (`session_id`,
//!   `hook_event_name`, `permission_mode`, ...) before re-parsing the same
//!   JSON into the concrete `HookInputEvent` variant.
//! - [`ClaudeOutputWire`] is the top-level JSON object written back to Claude
//!   on stdout (`continue`, `suppressOutput`, `decision`,
//!   `hookSpecificOutput`, ...).
//! - [`ClaudeHookSpecificOutput`] is the `hookSpecificOutput` payload nested
//!   inside [`ClaudeOutputWire`], with one variant per Claude hook phase.
//!   Each variant is produced from the matching protocol output type via a
//!   `From` impl; the `TryFrom<&HookOutputEvent>` impl dispatches on the
//!   `HookOutputEvent` discriminant and returns
//!   [`ConversionError::UnsupportedEvent`] for variants with no
//!   Claude-specific mapping.
//! - [`ClaudePermissionDecision`] is the nested `decision` object for the
//!   `PermissionRequest` hook phase's `hookSpecificOutput`.

use crate::error::{ClaudeDriverError, ConversionError};
use inceptool_protocol::{
    Effort, ElicitationOutput, HookOutputEvent, MessageDisplayOutput, PermissionBehavior,
    PermissionDeniedOutput, PermissionMode, PermissionRequestOutput, PostToolUseOutput,
    PreToolUseOutput, SessionStartOutput, SetupOutput, StopOutput, SubagentStartOutput,
    SubagentStopOutput, UserPromptExpansionOutput, UserPromptSubmitOutput, WorktreeCreateOutput,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Metadata associated with a Claude session payload.
#[derive(Deserialize)]
pub(crate) struct ClaudeMeta<'a> {
    /// The Claude Code session this hook event belongs to.
    pub session_id: Cow<'a, str>,
    /// Path to the session's transcript file, if provided.
    #[serde(default)]
    pub transcript_path: Option<Cow<'a, str>>,
    /// The working directory the session is running in, if provided.
    #[serde(default)]
    pub cwd: Option<Cow<'a, str>>,
    /// The Claude hook name (e.g. `"PreToolUse"`), used to select which
    /// `HookInputEvent` variant to deserialize the payload into.
    pub hook_event_name: Cow<'a, str>,
    /// The permission mode active for the session, if known.
    #[serde(default)]
    pub permission_mode: Option<PermissionMode>,
    /// The reasoning effort configured for the session, if known.
    #[serde(default)]
    pub effort: Option<Effort>,
    /// The identifier of the agent handling this session, if applicable.
    #[serde(default)]
    pub agent_id: Option<Cow<'a, str>>,
    /// The type of agent handling this session, if applicable.
    #[serde(default)]
    pub agent_type: Option<Cow<'a, str>>,
}

/// The top-level JSON object [`ClaudeDriver::map_output`](crate::driver::ClaudeDriver)
/// produces, written back to Claude Code on stdout. All fields are optional
/// and skipped when `None`.
#[derive(Debug, Serialize)]
pub struct ClaudeOutputWire<'a> {
    /// Indicates whether the driver should continue execution. If false, it halts.
    #[serde(rename = "continue")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continue_flag: Option<bool>,
    /// Whether to suppress standard tool output.
    #[serde(rename = "suppressOutput")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress_output: Option<bool>,
    /// The explicit reason for stopping execution, if any. Always `None` in
    /// [`ClaudeDriver::map_output`](crate::driver::ClaudeDriver) - reserved
    /// for schema completeness.
    #[serde(rename = "stopReason")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<&'a str>,
    /// The top-level gatekeeping decision.
    /// [`ClaudeDriver::map_output`](crate::driver::ClaudeDriver) only ever
    /// sets this to `Some("block")` (for `Decision::Deny` /
    /// `Decision::Block`); `Allow`/`Ask` decisions for tool-use hooks are
    /// instead conveyed via `hookSpecificOutput.permissionDecision`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<&'a str>,
    /// The explanation or reason backing the decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'a str>,
    /// An optional system message to display or log.
    #[serde(rename = "systemMessage")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_message: Option<&'a str>,
    /// Top-level permission decision string. Always `None` in
    /// [`ClaudeDriver::map_output`](crate::driver::ClaudeDriver) - per-tool
    /// permission decisions are instead nested under
    /// `hookSpecificOutput.permissionDecision`.
    #[serde(rename = "permissionDecision")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_decision: Option<&'a str>,
    /// Additional hook-specific payload nested within the output wire.
    #[serde(rename = "hookSpecificOutput")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_specific_output: Option<ClaudeHookSpecificOutput<'a>>,
}

/// The nested permission decision for a `PermissionRequest` hook-specific output.
///
/// Only constructed (and thus emitted as `hookSpecificOutput.decision`) when
/// the protocol's `PermissionRequestOutput.behavior` is `Some`; otherwise the
/// whole `decision` field is omitted from `hookSpecificOutput`.
#[derive(Debug, Serialize)]
pub struct ClaudePermissionDecision<'a> {
    /// The behavior to apply to the permission request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior: Option<PermissionBehavior>,
    /// The potentially modified input parameters for the tool.
    #[serde(rename = "updatedInput")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<&'a serde_json::Value>,
    /// An overridden permission rule definition to apply.
    #[serde(rename = "permissionRuleDefinition")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_rule_definition: Option<&'a serde_json::Value>,
}

/// The `hookSpecificOutput` payload nested in [`ClaudeOutputWire`].
///
/// Serialized `#[serde(untagged)]`, so only the active variant's fields
/// appear in the JSON. One variant exists per Claude hook phase that has a
/// defined `hookSpecificOutput` shape; each is constructed from the matching
/// protocol output type via the `From` impls below, dispatched by the
/// `TryFrom<&HookOutputEvent>` impl.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ClaudeHookSpecificOutput<'a> {
    /// Payload structure for the `PreToolUse` hook phase.
    PreToolUse {
        /// The name of the hook event (always "`PreToolUse`").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// The permission decision generated for the proposed tool use.
        #[serde(rename = "permissionDecision")]
        #[serde(skip_serializing_if = "Option::is_none")]
        permission_decision: Option<inceptool_protocol::Decision>,
        /// The justification for the permission decision.
        #[serde(rename = "permissionDecisionReason")]
        #[serde(skip_serializing_if = "Option::is_none")]
        permission_decision_reason: Option<&'a str>,
        /// The potentially modified input parameters for the tool.
        #[serde(rename = "updatedInput")]
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_input: Option<&'a serde_json::Value>,
        /// Additional context injected into the prompt.
        #[serde(rename = "additionalContext")]
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<&'a str>,
    },
    /// Payload structure for the `PostToolUse` hook phase.
    PostToolUse {
        /// The name of the hook event (always "`PostToolUse`").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// Additional context appended after tool execution.
        #[serde(rename = "additionalContext")]
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<&'a str>,
        /// Overridden output returned to the model.
        #[serde(rename = "updatedToolOutput")]
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_tool_output: Option<&'a serde_json::Value>,
    },
    /// Payload structure for the user prompt submission hook phase.
    UserPromptSubmit {
        /// The name of the hook event (always "`UserPromptSubmit`").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// Context injected alongside the user's prompt submission.
        #[serde(rename = "additionalContext")]
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<&'a str>,
    },
    /// Payload structure for the `SessionStart` hook phase.
    SessionStart {
        /// The name of the hook event (always "`SessionStart`").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// Additional context to start the session with.
        #[serde(rename = "additionalContext")]
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<&'a str>,
        /// An initial user message to seed the new session with.
        #[serde(rename = "initialUserMessage")]
        #[serde(skip_serializing_if = "Option::is_none")]
        initial_user_message: Option<&'a str>,
        /// A title to assign to the new session.
        #[serde(rename = "sessionTitle")]
        #[serde(skip_serializing_if = "Option::is_none")]
        session_title: Option<&'a str>,
        /// Additional paths to watch for the duration of the session.
        #[serde(rename = "watchPaths")]
        #[serde(skip_serializing_if = "Option::is_none")]
        watch_paths: Option<&'a [String]>,
        /// Whether to reload skills for the new session.
        #[serde(rename = "reloadSkills")]
        #[serde(skip_serializing_if = "Option::is_none")]
        reload_skills: Option<bool>,
    },
    /// Payload structure for the `Setup` hook phase.
    Setup {
        /// The name of the hook event (always "Setup").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// Additional context to inject during setup.
        #[serde(rename = "additionalContext")]
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<&'a str>,
    },
    /// Payload structure for the `PermissionRequest` hook phase.
    PermissionRequest {
        /// The name of the hook event (always "`PermissionRequest`").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// The permission decision for the request, if any.
        #[serde(skip_serializing_if = "Option::is_none")]
        decision: Option<ClaudePermissionDecision<'a>>,
    },
    /// Payload structure for the `WorktreeCreate` hook phase.
    WorktreeCreate {
        /// The name of the hook event (always "`WorktreeCreate`").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// The path to the created worktree, if modified by the hook.
        #[serde(rename = "worktreePath")]
        #[serde(skip_serializing_if = "Option::is_none")]
        worktree_path: Option<&'a str>,
    },
    /// Payload structure for the `Stop` hook phase.
    Stop {
        /// The name of the hook event (always "Stop").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// Additional context to continue with if the agent does not stop.
        #[serde(rename = "additionalContext")]
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<&'a str>,
    },
    /// Payload structure for the `UserPromptExpansion` hook phase.
    UserPromptExpansion {
        /// The name of the hook event (always "`UserPromptExpansion`").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// Additional context to append as a result of the expansion.
        #[serde(rename = "additionalContext")]
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<&'a str>,
    },
    /// Payload structure for the `SubagentStart` hook phase.
    SubagentStart {
        /// The name of the hook event (always "`SubagentStart`").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// Additional context to inject for the spawned subagent.
        #[serde(rename = "additionalContext")]
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<&'a str>,
    },
    /// Payload structure for the `SubagentStop` hook phase.
    SubagentStop {
        /// The name of the hook event (always "`SubagentStop`").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// Additional context to inject as a result of the subagent's run.
        #[serde(rename = "additionalContext")]
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<&'a str>,
    },
    /// Payload structure for the `PermissionDenied` hook phase.
    PermissionDenied {
        /// The name of the hook event (always "`PermissionDenied`").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// Additional context to inject after the denial.
        #[serde(rename = "additionalContext")]
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<&'a str>,
    },
    /// Payload structure for the `MessageDisplay` hook phase.
    MessageDisplay {
        /// The name of the hook event (always "`MessageDisplay`").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// Replacement text for the displayed message.
        #[serde(rename = "replacementText")]
        #[serde(skip_serializing_if = "Option::is_none")]
        replacement_text: Option<&'a str>,
    },
    /// Payload structure for the `Elicitation` hook phase.
    Elicitation {
        /// The name of the hook event (always "Elicitation").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// The response to send back to the MCP server.
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<&'a serde_json::Value>,
    },
}

impl<'a> From<&'a PreToolUseOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a PreToolUseOutput) -> Self {
        ClaudeHookSpecificOutput::PreToolUse {
            hook_event_name: "PreToolUse",
            permission_decision: o.decision,
            permission_decision_reason: o.reason.as_deref(),
            updated_input: o.updated_input.as_ref(),
            additional_context: o.additional_context.as_deref(),
        }
    }
}

impl<'a> From<&'a PostToolUseOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a PostToolUseOutput) -> Self {
        ClaudeHookSpecificOutput::PostToolUse {
            hook_event_name: "PostToolUse",
            additional_context: o.additional_context.as_deref(),
            updated_tool_output: o.updated_tool_output.as_ref(),
        }
    }
}

impl<'a> From<&'a UserPromptSubmitOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a UserPromptSubmitOutput) -> Self {
        ClaudeHookSpecificOutput::UserPromptSubmit {
            hook_event_name: "UserPromptSubmit",
            additional_context: o.additional_context.as_deref(),
        }
    }
}

impl<'a> From<&'a SessionStartOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a SessionStartOutput) -> Self {
        ClaudeHookSpecificOutput::SessionStart {
            hook_event_name: "SessionStart",
            additional_context: o.additional_context.as_deref(),
            initial_user_message: o.initial_user_message.as_deref(),
            session_title: o.session_title.as_deref(),
            watch_paths: o.watch_paths.as_deref(),
            reload_skills: o.reload_skills,
        }
    }
}

impl<'a> From<&'a SetupOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a SetupOutput) -> Self {
        ClaudeHookSpecificOutput::Setup {
            hook_event_name: "Setup",
            additional_context: o.additional_context.as_deref(),
        }
    }
}

impl<'a> From<&'a PermissionRequestOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a PermissionRequestOutput) -> Self {
        let decision = o.behavior.map(|behavior| ClaudePermissionDecision {
            behavior: Some(behavior),
            updated_input: o.updated_input.as_ref(),
            permission_rule_definition: o.permission_rule_definition.as_ref(),
        });

        ClaudeHookSpecificOutput::PermissionRequest {
            hook_event_name: "PermissionRequest",
            decision,
        }
    }
}

impl<'a> From<&'a WorktreeCreateOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a WorktreeCreateOutput) -> Self {
        ClaudeHookSpecificOutput::WorktreeCreate {
            hook_event_name: "WorktreeCreate",
            worktree_path: o.worktree_path.as_deref(),
        }
    }
}

impl<'a> From<&'a StopOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a StopOutput) -> Self {
        ClaudeHookSpecificOutput::Stop {
            hook_event_name: "Stop",
            additional_context: o.additional_context.as_deref(),
        }
    }
}

impl<'a> From<&'a UserPromptExpansionOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a UserPromptExpansionOutput) -> Self {
        ClaudeHookSpecificOutput::UserPromptExpansion {
            hook_event_name: "UserPromptExpansion",
            additional_context: o.additional_context.as_deref(),
        }
    }
}

impl<'a> From<&'a SubagentStartOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a SubagentStartOutput) -> Self {
        ClaudeHookSpecificOutput::SubagentStart {
            hook_event_name: "SubagentStart",
            additional_context: o.additional_context.as_deref(),
        }
    }
}

impl<'a> From<&'a SubagentStopOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a SubagentStopOutput) -> Self {
        ClaudeHookSpecificOutput::SubagentStop {
            hook_event_name: "SubagentStop",
            additional_context: o.additional_context.as_deref(),
        }
    }
}

impl<'a> From<&'a PermissionDeniedOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a PermissionDeniedOutput) -> Self {
        ClaudeHookSpecificOutput::PermissionDenied {
            hook_event_name: "PermissionDenied",
            additional_context: o.additional_context.as_deref(),
        }
    }
}

impl<'a> From<&'a MessageDisplayOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a MessageDisplayOutput) -> Self {
        ClaudeHookSpecificOutput::MessageDisplay {
            hook_event_name: "MessageDisplay",
            replacement_text: o.replacement_text.as_deref(),
        }
    }
}

impl<'a> From<&'a ElicitationOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a ElicitationOutput) -> Self {
        ClaudeHookSpecificOutput::Elicitation {
            hook_event_name: "Elicitation",
            response: o.response.as_ref(),
        }
    }
}

/// Dispatches on the `HookOutputEvent` discriminant to the matching
/// `From<&XxxOutput>` impl above, producing the Claude-specific
/// `hookSpecificOutput` payload for that hook phase.
///
/// Returns [`ConversionError::UnsupportedEvent`] for variants with no
/// Claude-specific mapping (e.g. `Notification`, `SessionEnd`,
/// `CwdChanged`). [`ClaudeDriver::map_output`](crate::driver::ClaudeDriver)
/// treats that as "no `hookSpecificOutput`" via `.ok()` rather than
/// propagating the error.
impl<'a> TryFrom<&'a HookOutputEvent> for ClaudeHookSpecificOutput<'a> {
    type Error = ClaudeDriverError;

    fn try_from(output: &'a HookOutputEvent) -> Result<Self, Self::Error> {
        match output {
            HookOutputEvent::PreToolUse(o) => Ok(o.into()),
            HookOutputEvent::PostToolUse(o) => Ok(o.into()),
            HookOutputEvent::UserPromptSubmit(o) => Ok(o.into()),
            HookOutputEvent::SessionStart(o) => Ok(o.into()),
            HookOutputEvent::Setup(o) => Ok(o.into()),
            HookOutputEvent::PermissionRequest(o) => Ok(o.into()),
            HookOutputEvent::WorktreeCreate(o) => Ok(o.into()),
            HookOutputEvent::Stop(o) => Ok(o.into()),
            HookOutputEvent::UserPromptExpansion(o) => Ok(o.into()),
            HookOutputEvent::SubagentStart(o) => Ok(o.into()),
            HookOutputEvent::SubagentStop(o) => Ok(o.into()),
            HookOutputEvent::PermissionDenied(o) => Ok(o.into()),
            HookOutputEvent::MessageDisplay(o) => Ok(o.into()),
            HookOutputEvent::Elicitation(o) => Ok(o.into()),
            e => Err(ConversionError::UnsupportedEvent(e.into()).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use inceptool_protocol::{
        Decision, HookOutputEvent, NotificationOutput, PostToolUseOutput, PreToolUseOutput,
        UserPromptSubmitOutput,
    };

    use core::assert_matches;

    #[test]
    fn claude_hook_specific_output_from_pre_tool_use() {
        let o = PreToolUseOutput {
            decision: Some(Decision::Allow),
            ..Default::default()
        };
        let h: ClaudeHookSpecificOutput<'_> = (&o).into();

        assert_matches!(
            h,
            ClaudeHookSpecificOutput::PreToolUse {
                permission_decision: Some(Decision::Allow),
                ..
            }
        );

        let e = HookOutputEvent::PreToolUse(o);
        assert!(ClaudeHookSpecificOutput::try_from(&e).is_ok());
    }

    #[test]
    fn claude_hook_specific_output_from_post_tool_use() {
        let o = PostToolUseOutput {
            additional_context: Some("ctx".into()),
            ..Default::default()
        };
        let h: ClaudeHookSpecificOutput<'_> = (&o).into();

        assert_matches!(
            h,
            ClaudeHookSpecificOutput::PostToolUse {
                additional_context: Some("ctx"),
                ..
            }
        );

        let e = HookOutputEvent::PostToolUse(o);
        assert!(ClaudeHookSpecificOutput::try_from(&e).is_ok());
    }

    #[test]
    fn claude_hook_specific_output_from_user_prompt_submit() {
        let o = UserPromptSubmitOutput {
            additional_context: Some("ctx".into()),
            ..Default::default()
        };
        let h: ClaudeHookSpecificOutput<'_> = (&o).into();

        assert_matches!(
            h,
            ClaudeHookSpecificOutput::UserPromptSubmit {
                additional_context: Some("ctx"),
                ..
            }
        );

        let e = HookOutputEvent::UserPromptSubmit(o);
        assert!(ClaudeHookSpecificOutput::try_from(&e).is_ok());
    }

    #[test]
    fn claude_hook_specific_output_try_from_err() {
        let e_err = HookOutputEvent::Notification(NotificationOutput::default());
        assert!(ClaudeHookSpecificOutput::try_from(&e_err).is_err());
    }
}
