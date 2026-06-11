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

    // --- Additional Hooks ---
    /// Triggered when the agent completes its initial setup phase.
    Setup(SetupInput<'a>),
    /// Triggered when a user prompt is expanded using custom instructions or memory.
    UserPromptExpansion(UserPromptExpansionInput<'a>),
    /// Triggered before a message is displayed to the user.
    MessageDisplay(MessageDisplayInput<'a>),
    /// Triggered when a tool requires explicit permission to execute.
    PermissionRequest(PermissionRequestInput<'a>),
    /// Triggered when a tool execution fails.
    PostToolUseFailure(PostToolUseFailureInput<'a>),
    /// Triggered after a batch of tools have finished executing.
    PostToolBatch(PostToolBatchInput<'a>),
    /// Triggered when permission to execute a tool is denied.
    PermissionDenied(PermissionDeniedInput<'a>),
    /// Triggered when a subagent is started.
    SubagentStart(SubagentStartInput<'a>),
    /// Triggered when a subagent finishes.
    SubagentStop(SubagentStopInput<'a>),
    /// Triggered when a background task is created.
    TaskCreated(TaskCreatedInput<'a>),
    /// Triggered when a background task completes.
    TaskCompleted(TaskCompletedInput<'a>),
    /// Triggered before the agent stops running.
    Stop(StopInput<'a>),
    /// Triggered if the agent fails to stop.
    StopFailure(StopFailureInput<'a>),
    /// Triggered when a teammate becomes idle.
    TeammateIdle(TeammateIdleInput<'a>),
    /// Triggered when the user's configuration changes.
    ConfigChange(ConfigChangeInput<'a>),
    /// Triggered after context compression has finished.
    PostCompact(PostCompactInput<'a>),
    /// Triggered when an elicitation request is sent.
    Elicitation(ElicitationInput<'a>),
    /// Triggered when an elicitation result is received.
    ElicitationResult(ElicitationResultInput<'a>),
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

/// Input payload for the `Setup` event.
///
/// Fires only when launching with --init-only, or with --init or --maintenance in print mode (-p).
/// It does not fire on normal startup. Use it for one-time dependency installation or scheduled cleanup.
#[derive(Debug, Deserialize)]
pub struct SetupInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `UserPromptExpansion` event.
///
/// Runs when a user-typed slash command expands into a prompt before reaching the model.
/// Use this to block specific commands from direct invocation, inject context for a particular skill,
/// or log which commands users invoke.
#[derive(Debug, Deserialize)]
pub struct UserPromptExpansionInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `MessageDisplay` event.
///
/// Runs while an assistant message streams to the screen. Displays the message in increments.
/// Each time a batch of newly completed lines is ready to render, the hook runs once with those lines
/// and renders the hook’s replacement text in their place.
#[derive(Debug, Deserialize)]
pub struct MessageDisplayInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `PermissionRequest` event.
///
/// Runs when the user is shown a permission dialog.
/// Use PermissionRequest decision control to allow or deny on behalf of the user.
#[derive(Debug, Deserialize)]
pub struct PermissionRequestInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `PostToolUseFailure` event.
///
/// Runs when a tool execution fails. This event fires for tool calls that throw errors or return failure results.
/// Use this to log failures, send alerts, or provide corrective feedback.
#[derive(Debug, Deserialize)]
pub struct PostToolUseFailureInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `PostToolBatch` event.
///
/// Runs once after every tool call in a batch has resolved, before sending the next request to the model.
/// It is the right place to inject context that depends on the set of tools that ran rather than on any single tool.
#[derive(Debug, Deserialize)]
pub struct PostToolBatchInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `PermissionDenied` event.
///
/// Runs when the auto mode classifier denies a tool call. This hook only fires in auto mode:
/// it does not run when you manually deny a permission dialog or when a PreToolUse hook blocks a call.
/// Use it to log classifier denials, adjust configuration, or tell the model it may retry the tool call.
#[derive(Debug, Deserialize)]
pub struct PermissionDeniedInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `SubagentStart` event.
///
/// Runs when a subagent is spawned via the Agent tool.
#[derive(Debug, Deserialize)]
pub struct SubagentStartInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `SubagentStop` event.
///
/// Runs when a subagent has finished responding.
#[derive(Debug, Deserialize)]
pub struct SubagentStopInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `TaskCreated` event.
///
/// Runs when a task is being created via the TaskCreate tool. Use this to enforce naming conventions,
/// require task descriptions, or prevent certain tasks from being created.
#[derive(Debug, Deserialize)]
pub struct TaskCreatedInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `TaskCompleted` event.
///
/// Runs when a task is being marked as completed. This fires in two situations: when any agent
/// explicitly marks a task as completed through the TaskUpdate tool, or when an agent team teammate
/// finishes its turn with in-progress tasks.
#[derive(Debug, Deserialize)]
pub struct TaskCompletedInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `Stop` event.
///
/// Runs when the main agent has finished responding. Does not run if the stoppage occurred due to a user interrupt.
#[derive(Debug, Deserialize)]
pub struct StopInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `StopFailure` event.
///
/// Runs instead of Stop when the turn ends due to an API error.
/// Use this to log failures, send alerts, or take recovery actions when the agent cannot complete
/// a response due to rate limits, authentication problems, or other API errors.
#[derive(Debug, Deserialize)]
pub struct StopFailureInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `TeammateIdle` event.
///
/// Runs when an agent team teammate is about to go idle after finishing its turn. Use this to enforce
/// quality gates before a teammate stops working, such as requiring passing lint checks or verifying
/// that output files exist.
#[derive(Debug, Deserialize)]
pub struct TeammateIdleInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `ConfigChange` event.
///
/// Runs when a configuration file changes during a session. Use this to audit settings changes,
/// enforce security policies, or block unauthorized modifications to configuration files.
#[derive(Debug, Deserialize)]
pub struct ConfigChangeInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `PostCompact` event.
///
/// Runs after a compact operation completes. Use this event to react to the new compacted state,
/// for example to log the generated summary or update external state.
#[derive(Debug, Deserialize)]
pub struct PostCompactInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `Elicitation` event.
///
/// Runs when an MCP server requests user input mid-task. Hooks can intercept this request
/// and respond programmatically, skipping the dialog entirely.
#[derive(Debug, Deserialize)]
pub struct ElicitationInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
}

/// Input payload for the `ElicitationResult` event.
///
/// Runs when an elicitation result is received.
#[derive(Debug, Deserialize)]
pub struct ElicitationResultInput<'a> {
    /// The raw payload of the hook event.
    #[serde(borrow)]
    pub raw: Option<RawJson<'a>>,
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
