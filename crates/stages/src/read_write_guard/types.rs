//! The guarded-file rule shape consumed by [`super::ReadWriteGuardStage`].

use serde::Deserialize;

use std::borrow::Cow;
use std::collections::BTreeMap;

/// Access policy for a guarded file, driving how [`super::ReadWriteGuardStage`]
/// responds to reads and writes of it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Deny both reads and writes. Reads get a flat, no-frills reason
    /// pointing at `git diff`; writes get `hint` and `note`. The policy used
    /// by all built-in rules.
    No {
        /// The terminal command suggested when a write is denied.
        hint: Cow<'static, str>,
        /// Additional caveats about the scope of `hint` (e.g. "updates ALL
        /// packages").
        note: Cow<'static, str>,
    },
    /// Like [`Access::No`], but reads are intended to get a richer
    /// diff/summary instead of the flat reason, once diff-content generation
    /// exists. Currently degrades to [`Access::No`]'s read behavior.
    /// Reserved for a future upgrade of any rule, built-in or
    /// user-supplied.
    Diff {
        /// The terminal command suggested when a write is denied.
        hint: Cow<'static, str>,
        /// Additional caveats about the scope of `hint`.
        note: Cow<'static, str>,
    },
    /// Deny writes only (with `hint` and `note`); reads pass through
    /// untouched.
    Write {
        /// The terminal command suggested when a write is denied.
        hint: Cow<'static, str>,
        /// Additional caveats about the scope of `hint`.
        note: Cow<'static, str>,
    },
    /// Deny reads only (flat reason); writes pass through untouched.
    Read,
}

/// A guarded-file rule: how [`super::ReadWriteGuardStage`] responds to reads
/// and writes of its `filename`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Rule {
    /// The filename this rule matches (e.g. `"Cargo.lock"`).
    filename: Cow<'static, str>,
    /// Which operations are intercepted, and how.
    access: Access,
}

impl Rule {
    /// Builds a rule matching `filename` with the given `access` policy.
    #[must_use = "constructs a new rule; discarding it does nothing"]
    pub const fn new(filename: Cow<'static, str>, access: Access) -> Self {
        Self { filename, access }
    }

    /// The access policy this rule applies.
    #[must_use = "returns the access policy; has no side effects"]
    pub const fn access(&self) -> &Access {
        &self.access
    }
}

/// Index of [`Rule`]s by filename, built via [`FromIterator`].
#[derive(Debug, Clone)]
pub struct RuleSet(BTreeMap<Cow<'static, str>, Rule>);

impl FromIterator<Rule> for RuleSet {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Rule>,
    {
        Self(
            iter.into_iter()
                .map(|rule| (rule.filename.clone(), rule))
                .collect(),
        )
    }
}

impl RuleSet {
    /// Looks up the [`Rule`] for `file_name`, or `None` if it isn't guarded.
    #[must_use = "returns the matching rule; has no side effects"]
    pub fn get<S>(&self, file_name: S) -> Option<&Rule>
    where
        S: AsRef<str>,
    {
        self.0.get(file_name.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    /// A small fixture [`RuleSet`] for exercising [`RuleSet::get`] directly.
    fn fixture_rules() -> RuleSet {
        [
            Rule::new(
                Cow::Borrowed("flake.lock"),
                Access::No {
                    hint: Cow::Borrowed("Run `nix flake update` to update it."),
                    note: Cow::Borrowed("(NOTE: updates ALL flake inputs)"),
                },
            ),
            Rule::new(
                Cow::Borrowed("Cargo.lock"),
                Access::No {
                    hint: Cow::Borrowed("Run `cargo update` to update it."),
                    note: Cow::Borrowed("(NOTE: updates ALL Rust dependencies)"),
                },
            ),
        ]
        .into_iter()
        .collect()
    }

    #[rstest]
    #[case::flake_lock("flake.lock")]
    #[case::cargo_lock("Cargo.lock")]
    fn rules_recognize_fixture_lockfiles(#[case] file_name: &str) {
        assert!(fixture_rules().get(file_name).is_some());
    }

    #[rstest]
    fn rules_return_none_for_unknown_file() {
        assert!(fixture_rules().get("main.rs").is_none());
    }
}
