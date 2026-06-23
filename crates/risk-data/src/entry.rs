//! The `'static`, codegen-target shapes [`crate::generate_command_table`]'s output constructs —
//! see [`CommandEntry`].
//!
//! Unlike the crate-internal `types::Command` and its siblings (heap-owned, parsed once from TOML
//! by the build script that calls this crate), every field here is `'static`: a command's whole
//! entry is generated Rust source, embedded directly into the caller's binary, so there's nothing
//! left to own at runtime.

use crate::types::{Effect, FlagGrammar, Platform, ProfilePatch, TakesValue};

/// A flag-value-conditioned rating override — see [`FlagEntry::value_rules`].
#[derive(Debug, Clone, Copy)]
pub struct ValueRuleEntry {
    /// A regex matched against the flag's value.
    pub pattern: &'static str,
    /// Whether a match rates the command worse or better.
    pub effect: Effect,
    /// The axis change a match applies.
    pub patch: ProfilePatch,
    /// Why a match has this effect.
    pub reason: &'static str,
}

/// One semantically distinct flag — every spelling rated identically.
#[derive(Debug, Clone, Copy)]
pub struct FlagEntry {
    /// Whether this rates the command worse or better.
    pub effect: Effect,
    /// The axis change this flag applies.
    pub patch: ProfilePatch,
    /// Why this flag has this effect.
    pub reason: &'static str,
    /// How this flag's value (if any) is spelled, when its rating depends on the value rather
    /// than just its presence.
    pub takes_value: Option<TakesValue>,
    /// Value-conditioned overrides, tried in order; this flag's own `effect`/`patch` apply when
    /// none match (or the flag carries no value at all).
    pub value_rules: &'static [ValueRuleEntry],
}

/// A rule that applies only when every required flag spelling is present on the same invocation.
#[derive(Debug, Clone, Copy)]
pub struct ComboRuleEntry {
    /// Every flag spelling that must be present for this rule to apply.
    pub requires: &'static [&'static str],
    /// Whether satisfying this combo rates the command worse or better.
    pub effect: Effect,
    /// The axis change this combo applies.
    pub patch: ProfilePatch,
    /// Why this combination has this effect.
    pub reason: &'static str,
}

/// A rule matched against every literal positional argument, independent of whether it's
/// flag-shaped.
#[derive(Debug, Clone, Copy)]
pub struct OperandRuleEntry {
    /// A regex matched against each literal argument's exact text.
    pub pattern: &'static str,
    /// Whether a match rates the command worse or better.
    pub effect: Effect,
    /// The axis change a match applies.
    pub patch: ProfilePatch,
    /// Why a match has this effect.
    pub reason: &'static str,
}

/// One scope's (a command's global scope, or one of its subcommands') baseline rating and the
/// flag/combo/operand rules that can escalate or mitigate it.
#[derive(Debug, Clone, Copy)]
pub struct RuleSetEntry {
    /// This scope's rating with no flags considered.
    pub baseline: ProfilePatch,
    /// Why the baseline is what it is.
    pub baseline_reason: &'static str,
    /// Whether this scope's multi-letter single-dash flags combine (`-ex` is `-e` plus `-x`)
    /// rather than being atomic single flags.
    pub short_flags_combinable: bool,
    /// This scope's individually rated flags, paired with each spelling they're known by.
    pub flags: &'static [(&'static str, FlagEntry)],
    /// Rules that escalate or mitigate based on more than one of this scope's flags being
    /// present at once.
    pub combo_rules: &'static [ComboRuleEntry],
    /// Rules matched against every literal positional argument, regardless of whether it's
    /// flag-shaped.
    pub operand_rules: &'static [OperandRuleEntry],
}

/// One command's classification rules: its global scope, plus zero or more named subcommands
/// each with their own independent [`RuleSetEntry`].
///
/// The value [`PlatformEntry::entry`] holds — one per [`Platform`] a command declares.
#[derive(Debug, Clone, Copy)]
pub struct CommandEntry {
    /// Which flag-syntax family this command's tokens follow.
    pub grammar: FlagGrammar,
    /// Whether this command's flag spellings match case-sensitively.
    pub case_sensitive: bool,
    /// This command's global-scope ruleset — applies before any subcommand is identified, or
    /// when this command declares no subcommands at all.
    pub rules: RuleSetEntry,
    /// This command's subcommands, paired with each name/alias they're known by.
    pub subcommands: &'static [(&'static str, RuleSetEntry)],
}

/// One [`CommandEntry`], tagged with the [`Platform`] it models.
///
/// The value type every key in [`crate::generate_command_table`]'s generated
/// `phf::Map<&'static str, &'static [PlatformEntry]>` maps to a slice of — usually length 1
/// (the overwhelming majority of commands declare only one implementation), longer only for a
/// command that bothers to declare more than one.
#[derive(Debug, Clone, Copy)]
pub struct PlatformEntry {
    /// The concrete implementation [`Self::entry`] models.
    pub platform: Platform,
    /// That implementation's classification rules.
    pub entry: CommandEntry,
}
