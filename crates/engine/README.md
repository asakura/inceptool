# Inceptool Engine

`inceptool-engine` is the runtime core of `inceptool-rs`. It defines the
[`Stage`] trait that pipeline stages implement, and the [`Registry`] that
builds and executes a pipeline of stages against an incoming
[`Conn`](../protocol) — Plug-style, but using `&mut Conn` instead of
immutable structs for performance and simplicity.

A driver (e.g. `driver-claude`, `driver-gemini`) normalizes a wire-level hook
event into a `Conn`, and maps the CLI's raw hook name to a `HookKind` via
`Driver::hook_kind`. `Registry::run_pipeline(kind, conn)` then selects and
runs the stages registered for that `HookKind`, folding their outputs into a
single `HookOutputEvent` that the caller serializes back to the agent.

## The Stage Trait

A [`Stage`] is `Send + Sync` middleware that inspects and/or mutates a
`Conn`. It requires:

- `fn name(&self) -> &'static str` — a unique, human-readable name for the
  stage (used for diagnostics/logging).
- `fn hook(&self) -> HookKind` — the single [`HookKind`](inceptool_protocol::HookKind)
  this stage runs for. `Registry::register` places the stage into the
  pipeline bucket for this kind, and `Stage::run` is never invoked for events
  of any other kind.
- `fn tool_names(&self) -> &'static [&'static str]` — the tool names this
  stage applies to. Defaults to `&["*"]`, which matches any tool name,
  including events whose `tool_name()` is `None`.
- `fn run(&self, conn: &mut Conn) -> Result<Option<HookOutputEvent>, EngineError>` —
  processes the connection. Returns `Some(HookOutputEvent)` if the stage
  wants to override data, add context, or halt the pipeline; returns `None`
  to let the next stage run unchanged.

## Pipeline / Registry

[`Registry`] holds one pipeline per `HookKind` — a fixed-size array of
`Vec<PipelineEntry>` buckets (`HookKind::COUNT` entries), built up at
construction time via repeated `Registry::register` calls. Each
`PipelineEntry` pairs a boxed `Stage` with the `tool_names` it was registered
with.

- **`Registry::new()`** — creates an empty registry with no stages
  registered (all buckets empty). `Registry::default()` is equivalent.
- **`Registry::register(stage)`** — reads `stage.hook()` and
  `stage.tool_names()`, then pushes the stage into the bucket for that
  `HookKind`. Stages run in the order they are registered within a bucket.
- **`Registry::run_pipeline(kind, conn)`** —
  1. Selects the bucket for the `kind` ([`HookKind`](inceptool_protocol::HookKind))
     passed in. The caller determines this via `Driver::hook_kind` from the
     CLI invocation — not by inspecting `conn`.
  2. Iterates that bucket's stages in registration order. A stage only runs
     if its `tool_names` matches `conn.event.tool_name()` via
     `tool_names_match`: `"*"` matches any tool name (including events that
     carry no tool name at all — `tool_name() == None`); otherwise the event
     must carry a tool name that's contained in `tool_names`.
  3. If `Stage::run` returns `Some(output)`, that output becomes (replaces)
     the pipeline's running result.
  4. If that output is **terminal** (see below), the pipeline stops
     immediately and the output is returned as-is — later stages
     (including any that would error) never run.
  5. Otherwise, execution continues so later stages can add context or
     override the result, and the running decision is folded into
     `combined_decision` (see "Decision Combination" below).
  6. If no stage in the bucket produced an output (or no stage matched),
     `run_pipeline` returns `Ok(None)`, signaling the caller to fall back to
     the default (allow) behavior.

### Terminal outputs

`is_terminal(output)` determines whether an output halts the pipeline:

- For `HookOutputEvent::PermissionRequest`, the output is terminal iff
  `behavior` is `Some(_)` (a `PermissionBehavior` decision has been made).
  `PermissionRequest` is handled separately because its decision is conveyed
  via `behavior`, not the generic `Decision` accessor.
- For all other output kinds, the output is terminal if
  `output.decision()` is `Some(Decision::Deny)` or `Some(Decision::Block)`,
  **or** if `output.halt() == Some(true)`.
- `Decision::Allow` and `Decision::Ask` are **not** terminal on their own —
  later stages still run.

## Decision Combination

While the pipeline runs, `run_pipeline` accumulates a `combined_decision`
across all non-terminal outputs:

- `Decision::Ask` wins over everything: if either the existing combined
  decision or the new output's decision is `Ask`, the combined decision
  becomes `Ask` (and stays `Ask` even if a later stage returns `Allow`).
- `Decision::Allow` is recorded if the new output's decision is `Allow` and
  the combined decision isn't already `Ask`.
- Otherwise the existing combined decision is kept (e.g. an output with no
  decision at all doesn't change it).

So once all matching stages have run without any `Deny`/`Block` (which would
have short-circuited the pipeline as terminal), the final combined decision
is:

- `Ask`, if any stage returned `Ask`.
- `Allow`, if at least one stage returned `Allow` and none returned `Ask`.
- Unset (`None`), if no matching stage expressed a decision at all.

This combined decision is written back onto the final output via
[`HookOutputEvent::set_decision`](inceptool_protocol::HookOutputEvent::set_decision)
before it's returned, so a single early `Allow` can never silently suppress a
later `Ask`, `Deny`, or `Block` from another stage. If no stage produced an
output, there is nothing to write the decision onto, and `run_pipeline`
simply returns `Ok(None)`.

## Errors

[`EngineError`] is the crate's `thiserror`-based error enum, with a single
variant: `StageExecution(String)`, raised when a stage's `run` returns
`Err(_)`. If any stage returns an error, `run_pipeline` aborts the pipeline
immediately and propagates the error (`Err(EngineError)`) to the caller —
no further stages run, and no output is returned for that invocation.

[`Stage`]: src/stage.rs
[`Registry`]: src/registry.rs
[`EngineError`]: src/error.rs
