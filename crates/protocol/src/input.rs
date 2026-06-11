//! Definitions for all input payloads received by hooks.

use crate::types::RawJson;

use serde::Deserialize;

use std::borrow::Cow;

/// Represents the various events that can trigger a hook.
///
/// Each variant carries an event-specific input payload.
#[derive(Debug)]
pub enum HookInputEvent<'a> {
    /// Triggered before a tool is executed.
    BeforeTool(BeforeToolInput<'a>),
    /// Triggered after a tool is executed.
    AfterTool(AfterToolInput<'a>),
    /// Triggered before an agent begins processing a prompt.
    BeforeAgent(BeforeAgentInput<'a>),
    /// Triggered after an agent completes processing.
    AfterAgent(AfterAgentInput<'a>),
    /// Triggered before the underlying LLM is called.
    BeforeModel(BeforeModelInput<'a>),
    /// Triggered after the underlying LLM returns a response.
    AfterModel(AfterModelInput<'a>),
    /// Triggered before the system decides which tools the LLM can access.
    BeforeToolSelection(BeforeToolSelectionInput<'a>),
    /// Triggered when a new session begins.
    SessionStart(SessionStartInput<'a>),
    /// Triggered when a session ends.
    SessionEnd(SessionEndInput<'a>),
    /// Triggered for a generic notification.
    Notification(NotificationInput<'a>),
    /// Triggered before context compression occurs.
    PreCompress(PreCompressInput<'a>),
    /// Triggered when the session's current working directory changes.
    CwdChanged(CwdChangedInput<'a>),
    /// Triggered when a file is changed by the agent.
    FileChanged(FileChangedInput<'a>),
    /// Triggered when custom instructions or memories are loaded.
    InstructionsLoaded(InstructionsLoadedInput<'a>),
    /// Triggered when the user submits a new prompt.
    UserPromptSubmit(UserPromptSubmitInput<'a>),
    /// Triggered when a new git worktree is created.
    WorktreeCreate(WorktreeCreateInput<'a>),
    /// Triggered when a git worktree is removed.
    WorktreeRemove(WorktreeRemoveInput<'a>),
}

/// Input payload for the `BeforeTool` event.
#[derive(Debug, Deserialize)]
pub struct BeforeToolInput<'a> {
    /// The name of the tool about to be executed.
    pub tool_name: Cow<'a, str>,
    /// The raw JSON arguments provided to the tool.
    #[serde(borrow)]
    pub tool_input: RawJson<'a>,
    /// The context from an MCP (Model Context Protocol) server, if applicable.
    pub mcp_context: Option<RawJson<'a>>,
    /// The original request name, if the tool execution is part of a larger chain.
    pub original_request_name: Option<Cow<'a, str>>,
}

/// Input payload for the `AfterTool` event.
#[derive(Debug, Deserialize)]
pub struct AfterToolInput<'a> {
    /// The name of the tool that was executed.
    pub tool_name: Cow<'a, str>,
    /// The raw JSON arguments provided to the tool.
    #[serde(borrow)]
    pub tool_input: RawJson<'a>,
    /// The raw JSON response returned by the tool.
    #[serde(borrow)]
    pub tool_response: RawJson<'a>,
    /// The context from an MCP server, if applicable.
    pub mcp_context: Option<RawJson<'a>>,
    /// The original request name.
    pub original_request_name: Option<Cow<'a, str>>,
}

/// Input payload for the `BeforeAgent` event.
#[derive(Debug, Deserialize)]
pub struct BeforeAgentInput<'a> {
    /// The prompt that will be passed to the agent.
    pub prompt: Cow<'a, str>,
}

/// Input payload for the `AfterAgent` event.
#[derive(Debug, Deserialize)]
pub struct AfterAgentInput<'a> {
    /// The prompt that was passed to the agent.
    pub prompt: Cow<'a, str>,
    /// The resulting response from the agent.
    pub prompt_response: Cow<'a, str>,
    /// Indicates whether a stop hook is actively halting further processing.
    #[serde(default)]
    pub stop_hook_active: bool,
}

/// Input payload for the `BeforeModel` event.
#[derive(Debug, Deserialize)]
pub struct BeforeModelInput<'a> {
    /// The raw LLM request object.
    #[serde(borrow)]
    pub llm_request: RawJson<'a>,
}

/// Input payload for the `AfterModel` event.
#[derive(Debug, Deserialize)]
pub struct AfterModelInput<'a> {
    /// The raw LLM response object.
    #[serde(borrow)]
    pub llm_response: RawJson<'a>,
}

/// Input payload for the `BeforeToolSelection` event.
#[derive(Debug, Deserialize)]
pub struct BeforeToolSelectionInput<'a> {
    /// The LLM request that includes the tools to be selected.
    #[serde(borrow)]
    pub llm_request: Option<RawJson<'a>>,
}

/// Input payload for the `SessionStart` event.
#[derive(Debug, Deserialize)]
pub struct SessionStartInput<'a> {
    /// The source that triggered the session (e.g., CLI, VSCode).
    pub source: Option<Cow<'a, str>>,
    /// The specific model configured for this session.
    pub model: Option<Cow<'a, str>>,
    /// The type of agent running the session.
    pub agent_type: Option<Cow<'a, str>>,
    /// Path to the environment file loaded for this session.
    pub env_file: Option<Cow<'a, str>>,
}

/// Input payload for the `SessionEnd` event.
#[derive(Debug, Deserialize)]
pub struct SessionEndInput<'a> {
    /// The reason why the session ended.
    pub reason: Cow<'a, str>,
}

/// Input payload for the `Notification` event.
#[derive(Debug, Deserialize)]
pub struct NotificationInput<'a> {
    /// The message content of the notification.
    pub message: Cow<'a, str>,
    /// The category or type of the notification.
    pub notification_type: Option<Cow<'a, str>>,
    /// The title of the notification.
    pub title: Option<Cow<'a, str>>,
}

/// Input payload for the `PreCompress` event.
#[derive(Debug, Deserialize)]
pub struct PreCompressInput<'a> {
    /// The trigger reason for the compression.
    pub trigger: Cow<'a, str>,
    /// Custom instructions provided during the compression phase.
    pub custom_instructions: Option<Cow<'a, str>>,
}

/// Input payload for the `CwdChanged` event.
#[derive(Debug, Deserialize)]
pub struct CwdChangedInput<'a> {
    /// The previous current working directory.
    pub previous_cwd: Cow<'a, str>,
    /// The new current working directory.
    pub new_cwd: Cow<'a, str>,
    /// Path to an environment file related to the directory change.
    pub env_file: Option<Cow<'a, str>>,
}

/// Input payload for the `FileChanged` event.
#[derive(Debug, Deserialize)]
pub struct FileChangedInput<'a> {
    /// The path of the file that changed.
    pub file_path: Cow<'a, str>,
    /// The type of change event (e.g., created, modified, deleted).
    pub event: Option<Cow<'a, str>>,
    /// Path to the environment file associated with this change.
    pub env_file: Option<Cow<'a, str>>,
}

/// Input payload for the `InstructionsLoaded` event.
#[derive(Debug, Deserialize)]
pub struct InstructionsLoadedInput<'a> {
    /// Path to the instructions file loaded.
    pub file_path: Cow<'a, str>,
    /// The type of memory/instructions loaded.
    pub memory_type: Cow<'a, str>,
    /// Reason why these instructions were loaded.
    pub load_reason: Cow<'a, str>,
    /// A list of glob patterns relevant to these instructions.
    #[serde(default)]
    pub globs: Vec<Cow<'a, str>>,
    /// The path to the file that triggered loading these instructions.
    pub trigger_file_path: Option<Cow<'a, str>>,
    /// The path to a parent file, if applicable.
    pub parent_file_path: Option<Cow<'a, str>>,
}

/// Input payload for the `UserPromptSubmit` event.
#[derive(Debug, Deserialize)]
pub struct UserPromptSubmitInput<'a> {
    /// The prompt text submitted by the user.
    pub prompt: Cow<'a, str>,
}

/// Input payload for the `WorktreeCreate` event.
#[derive(Debug, Deserialize)]
pub struct WorktreeCreateInput<'a> {
    /// The name of the newly created worktree.
    pub name: Cow<'a, str>,
}

/// Input payload for the `WorktreeRemove` event.
#[derive(Debug, Deserialize)]
pub struct WorktreeRemoveInput<'a> {
    /// The path of the removed worktree.
    pub worktree_path: Cow<'a, str>,
}

impl<'a> BeforeToolInput<'a> {
    /// Parses the raw tool input into a specific expected type `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use inceptool_protocol::error::ProtocolError;
    /// # fn main() -> Result<(), ProtocolError> {
    /// use inceptool_protocol::{BeforeToolInput, RawJson};
    /// use serde_json::value::RawValue;
    /// use serde::Deserialize;
    /// use std::borrow::Cow;
    ///
    /// #[derive(Deserialize)]
    /// struct MyArgs {
    ///     id: u32,
    /// }
    ///
    /// let raw = RawValue::from_string(r#"{"id": 42}"#.to_string())?;
    /// let input = BeforeToolInput {
    ///     tool_name: Cow::Borrowed("my_tool"),
    ///     tool_input: RawJson(&raw),
    ///     mcp_context: None,
    ///     original_request_name: None,
    /// };
    ///
    /// let args: MyArgs = input.parse_tool_input()?;
    /// assert_eq!(args.id, 42);
    /// # Ok(())
    /// # }
    /// ```
    pub fn parse_tool_input<T: serde::de::Deserialize<'a>>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(self.tool_input.0.get())
    }
}

impl<'a> AfterToolInput<'a> {
    /// Parses the raw tool input into a specific expected type `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use inceptool_protocol::error::ProtocolError;
    /// # fn main() -> Result<(), ProtocolError> {
    /// use inceptool_protocol::{AfterToolInput, RawJson};
    /// use serde_json::value::RawValue;
    /// use serde::Deserialize;
    /// use std::borrow::Cow;
    ///
    /// #[derive(Deserialize)]
    /// struct MyArgs {
    ///     id: u32,
    /// }
    ///
    /// let raw_in = RawValue::from_string(r#"{"id": 42}"#.to_string())?;
    /// let raw_out = RawValue::from_string(r#"{}"#.to_string())?;
    /// let input = AfterToolInput {
    ///     tool_name: Cow::Borrowed("my_tool"),
    ///     tool_input: RawJson(&raw_in),
    ///     tool_response: RawJson(&raw_out),
    ///     mcp_context: None,
    ///     original_request_name: None,
    /// };
    ///
    /// let args: MyArgs = input.parse_tool_input()?;
    /// assert_eq!(args.id, 42);
    /// # Ok(())
    /// # }
    /// ```
    pub fn parse_tool_input<T: serde::de::Deserialize<'a>>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(self.tool_input.0.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ProtocolError;
    use crate::types::RawJson;

    use rstest::{fixture, rstest};
    use serde_json::json;

    #[derive(thiserror::Error, Debug)]
    pub enum TestError {
        #[error(transparent)]
        Protocol(#[from] ProtocolError),
        #[error(transparent)]
        Json(#[from] serde_json::Error),
    }

    #[fixture]
    fn raw_tool_input_json() -> String {
        r#"{"key": "value"}"#.to_string()
    }

    #[fixture]
    fn grep_search_json() -> String {
        r#"{
            "tool_name": "grep_search",
            "tool_input": {"query": "foo", "path": "/"},
            "original_request_name": "search"
        }"#
        .to_string()
    }

    #[rstest]
    fn test_before_tool_input_deserialization_tool_name(
        grep_search_json: String,
    ) -> Result<(), TestError> {
        let input: BeforeToolInput = serde_json::from_str(&grep_search_json)?;
        assert_eq!(input.tool_name, "grep_search");
        Ok(())
    }

    #[rstest]
    fn test_before_tool_input_deserialization_original_name(
        grep_search_json: String,
    ) -> Result<(), TestError> {
        let input: BeforeToolInput = serde_json::from_str(&grep_search_json)?;
        assert_eq!(input.original_request_name.as_deref(), Some("search"));
        Ok(())
    }

    #[rstest]
    fn test_before_tool_input_deserialization_mcp_context(
        grep_search_json: String,
    ) -> Result<(), TestError> {
        let input: BeforeToolInput = serde_json::from_str(&grep_search_json)?;
        assert!(input.mcp_context.is_none());
        Ok(())
    }

    #[rstest]
    fn test_before_tool_input_deserialization_payload(
        grep_search_json: String,
    ) -> Result<(), TestError> {
        let input: BeforeToolInput = serde_json::from_str(&grep_search_json)?;
        let parsed_tool_input: serde_json::Value = serde_json::from_str(input.tool_input.0.get())?;

        assert_eq!(parsed_tool_input, json!({"query": "foo", "path": "/"}));

        Ok(())
    }

    #[rstest]
    fn test_before_tool_input_parse(raw_tool_input_json: String) -> Result<(), TestError> {
        let raw = serde_json::value::RawValue::from_string(raw_tool_input_json)?;
        let input = BeforeToolInput {
            tool_name: std::borrow::Cow::Borrowed("test"),
            tool_input: RawJson(&raw),
            mcp_context: None,
            original_request_name: None,
        };

        let parsed: serde_json::Value = input.parse_tool_input()?;
        assert_eq!(parsed, json!({"key": "value"}));

        Ok(())
    }

    #[rstest]
    fn test_after_tool_input_parse(raw_tool_input_json: String) -> Result<(), TestError> {
        let raw = serde_json::value::RawValue::from_string(raw_tool_input_json)?;
        let after_input = AfterToolInput {
            tool_name: std::borrow::Cow::Borrowed("test"),
            tool_input: RawJson(&raw),
            tool_response: RawJson(&raw),
            mcp_context: None,
            original_request_name: None,
        };

        let parsed: serde_json::Value = after_input.parse_tool_input()?;
        assert_eq!(parsed, json!({"key": "value"}));

        Ok(())
    }
}
