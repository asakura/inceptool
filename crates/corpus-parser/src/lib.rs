#![cfg_attr(
    test,
    expect(
        clippy::panic_in_result_fn,
        reason = "rstest cases return Result for ?-based setup but use assert_matches! for assertions"
    )
)]
//! # Corpus Parser Architecture
//!
//! Parses the `.tests` file format used by the `inceptool-parable` corpus test suite.
//!
//! ## Core Design
//!
//! Each `.tests` file declares a single test suite with a unique number and name,
//! contains one or more test groups (delimited by `# === Name ===` headers), and
//! within each group, one or more test cases with input/expected sections.
//!
//! ## Flow
//!
//! 1. Call [`TestSuite::parse`] with a file stem and the raw file content.
//! 2. The parser validates the suite header, group structure, and case formatting.
//! 3. On success, a [`TestSuite`] containing [`TestGroup`]s and [`CorpusCase`]s is returned.
//!
//! ## Edge Cases
//!
//! - Missing or malformed suite headers: [`CorpusParseError`].
//! - Empty groups or suites: [`CorpusParseError`].
//! - Comments that violate the `# ` prefix rule: [`CorpusParseError`].
//! - A case's input/expected text may contain a literal `---`/`===` line by
//!   escaping it as `\---`/`\===`.
//! - A case may mark itself negative with `--- <error>` instead of the bare
//!   `---` separator: see [`CaseExpectation`].

pub mod error;
pub mod ident;
pub mod parser;
pub mod types;

pub use error::{CorpusParseError, CorpusParseErrorKind};
pub use ident::to_ident_fragment;
pub use types::{CaseExpectation, CorpusCase, TestGroup, TestSuite};
