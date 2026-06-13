#![cfg_attr(
    test,
    allow(
        clippy::panic_in_result_fn,
        reason = "rstest cases return Result for `?`-based setup but use assert_eq!/assert_matches! \
                  for assertions per project convention"
    )
)]

//! # Inceptool Stages
//!
//! This crate contains all the built-in stages used by the engine to augment
//! and enforce agent behavior during execution.

/// Intercepts and rewrites bash commands for the RTK suite.
pub mod rtk;
pub use rtk::RtkStage;
