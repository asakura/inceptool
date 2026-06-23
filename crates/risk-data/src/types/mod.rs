//! The TOML schema for a command-risk data file — see [`Dataset`].
//!
//! Every string field here is an owned [`Box<str>`], not the zero-copy `Cow<'a, str>` this
//! workspace otherwise uses for deserialization types: a [`Dataset`] is parsed exactly once per
//! `cargo build`, by a build script, purely to drive code generation — the generated Rust source
//! is what callers actually use at runtime. Threading a borrowed lifetime through every type here
//! (and keeping every source `.toml` file's content alive across a multi-file merge) would add
//! real complexity for a one-shot, off-the-hot-path read.

use crate::error::RiskDataError;

use serde::Deserialize;

use std::collections::BTreeSet;
use std::iter;

pub use self::{
    combo_rule::ComboRule,
    command::{Command, Subcommand},
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
mod flag;
mod grammar;
mod operand_rule;
mod platform;
mod rating;

/// A fully merged set of command-risk rules, parsed from one or more `.toml` files via
/// [`Dataset::parse`] and checked for internal consistency via [`Dataset::validate`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dataset {
    /// Every command declared across all parsed files.
    #[serde(default)]
    pub command: Vec<Command>,
}

impl Dataset {
    /// Parses each `(file_name, contents)` pair as a TOML document matching this schema, and
    /// merges their `command` lists in file order.
    ///
    /// # Errors
    ///
    /// Returns [`RiskDataError::Toml`] if any file's content isn't valid TOML, or doesn't match
    /// this schema.
    #[must_use = "parsing has no effect unless the caller uses the resulting Dataset"]
    pub fn parse(files: Vec<(Box<str>, Box<str>)>) -> Result<Self, RiskDataError> {
        let mut merged = Self::default();

        for (file, contents) in files {
            let parsed: Self = toml::from_str(&contents).map_err(|source| RiskDataError::Toml {
                file,
                source: Box::new(source),
            })?;

            merged.command.extend(parsed.command);
        }

        merged.validate()?;

        Ok(merged)
    }

    /// Checks every cross-reference this schema can't express structurally: no two commands (or
    /// a command and another's alias) share a name under the same [`Platform`] — distinct
    /// `Platform`-tagged declarations of the same name are intentional, modeling multiple real
    /// implementations, not a collision — no command repeats a flag spelling (globally or within
    /// one of its subcommands), every combo rule's `requires` names a flag the command (or its
    /// subcommand, plus the command's own global flags) actually declares, and every regex
    /// pattern compiles.
    ///
    /// # Errors
    ///
    /// Returns the first [`RiskDataError`] found.
    fn validate(&self) -> Result<(), RiskDataError> {
        let mut seen_names = BTreeSet::new();

        for command in &self.command {
            let own_names: BTreeSet<&str> = iter::once(command.name.as_ref())
                .chain(command.aliases.iter().map(AsRef::as_ref))
                .collect();

            for name in own_names {
                if !seen_names.insert((name, command.platform)) {
                    return Err(RiskDataError::DuplicateCommand {
                        name: name.into(),
                        platform: command.platform,
                    });
                }
            }

            command.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::assert_matches;
    use rstest::rstest;

    #[derive(Debug, thiserror::Error)]
    enum TestError {
        #[error(transparent)]
        RiskData(#[from] RiskDataError),
    }

    /// `files`' `&str` literal `(path, contents)` pairs converted into the owned
    /// `Vec<(Box<str>, Box<str>)>` `Dataset::parse` expects, so fixtures can stay terse string
    /// literals.
    fn parse_files<const N: usize>(files: [(&str, &str); N]) -> Result<Dataset, RiskDataError> {
        Dataset::parse(
            files
                .into_iter()
                .map(|(path, contents)| (path.into(), contents.into()))
                .collect(),
        )
    }

    const VALID_KILL: &str = r#"
        [[command]]
        name = "kill"
        kind = "builtin"
        baseline_reason = "Sends a signal to another process."

          [[command.flag]]
          spellings = ["-9", "-KILL", "-SIGKILL"]
          effect = "escalate"
          profile = { reversibility = "irreversible" }
          reason = "SIGKILL can't be caught or cleaned up after."

          [[command.operand_rule]]
          pattern = "^-1$"
          effect = "escalate"
          profile = { blast_radius = "broad" }
          reason = "PID -1 broadcasts to every process the caller may signal."
    "#;

    mod parse {
        use super::*;

        #[rstest]
        fn merges_commands_from_every_file() -> Result<(), TestError> {
            let other = r#"
                [[command]]
                name = "eval"
                kind = "builtin"
                baseline_reason = "Executes a constructed string as a command."
            "#;

            let dataset = parse_files([("a.toml", VALID_KILL), ("b.toml", other)])?;

            assert_eq!(dataset.command.len(), 2);
            Ok(())
        }

        #[rstest]
        fn malformed_toml_is_a_toml_error() {
            let result = parse_files([("bad.toml", "not = [valid")]);
            assert_matches!(result, Err(RiskDataError::Toml { .. }));
        }
    }

    mod validate {
        use super::*;

        #[rstest]
        fn accepts_a_well_formed_dataset() -> Result<(), TestError> {
            parse_files([("kill.toml", VALID_KILL)])?;
            Ok(())
        }

        #[rstest]
        fn rejects_a_duplicate_command_name() {
            let result = parse_files([("a.toml", VALID_KILL), ("b.toml", VALID_KILL)]);
            assert_matches!(result, Err(RiskDataError::DuplicateCommand { .. }));
        }

        #[rstest]
        fn accepts_a_command_whose_alias_redundantly_repeats_its_own_name() -> Result<(), TestError>
        {
            let redundant_self_alias = r#"
                [[command]]
                name = "eval"
                aliases = ["eval"]
                kind = "builtin"
                baseline_reason = "Executes a constructed string as a command."
            "#;

            parse_files([("eval.toml", redundant_self_alias)])?;

            Ok(())
        }

        #[rstest]
        fn rejects_a_command_whose_alias_collides_with_another_command() {
            let aliased = r#"
                [[command]]
                name = "readarray"
                aliases = ["kill"]
                kind = "builtin"
                baseline_reason = "Reads lines into an array."
            "#;

            let result = parse_files([("kill.toml", VALID_KILL), ("aliased.toml", aliased)]);

            assert_matches!(result, Err(RiskDataError::DuplicateCommand { .. }));
        }
    }
}
