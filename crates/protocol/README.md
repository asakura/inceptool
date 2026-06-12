# Protocol

`protocol` is the canonical communication interface for the `inceptool-rs` project.

It provides driver-agnostic data structures to standardize the payloads
exchanged between the CLI, the hook engine, and specific agent drivers
(like Claude or Gemini).

## Features

- **Decoupled Payloads**: By using `RawJson` and borrowing where possible,
  the protocol avoids parsing full tool payloads and model requests unless
  a hook actively inspects or alters them.
- **Hook Inputs & Outputs**: Every hook event documented at
  [code.claude.com/docs/en/hooks](https://code.claude.com/docs/en/hooks) —
  including the "Additional Hooks" (`PermissionRequest`, `PostToolUseFailure`,
  `PostToolBatch`, `SubagentStart`/`SubagentStop`, `TaskCreated`/`TaskCompleted`,
  `Stop`, `StopFailure`, `TeammateIdle`, `ConfigChange`, `PostCompact`,
  `Elicitation`/`ElicitationResult`, `MessageDisplay`, `UserPromptExpansion`,
  `PermissionDenied`, `Setup`) — has a fully concrete, zero-copy input and
  output struct. No event payload falls back to an untyped `RawJson` blob.
- **Driver Abstraction**: Defines the foundational `Driver` trait that any
  specific agent implementation must implement to integrate with `inceptool-rs`,
  plus the [`from_wire`](#from_wire--to_wire) / [`to_wire`](#from_wire--to_wire)
  helpers used to drive that trait end-to-end.

## Modularity

The crate is broken down into clean, single-purpose modules:

- `types`: Core base types (`RawJson`, `Decision`, `PermissionMode`, `Effort`,
  `EffortLevel`, `PermissionBehavior`).
- `session`: Connection records (`Conn`) and session-level metadata tracking.
- `input`: All inbound payloads provided to hook executions, plus the
  `HookKind` dispatch enum.
- `output`: All outbound payloads expected from hook executions.
- `driver`: The universal abstraction for an AI driver, plus the `from_wire`
  and `to_wire` entry points.
- `error`: Strongly typed errors for protocol mapping operations.

## Hook Inputs & Outputs

`HookInputEvent<'a>` and `HookOutputEvent` are the two top-level enums. Every
variant of `HookInputEvent` has a matching variant in `HookOutputEvent` (same
name, `*Input` vs `*Output` struct), with the following 35 events:

`PreToolUse`, `PostToolUse`, `BeforeAgent`, `AfterAgent`, `BeforeModel`,
`AfterModel`, `BeforeToolSelection`, `SessionStart`, `SessionEnd`,
`Notification`, `PreCompact`, `CwdChanged`, `FileChanged`,
`InstructionsLoaded`, `UserPromptSubmit`, `WorktreeCreate`, `WorktreeRemove`,
`Setup`, `UserPromptExpansion`, `MessageDisplay`, `PermissionRequest`,
`PostToolUseFailure`, `PostToolBatch`, `PermissionDenied`, `SubagentStart`,
`SubagentStop`, `TaskCreated`, `TaskCompleted`, `Stop`, `StopFailure`,
`TeammateIdle`, `ConfigChange`, `PostCompact`, `Elicitation`,
`ElicitationResult`.

`HookOutputEvent` is `#[serde(untagged)]`, so it serializes directly as the
underlying variant's struct (e.g. `{"decision": "allow", ...}` rather than
`{"PreToolUse": {...}}`). Two variants (`WorktreeRemove`, `StopFailure`,
`ElicitationResult`) reuse a shared `EmptyOutput {}` struct since they carry
no return fields.

### `HookKind`

`HookKind` is a fieldless enum that mirrors `HookInputEvent` one-for-one (35
variants, exposed as `HookKind::COUNT`). It exists so the engine's `Registry`
can bucket stage pipelines into a fixed-size array (`[Vec<PipelineEntry>;
HookKind::COUNT]`) and dispatch by `kind as usize` without hashing or
allocating.

`HookInputEvent` exposes two helper methods built around this:

- `kind(&self) -> HookKind` — returns the discriminant for the event, used by
  the engine to select the correct pipeline.
- `tool_name(&self) -> Option<&str>` — returns the tool name for the variants
  that carry one (`PreToolUse`, `PostToolUse`, `PermissionRequest`,
  `PostToolUseFailure`, `PermissionDenied`); all other variants return `None`.

### `HookOutputEvent` accessors

`HookOutputEvent` provides a set of accessor/mutator methods so callers (the
engine, drivers, the CLI) don't need to match on every variant themselves:

- `decision(&self) -> Option<Decision>` — the hook's decision, for the
  variants that carry one.
- `set_decision(&mut self, decision: Decision)` — overwrites the `decision`
  field on variants that carry one (e.g. `PreToolUse`, `PostToolUse`,
  `Stop`, `TaskCreated`, ...). It is a no-op for variants without a
  `decision` field, such as `SessionStart` or `PermissionRequest` (which
  conveys its outcome via `behavior` instead).
- `reason(&self) -> Option<&str>` — the reason string associated with the
  decision, if any.
- `halt(&self) -> Option<bool>` — whether the hook is requesting the process
  halt entirely (`PreToolUse`, `PostToolUse`, `BeforeAgent`, `AfterAgent`,
  `BeforeModel`, `AfterModel`).
- `suppress_output(&self) -> Option<bool>` — whether to suppress the tool's
  output from the model (`PostToolUse` only).
- `system_message(&self) -> Option<&str>` — a system message to surface to
  the user (`SessionStart`, `SessionEnd`, `Notification`, `PreCompact`,
  `InstructionsLoaded`, `PostCompact`).
- `exit_metadata(&self) -> (Option<i32>, Option<&str>)` — the `(exit_code,
reason)` pair for variants that can halt the process with a specific exit
  code (`PreToolUse`, `PostToolUse`, `BeforeAgent`, `AfterAgent`,
  `BeforeModel`, `AfterModel`, `PreCompact`). All other variants return
  `(None, None)`.

## `from_wire` / `to_wire`

`driver` defines two generic free functions that drive the `Driver` trait
end-to-end:

- `from_wire<D: Driver>(driver: &D, raw_json: &str) -> Result<Conn, D::Error>`
  — deserializes the driver's wire-format input (`D::InputWire`) from a raw
  JSON string and maps it into a canonical `Conn` via `Driver::map_input`.
- `to_wire<D: Driver>(driver: &D, event_name: &str, output: &HookOutputEvent) -> Result<String, D::Error>`
  — maps a canonical `HookOutputEvent` into the driver's wire-format output
  (`D::OutputWire`) via `Driver::map_output` and serializes it back to a JSON
  string.

These are the functions the CLI entry point calls to translate between the
raw JSON it reads from stdin/writes to stdout and the canonical `Conn` /
`HookOutputEvent` types used throughout the engine and stages.

## Session Metadata

In addition to the per-event payload, every hook receives a common
`SessionMeta` (`session_id`, `transcript_path`, `cwd`, `timestamp`, `driver`,
`driver_meta`) plus the fields documented as common to all hook inputs:

- `permission_mode`: the active `PermissionMode` (`default`, `plan`,
  `acceptEdits`, `auto`, `dontAsk`, `bypassPermissions`, or `unknown` for
  forward-compatibility with future modes).
- `effort`: the configured `Effort`, exposing an `EffortLevel`
  (`low`, `medium`, `high`, `xhigh`, `max`, or `unknown`).
- `agent_id` / `agent_type`: identify the agent handling the session, when
  the hook is running in a sub-agent context.

All four fields are optional and default to `None` when a driver does not
provide them.
