#![deny(missing_docs)]
//! Gemini driver implementation for the protocol.

pub mod driver;
pub mod error;
pub mod types;

pub use driver::GeminiDriver;
pub use types::{GeminiHookSpecificOutput, GeminiOutputWire};
