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
  specific agent implementation must implement to integrate with `inceptool-rs`.

## Modularity

The crate is broken down into clean, single-purpose modules:

- `types`: Core base types (`RawJson`, `Decision`, `PermissionMode`, `Effort`,
  `EffortLevel`, `PermissionBehavior`).
- `session`: Connection records (`Conn`) and session-level metadata tracking.
- `input`: All inbound payloads provided to hook executions.
- `output`: All outbound payloads expected from hook executions.
- `driver`: The universal abstraction for an AI driver.
- `error`: Strongly typed errors for protocol mapping operations.

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
