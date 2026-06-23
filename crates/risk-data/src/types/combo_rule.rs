//! A rule that escalates or mitigates only when every listed flag is present at once — see
//! [`ComboRule`].

use super::{Effect, ProfilePatch};

use serde::Deserialize;

/// A rule that escalates or mitigates only when *every* listed flag spelling is present on the
/// same invocation — see [`crate::types::Command::combo_rule`].
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComboRule {
    /// Every flag spelling that must be present for this rule to apply. Each must be declared by
    /// some [`crate::types::Flag`] on the same command (checked by
    /// [`crate::types::Dataset::validate`]).
    pub requires: Vec<Box<str>>,
    /// Whether satisfying this combo rates the command worse or better.
    pub effect: Effect,
    /// The axis change this combo applies.
    #[serde(default)]
    pub profile: ProfilePatch,
    /// Why this combination has this effect.
    pub reason: Box<str>,
}
