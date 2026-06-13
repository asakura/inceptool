//! [`Registry`] runs a sequence of [`Stage`]s against an incoming
//! [`Conn`](inceptool_protocol::Conn), Plug-style.
//!
//! ## Pipeline semantics
//!
//! [`Registry`] holds one pipeline per
//! [`HookKind`](inceptool_protocol::HookKind) — a fixed-size array of stage
//! buckets built once at construction time from [`Registry::register`] calls.
//! [`Registry::run_pipeline`] takes the [`HookKind`](inceptool_protocol::HookKind)
//! to dispatch on as an explicit argument (the caller determines it via
//! [`Driver::hook_kind`](inceptool_protocol::Driver::hook_kind) from the CLI
//! invocation, not by inspecting `conn`), then:
//!
//! 1. Selects the bucket for that [`HookKind`](inceptool_protocol::HookKind)
//!    and iterates its stages in registration order.
//! 2. Within that bucket, a stage only runs if its [`Stage::tool_names`] matches
//!    `conn.event`'s tool name (via
//!    [`HookInputEvent::tool_name`](inceptool_protocol::HookInputEvent::tool_name)) —
//!    `"*"` matches any tool name, including events that carry none at all.
//! 3. If [`Stage::run`] returns `Some(output)`, that output replaces the pipeline's
//!    running result.
//! 4. If the output is *terminal* (see [`is_terminal`]), the pipeline stops
//!    immediately and that output is returned as-is. Otherwise execution
//!    continues, allowing later stages to add context or override the result.
//!
//! ## Decision combination
//!
//! A [`Decision`](inceptool_protocol::Decision) of [`Deny`](inceptool_protocol::Decision::Deny)
//! or [`Block`](inceptool_protocol::Decision::Block) is terminal: it halts the pipeline
//! immediately and is returned as the final output, regardless of what earlier stages
//! decided.
//!
//! [`Allow`](inceptool_protocol::Decision::Allow) and [`Ask`](inceptool_protocol::Decision::Ask)
//! are *not* terminal on their own, so later stages still run. Once all matching stages have
//! run without any `Deny`/`Block`, the combined decision is:
//!
//! - [`Ask`](inceptool_protocol::Decision::Ask), if any stage returned `Ask`.
//! - [`Allow`](inceptool_protocol::Decision::Allow), if at least one stage returned `Allow`
//!   and none returned `Ask`.
//! - Unset, if no matching stage expressed a decision.
//!
//! This combined decision is written back onto the final output via
//! [`HookOutputEvent::set_decision`](inceptool_protocol::HookOutputEvent::set_decision),
//! so a single early `Allow` can never silently suppress a later `Ask`/`Deny`/`Block`.
//!
//! If no stage produces an output, [`Registry::run_pipeline`] returns `Ok(None)`,
//! signaling the caller to fall back to the default (allow) behavior.
//!
//! ## Errors
//!
//! Stages report failures via [`EngineError`], which immediately aborts the
//! pipeline and propagates to the caller.

use crate::{EngineError, Stage};

use inceptool_protocol::{Conn, Decision, HookKind, HookOutputEvent};

/// A registered stage paired with the tool names it runs for.
struct PipelineEntry {
    tool_names: &'static [&'static str],
    stage: Box<dyn Stage>,
}

/// Manages registration and execution of stages.
///
/// Stages are bucketed into one pipeline per [`HookKind`], built once at
/// construction time from [`Registry::register`] calls.
/// [`Registry::run_pipeline`] only considers the bucket for the [`HookKind`]
/// passed to it.
pub struct Registry {
    pipelines: [Vec<PipelineEntry>; HookKind::COUNT],
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    /// Creates an empty registry with no stages registered.
    pub fn new() -> Self {
        Self {
            pipelines: core::array::from_fn(|_| Vec::new()),
        }
    }

    /// Registers a stage into the pipeline for its [`Stage::hook`].
    ///
    /// Stages run in the order they are registered within that pipeline.
    pub fn register<S: Stage + 'static>(&mut self, stage: S) {
        let tool_names = stage.tool_names();
        let kind = stage.hook();

        self.pipelines[kind as usize].push(PipelineEntry {
            tool_names,
            stage: Box::new(stage),
        });
    }

    /// Run the pipeline for a given connection.
    /// Folds the outputs of all matching stages.
    ///
    /// # Examples
    ///
    /// ```
    /// # use inceptool_engine::EngineError;
    /// # fn main() -> Result<(), EngineError> {
    /// use inceptool_engine::{Registry, Stage};
    /// use inceptool_protocol::{
    ///     BeforeAgentInput, BeforeAgentOutput, Conn, Decision, HookInputEvent, HookKind,
    ///     HookOutputEvent, SessionMeta,
    /// };
    /// use std::borrow::Cow;
    ///
    /// struct DenyStage;
    ///
    /// impl Stage for DenyStage {
    ///     fn name(&self) -> &'static str {
    ///         "deny"
    ///     }
    ///
    ///     fn hook(&self) -> HookKind {
    ///         HookKind::BeforeAgent
    ///     }
    ///
    ///     fn run(&self, _conn: &mut Conn<'_>) -> Result<Option<HookOutputEvent>, EngineError> {
    ///         Ok(Some(HookOutputEvent::BeforeAgent(BeforeAgentOutput {
    ///             decision: Some(Decision::Block),
    ///             reason: Some("blocked by example".into()),
    ///             ..Default::default()
    ///         })))
    ///     }
    /// }
    ///
    /// let mut registry = Registry::new();
    /// registry.register(DenyStage);
    ///
    /// let mut conn = Conn {
    ///     session: SessionMeta {
    ///         session_id: Cow::Borrowed("session-1"),
    ///         transcript_path: None,
    ///         cwd: None,
    ///         timestamp: None,
    ///         driver: Cow::Borrowed("Test"),
    ///         driver_meta: None,
    ///         permission_mode: None,
    ///         effort: None,
    ///         agent_id: None,
    ///         agent_type: None,
    ///     },
    ///     event: HookInputEvent::BeforeAgent(BeforeAgentInput {
    ///         prompt: Cow::Borrowed("rm -rf /"),
    ///     }),
    /// };
    ///
    /// let output = registry.run_pipeline(HookKind::BeforeAgent, &mut conn)?;
    /// assert_eq!(output.and_then(|o| o.decision()), Some(Decision::Block));
    /// # Ok(())
    /// # }
    /// ```
    pub fn run_pipeline(
        &self,
        kind: HookKind,
        conn: &mut Conn<'_>,
    ) -> Result<Option<HookOutputEvent>, EngineError> {
        let mut final_output = None;
        let mut combined_decision = None;

        for entry in &self.pipelines[kind as usize] {
            if !tool_names_match(entry.tool_names, conn.event.tool_name()) {
                continue;
            }

            let Some(output) = entry.stage.run(conn)? else {
                continue;
            };

            if is_terminal(&output) {
                return Ok(Some(output));
            }

            combined_decision = match (combined_decision, output.decision()) {
                (Some(Decision::Ask), _) | (_, Some(Decision::Ask)) => Some(Decision::Ask),
                (_, Some(Decision::Allow)) => Some(Decision::Allow),
                (existing, _) => existing,
            };

            final_output = Some(output);
        }

        if let (Some(output), Some(decision)) = (final_output.as_mut(), combined_decision) {
            output.set_decision(decision);
        }

        Ok(final_output)
    }
}

/// Returns whether `tool_names` matches `event_tool_name`.
///
/// `"*"` matches any tool name, including `None` (events that carry no tool
/// name at all). Otherwise, matches only if `event_tool_name` is `Some` and is
/// contained in `tool_names`.
fn tool_names_match(tool_names: &[&str], event_tool_name: Option<&str>) -> bool {
    if tool_names.contains(&"*") {
        return true;
    }

    matches!(event_tool_name, Some(name) if tool_names.contains(&name))
}

/// Determines whether a stage output should halt further stage execution in the pipeline.
///
/// `PermissionRequest` is handled separately because its decision is conveyed via
/// `behavior` (a [`PermissionBehavior`](inceptool_protocol::PermissionBehavior)),
/// not the generic [`Decision`](inceptool_protocol::Decision) accessor.
///
/// Only [`Decision::Deny`] and [`Decision::Block`] are terminal decisions: they
/// immediately abort the pipeline. [`Decision::Allow`] and [`Decision::Ask`] are
/// combined across all matching stages instead (see the module docs).
fn is_terminal(output: &HookOutputEvent) -> bool {
    if let HookOutputEvent::PermissionRequest(o) = output {
        return o.behavior.is_some();
    }

    matches!(
        output.decision(),
        Some(Decision::Deny) | Some(Decision::Block)
    ) || output.halt() == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    use inceptool_protocol::{
        BeforeAgentInput, BeforeAgentOutput, Decision, HookInputEvent, HookKind,
        PermissionBehavior, PermissionRequestOutput, PreToolUseInput, SessionMeta,
    };

    use rstest::{fixture, rstest};

    use core::assert_matches;
    use std::borrow::Cow;

    #[derive(thiserror::Error, Debug)]
    pub enum TestError {
        #[error(transparent)]
        Engine(#[from] EngineError),
        #[error(transparent)]
        Json(#[from] serde_json::Error),
        #[error("Test failure: {0}")]
        Failure(String),
    }

    /// A stage with statically-configured `hook`/`tool_names`/`run` behavior, used to
    /// drive [`Registry::run_pipeline`] without depending on real stage implementations.
    struct StubStage {
        name: &'static str,
        hook: HookKind,
        tool_names: &'static [&'static str],
        outcome: StubOutcome,
    }

    enum StubOutcome {
        None,
        Output(fn() -> HookOutputEvent),
        Error,
    }

    impl Stage for StubStage {
        fn name(&self) -> &'static str {
            self.name
        }

        fn hook(&self) -> HookKind {
            self.hook
        }

        fn tool_names(&self) -> &'static [&'static str] {
            self.tool_names
        }

        fn run(&self, _conn: &mut Conn<'_>) -> Result<Option<HookOutputEvent>, EngineError> {
            match self.outcome {
                StubOutcome::None => Ok(None),
                StubOutcome::Output(build) => Ok(Some(build())),
                StubOutcome::Error => Err(EngineError::StageExecution(self.name.to_string())),
            }
        }
    }

    /// A [`SessionMeta`] with placeholder values, shared by all `Conn` fixtures below.
    fn session_meta() -> SessionMeta<'static> {
        SessionMeta {
            session_id: Cow::Borrowed("test-session"),
            transcript_path: None,
            cwd: None,
            timestamp: None,
            driver: Cow::Borrowed("Test"),
            driver_meta: None,
            permission_mode: None,
            effort: None,
            agent_id: None,
            agent_type: None,
        }
    }

    /// Builds a `PreToolUse` `Conn` from a JSON [`PreToolUseInput`] payload.
    fn pre_tool_use_conn(json: &str) -> Result<Conn<'_>, TestError> {
        let input: PreToolUseInput<'_> = serde_json::from_str(json)?;
        Ok(Conn {
            session: session_meta(),
            event: HookInputEvent::PreToolUse(input),
        })
    }

    fn additional_context_output() -> HookOutputEvent {
        HookOutputEvent::BeforeAgent(BeforeAgentOutput {
            additional_context: Some("extra context".into()),
            ..Default::default()
        })
    }

    fn block_decision_output() -> HookOutputEvent {
        HookOutputEvent::BeforeAgent(BeforeAgentOutput {
            decision: Some(Decision::Block),
            ..Default::default()
        })
    }

    fn deny_decision_output() -> HookOutputEvent {
        HookOutputEvent::BeforeAgent(BeforeAgentOutput {
            decision: Some(Decision::Deny),
            ..Default::default()
        })
    }

    fn allow_decision_output() -> HookOutputEvent {
        HookOutputEvent::BeforeAgent(BeforeAgentOutput {
            decision: Some(Decision::Allow),
            ..Default::default()
        })
    }

    fn ask_decision_output() -> HookOutputEvent {
        HookOutputEvent::BeforeAgent(BeforeAgentOutput {
            decision: Some(Decision::Ask),
            ..Default::default()
        })
    }

    fn halt_output() -> HookOutputEvent {
        HookOutputEvent::BeforeAgent(BeforeAgentOutput {
            halt: Some(true),
            ..Default::default()
        })
    }

    fn permission_allow_output() -> HookOutputEvent {
        HookOutputEvent::PermissionRequest(PermissionRequestOutput {
            behavior: Some(PermissionBehavior::Allow),
            ..Default::default()
        })
    }

    fn permission_undecided_output() -> HookOutputEvent {
        HookOutputEvent::PermissionRequest(PermissionRequestOutput::default())
    }

    #[fixture]
    fn conn() -> Conn<'static> {
        Conn {
            session: session_meta(),
            event: HookInputEvent::BeforeAgent(BeforeAgentInput {
                prompt: Cow::Borrowed("hello"),
            }),
        }
    }

    #[fixture]
    fn pre_tool_use_bash_json() -> String {
        r#"{
            "tool_name": "Bash",
            "tool_input": {"command": "ls"},
            "mcp_context": null,
            "original_request_name": null
        }"#
        .to_string()
    }

    #[fixture]
    fn pre_tool_use_read_json() -> String {
        r#"{
            "tool_name": "Read",
            "tool_input": {"file_path": "/a.txt"},
            "mcp_context": null,
            "original_request_name": null
        }"#
        .to_string()
    }

    #[rstest]
    #[case::block(Decision::Block)]
    #[case::deny(Decision::Deny)]
    fn test_is_terminal_true_for_deny_or_block_decision(#[case] decision: Decision) {
        let output = HookOutputEvent::BeforeAgent(BeforeAgentOutput {
            decision: Some(decision),
            ..Default::default()
        });

        assert!(is_terminal(&output));
    }

    #[rstest]
    #[case::allow(Decision::Allow)]
    #[case::ask(Decision::Ask)]
    fn test_is_terminal_false_for_allow_or_ask_decision(#[case] decision: Decision) {
        let output = HookOutputEvent::BeforeAgent(BeforeAgentOutput {
            decision: Some(decision),
            ..Default::default()
        });

        assert!(!is_terminal(&output));
    }

    #[rstest]
    #[case::halt_false(Some(false))]
    #[case::halt_unset(None)]
    fn test_is_terminal_false_without_decision_or_explicit_halt(#[case] halt: Option<bool>) {
        let output = HookOutputEvent::BeforeAgent(BeforeAgentOutput {
            halt,
            ..Default::default()
        });

        assert!(!is_terminal(&output));
    }

    #[rstest]
    fn test_is_terminal_true_when_halt_true() {
        assert!(is_terminal(&halt_output()));
    }

    #[rstest]
    #[case::allow(Some(PermissionBehavior::Allow), true)]
    #[case::deny(Some(PermissionBehavior::Deny), true)]
    #[case::undecided(None, false)]
    fn test_is_terminal_permission_request(
        #[case] behavior: Option<PermissionBehavior>,
        #[case] expected: bool,
    ) {
        let output = HookOutputEvent::PermissionRequest(PermissionRequestOutput {
            behavior,
            ..Default::default()
        });

        assert_eq!(is_terminal(&output), expected);
    }

    #[rstest]
    fn test_run_pipeline_with_no_stages_returns_none(mut conn: Conn<'_>) -> Result<(), TestError> {
        let registry = Registry::new();

        assert_matches!(
            registry.run_pipeline(HookKind::BeforeAgent, &mut conn)?,
            None
        );

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_default_registry_has_no_stages(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let registry = Registry::default();

        assert_matches!(
            registry.run_pipeline(HookKind::BeforeAgent, &mut conn)?,
            None
        );

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_skips_non_matching_stage(mut conn: Conn<'_>) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "non-matching",
            hook: HookKind::PostToolUse,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(block_decision_output),
        });

        assert_matches!(
            registry.run_pipeline(HookKind::BeforeAgent, &mut conn)?,
            None
        );

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_ignores_matching_stage_with_no_output(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "no-output",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::None,
        });

        assert_matches!(
            registry.run_pipeline(HookKind::BeforeAgent, &mut conn)?,
            None
        );

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_returns_output_from_matching_stage(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "context",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(additional_context_output),
        });

        let output = registry
            .run_pipeline(HookKind::BeforeAgent, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_matches!(output, HookOutputEvent::BeforeAgent(_));

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_non_terminal_output_lets_later_stages_run(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "context",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(additional_context_output),
        });

        registry.register(StubStage {
            name: "block",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(block_decision_output),
        });

        let output = registry
            .run_pipeline(HookKind::BeforeAgent, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_eq!(output.decision(), Some(Decision::Block));

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_terminal_decision_stops_remaining_stages(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "block",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(block_decision_output),
        });

        registry.register(StubStage {
            name: "unreachable",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Error,
        });

        let output = registry
            .run_pipeline(HookKind::BeforeAgent, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_eq!(output.decision(), Some(Decision::Block));

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_halt_flag_stops_remaining_stages(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "halt",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(halt_output),
        });

        registry.register(StubStage {
            name: "unreachable",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Error,
        });

        let output = registry
            .run_pipeline(HookKind::BeforeAgent, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_eq!(output.halt(), Some(true));

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_permission_request_with_behavior_stops_remaining_stages(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "permission",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(permission_allow_output),
        });

        registry.register(StubStage {
            name: "unreachable",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Error,
        });

        let output = registry.run_pipeline(HookKind::BeforeAgent, &mut conn)?;

        assert_matches!(
            output,
            Some(HookOutputEvent::PermissionRequest(
                PermissionRequestOutput {
                    behavior: Some(PermissionBehavior::Allow),
                    ..
                }
            ))
        );

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_permission_request_without_behavior_lets_later_stages_run(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "permission",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(permission_undecided_output),
        });

        registry.register(StubStage {
            name: "block",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(block_decision_output),
        });

        let output = registry
            .run_pipeline(HookKind::BeforeAgent, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_eq!(output.decision(), Some(Decision::Block));

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_propagates_stage_error(mut conn: Conn<'_>) {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "failing",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Error,
        });

        let result = registry.run_pipeline(HookKind::BeforeAgent, &mut conn);

        assert_matches!(result, Err(EngineError::StageExecution(_)));
    }

    #[rstest]
    fn test_run_pipeline_all_allow_decisions_combine_to_allow(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "first",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(allow_decision_output),
        });

        registry.register(StubStage {
            name: "second",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(allow_decision_output),
        });

        let output = registry
            .run_pipeline(HookKind::BeforeAgent, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_eq!(output.decision(), Some(Decision::Allow));

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_allow_decision_does_not_stop_remaining_stages(mut conn: Conn<'_>) {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "allow",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(allow_decision_output),
        });

        registry.register(StubStage {
            name: "later",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Error,
        });

        let result = registry.run_pipeline(HookKind::BeforeAgent, &mut conn);

        assert_matches!(result, Err(EngineError::StageExecution(_)));
    }

    #[rstest]
    fn test_run_pipeline_ask_decision_does_not_stop_remaining_stages(mut conn: Conn<'_>) {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "ask",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(ask_decision_output),
        });

        registry.register(StubStage {
            name: "later",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Error,
        });

        let result = registry.run_pipeline(HookKind::BeforeAgent, &mut conn);

        assert_matches!(result, Err(EngineError::StageExecution(_)));
    }

    #[rstest]
    fn test_run_pipeline_combines_allow_then_ask_into_ask(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "allow",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(allow_decision_output),
        });

        registry.register(StubStage {
            name: "ask",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(ask_decision_output),
        });

        let output = registry
            .run_pipeline(HookKind::BeforeAgent, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_eq!(output.decision(), Some(Decision::Ask));

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_ask_decision_is_not_downgraded_by_later_allow(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "ask",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(ask_decision_output),
        });

        registry.register(StubStage {
            name: "allow",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(allow_decision_output),
        });

        let output = registry
            .run_pipeline(HookKind::BeforeAgent, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_eq!(output.decision(), Some(Decision::Ask));

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_allow_decision_does_not_suppress_later_deny(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "allow",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(allow_decision_output),
        });

        registry.register(StubStage {
            name: "deny",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(deny_decision_output),
        });

        let output = registry
            .run_pipeline(HookKind::BeforeAgent, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_eq!(output.decision(), Some(Decision::Deny));

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_ask_decision_does_not_suppress_later_deny(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "ask",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(ask_decision_output),
        });

        registry.register(StubStage {
            name: "deny",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(deny_decision_output),
        });

        let output = registry
            .run_pipeline(HookKind::BeforeAgent, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_eq!(output.decision(), Some(Decision::Deny));

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_no_decision_from_any_stage_leaves_decision_unset(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "context",
            hook: HookKind::BeforeAgent,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(additional_context_output),
        });

        let output = registry
            .run_pipeline(HookKind::BeforeAgent, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_eq!(output.decision(), None);

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_wildcard_tool_names_matches_any_tool(
        pre_tool_use_bash_json: String,
    ) -> Result<(), TestError> {
        let mut conn = pre_tool_use_conn(&pre_tool_use_bash_json)?;
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "wildcard",
            hook: HookKind::PreToolUse,
            tool_names: ["*"].as_slice(),
            outcome: StubOutcome::Output(additional_context_output),
        });

        let output = registry
            .run_pipeline(HookKind::PreToolUse, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_matches!(output, HookOutputEvent::BeforeAgent(_));

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_specific_tool_name_matches(
        pre_tool_use_bash_json: String,
    ) -> Result<(), TestError> {
        let mut conn = pre_tool_use_conn(&pre_tool_use_bash_json)?;
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "bash-only",
            hook: HookKind::PreToolUse,
            tool_names: ["Bash", "run_shell_command"].as_slice(),
            outcome: StubOutcome::Output(additional_context_output),
        });

        let output = registry
            .run_pipeline(HookKind::PreToolUse, &mut conn)?
            .ok_or_else(|| TestError::Failure("expected an output".into()))?;

        assert_matches!(output, HookOutputEvent::BeforeAgent(_));

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_specific_tool_name_does_not_match_skips_stage(
        pre_tool_use_read_json: String,
    ) -> Result<(), TestError> {
        let mut conn = pre_tool_use_conn(&pre_tool_use_read_json)?;
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "bash-only",
            hook: HookKind::PreToolUse,
            tool_names: ["Bash", "run_shell_command"].as_slice(),
            outcome: StubOutcome::Error,
        });

        assert_matches!(
            registry.run_pipeline(HookKind::PreToolUse, &mut conn)?,
            None
        );

        Ok(())
    }

    #[rstest]
    fn test_run_pipeline_non_wildcard_tool_names_never_match_event_without_tool_name(
        mut conn: Conn<'_>,
    ) -> Result<(), TestError> {
        let mut registry = Registry::new();

        registry.register(StubStage {
            name: "bash-only",
            hook: HookKind::BeforeAgent,
            tool_names: ["Bash"].as_slice(),
            outcome: StubOutcome::Error,
        });

        assert_matches!(
            registry.run_pipeline(HookKind::BeforeAgent, &mut conn)?,
            None
        );

        Ok(())
    }
}
