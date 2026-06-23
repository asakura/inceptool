//! A command's individually rated flags — see [`Flag`].

use super::{Effect, ProfilePatch};
use crate::error::RiskDataError;

use serde::Deserialize;

/// One semantically distinct flag — every spelling/alias of it rated identically, rather than
/// one row per spelling.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flag {
    /// Every spelling this one semantic flag is known by (e.g. `["-9", "-KILL", "-SIGKILL"]`).
    pub spellings: Vec<Box<str>>,
    /// Whether this rates the command worse or better.
    pub effect: Effect,
    /// The axis change this flag applies.
    #[serde(default)]
    pub profile: ProfilePatch,
    /// Why this flag has this effect.
    pub reason: Box<str>,
    /// How this flag's value (if any) is spelled, when its rating depends on the value rather
    /// than just its presence.
    #[serde(default)]
    pub takes_value: Option<TakesValue>,
    /// Value-conditioned overrides, tried in order; the flag's own `effect`/`profile` apply when
    /// none match (or the flag carries no value at all).
    #[serde(default)]
    pub value_rule: Vec<ValueRule>,
}

/// How a flag's value is spelled relative to the flag itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakesValue {
    /// `--name=value`: the value is joined to the flag with `=`.
    Combined,
    /// `-s value`: the value is the *next* argument.
    Separate,
}

/// A flag-value-conditioned rating override — see [`Flag::value_rule`].
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueRule {
    /// A regex matched against the flag's value.
    pub pattern: Box<str>,
    /// Whether a match rates the command worse or better.
    pub effect: Effect,
    /// The axis change a match applies.
    #[serde(default)]
    pub profile: ProfilePatch,
    /// Why a match has this effect.
    pub reason: Box<str>,
}

impl ValueRule {
    /// Checks that [`Self::pattern`] actually compiles — see [`crate::types::Dataset::validate`].
    /// `subcommand` is `Some` when this flag belongs to a [`crate::types::Subcommand`] rather
    /// than the command's global scope, purely so a failure's message says where.
    pub(crate) fn validate(
        &self,
        command: &str,
        subcommand: Option<&str>,
    ) -> Result<(), RiskDataError> {
        regex::Regex::new(&self.pattern).map_err(|source| RiskDataError::InvalidPattern {
            command: command.into(),
            subcommand: subcommand.map(Into::into),
            pattern: self.pattern.clone(),
            source: Box::new(source),
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    use core::assert_matches;

    #[derive(Debug, thiserror::Error)]
    enum TestError {
        #[error(transparent)]
        RiskData(#[from] RiskDataError),
    }

    mod validate {
        use super::*;

        fn value_rule(pattern: &str) -> ValueRule {
            ValueRule {
                pattern: pattern.into(),
                effect: Effect::Escalate,
                profile: ProfilePatch::EMPTY,
                reason: "test fixture".into(),
            }
        }

        #[rstest]
        fn accepts_a_well_formed_pattern() -> Result<(), TestError> {
            value_rule("^--$").validate("scp", None)?;
            Ok(())
        }

        #[rstest]
        fn rejects_an_invalid_regex_pattern() {
            let result = value_rule("(").validate("scp", Some("put"));
            assert_matches!(result, Err(RiskDataError::InvalidPattern { .. }));
        }
    }
}
