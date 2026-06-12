//! # Inceptool Hooks
//!
//! This crate contains all the built-in stages used by the engine to augment
//! and enforce agent behavior during execution.

/// Intercepts and rewrites bash commands for the RTK suite.
pub mod rtk;
pub use rtk::RtkStage;
