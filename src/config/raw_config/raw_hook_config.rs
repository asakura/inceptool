//! The TOML-facing shape of a `[hooks.<name>]` table.

use serde::{Deserialize, Serialize};

/// Per-stage enable/disable override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct RawHookConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

const fn default_true() -> bool {
    true
}
