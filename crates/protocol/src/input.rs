//! Definitions for all input payloads received by hooks.

use crate::error::ProtocolError;
use crate::types::RawJson;

use serde::Deserialize;

use std::borrow::Cow;

/// Represents the various events that can trigger a hook.
///
/// Each variant carries an event-specific input payload.
#[derive(Debug)]
pub enum HookInputEvent<'a> {
    /// Triggered before a tool is executed.
    PreToolUse(PreToolUseInput<'a>),
    /// Triggered after a tool is executed.
    PostToolUse(PostToolUseInput<'a>),
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
    PreCompact(PreCompactInput<'a>),
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

/// A fieldless discriminant mirroring [`HookInputEvent`], one variant per CLI hook.
///
/// Used as an array index (`kind as usize`) to dispatch a [`HookInputEvent`] to its
/// corresponding pipeline without allocating or hashing.
///
/// The derived [`strum_macros::EnumString`] implementation parses the
/// canonical hook event name (e.g. `"PreToolUse"`) — see [`HookKind::parse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum_macros::EnumString)]
pub enum HookKind {
    /// See [`HookInputEvent::PreToolUse`].
    PreToolUse,
    /// See [`HookInputEvent::PostToolUse`].
    PostToolUse,
    /// See [`HookInputEvent::BeforeAgent`].
    BeforeAgent,
    /// See [`HookInputEvent::AfterAgent`].
    AfterAgent,
    /// See [`HookInputEvent::BeforeModel`].
    BeforeModel,
    /// See [`HookInputEvent::AfterModel`].
    AfterModel,
    /// See [`HookInputEvent::BeforeToolSelection`].
    BeforeToolSelection,
    /// See [`HookInputEvent::SessionStart`].
    SessionStart,
    /// See [`HookInputEvent::SessionEnd`].
    SessionEnd,
    /// See [`HookInputEvent::Notification`].
    Notification,
    /// See [`HookInputEvent::PreCompact`].
    PreCompact,
    /// See [`HookInputEvent::CwdChanged`].
    CwdChanged,
    /// See [`HookInputEvent::FileChanged`].
    FileChanged,
    /// See [`HookInputEvent::InstructionsLoaded`].
    InstructionsLoaded,
    /// See [`HookInputEvent::UserPromptSubmit`].
    UserPromptSubmit,
    /// See [`HookInputEvent::WorktreeCreate`].
    WorktreeCreate,
    /// See [`HookInputEvent::WorktreeRemove`].
    WorktreeRemove,
    /// See [`HookInputEvent::Setup`].
    Setup,
    /// See [`HookInputEvent::UserPromptExpansion`].
    UserPromptExpansion,
    /// See [`HookInputEvent::MessageDisplay`].
    MessageDisplay,
    /// See [`HookInputEvent::PermissionRequest`].
    PermissionRequest,
    /// See [`HookInputEvent::PostToolUseFailure`].
    PostToolUseFailure,
    /// See [`HookInputEvent::PostToolBatch`].
    PostToolBatch,
    /// See [`HookInputEvent::PermissionDenied`].
    PermissionDenied,
    /// See [`HookInputEvent::SubagentStart`].
    SubagentStart,
    /// See [`HookInputEvent::SubagentStop`].
    SubagentStop,
    /// See [`HookInputEvent::TaskCreated`].
    TaskCreated,
    /// See [`HookInputEvent::TaskCompleted`].
    TaskCompleted,
    /// See [`HookInputEvent::Stop`].
    Stop,
    /// See [`HookInputEvent::StopFailure`].
    StopFailure,
    /// See [`HookInputEvent::TeammateIdle`].
    TeammateIdle,
    /// See [`HookInputEvent::ConfigChange`].
    ConfigChange,
    /// See [`HookInputEvent::PostCompact`].
    PostCompact,
    /// See [`HookInputEvent::Elicitation`].
    Elicitation,
    /// See [`HookInputEvent::ElicitationResult`].
    ElicitationResult,
}

impl HookKind {
    /// The total number of [`HookKind`] variants.
    pub const COUNT: usize = 35;

    /// Parses a canonical hook event name (e.g. `"PreToolUse"`) into its
    /// [`HookKind`].
    ///
    /// Used by [`Driver::hook_kind`](crate::driver::Driver::hook_kind)
    /// implementations to map a driver-specific raw hook event name onto the
    /// canonical [`HookKind`] used for pipeline dispatch. Returns
    /// [`ProtocolError::UnsupportedEvent`] for names that don't match any
    /// [`HookKind`] variant.
    ///
    /// ```
    /// # use inceptool_protocol::ProtocolError;
    /// # fn main() -> Result<(), ProtocolError> {
    /// use inceptool_protocol::HookKind;
    ///
    /// assert_eq!(HookKind::parse("PreToolUse")?, HookKind::PreToolUse);
    /// assert!(HookKind::parse("NotAHook").is_err());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `ProtocolError::UnsupportedEvent` if `name` does not
    /// correspond to a known `HookKind`.
    pub fn parse(name: &str) -> Result<Self, ProtocolError> {
        name.parse()
            .map_err(|_| ProtocolError::UnsupportedEvent(name.to_string()))
    }
}

impl HookInputEvent<'_> {
    /// Returns the [`HookKind`] discriminant for this event.
    #[must_use]
    pub const fn kind(&self) -> HookKind {
        match self {
            HookInputEvent::PreToolUse(_) => HookKind::PreToolUse,
            HookInputEvent::PostToolUse(_) => HookKind::PostToolUse,
            HookInputEvent::BeforeAgent(_) => HookKind::BeforeAgent,
            HookInputEvent::AfterAgent(_) => HookKind::AfterAgent,
            HookInputEvent::BeforeModel(_) => HookKind::BeforeModel,
            HookInputEvent::AfterModel(_) => HookKind::AfterModel,
            HookInputEvent::BeforeToolSelection(_) => HookKind::BeforeToolSelection,
            HookInputEvent::SessionStart(_) => HookKind::SessionStart,
            HookInputEvent::SessionEnd(_) => HookKind::SessionEnd,
            HookInputEvent::Notification(_) => HookKind::Notification,
            HookInputEvent::PreCompact(_) => HookKind::PreCompact,
            HookInputEvent::CwdChanged(_) => HookKind::CwdChanged,
            HookInputEvent::FileChanged(_) => HookKind::FileChanged,
            HookInputEvent::InstructionsLoaded(_) => HookKind::InstructionsLoaded,
            HookInputEvent::UserPromptSubmit(_) => HookKind::UserPromptSubmit,
            HookInputEvent::WorktreeCreate(_) => HookKind::WorktreeCreate,
            HookInputEvent::WorktreeRemove(_) => HookKind::WorktreeRemove,
            HookInputEvent::Setup(_) => HookKind::Setup,
            HookInputEvent::UserPromptExpansion(_) => HookKind::UserPromptExpansion,
            HookInputEvent::MessageDisplay(_) => HookKind::MessageDisplay,
            HookInputEvent::PermissionRequest(_) => HookKind::PermissionRequest,
            HookInputEvent::PostToolUseFailure(_) => HookKind::PostToolUseFailure,
            HookInputEvent::PostToolBatch(_) => HookKind::PostToolBatch,
            HookInputEvent::PermissionDenied(_) => HookKind::PermissionDenied,
            HookInputEvent::SubagentStart(_) => HookKind::SubagentStart,
            HookInputEvent::SubagentStop(_) => HookKind::SubagentStop,
            HookInputEvent::TaskCreated(_) => HookKind::TaskCreated,
            HookInputEvent::TaskCompleted(_) => HookKind::TaskCompleted,
            HookInputEvent::Stop(_) => HookKind::Stop,
            HookInputEvent::StopFailure(_) => HookKind::StopFailure,
            HookInputEvent::TeammateIdle(_) => HookKind::TeammateIdle,
            HookInputEvent::ConfigChange(_) => HookKind::ConfigChange,
            HookInputEvent::PostCompact(_) => HookKind::PostCompact,
            HookInputEvent::Elicitation(_) => HookKind::Elicitation,
            HookInputEvent::ElicitationResult(_) => HookKind::ElicitationResult,
        }
    }

    /// Returns the tool name associated with this event, if any.
    ///
    /// Only [`PreToolUse`](HookInputEvent::PreToolUse),
    /// [`PostToolUse`](HookInputEvent::PostToolUse),
    /// [`PermissionRequest`](HookInputEvent::PermissionRequest),
    /// [`PostToolUseFailure`](HookInputEvent::PostToolUseFailure), and
    /// [`PermissionDenied`](HookInputEvent::PermissionDenied) carry a tool name;
    /// all other variants return `None`.
    #[must_use]
    pub fn tool_name(&self) -> Option<&str> {
        match self {
            HookInputEvent::PreToolUse(input) => Some(input.tool_name.as_ref()),
            HookInputEvent::PostToolUse(input) => Some(input.tool_name.as_ref()),
            HookInputEvent::PermissionRequest(input) => Some(input.tool_name.as_ref()),
            HookInputEvent::PostToolUseFailure(input) => Some(input.tool_name.as_ref()),
            HookInputEvent::PermissionDenied(input) => Some(input.tool_name.as_ref()),
            _ => None,
        }
    }
}

/// Input payload for the `PreToolUse` event.
#[derive(Debug, Deserialize)]
pub struct PreToolUseInput<'a> {
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

/// Input payload for the `PostToolUse` event.
#[derive(Debug, Deserialize)]
pub struct PostToolUseInput<'a> {
    /// The name of the tool that was executed.
    pub tool_name: Cow<'a, str>,
    /// The raw JSON arguments provided to the tool.
    #[serde(borrow)]
    pub tool_input: RawJson<'a>,
    /// The raw JSON output returned by the tool.
    #[serde(borrow, alias = "tool_response")]
    pub tool_output: RawJson<'a>,
    /// The source of the tool output (e.g., "tool", "hook").
    pub tool_output_source: Option<Cow<'a, str>>,
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
    /// The source that triggered the session (e.g., CLI, `VSCode`).
    pub source: Option<Cow<'a, str>>,
    /// The specific model configured for this session.
    pub model: Option<Cow<'a, str>>,
    /// The type of agent running the session.
    pub agent_type: Option<Cow<'a, str>>,
    /// Path to the environment file loaded for this session.
    pub env_file: Option<Cow<'a, str>>,
    /// The title of the session, if one has been set.
    pub session_title: Option<Cow<'a, str>>,
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
    /// The severity level of the notification (e.g., "info", "warning", "error").
    pub severity: Option<Cow<'a, str>>,
}

/// Input payload for the `PreCompact` event.
#[derive(Debug, Deserialize)]
pub struct PreCompactInput<'a> {
    /// The trigger reason for the compression.
    pub trigger: Cow<'a, str>,
    /// Custom instructions provided during the compression phase.
    pub custom_instructions: Option<Cow<'a, str>>,
}

/// Input payload for the `CwdChanged` event.
#[derive(Debug, Deserialize)]
pub struct CwdChangedInput<'a> {
    /// The previous current working directory.
    #[serde(alias = "previous_cwd")]
    pub old_cwd: Cow<'a, str>,
    /// The new current working directory.
    pub new_cwd: Cow<'a, str>,
    /// The reason the working directory changed.
    pub change_reason: Option<Cow<'a, str>>,
    /// Path to an environment file related to the directory change.
    pub env_file: Option<Cow<'a, str>>,
}

/// Input payload for the `FileChanged` event.
#[derive(Debug, Deserialize)]
pub struct FileChangedInput<'a> {
    /// The path of the file that changed.
    pub file_path: Cow<'a, str>,
    /// The type of change event (e.g., created, modified, deleted).
    #[serde(alias = "event")]
    pub change_type: Option<Cow<'a, str>>,
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
    /// The name of the subagent the worktree is being created for, if any.
    pub subagent_name: Option<Cow<'a, str>>,
    /// A unique identifier for the worktree.
    pub worktree_id: Option<Cow<'a, str>>,
    /// The root of the git repository the worktree was created from.
    pub git_root: Option<Cow<'a, str>>,
    /// The path of the parent worktree, if this worktree was created from another worktree.
    pub parent_path: Option<Cow<'a, str>>,
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
    /// The trigger that initiated this setup run (e.g., "init" or "maintenance").
    pub trigger: Cow<'a, str>,
}

/// Input payload for the `UserPromptExpansion` event.
///
/// Runs when a user-typed slash command expands into a prompt before reaching the model.
/// Use this to block specific commands from direct invocation, inject context for a particular skill,
/// or log which commands users invoke.
#[derive(Debug, Deserialize)]
pub struct UserPromptExpansionInput<'a> {
    /// The name of the slash command that was expanded.
    pub command_name: Cow<'a, str>,
    /// The prompt text the command expanded into, if available.
    pub prompt: Option<Cow<'a, str>>,
}

/// Input payload for the `MessageDisplay` event.
///
/// Runs while an assistant message streams to the screen. Displays the message in increments.
/// Each time a batch of newly completed lines is ready to render, the hook runs once with those lines
/// and renders the hook’s replacement text in their place.
#[derive(Debug, Deserialize)]
pub struct MessageDisplayInput<'a> {
    /// The batch of newly completed lines ready to render.
    #[serde(default)]
    pub lines: Vec<Cow<'a, str>>,
}

/// Input payload for the `PermissionRequest` event.
///
/// Runs when the user is shown a permission dialog.
/// Use `PermissionRequest` decision control to allow or deny on behalf of the user.
#[derive(Debug, Deserialize)]
pub struct PermissionRequestInput<'a> {
    /// The name of the tool the permission dialog is being shown for.
    pub tool_name: Cow<'a, str>,
    /// The raw JSON arguments proposed for the tool.
    #[serde(borrow)]
    pub tool_input: RawJson<'a>,
    /// The name of the permission rule that triggered the dialog, if any.
    pub permission_rule_name: Option<Cow<'a, str>>,
}

/// Input payload for the `PostToolUseFailure` event.
///
/// Runs when a tool execution fails. This event fires for tool calls that throw errors or return failure results.
/// Use this to log failures, send alerts, or provide corrective feedback.
#[derive(Debug, Deserialize)]
pub struct PostToolUseFailureInput<'a> {
    /// The name of the tool that failed.
    pub tool_name: Cow<'a, str>,
    /// The raw JSON arguments that were provided to the tool.
    #[serde(borrow)]
    pub tool_input: RawJson<'a>,
    /// The error message produced by the failed tool execution.
    pub tool_error: Cow<'a, str>,
}

/// Input payload for the `PostToolBatch` event.
///
/// Runs once after every tool call in a batch has resolved, before sending the next request to the model.
/// It is the right place to inject context that depends on the set of tools that ran rather than on any single tool.
#[derive(Debug, Deserialize)]
pub struct PostToolBatchInput<'a> {
    /// The raw JSON details of each tool call resolved in this batch.
    #[serde(borrow, default)]
    pub tool_calls: Vec<RawJson<'a>>,
}

/// Input payload for the `PermissionDenied` event.
///
/// Runs when the auto mode classifier denies a tool call. This hook only fires in auto mode:
/// it does not run when you manually deny a permission dialog or when a `PreToolUse` hook blocks a call.
/// Use it to log classifier denials, adjust configuration, or tell the model it may retry the tool call.
#[derive(Debug, Deserialize)]
pub struct PermissionDeniedInput<'a> {
    /// The name of the tool that was denied.
    pub tool_name: Cow<'a, str>,
    /// The raw JSON arguments that were proposed for the tool.
    #[serde(borrow)]
    pub tool_input: RawJson<'a>,
    /// The reason the auto mode classifier denied the tool call, if provided.
    pub reason: Option<Cow<'a, str>>,
}

/// Input payload for the `SubagentStart` event.
///
/// Runs when a subagent is spawned via the Agent tool.
#[derive(Debug, Deserialize)]
pub struct SubagentStartInput<'a> {
    /// The type of the subagent that was spawned.
    pub agent_type: Cow<'a, str>,
    /// The prompt passed to the subagent, if available.
    pub prompt: Option<Cow<'a, str>>,
}

/// Input payload for the `SubagentStop` event.
///
/// Runs when a subagent has finished responding.
#[derive(Debug, Deserialize)]
pub struct SubagentStopInput<'a> {
    /// The type of the subagent that finished.
    pub agent_type: Cow<'a, str>,
    /// The result returned by the subagent, if available.
    pub result: Option<Cow<'a, str>>,
}

/// Input payload for the `TaskCreated` event.
///
/// Runs when a task is being created via the `TaskCreate` tool. Use this to enforce naming conventions,
/// require task descriptions, or prevent certain tasks from being created.
#[derive(Debug, Deserialize)]
pub struct TaskCreatedInput<'a> {
    /// The raw JSON details of the task being created.
    #[serde(borrow)]
    pub task: RawJson<'a>,
}

/// Input payload for the `TaskCompleted` event.
///
/// Runs when a task is being marked as completed. This fires in two situations: when any agent
/// explicitly marks a task as completed through the `TaskUpdate` tool, or when an agent team teammate
/// finishes its turn with in-progress tasks.
#[derive(Debug, Deserialize)]
pub struct TaskCompletedInput<'a> {
    /// The raw JSON details of the task being completed.
    #[serde(borrow)]
    pub task: RawJson<'a>,
}

/// Input payload for the `Stop` event.
///
/// Runs when the main agent has finished responding. Does not run if the stoppage occurred due to a user interrupt.
#[derive(Debug, Deserialize)]
pub struct StopInput<'a> {
    /// Claude's final response message for the turn, if available.
    pub message: Option<Cow<'a, str>>,
}

/// Input payload for the `StopFailure` event.
///
/// Runs instead of Stop when the turn ends due to an API error.
/// Use this to log failures, send alerts, or take recovery actions when the agent cannot complete
/// a response due to rate limits, authentication problems, or other API errors.
#[derive(Debug, Deserialize)]
pub struct StopFailureInput<'a> {
    /// The category of error that prevented the agent from completing its response.
    pub error_type: Cow<'a, str>,
    /// A human-readable description of the error.
    pub error_message: Cow<'a, str>,
}

/// Input payload for the `TeammateIdle` event.
///
/// Runs when an agent team teammate is about to go idle after finishing its turn. Use this to enforce
/// quality gates before a teammate stops working, such as requiring passing lint checks or verifying
/// that output files exist.
#[derive(Debug, Deserialize)]
pub struct TeammateIdleInput<'a> {
    /// The result the teammate produced before going idle, if available.
    pub result: Option<Cow<'a, str>>,
}

/// Input payload for the `ConfigChange` event.
///
/// Runs when a configuration file changes during a session. Use this to audit settings changes,
/// enforce security policies, or block unauthorized modifications to configuration files.
#[derive(Debug, Deserialize)]
pub struct ConfigChangeInput<'a> {
    /// The source of the changed configuration (e.g., "`user_settings`", "`project_settings`").
    pub config_source: Cow<'a, str>,
    /// The path of the configuration file that changed.
    pub changed_file: Cow<'a, str>,
}

/// Input payload for the `PostCompact` event.
///
/// Runs after a compact operation completes. Use this event to react to the new compacted state,
/// for example to log the generated summary or update external state.
#[derive(Debug, Deserialize)]
pub struct PostCompactInput<'a> {
    /// The trigger reason for the compaction that just completed.
    pub trigger: Option<Cow<'a, str>>,
    /// A summary of the compaction that was performed.
    pub summary: Option<Cow<'a, str>>,
}

/// Input payload for the `Elicitation` event.
///
/// Runs when an MCP server requests user input mid-task. Hooks can intercept this request
/// and respond programmatically, skipping the dialog entirely.
#[derive(Debug, Deserialize)]
pub struct ElicitationInput<'a> {
    /// The name of the MCP server that requested input, if known.
    pub server_name: Option<Cow<'a, str>>,
    /// The raw JSON elicitation request sent by the MCP server.
    #[serde(borrow)]
    pub request: RawJson<'a>,
}

/// Input payload for the `ElicitationResult` event.
///
/// Runs when an elicitation result is received.
#[derive(Debug, Deserialize)]
pub struct ElicitationResultInput<'a> {
    /// The raw JSON result of the elicitation request.
    #[serde(borrow)]
    pub result: RawJson<'a>,
}

impl<'a> PreToolUseInput<'a> {
    /// Parses the raw tool input into a specific expected type `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use inceptool_protocol::error::ProtocolError;
    /// # fn main() -> Result<(), ProtocolError> {
    /// use inceptool_protocol::{PreToolUseInput, RawJson};
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
    /// let input = PreToolUseInput {
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
    ///
    /// # Errors
    ///
    /// Returns an error if the raw tool input JSON cannot be deserialized
    /// into `T`.
    pub fn parse_tool_input<T: serde::de::Deserialize<'a>>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(self.tool_input.0.get())
    }
}

impl<'a> PostToolUseInput<'a> {
    /// Parses the raw tool input into a specific expected type `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use inceptool_protocol::error::ProtocolError;
    /// # fn main() -> Result<(), ProtocolError> {
    /// use inceptool_protocol::{PostToolUseInput, RawJson};
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
    /// let input = PostToolUseInput {
    ///     tool_name: Cow::Borrowed("my_tool"),
    ///     tool_input: RawJson(&raw_in),
    ///     tool_output: RawJson(&raw_out),
    ///     tool_output_source: None,
    ///     mcp_context: None,
    ///     original_request_name: None,
    /// };
    ///
    /// let args: MyArgs = input.parse_tool_input()?;
    /// assert_eq!(args.id, 42);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the raw tool input JSON cannot be deserialized
    /// into `T`.
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
    fn test_pre_tool_use_input_deserialization_tool_name(
        grep_search_json: String,
    ) -> Result<(), TestError> {
        let input: PreToolUseInput<'_> = serde_json::from_str(&grep_search_json)?;
        assert_eq!(input.tool_name, "grep_search");
        Ok(())
    }

    #[rstest]
    fn test_pre_tool_use_input_deserialization_original_name(
        grep_search_json: String,
    ) -> Result<(), TestError> {
        let input: PreToolUseInput<'_> = serde_json::from_str(&grep_search_json)?;
        assert_eq!(input.original_request_name.as_deref(), Some("search"));
        Ok(())
    }

    #[rstest]
    fn test_pre_tool_use_input_deserialization_mcp_context(
        grep_search_json: String,
    ) -> Result<(), TestError> {
        let input: PreToolUseInput<'_> = serde_json::from_str(&grep_search_json)?;
        assert!(input.mcp_context.is_none());
        Ok(())
    }

    #[rstest]
    fn test_pre_tool_use_input_deserialization_payload(
        grep_search_json: String,
    ) -> Result<(), TestError> {
        let input: PreToolUseInput<'_> = serde_json::from_str(&grep_search_json)?;
        let parsed_tool_input: serde_json::Value = serde_json::from_str(input.tool_input.0.get())?;

        assert_eq!(parsed_tool_input, json!({"query": "foo", "path": "/"}));

        Ok(())
    }

    #[rstest]
    fn test_pre_tool_use_input_parse(raw_tool_input_json: String) -> Result<(), TestError> {
        let raw = serde_json::value::RawValue::from_string(raw_tool_input_json)?;
        let input = PreToolUseInput {
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
    fn test_post_tool_use_input_parse(raw_tool_input_json: String) -> Result<(), TestError> {
        let raw = serde_json::value::RawValue::from_string(raw_tool_input_json)?;
        let after_input = PostToolUseInput {
            tool_name: std::borrow::Cow::Borrowed("test"),
            tool_input: RawJson(&raw),
            tool_output: RawJson(&raw),
            tool_output_source: None,
            mcp_context: None,
            original_request_name: None,
        };

        let parsed: serde_json::Value = after_input.parse_tool_input()?;
        assert_eq!(parsed, json!({"key": "value"}));

        Ok(())
    }

    #[rstest]
    fn test_post_tool_use_input_deserialization_tool_output_alias() -> Result<(), TestError> {
        let json = r#"{
            "tool_name": "Bash",
            "tool_input": {"command": "ls"},
            "tool_response": {"stdout": "ok"},
            "mcp_context": null,
            "original_request_name": null
        }"#;

        let input: PostToolUseInput<'_> = serde_json::from_str(json)?;
        let parsed: serde_json::Value = serde_json::from_str(input.tool_output.0.get())?;

        assert_eq!(parsed, json!({"stdout": "ok"}));

        Ok(())
    }

    #[rstest]
    fn test_post_tool_use_input_deserialization_tool_output_new_name() -> Result<(), TestError> {
        let json = r#"{
            "tool_name": "Bash",
            "tool_input": {"command": "ls"},
            "tool_output": {"stdout": "ok"},
            "tool_output_source": "tool",
            "mcp_context": null,
            "original_request_name": null
        }"#;

        let input: PostToolUseInput<'_> = serde_json::from_str(json)?;
        let parsed: serde_json::Value = serde_json::from_str(input.tool_output.0.get())?;

        assert_eq!(parsed, json!({"stdout": "ok"}));
        assert_eq!(input.tool_output_source.as_deref(), Some("tool"));

        Ok(())
    }

    #[rstest]
    fn test_cwd_changed_input_deserialization_old_cwd_alias() -> Result<(), TestError> {
        let json = r#"{"previous_cwd": "/old", "new_cwd": "/new"}"#;
        let input: CwdChangedInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.old_cwd, "/old");

        Ok(())
    }

    #[rstest]
    fn test_cwd_changed_input_deserialization_old_cwd_new_name() -> Result<(), TestError> {
        let json = r#"{"old_cwd": "/old", "new_cwd": "/new"}"#;
        let input: CwdChangedInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.old_cwd, "/old");

        Ok(())
    }

    #[rstest]
    fn test_cwd_changed_input_deserialization_change_reason() -> Result<(), TestError> {
        let json = r#"{"old_cwd": "/old", "new_cwd": "/new", "change_reason": "cd command"}"#;
        let input: CwdChangedInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.change_reason.as_deref(), Some("cd command"));

        Ok(())
    }

    #[rstest]
    fn test_file_changed_input_deserialization_change_type_alias() -> Result<(), TestError> {
        let json = r#"{"file_path": "/a.txt", "event": "modified"}"#;
        let input: FileChangedInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.change_type.as_deref(), Some("modified"));

        Ok(())
    }

    #[rstest]
    fn test_file_changed_input_deserialization_change_type_new_name() -> Result<(), TestError> {
        let json = r#"{"file_path": "/a.txt", "change_type": "created"}"#;
        let input: FileChangedInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.change_type.as_deref(), Some("created"));

        Ok(())
    }

    #[rstest]
    fn test_notification_input_deserialization_severity() -> Result<(), TestError> {
        let json = r#"{"message": "disk low", "severity": "warning"}"#;
        let input: NotificationInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.severity.as_deref(), Some("warning"));

        Ok(())
    }

    #[rstest]
    fn test_session_start_input_deserialization_session_title() -> Result<(), TestError> {
        let json = r#"{"session_title": "Refactor auth module"}"#;
        let input: SessionStartInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.session_title.as_deref(), Some("Refactor auth module"));

        Ok(())
    }

    #[rstest]
    fn test_worktree_create_input_deserialization() -> Result<(), TestError> {
        let json = r#"{
            "subagent_name": "explorer",
            "worktree_id": "wt-1",
            "git_root": "/repo",
            "parent_path": "/repo/.worktrees/main"
        }"#;
        let input: WorktreeCreateInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.subagent_name.as_deref(), Some("explorer"));
        assert_eq!(input.worktree_id.as_deref(), Some("wt-1"));
        assert_eq!(input.git_root.as_deref(), Some("/repo"));
        assert_eq!(input.parent_path.as_deref(), Some("/repo/.worktrees/main"));

        Ok(())
    }

    #[rstest]
    #[case::init("init")]
    #[case::maintenance("maintenance")]
    fn test_setup_input_deserialization_trigger(#[case] trigger: &str) -> Result<(), TestError> {
        let json = format!(r#"{{"trigger": "{trigger}"}}"#);
        let input: SetupInput<'_> = serde_json::from_str(&json)?;

        assert_eq!(input.trigger, trigger);

        Ok(())
    }

    #[rstest]
    fn test_user_prompt_expansion_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"command_name": "/review", "prompt": "Review this PR"}"#;
        let input: UserPromptExpansionInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.command_name, "/review");
        assert_eq!(input.prompt.as_deref(), Some("Review this PR"));

        Ok(())
    }

    #[rstest]
    fn test_message_display_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"lines": ["line one", "line two"]}"#;
        let input: MessageDisplayInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.lines, vec!["line one", "line two"]);

        Ok(())
    }

    #[rstest]
    fn test_message_display_input_deserialization_defaults_to_empty() -> Result<(), TestError> {
        let input: MessageDisplayInput<'_> = serde_json::from_str("{}")?;
        assert!(input.lines.is_empty());
        Ok(())
    }

    #[rstest]
    fn test_permission_request_input_deserialization() -> Result<(), TestError> {
        let json = r#"{
            "tool_name": "Bash",
            "tool_input": {"command": "rm -rf /tmp/x"},
            "permission_rule_name": "Bash(rm:*)"
        }"#;
        let input: PermissionRequestInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.tool_name, "Bash");
        assert_eq!(input.permission_rule_name.as_deref(), Some("Bash(rm:*)"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(input.tool_input.0.get())?,
            json!({"command": "rm -rf /tmp/x"})
        );

        Ok(())
    }

    #[rstest]
    fn test_post_tool_use_failure_input_deserialization() -> Result<(), TestError> {
        let json = r#"{
            "tool_name": "Bash",
            "tool_input": {"command": "false"},
            "tool_error": "exit status 1"
        }"#;
        let input: PostToolUseFailureInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.tool_name, "Bash");
        assert_eq!(input.tool_error, "exit status 1");

        Ok(())
    }

    #[rstest]
    fn test_post_tool_batch_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"tool_calls": [{"tool_name": "Bash"}, {"tool_name": "Read"}]}"#;
        let input: PostToolBatchInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.tool_calls.len(), 2);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(input.tool_calls[0].0.get())?,
            json!({"tool_name": "Bash"})
        );

        Ok(())
    }

    #[rstest]
    fn test_post_tool_batch_input_deserialization_defaults_to_empty() -> Result<(), TestError> {
        let input: PostToolBatchInput<'_> = serde_json::from_str("{}")?;
        assert!(input.tool_calls.is_empty());
        Ok(())
    }

    #[rstest]
    fn test_permission_denied_input_deserialization() -> Result<(), TestError> {
        let json = r#"{
            "tool_name": "Bash",
            "tool_input": {"command": "curl evil.com"},
            "reason": "network access denied"
        }"#;
        let input: PermissionDeniedInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.tool_name, "Bash");
        assert_eq!(input.reason.as_deref(), Some("network access denied"));

        Ok(())
    }

    #[rstest]
    fn test_subagent_start_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"agent_type": "Explore", "prompt": "Find usages of foo"}"#;
        let input: SubagentStartInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.agent_type, "Explore");
        assert_eq!(input.prompt.as_deref(), Some("Find usages of foo"));

        Ok(())
    }

    #[rstest]
    fn test_subagent_stop_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"agent_type": "Explore", "result": "Found 3 usages"}"#;
        let input: SubagentStopInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.agent_type, "Explore");
        assert_eq!(input.result.as_deref(), Some("Found 3 usages"));

        Ok(())
    }

    #[rstest]
    fn test_task_created_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"task": {"id": "task-1", "title": "Write tests"}}"#;
        let input: TaskCreatedInput<'_> = serde_json::from_str(json)?;

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(input.task.0.get())?,
            json!({"id": "task-1", "title": "Write tests"})
        );

        Ok(())
    }

    #[rstest]
    fn test_task_completed_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"task": {"id": "task-1", "status": "done"}}"#;
        let input: TaskCompletedInput<'_> = serde_json::from_str(json)?;

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(input.task.0.get())?,
            json!({"id": "task-1", "status": "done"})
        );

        Ok(())
    }

    #[rstest]
    fn test_stop_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"message": "All done"}"#;
        let input: StopInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.message.as_deref(), Some("All done"));

        Ok(())
    }

    #[rstest]
    fn test_stop_failure_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"error_type": "rate_limit", "error_message": "Too many requests"}"#;
        let input: StopFailureInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.error_type, "rate_limit");
        assert_eq!(input.error_message, "Too many requests");

        Ok(())
    }

    #[rstest]
    fn test_teammate_idle_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"result": "Implemented feature X"}"#;
        let input: TeammateIdleInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.result.as_deref(), Some("Implemented feature X"));

        Ok(())
    }

    #[rstest]
    fn test_config_change_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"config_source": "project_settings", "changed_file": "/repo/.claude/settings.json"}"#;
        let input: ConfigChangeInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.config_source, "project_settings");
        assert_eq!(input.changed_file, "/repo/.claude/settings.json");

        Ok(())
    }

    #[rstest]
    fn test_post_compact_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"trigger": "auto", "summary": "Compacted 50 messages"}"#;
        let input: PostCompactInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.trigger.as_deref(), Some("auto"));
        assert_eq!(input.summary.as_deref(), Some("Compacted 50 messages"));

        Ok(())
    }

    #[rstest]
    fn test_elicitation_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"server_name": "filesystem", "request": {"prompt": "Confirm overwrite?"}}"#;
        let input: ElicitationInput<'_> = serde_json::from_str(json)?;

        assert_eq!(input.server_name.as_deref(), Some("filesystem"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(input.request.0.get())?,
            json!({"prompt": "Confirm overwrite?"})
        );

        Ok(())
    }

    #[rstest]
    fn test_elicitation_result_input_deserialization() -> Result<(), TestError> {
        let json = r#"{"result": {"accepted": true}}"#;
        let input: ElicitationResultInput<'_> = serde_json::from_str(json)?;

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(input.result.0.get())?,
            json!({"accepted": true})
        );

        Ok(())
    }

    #[rstest]
    fn test_hook_input_event_kind_pre_tool_use(grep_search_json: String) -> Result<(), TestError> {
        let input: PreToolUseInput<'_> = serde_json::from_str(&grep_search_json)?;
        let event = HookInputEvent::PreToolUse(input);

        assert_eq!(event.kind(), HookKind::PreToolUse);

        Ok(())
    }

    #[rstest]
    fn test_hook_input_event_kind_before_agent() -> Result<(), TestError> {
        let input: BeforeAgentInput<'_> = serde_json::from_str(r#"{"prompt": "hello"}"#)?;
        let event = HookInputEvent::BeforeAgent(input);

        assert_eq!(event.kind(), HookKind::BeforeAgent);

        Ok(())
    }

    #[rstest]
    fn test_hook_input_event_kind_session_start() -> Result<(), TestError> {
        let input: SessionStartInput<'_> = serde_json::from_str("{}")?;
        let event = HookInputEvent::SessionStart(input);

        assert_eq!(event.kind(), HookKind::SessionStart);

        Ok(())
    }

    #[rstest]
    fn test_hook_input_event_kind_elicitation_result() -> Result<(), TestError> {
        let json = r#"{"result": {"accepted": true}}"#;
        let input: ElicitationResultInput<'_> = serde_json::from_str(json)?;
        let event = HookInputEvent::ElicitationResult(input);

        assert_eq!(event.kind(), HookKind::ElicitationResult);

        Ok(())
    }

    #[rstest]
    fn test_hook_input_event_tool_name_pre_tool_use(
        grep_search_json: String,
    ) -> Result<(), TestError> {
        let input: PreToolUseInput<'_> = serde_json::from_str(&grep_search_json)?;
        let event = HookInputEvent::PreToolUse(input);

        assert_eq!(event.tool_name(), Some("grep_search"));

        Ok(())
    }

    #[rstest]
    fn test_hook_input_event_tool_name_permission_denied() -> Result<(), TestError> {
        let json = r#"{
            "tool_name": "Bash",
            "tool_input": {"command": "curl evil.com"},
            "reason": "network access denied"
        }"#;
        let input: PermissionDeniedInput<'_> = serde_json::from_str(json)?;
        let event = HookInputEvent::PermissionDenied(input);

        assert_eq!(event.tool_name(), Some("Bash"));

        Ok(())
    }

    #[rstest]
    fn test_hook_input_event_tool_name_before_agent_is_none() -> Result<(), TestError> {
        let input: BeforeAgentInput<'_> = serde_json::from_str(r#"{"prompt": "hello"}"#)?;
        let event = HookInputEvent::BeforeAgent(input);

        assert_eq!(event.tool_name(), None);

        Ok(())
    }

    #[rstest]
    fn test_hook_kind_count_matches_variant_count() {
        assert_eq!(HookKind::ElicitationResult as usize + 1, HookKind::COUNT);
    }
}
