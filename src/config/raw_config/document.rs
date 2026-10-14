use super::raw_hook_config::RawHookConfig;
use super::read_write_guard_raw_config::ReadWriteGuardRawConfig;
use super::rule::RawRule;
use crate::config::{Config, ConfigError};

use inceptool_stages::read_write_guard::Rule;

use serde::{Deserialize, Serialize};

use std::{borrow::Cow, collections::BTreeMap, fs, io, path::Path};

/// The embedded base config: built-in defaults for every stage, in the same
/// shape as a user `inceptool.toml`. Parsed by [`RawConfig::parse_base_config`]
/// and resolved into a [`Config`] exactly like any other layer, then merged
/// with user layers via [`Config::merge`] — the binary has no separate
/// "built-in" code path.
const BASE_CONFIG_TOML: &str = include_str!("../base.toml");

/// Intermediate deserialization view of `inceptool.toml` (or the embedded
/// base config); never exposed publicly. Mirrors the TOML shape exactly —
/// [`Config`] is built from this via [`TryFrom`]. See the module doc for
/// why this layer exists rather than deserializing straight into
/// [`Config`]'s own field types.
#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "shared within the config module hierarchy"
)]
pub struct RawConfig {
    #[serde(default)]
    pub(super) hooks: BTreeMap<Cow<'static, str>, RawHookConfig>,
    #[serde(default, rename = "read-write-guard")]
    pub(super) read_write_guard: ReadWriteGuardRawConfig,
}

/// Renders `path` for [`ConfigError::Read`]/[`ConfigError::Parse`]; shared
/// so [`RawConfig::load_layer`]'s two error arms don't each spell out the
/// same lossy-conversion chain.
fn path_display(path: &Path) -> Cow<'static, str> {
    path.to_string_lossy().into_owned().into()
}

impl RawConfig {
    /// Parses the embedded base config (built-in stage defaults).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::EmbeddedConfig`] if the embedded TOML fails to
    /// parse — guarded by `tests::parse_base_config::embedded_config_is_valid`.
    pub(in crate::config) fn parse_base_config() -> Result<Self, ConfigError> {
        toml::from_str(BASE_CONFIG_TOML).map_err(ConfigError::EmbeddedConfig)
    }

    /// Loads and parses `path` if it exists, or returns `Ok(None)` if it's
    /// missing. Reads `path` directly rather than checking existence first,
    /// so a file deleted between the two (e.g. an editor's atomic save)
    /// can't turn a benign "missing" case into a spurious read error.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Read`] if `path` exists but cannot be read, or
    /// [`ConfigError::Parse`] if its contents are not valid TOML.
    pub(in crate::config) fn load_layer(path: &Path) -> Result<Option<Self>, ConfigError> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(inner) if inner.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(inner) => {
                return Err(ConfigError::Read {
                    inner,
                    path: path_display(path),
                });
            }
        };

        toml::from_str(&content)
            .map(Some)
            .map_err(|inner| ConfigError::Parse {
                inner,
                path: path_display(path),
            })
    }

    /// Renders `self` as TOML text, for `inceptool config` (see
    /// [`Config::to_toml`]).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Serialize`] if `self` fails to serialize —
    /// this would indicate a bug in this type's `Serialize` derive output,
    /// not a user error.
    #[must_use = "returns the rendered TOML text; has no side effects"]
    pub(in crate::config) fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self).map_err(ConfigError::Serialize)
    }
}

impl TryFrom<RawConfig> for Config {
    type Error = ConfigError;

    fn try_from(raw: RawConfig) -> Result<Self, Self::Error> {
        Ok(Self {
            hooks: raw
                .hooks
                .into_iter()
                .map(|(name, hook)| (name, hook.enabled))
                .collect(),
            read_write_guard_rules: raw
                .read_write_guard
                .rules
                .into_iter()
                .map(Rule::from)
                .collect(),
            repo_root: None,
        })
    }
}

/// Inverse of [`TryFrom<RawConfig> for Config`] above: renders a resolved
/// [`Config`] back into the TOML-facing [`RawConfig`] shape, for `inceptool
/// config` (the `git config --list` equivalent for this binary). Unlike
/// that direction, this one can't fail — `Config` is already valid data,
/// just reshaped. Takes `config` by value rather than `&Config`: every
/// caller is done with it afterward, so each hook name and rule can move
/// into its `RawConfig` counterpart instead of being cloned.
///
/// This reflects the *merged* result only: built-in vs. user-layer
/// provenance isn't tracked anywhere past [`Config::merge`], so e.g. a
/// built-in rule a user override replaced is indistinguishable here from
/// one that was always user-supplied.
impl From<Config> for RawConfig {
    fn from(config: Config) -> Self {
        Self {
            hooks: config
                .hooks
                .into_iter()
                .map(|(name, enabled)| (name, RawHookConfig { enabled }))
                .collect(),
            read_write_guard: ReadWriteGuardRawConfig {
                rules: config
                    .read_write_guard_rules
                    .into_iter()
                    .map(RawRule::from)
                    .collect(),
            },
        }
    }
}
