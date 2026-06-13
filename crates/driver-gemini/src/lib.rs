#![cfg_attr(
    test,
    expect(
        clippy::panic_in_result_fn,
        reason = "rstest cases return Result for `?`-based setup but use assert_eq!/assert_matches! \
                  for assertions per project convention"
    )
)]

//! Gemini driver implementation for the protocol.

pub mod driver;
pub mod error;
pub mod types;

pub use driver::GeminiDriver;
pub use types::{GeminiHookSpecificOutput, GeminiOutputWire};
