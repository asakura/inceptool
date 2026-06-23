//! A rule matched against every literal positional argument — see [`OperandRule`].

use super::{Effect, ProfilePatch};
use crate::error::RiskDataError;

use serde::Deserialize;

/// A rule matched against every literal positional argument, independent of whether it's
/// flag-shaped — see [`crate::types::Command::operand_rule`].
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperandRule {
    /// A regex matched against each literal argument's exact text.
    pub pattern: Box<str>,
    /// Whether a match rates the command worse or better.
    pub effect: Effect,
    /// The axis change a match applies.
    #[serde(default)]
    pub profile: ProfilePatch,
    /// Why a match has this effect.
    pub reason: Box<str>,
}

impl OperandRule {
    /// Checks that [`Self::pattern`] actually compiles — see [`crate::types::Dataset::validate`].
    /// `subcommand` is `Some` when this rule belongs to a [`crate::types::Subcommand`] rather
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
    use crate::types::Dataset;

    use rstest::rstest;

    use core::assert_matches;

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

    mod validate {
        use super::*;

        #[rstest]
        fn rejects_an_invalid_regex_pattern() {
            let bad_pattern = r#"
                [[command]]
                name = "kill"
                kind = "builtin"
                baseline_reason = "Sends a signal to another process."

                  [[command.operand_rule]]
                  pattern = "("
                  effect = "escalate"
                  reason = "Unbalanced paren, should fail to compile."
            "#;

            let result = parse_files([("kill.toml", bad_pattern)]);

            assert_matches!(result, Err(RiskDataError::InvalidPattern { .. }));
        }
    }
}
