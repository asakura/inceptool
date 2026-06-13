#![cfg_attr(
    test,
    expect(
        clippy::panic_in_result_fn,
        reason = "rstest cases return Result for `?`-based setup but use assert_eq!/assert_matches! \
                  for assertions per project convention"
    )
)]

//! Claude Code driver implementation for the `inceptool` protocol.
//!
//! [`ClaudeDriver`] implements [`inceptool_protocol::Driver`] for
//! [Claude Code](https://code.claude.com), translating between Claude's
//! `hook_event_name`-tagged JSON wire format and the protocol's normalized
//! [`inceptool_protocol::Conn`], [`inceptool_protocol::HookInputEvent`], and
//! [`inceptool_protocol::HookOutputEvent`] types.
//!
//! - [`driver`]: [`ClaudeDriver`] and its `Driver` implementation.
//!   `map_input` matches on `hook_event_name` to deserialize the raw JSON
//!   into the corresponding `HookInputEvent` variant; `map_output`
//!   serializes a `HookOutputEvent` back into Claude's
//!   `continue`/`decision`/`hookSpecificOutput` shape.
//! - [`types`]: the wire types exchanged with Claude Code -
//!   [`ClaudeOutputWire`], [`ClaudeHookSpecificOutput`] (one variant per hook
//!   phase), and `ClaudePermissionDecision`.
//! - [`error`]: [`error::ClaudeDriverError`] (the `Driver::Error` type) and
//!   [`error::ConversionError`] (failures mapping a `HookOutputEvent` to a
//!   Claude-specific `hookSpecificOutput`).

pub mod driver;
pub mod error;
pub mod types;

pub use driver::ClaudeDriver;
pub use types::{ClaudeHookSpecificOutput, ClaudeOutputWire};
