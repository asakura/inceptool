#![cfg_attr(
    test,
    expect(
        clippy::panic_in_result_fn,
        reason = "rstest cases return Result for `?`-based setup but use assert_eq!/assert_matches! \
                  for assertions per project convention"
    )
)]

//! This crate defines the canonical protocol for `inceptool`.
//!
//! It provides the core data structures used to communicate between the CLI,
//! hooks, and the underlying driver (e.g., Claude or Gemini).
//! The protocol is organized into the following modules:
//!
//! - [`types`]: Common base types like `RawJson` and `Decision`.
//! - [`session`]: Connection and session metadata definitions.
//! - [`input`]: Definitions for all input payloads received by hooks.
//! - [`output`]: Definitions for all output payloads returned by hooks.
//! - [`error`]: Protocol-specific error definitions.
//! - [`driver`]: The `Driver` trait which abstractly defines how a backend interacts with the protocol.

pub mod driver;
pub mod error;
pub mod input;
pub mod output;
pub mod session;
pub mod types;

pub use driver::*;
pub use error::*;
pub use input::*;
pub use output::*;
pub use session::*;
pub use types::*;
