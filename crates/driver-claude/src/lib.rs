#![deny(missing_docs)]
//! Claude driver implementation for the protocol.

pub mod driver;
pub mod error;
pub mod types;

pub use driver::ClaudeDriver;
pub use types::{ClaudeHookSpecificOutput, ClaudeOutputWire};
