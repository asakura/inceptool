//! # Risk Data Architecture
//!
//! Given a directory of command-risk `.toml` files, [`generate_command_table`] is the one thing
//! this crate does: recursively scan it, parse and merge every file, validate the result, and
//! hand back a [`proc_macro2::TokenStream`] for the `phf` lookup table a caller's runtime
//! classification logic reads. `inceptool-parable`'s build script is the crate's one real caller
//! today.
//!
//! ## Core Design
//!
//! A `.toml` file declares zero or more `[[command]]` tables — see the crate-internal
//! `types::Command` for the full field list. That schema, and the TOML parsing/merging/validation
//! built on it, are crate-internal: nothing outside [`codegen`] needs to construct a `Command` by
//! hand, since the one public function does the whole pipeline end to end.
//!
//! What *is* public, besides [`generate_command_table`] itself, is the plain-data vocabulary a
//! caller's own classification logic needs in scope: the risk axes (`TrustImpact`,
//! `Reversibility`, `BlastRadius`, `Disclosure`, `Persistence`, `Privilege`, `Auditability`,
//! `Exposure`, `Verification`), how rules compose (`Effect`, `ProfilePatch`), and the
//! `'static` runtime shapes ([`CommandEntry`] and its siblings in [`entry`]) the generated
//! `phf::Map`'s values are. The generated tokens themselves name every one of these (and `phf`
//! itself) through an absolute `::inceptool_risk_data::`/`::phf::` path, so a caller's own `use`
//! block is never load-bearing for the `include!`d text; these exports exist for the caller's
//! *own* code (constructing a `Platform` to pass in, matching on a looked-up `CommandEntry`), not
//! to satisfy the generated tokens.
//!
//! ## Flow
//!
//! 1. The directory tree is walked recursively for `.toml` files.
//! 2. Every file is parsed and merged, then validated for every cross-reference the schema alone
//!    can't enforce (duplicate names, an undeclared combo flag, an invalid regex pattern).
//! 3. Each command is rendered into [`CommandEntry`]-shaped tokens and perfect-hashed via
//!    `phf_codegen`, once, at this point — not at the caller's runtime.
//!
//! ## Edge Cases
//!
//! - A missing, or non-directory, root is an error ([`error::RiskDataError::MissingRoot`]), as
//!   is a root containing no `.toml` file recursively ([`error::RiskDataError::NoTomlFiles`]).
//! - A malformed TOML file, or one violating the schema, or data that fails cross-reference
//!   validation, all fail before any tokens are produced.

#![cfg_attr(
    test,
    expect(
        clippy::panic_in_result_fn,
        reason = "rstest cases return Result<(), TestError> per project convention and use \
                  assert_eq!/assert_matches! for assertions"
    )
)]

pub mod codegen;
pub mod entry;
pub mod error;

mod types;

pub use codegen::generate_command_table;
pub use entry::{
    ComboRuleEntry, CommandEntry, FlagEntry, OperandRuleEntry, PlatformEntry, RuleSetEntry,
    ValueRuleEntry,
};
pub use error::RiskDataError;
pub use types::{
    Auditability, BlastRadius, Disclosure, Effect, Exposure, FlagGrammar, Persistence, Platform,
    Privilege, ProfilePatch, Reversibility, TakesValue, TrustImpact, Verification,
};
