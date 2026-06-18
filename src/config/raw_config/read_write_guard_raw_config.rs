//! The `[read-write-guard]` table's TOML-facing shape: a flat list of
//! guarded-file rules.

use super::rule::RawRule;

use serde::{Deserialize, Serialize};

/// Raw guarded-file rules for [`ReadWriteGuardStage`](inceptool_stages::ReadWriteGuardStage):
/// the built-ins in the embedded base config, and any user-supplied
/// additions/overrides in a user `inceptool.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadWriteGuardRawConfig {
    #[serde(default)]
    pub rules: Vec<RawRule>,
}
