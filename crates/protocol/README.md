# Protocol

`protocol` is the canonical communication interface for the `inceptool-rs` project.

It provides driver-agnostic data structures to standardize the payloads
exchanged between the CLI, the hook engine, and specific agent drivers
(like Claude or Gemini).

## Features

- **Decoupled Payloads**: By using `RawJson` and borrowing where possible,
  the protocol avoids parsing full tool payloads and model requests unless
  a hook actively inspects or alters them.
- **Hook Inputs & Outputs**: Comprehensive structs represent the exact state and
  context for every hook phase (e.g., `BeforeTool`, `AfterModel`, `SessionStart`).
- **Driver Abstraction**: Defines the foundational `Driver` trait that any
  specific agent implementation must implement to integrate with `inceptool-rs`.

## Modularity

The crate is broken down into clean, single-purpose modules:

- `types`: Core base types (`RawJson`, `Decision`).
- `session`: Connection records (`Conn`) and session-level metadata tracking.
- `input`: All inbound payloads provided to hook executions.
- `output`: All outbound payloads expected from hook executions.
- `driver`: The universal abstraction for an AI driver.
- `error`: Strongly typed errors for protocol mapping operations.
