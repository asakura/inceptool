#![cfg_attr(
    test,
    expect(
        clippy::panic_in_result_fn,
        reason = "rstest cases return Result for `?`-based setup but use assert_eq!/assert_matches! \
                  for assertions per project convention"
    )
)]

//! # Inceptool Stages
//!
//! This crate contains all the built-in stages used by the engine to augment
//! and enforce agent behavior during execution.

pub mod flake_lock;
pub use flake_lock::FlakeLockSummarizationStage;

pub mod read_write_guard;
pub use read_write_guard::ReadWriteGuardStage;

pub mod rtk;
pub use rtk::RtkStage;
