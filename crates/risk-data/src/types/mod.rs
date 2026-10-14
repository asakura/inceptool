//! The TOML schema for a command-risk data file — see [`Dataset`].
//!
//! Every string field here is an owned [`Box<str>`], not the zero-copy `Cow<'a, str>` this
//! workspace otherwise uses for deserialization types: a [`Dataset`] is parsed exactly once per
//! `cargo build`, by a build script, purely to drive code generation — the generated Rust source
//! is what callers actually use at runtime. Threading a borrowed lifetime through every type here
//! (and keeping every source `.toml` file's content alive across a multi-file merge) would add
//! real complexity for a one-shot, off-the-hot-path read.

pub use self::{
    combo_rule::ComboRule,
    command::{Command, Subcommand},
    dataset::Dataset,
    flag::{Flag, TakesValue, ValueRule},
    grammar::FlagGrammar,
    operand_rule::OperandRule,
    platform::Platform,
    rating::{
        Auditability, BlastRadius, Disclosure, Effect, Exposure, Persistence, Privilege,
        ProfilePatch, Reversibility, TrustImpact, Verification,
    },
};

mod combo_rule;
mod command;
mod dataset;
mod flag;
mod grammar;
mod operand_rule;
mod platform;
mod rating;
