//! Type definitions for the Claude driver.

use crate::error::{ClaudeDriverError, ConversionError};
use inceptool_protocol::{
    AfterToolOutput, BeforeToolOutput, HookOutputEvent, UserPromptSubmitOutput,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Metadata associated with a Claude session payload.
#[derive(Deserialize)]
pub(crate) struct ClaudeMeta<'a> {
    pub(crate) session_id: Cow<'a, str>,
    #[serde(default)]
    pub(crate) transcript_path: Option<Cow<'a, str>>,
    #[serde(default)]
    pub(crate) cwd: Option<Cow<'a, str>>,
    pub(crate) hook_event_name: Cow<'a, str>,
}

/// The output wire format for Claude driver.
#[derive(Serialize)]
pub struct ClaudeOutputWire<'a> {
    /// Indicates whether the driver should continue execution. If false, it halts.
    #[serde(rename = "continue")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continue_flag: Option<bool>,
    /// Whether to suppress standard tool output.
    #[serde(rename = "suppressOutput")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress_output: Option<bool>,
    /// The explicit reason for stopping execution, if any.
    #[serde(rename = "stopReason")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<&'a str>,
    /// The decision rendered by a gatekeeping hook (e.g., "block").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<&'a str>,
    /// The explanation or reason backing the decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'a str>,
    /// An optional system message to display or log.
    #[serde(rename = "systemMessage")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_message: Option<&'a str>,
    /// Specific permission decision for operations requiring authorization.
    #[serde(rename = "permissionDecision")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_decision: Option<&'a str>,
    /// Additional hook-specific payload nested within the output wire.
    #[serde(rename = "hookSpecificOutput")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_specific_output: Option<ClaudeHookSpecificOutput<'a>>,
}

/// Hook-specific output payload for Claude.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ClaudeHookSpecificOutput<'a> {
    /// Payload structure for the `PreToolUse` hook phase.
    BeforeTool {
        /// The name of the hook event (always "PreToolUse").
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
    },
    /// Payload structure for the `PostToolUse` hook phase.
    AfterTool {
        /// The name of the hook event (always "PostToolUse").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// Additional context appended after tool execution.
        #[serde(rename = "additionalContext")]
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<&'a str>,
    },
    /// Payload structure for the user prompt submission hook phase.
    UserPromptSubmit {
        /// The name of the hook event (always "UserPromptSubmit").
        #[serde(rename = "hookEventName")]
        hook_event_name: &'a str,
        /// Context injected alongside the user's prompt submission.
        #[serde(rename = "additionalContext")]
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_context: Option<&'a str>,
    },
}

impl<'a> From<&'a BeforeToolOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a BeforeToolOutput) -> Self {
        ClaudeHookSpecificOutput::BeforeTool {
            hook_event_name: "PreToolUse",
            permission_decision: o.decision,
            permission_decision_reason: o.reason.as_deref(),
            updated_input: o.updated_input.as_ref(),
        }
    }
}

impl<'a> From<&'a AfterToolOutput> for ClaudeHookSpecificOutput<'a> {
    fn from(o: &'a AfterToolOutput) -> Self {
        ClaudeHookSpecificOutput::AfterTool {
            hook_event_name: "PostToolUse",
            additional_context: o.additional_context.as_deref(),
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

impl<'a> TryFrom<&'a HookOutputEvent> for ClaudeHookSpecificOutput<'a> {
    type Error = ClaudeDriverError;

    fn try_from(output: &'a HookOutputEvent) -> Result<Self, Self::Error> {
        match output {
            HookOutputEvent::BeforeTool(o) => Ok(o.into()),
            HookOutputEvent::AfterTool(o) => Ok(o.into()),
            HookOutputEvent::UserPromptSubmit(o) => Ok(o.into()),
            e => Err(ConversionError::UnsupportedEvent(e.into()).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use inceptool_protocol::{
        AfterToolOutput, BeforeToolOutput, Decision, HookOutputEvent, UserPromptSubmitOutput,
    };

    use core::assert_matches;

    #[test]
    fn test_claude_hook_specific_output_from_before_tool() {
        let o = BeforeToolOutput {
            decision: Some(Decision::Allow),
            ..Default::default()
        };
        let h: ClaudeHookSpecificOutput = (&o).into();

        assert_matches!(
            h,
            ClaudeHookSpecificOutput::BeforeTool {
                permission_decision: Some(Decision::Allow),
                ..
            }
        );

        let e = HookOutputEvent::BeforeTool(o);
        assert!(ClaudeHookSpecificOutput::try_from(&e).is_ok());
    }

    #[test]
    fn test_claude_hook_specific_output_from_after_tool() {
        let o = AfterToolOutput {
            additional_context: Some("ctx".into()),
            ..Default::default()
        };
        let h: ClaudeHookSpecificOutput = (&o).into();

        assert_matches!(
            h,
            ClaudeHookSpecificOutput::AfterTool {
                additional_context: Some("ctx"),
                ..
            }
        );

        let e = HookOutputEvent::AfterTool(o);
        assert!(ClaudeHookSpecificOutput::try_from(&e).is_ok());
    }

    #[test]
    fn test_claude_hook_specific_output_from_user_prompt_submit() {
        let o = UserPromptSubmitOutput {
            additional_context: Some("ctx".into()),
            ..Default::default()
        };
        let h: ClaudeHookSpecificOutput = (&o).into();

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
    fn test_claude_hook_specific_output_try_from_err() {
        let e_err = HookOutputEvent::Notification(Default::default());
        assert!(ClaudeHookSpecificOutput::try_from(&e_err).is_err());
    }
}
