# inceptool-driver-claude

Claude Code driver implementation for the `inceptool` protocol.

This crate implements the [`Driver`](../protocol/src/driver.rs) trait from
`inceptool-protocol` for [Claude Code](https://code.claude.com). It is the
adapter layer that lets the rest of `inceptool` (the engine, stages, and
CLI) speak Claude Code's hook JSON wire format while working internally with
the protocol's normalized, driver-agnostic types (`Conn`, `HookInputEvent`,
`HookOutputEvent`).

## Responsibilities

The crate's sole exported type, [`ClaudeDriver`](src/driver.rs), is a unit
struct that implements `Driver` with:

- `type InputWire<'a> = &'a serde_json::value::RawValue` — Claude Code hook
  payloads are consumed as raw, borrowed JSON and only parsed once their
  concrete shape is known.
- `type OutputWire<'a> = ClaudeOutputWire<'a>` — the JSON shape Claude Code
  expects back on stdout from a hook.
- `type Error = ClaudeDriverError`.

### `map_input` (parsing Claude's JSON into `Conn`)

`map_input` (invoked via `inceptool_protocol::from_wire`) does the following:

1. Deserializes a small `ClaudeMeta` struct from the raw JSON to read the
   common envelope fields: `session_id`, `transcript_path`, `cwd`,
   `hook_event_name`, `permission_mode`, `effort`, `agent_id`, and
   `agent_type`.
2. Re-parses the same raw JSON as a borrowed `&RawValue` so the full,
   unmodified payload can be retained as `driver_meta` on the resulting
   `Conn` (zero-copy — no second allocation of the JSON text).
3. Matches on `hook_event_name` and deserializes the same raw JSON a second
   time into the matching `HookInputEvent` variant (e.g. `"PreToolUse"` ->
   `HookInputEvent::PreToolUse`, `"PostToolUse"` -> `HookInputEvent::PostToolUse`,
   etc.). Every hook event documented for Claude Code is handled, including
   the "additional hooks": `PostToolUseFailure`, `PostToolBatch`,
   `SubagentStart`/`SubagentStop`, `TaskCreated`/`TaskCompleted`, `Stop`,
   `StopFailure`, `TeammateIdle`, `ConfigChange`, `PreCompact`/`PostCompact`,
   `Elicitation`/`ElicitationResult`, `Notification`, `MessageDisplay`,
   `UserPromptExpansion`, `PermissionRequest`, `PermissionDenied`, `Setup`,
   `WorktreeCreate`/`WorktreeRemove`, `CwdChanged`, `FileChanged`,
   `InstructionsLoaded`, `SessionStart`/`SessionEnd`.
4. Any `hook_event_name` not recognized results in
   `ProtocolError::UnsupportedEvent(name)`.
5. Assembles a `Conn` whose `SessionMeta` always sets `driver: "Claude"` and
   `timestamp: None` (Claude Code does not provide a session timestamp in its
   hook payloads).

### `map_output` (serializing protocol output back to Claude's JSON)

`map_output` (invoked via `inceptool_protocol::to_wire`) builds a
`ClaudeOutputWire` from a `&HookOutputEvent` using the generic accessor
methods on `HookOutputEvent`:

- `continue_flag` is set to the inverse of `output.halt()` (i.e. `halt:
Some(true)` becomes `"continue": false`).
- `suppress_output` is taken directly from `output.suppress_output()`.
- `stop_reason` is always `None` (not currently populated by this driver).
- `decision` is set to the string `"block"` only when
  `output.decision()` is `Decision::Deny` or `Decision::Block`; all other
  decisions (`Allow`, `Ask`, or `None`) are omitted from the top-level
  `decision` field entirely. (Per-hook decisions such as `Allow`/`Ask` for
  `PreToolUse` are instead conveyed via the nested
  `hookSpecificOutput.permissionDecision` field.)
- `reason` and `system_message` are copied from `output.reason()` and
  `output.system_message()`.
- `permission_decision` (the top-level `permissionDecision` field) is always
  `None` — this driver only ever populates the equivalent field nested inside
  `hookSpecificOutput`.
- `hook_specific_output` is populated via
  `ClaudeHookSpecificOutput::try_from(output)`. If the output event variant
  has no Claude-specific mapping (see below), the conversion fails and
  `hook_specific_output` is simply omitted (`.ok()` swallows the error rather
  than propagating it) — the rest of the wire payload (`continue`, `decision`,
  `reason`, etc.) is still emitted.

## Key Types

- **`ClaudeDriver`** (`src/driver.rs`) — a zero-sized, `Copy`/`Default` unit
  struct implementing `inceptool_protocol::Driver`. All behavior lives in the
  `map_input`/`map_output` trait methods described above.

- **`ClaudeOutputWire<'a>`** (`src/types.rs`) — the top-level JSON object
  written back to Claude Code on stdout. Mirrors Claude's documented hook
  output schema with `serde` renames to camelCase fields: `continue`,
  `suppressOutput`, `stopReason`, `decision`, `reason`, `systemMessage`,
  `permissionDecision`, and `hookSpecificOutput`. All fields are optional and
  skipped when `None`.

- **`ClaudeHookSpecificOutput<'a>`** (`src/types.rs`) — an `untagged`,
  `Serialize`-only enum representing the `hookSpecificOutput` payload nested
  inside `ClaudeOutputWire`. Each variant corresponds to one Claude hook
  phase and carries only the fields Claude expects for that phase, e.g.:
  - `PreToolUse { hookEventName, permissionDecision, permissionDecisionReason,
updatedInput, additionalContext }`
  - `PostToolUse { hookEventName, additionalContext, updatedToolOutput }`
  - `UserPromptSubmit`, `SessionStart` (including `initialUserMessage`,
    `sessionTitle`, `watchPaths`, `reloadSkills`), `Setup`,
    `PermissionRequest` (nests a `ClaudePermissionDecision`), `WorktreeCreate`
    (`worktreePath`), `Stop`, `UserPromptExpansion`, `SubagentStart`,
    `SubagentStop`, `PermissionDenied`, `MessageDisplay`
    (`replacementText`), and `Elicitation` (`response`).

  Each variant is produced via a dedicated `From<&XxxOutput> for
ClaudeHookSpecificOutput<'a>` impl (one per protocol output type), and the
  overall `TryFrom<&HookOutputEvent> for ClaudeHookSpecificOutput<'a>`
  dispatches to the matching `From` impl based on the `HookOutputEvent`
  variant. Output variants without a corresponding Claude mapping (e.g.
  `HookOutputEvent::Notification`, `SessionEnd`, `CwdChanged`, ...) yield
  `ConversionError::UnsupportedEvent`.

- **`ClaudePermissionDecision<'a>`** (`src/types.rs`) — the nested `decision`
  object for `PermissionRequest` output, carrying `behavior`
  (`PermissionBehavior`, e.g. `allow`/`deny`/`ask`), `updatedInput`, and
  `permissionRuleDefinition`. It is only emitted when the protocol's
  `PermissionRequestOutput.behavior` is `Some`; otherwise the whole `decision`
  field is omitted from `hookSpecificOutput`.

- **`ClaudeMeta<'a>`** (`src/types.rs`, `pub(crate)`) — the minimal envelope
  struct used to peek at `session_id`, `transcript_path`, `cwd`,
  `hook_event_name`, `permission_mode`, `effort`, `agent_id`, and `agent_type`
  before dispatching to the full per-event deserialization. Not part of the
  public API.

## Claude-specific quirks worth noting

- **Decision collapsing to `"block"`**: at the top level, only `Deny` and
  `Block` decisions surface as `"decision": "block"`. `Allow`/`Ask` decisions
  for tool-use hooks are communicated exclusively through
  `hookSpecificOutput.permissionDecision`, matching Claude Code's documented
  behavior of using `hookSpecificOutput` for fine-grained per-tool decisions
  and the top-level `decision`/`continue` fields for coarse halt/block
  signaling.
- **`continue` is derived from `halt`**: the protocol's `halt: Option<bool>`
  is inverted into Claude's `continue` field; only `PreToolUse`,
  `PostToolUse`, `BeforeAgent`/`AfterAgent`, and `BeforeModel`/`AfterModel`
  outputs carry a `halt` value (per `HookOutputEvent::halt`), so `continue`
  is omitted for all other event types.
- **`stopReason` and top-level `permissionDecision` are never set**: both
  fields exist in `ClaudeOutputWire` for schema completeness but are
  hard-coded to `None` by `ClaudeDriver::map_output`.
- **Best-effort `hookSpecificOutput`**: `ClaudeHookSpecificOutput::try_from`
  failures (unsupported event variants) are silently discarded via `.ok()`
  rather than failing the whole `map_output` call — a hook can still report
  `decision`/`reason`/`continue` even for event types that have no
  Claude-specific nested payload.
- **`WorktreeCreate`** has a dedicated `hookSpecificOutput` shape
  (`hookEventName: "WorktreeCreate"`, optional `worktreePath`), allowing a
  hook to override the path of a worktree Claude Code is about to create.
- **Field aliases on input**: the underlying protocol input types (deserialized
  from the same raw JSON) accept Claude's alternate field names where Claude's
  schema has changed over time — e.g. `PostToolUse` accepts either
  `tool_output` or `tool_response`, and `CwdChanged` accepts either `old_cwd`
  or `previous_cwd` (exercised by `test_parse_valid_events` in
  `src/driver.rs`).
- **No session timestamp**: `SessionMeta::timestamp` is always `None` for
  Claude, since Claude Code's hook payloads do not include one.
- **Raw payload retained as `driver_meta`**: the full original JSON payload is
  preserved (zero-copy, as `RawJson`) on `Conn.session.driver_meta`, so
  downstream stages can inspect Claude-specific fields not modeled by the
  protocol's typed `HookInputEvent` variants.

## Errors

[`ClaudeDriverError`](src/error.rs) (the `Error` associated type for
`ClaudeDriver`) is a `thiserror`-based enum covering every failure mode the
driver can produce:

- `Protocol(#[from] ProtocolError)` — transparent wrapper for protocol-level
  errors, most notably `ProtocolError::UnsupportedEvent` when
  `hook_event_name` does not match any known Claude hook.
- `Json(#[from] serde_json::Error)` — JSON (de)serialization failures while
  parsing the input wire or producing the output wire.
- `Conversion(#[from] ConversionError)` — transparent wrapper for
  driver-local conversion errors.

[`ConversionError`](src/error.rs) currently has a single variant,
`UnsupportedEvent(&'static str)`, returned by
`ClaudeHookSpecificOutput::try_from` when a `HookOutputEvent` variant has no
corresponding Claude `hookSpecificOutput` shape. As noted above, this error is
caught with `.ok()` inside `map_output` and does not propagate to callers —
it exists primarily as an internal signal and for direct use/testing of the
`TryFrom` conversion.
