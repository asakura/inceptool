//! Integration tests for the parable parser.

use std::{fmt, io::Error as IoError};

#[derive(thiserror::Error)]
enum TestError {
    #[error("Test failure: {0}")]
    Failure(String),
    #[error(transparent)]
    Io(#[from] IoError),
}

impl fmt::Debug for TestError {
    /// Delegates to `Display` so libtest's `Err`-via-`Debug` test-failure
    /// output renders the readable message instead of an escaped tuple.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

include!(concat!(env!("OUT_DIR"), "/generated_tests.rs"));
