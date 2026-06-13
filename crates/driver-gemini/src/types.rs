//! Type definitions for the Gemini driver.

use crate::error::{ConversionError, GeminiDriverError};

use inceptool_protocol::{
    AfterAgentOutput, AfterModelOutput, BeforeAgentOutput, BeforeModelOutput,
    BeforeToolSelectionOutput, HookOutputEvent, PostToolUseOutput, PreToolUseOutput,
    SessionStartOutput,
};

use serde::{Deserialize, Serialize};

use std::borrow::Cow;

/// Metadata associated with a Gemini session payload.
#[derive(Deserialize)]
pub(crate) struct GeminiMeta<'a> {
    pub(crate) session_id: Cow<'a, str>,
    #[serde(default)]
    pub(crate) transcript_path: Option<Cow<'a, str>>,
    #[serde(default)]
    pub(crate) cwd: Option<Cow<'a, str>>,
    pub(crate) hook_event_name: Cow<'a, str>,
    #[serde(default)]
    pub(crate) timestamp: Option<Cow<'a, str>>,
}

/// The output wire format for the Gemini driver.
#[derive(Debug, Serialize)]
pub struct GeminiOutputWire<'a> {
    /// The decision rendered by a gatekeeping hook (e.g., allow, block).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<inceptool_protocol::Decision>,
    /// The explanation or reason backing the decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'a str>,
    /// Indicates whether the driver should continue execution. If false, it halts.
    #[serde(rename = "continue")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continue_flag: Option<bool>,
    /// Whether to suppress standard tool output.
    #[serde(rename = "suppressOutput")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress_output: Option<bool>,
    /// An optional system message to display or log.
    #[serde(rename = "systemMessage")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_message: Option<&'a str>,
    /// Additional hook-specific payload nested within the output wire.
    #[serde(rename = "hookSpecificOutput")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_specific_output: Option<GeminiHookSpecificOutput<'a>>,
}

/// Hook-specific output payload for Gemini.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum GeminiHookSpecificOutput<'a> {
    /// Payload structure prior to tool invocation.
    BeforeTool {
        /// The potentially modified input parameters for the tool.
        #[serde(rename = "tool_input")]
        #[serde(skip_serializing_if = "Option::is_none")]
        updated_input: Option<&'a serde_json::Value>,
    },
    /// Payload structure after tool invocation.
    AfterTool {
        /// The updated or finalized output resulting from the tool's execution.
        #[serde(rename = "updatedToolOutput")]
        updated_tool_output: &'a serde_json::Value,
    },
    /// Payload structure before delegating to an agent.
    BeforeAgent {
        /// Supplemental context provided to the agent before it begins.
        #[serde(rename = "additionalContext")]
        additional_context: &'a str,
    },
    /// Payload structure after an agent completes its task.
    AfterAgent {
        /// Flag indicating whether the agent's context window should be cleared.
        #[serde(rename = "clearContext")]
        clear_context: bool,
    },
    /// Payload structure before making an LLM request.
    BeforeModel {
        /// The potentially modified model request payload.
        #[serde(rename = "llm_request")]
        #[serde(skip_serializing_if = "Option::is_none")]
        llm_request: Option<&'a serde_json::Value>,
        /// A mocked or overridden model response to be injected.
        #[serde(rename = "llm_response")]
        #[serde(skip_serializing_if = "Option::is_none")]
        llm_response: Option<&'a serde_json::Value>,
    },
    /// Payload structure after receiving an LLM response.
    AfterModel {
        /// The potentially modified model response payload.
        #[serde(rename = "llm_response")]
        llm_response: &'a serde_json::Value,
    },
    /// Payload structure before evaluating tool selections.
    BeforeToolSelection {
        /// The configuration describing the available tools.
        #[serde(rename = "toolConfig")]
        tool_config: &'a serde_json::Value,
    },
    /// Payload structure for when a session initializes.
    SessionStart {
        /// Supplemental context to prime the session state.
        #[serde(rename = "additionalContext")]
        additional_context: &'a str,
    },
}

impl<'a> TryFrom<&'a PreToolUseOutput> for GeminiHookSpecificOutput<'a> {
    type Error = GeminiDriverError;

    fn try_from(o: &'a PreToolUseOutput) -> Result<Self, Self::Error> {
        if o.updated_input.is_some() {
            Ok(GeminiHookSpecificOutput::BeforeTool {
                updated_input: o.updated_input.as_ref(),
            })
        } else {
            Err(ConversionError::MissingUpdatedInput.into())
        }
    }
}

impl<'a> TryFrom<&'a PostToolUseOutput> for GeminiHookSpecificOutput<'a> {
    type Error = GeminiDriverError;

    fn try_from(o: &'a PostToolUseOutput) -> Result<Self, Self::Error> {
        o.updated_tool_output
            .as_ref()
            .map(|v| GeminiHookSpecificOutput::AfterTool {
                updated_tool_output: v,
            })
            .ok_or(ConversionError::MissingUpdatedToolOutput.into())
    }
}

impl<'a> TryFrom<&'a BeforeAgentOutput> for GeminiHookSpecificOutput<'a> {
    type Error = GeminiDriverError;

    fn try_from(o: &'a BeforeAgentOutput) -> Result<Self, Self::Error> {
        o.additional_context
            .as_deref()
            .map(|s| GeminiHookSpecificOutput::BeforeAgent {
                additional_context: s,
            })
            .ok_or(ConversionError::MissingAdditionalContext.into())
    }
}

impl<'a> TryFrom<&'a AfterAgentOutput> for GeminiHookSpecificOutput<'a> {
    type Error = GeminiDriverError;

    fn try_from(o: &'a AfterAgentOutput) -> Result<Self, Self::Error> {
        if o.clear_context == Some(true) {
            Ok(GeminiHookSpecificOutput::AfterAgent {
                clear_context: true,
            })
        } else {
            Err(ConversionError::ClearContextNotTrue.into())
        }
    }
}

impl<'a> TryFrom<&'a BeforeModelOutput> for GeminiHookSpecificOutput<'a> {
    type Error = GeminiDriverError;

    fn try_from(o: &'a BeforeModelOutput) -> Result<Self, Self::Error> {
        if o.llm_request.is_some() || o.llm_response.is_some() {
            Ok(GeminiHookSpecificOutput::BeforeModel {
                llm_request: o.llm_request.as_ref(),
                llm_response: o.llm_response.as_ref(),
            })
        } else {
            Err(ConversionError::MissingLlmRequestAndResponse.into())
        }
    }
}

impl<'a> TryFrom<&'a AfterModelOutput> for GeminiHookSpecificOutput<'a> {
    type Error = GeminiDriverError;

    fn try_from(o: &'a AfterModelOutput) -> Result<Self, Self::Error> {
        o.llm_response
            .as_ref()
            .map(|v| GeminiHookSpecificOutput::AfterModel { llm_response: v })
            .ok_or(ConversionError::MissingLlmResponse.into())
    }
}

impl<'a> TryFrom<&'a BeforeToolSelectionOutput> for GeminiHookSpecificOutput<'a> {
    type Error = GeminiDriverError;

    fn try_from(o: &'a BeforeToolSelectionOutput) -> Result<Self, Self::Error> {
        o.tool_config
            .as_ref()
            .map(|v| GeminiHookSpecificOutput::BeforeToolSelection { tool_config: v })
            .ok_or(ConversionError::MissingToolConfig.into())
    }
}

impl<'a> TryFrom<&'a SessionStartOutput> for GeminiHookSpecificOutput<'a> {
    type Error = GeminiDriverError;

    fn try_from(o: &'a SessionStartOutput) -> Result<Self, Self::Error> {
        o.additional_context
            .as_deref()
            .map(|s| GeminiHookSpecificOutput::SessionStart {
                additional_context: s,
            })
            .ok_or(ConversionError::MissingAdditionalContext.into())
    }
}

impl<'a> TryFrom<&'a HookOutputEvent> for GeminiHookSpecificOutput<'a> {
    type Error = GeminiDriverError;

    fn try_from(output: &'a HookOutputEvent) -> Result<Self, Self::Error> {
        match output {
            HookOutputEvent::PreToolUse(o) => o.try_into(),
            HookOutputEvent::PostToolUse(o) => o.try_into(),
            HookOutputEvent::BeforeAgent(o) => o.try_into(),
            HookOutputEvent::AfterAgent(o) => o.try_into(),
            HookOutputEvent::BeforeModel(o) => o.try_into(),
            HookOutputEvent::AfterModel(o) => o.try_into(),
            HookOutputEvent::BeforeToolSelection(o) => o.try_into(),
            HookOutputEvent::SessionStart(o) => o.try_into(),
            e => Err(ConversionError::UnsupportedEvent(e.into()).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use inceptool_protocol::NotificationOutput;

    #[test]
    fn test_gemini_hook_specific_output_before_tool() {
        let o = PreToolUseOutput {
            updated_input: Some(serde_json::json!({})),
            ..Default::default()
        };

        assert!(GeminiHookSpecificOutput::try_from(&o).is_ok());
        assert!(GeminiHookSpecificOutput::try_from(&HookOutputEvent::PreToolUse(o)).is_ok());

        let o_err = PreToolUseOutput::default();
        assert!(GeminiHookSpecificOutput::try_from(&o_err).is_err());
    }

    #[test]
    fn test_gemini_hook_specific_output_after_tool() {
        let o = PostToolUseOutput {
            updated_tool_output: Some(serde_json::json!({})),
            ..Default::default()
        };

        assert!(GeminiHookSpecificOutput::try_from(&o).is_ok());
        assert!(GeminiHookSpecificOutput::try_from(&HookOutputEvent::PostToolUse(o)).is_ok());

        let o_err = PostToolUseOutput::default();
        assert!(GeminiHookSpecificOutput::try_from(&o_err).is_err());
    }

    #[test]
    fn test_gemini_hook_specific_output_before_agent() {
        let o = BeforeAgentOutput {
            additional_context: Some("ctx".into()),
            ..Default::default()
        };

        assert!(GeminiHookSpecificOutput::try_from(&o).is_ok());
        assert!(GeminiHookSpecificOutput::try_from(&HookOutputEvent::BeforeAgent(o)).is_ok());

        let o_err = BeforeAgentOutput::default();
        assert!(GeminiHookSpecificOutput::try_from(&o_err).is_err());
    }

    #[test]
    fn test_gemini_hook_specific_output_after_agent() {
        let o = AfterAgentOutput {
            clear_context: Some(true),
            ..Default::default()
        };

        assert!(GeminiHookSpecificOutput::try_from(&o).is_ok());
        assert!(GeminiHookSpecificOutput::try_from(&HookOutputEvent::AfterAgent(o)).is_ok());

        let o_err = AfterAgentOutput::default();
        assert!(GeminiHookSpecificOutput::try_from(&o_err).is_err());
    }

    #[test]
    fn test_gemini_hook_specific_output_before_model() {
        let o = BeforeModelOutput {
            llm_request: Some(serde_json::json!({})),
            ..Default::default()
        };

        assert!(GeminiHookSpecificOutput::try_from(&o).is_ok());
        assert!(GeminiHookSpecificOutput::try_from(&HookOutputEvent::BeforeModel(o)).is_ok());

        let o_err = BeforeModelOutput::default();
        assert!(GeminiHookSpecificOutput::try_from(&o_err).is_err());
    }

    #[test]
    fn test_gemini_hook_specific_output_after_model() {
        let o = AfterModelOutput {
            llm_response: Some(serde_json::json!({})),
            ..Default::default()
        };

        assert!(GeminiHookSpecificOutput::try_from(&o).is_ok());
        assert!(GeminiHookSpecificOutput::try_from(&HookOutputEvent::AfterModel(o)).is_ok());

        let o_err = AfterModelOutput::default();
        assert!(GeminiHookSpecificOutput::try_from(&o_err).is_err());
    }

    #[test]
    fn test_gemini_hook_specific_output_before_tool_selection() {
        let o = BeforeToolSelectionOutput {
            tool_config: Some(serde_json::json!({})),
        };

        assert!(GeminiHookSpecificOutput::try_from(&o).is_ok());
        assert!(
            GeminiHookSpecificOutput::try_from(&HookOutputEvent::BeforeToolSelection(o)).is_ok()
        );

        let o_err = BeforeToolSelectionOutput::default();
        assert!(GeminiHookSpecificOutput::try_from(&o_err).is_err());
    }

    #[test]
    fn test_gemini_hook_specific_output_session_start() {
        let o = SessionStartOutput {
            additional_context: Some("ctx".into()),
            ..Default::default()
        };

        assert!(GeminiHookSpecificOutput::try_from(&o).is_ok());
        assert!(GeminiHookSpecificOutput::try_from(&HookOutputEvent::SessionStart(o)).is_ok());

        let o_err = SessionStartOutput::default();
        assert!(GeminiHookSpecificOutput::try_from(&o_err).is_err());
    }

    #[test]
    fn test_gemini_hook_specific_output_err() {
        let e_err = HookOutputEvent::Notification(NotificationOutput::default());
        assert!(GeminiHookSpecificOutput::try_from(&e_err).is_err());
    }
}
