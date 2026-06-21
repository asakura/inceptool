//! Integration tests for the parable parser.
// Corpus cases embed Bash parameter-expansion syntax (e.g. `${NAME:-default}`) as string
// literals in generated `#[case]` attributes; clippy misreads `:-` / `:+` inside those literals
// as format-string argument placeholders.
#![expect(
    clippy::literal_string_with_formatting_args,
    reason = "generated #[case] attributes contain Bash parameter-expansion syntax that \
              resembles format arguments but is not"
)]

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
