use super::{Config, ConfigError};

use inceptool_stages::read_write_guard::Rule;

use serde::Deserialize;

use std::{borrow::Cow, collections::BTreeMap, fs, path::Path};

/// The embedded base config: built-in defaults for every stage, in the same
/// shape as a user `inceptool.toml`. Parsed by [`RawConfig::parse_base_config`]
/// and merged with user layers exactly like any other layer via
/// [`RawConfig::merge`] — the binary has no separate "built-in" code path.
const BASE_CONFIG_TOML: &str = include_str!("base.toml");

/// Intermediate deserialization view of `inceptool.toml` (or the embedded
/// base config); never exposed publicly. Mirrors the TOML shape exactly —
/// [`Config`] is built from this via [`TryFrom`].
#[derive(Debug, Deserialize, Default)]
pub(super) struct RawConfig {
    #[serde(default)]
    hooks: BTreeMap<String, RawHookConfig>,
    #[serde(default, rename = "read-write-guard")]
    read_write_guard: ReadWriteGuardRawConfig,
}

/// Per-stage enable/disable override.
#[derive(Debug, Deserialize, Default)]
struct RawHookConfig {
    #[serde(default = "default_true")]
    enabled: bool,
}

const fn default_true() -> bool {
    true
}

/// Raw guarded-file rules for [`ReadWriteGuardStage`](inceptool_stages::ReadWriteGuardStage):
/// the built-ins in the embedded base config, and any user-supplied
/// additions/overrides in a user `inceptool.toml`.
#[derive(Debug, Deserialize, Default)]
struct ReadWriteGuardRawConfig {
    #[serde(default)]
    rules: Vec<Rule>,
}

impl RawConfig {
    /// Parses the embedded base config (built-in stage defaults).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::EmbeddedConfig`] if the embedded TOML fails to
    /// parse — guarded by `tests::parse_base_config::embedded_config_is_valid`.
    pub(super) fn parse_base_config() -> Result<Self, ConfigError> {
        toml::from_str(BASE_CONFIG_TOML).map_err(ConfigError::EmbeddedConfig)
    }

    /// Loads and parses `path` if it exists, or returns `Ok(None)` if it's
    /// missing.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Read`] if `path` exists but cannot be read, or
    /// [`ConfigError::Parse`] if its contents are not valid TOML.
    pub(super) fn load_layer(path: &Path) -> Result<Option<Self>, ConfigError> {
        if !path.exists() {
            return Ok(None);
        }

        let path_display: Cow<'static, str> = path.to_string_lossy().into_owned().into();

        let content = fs::read_to_string(path).map_err(|inner| ConfigError::Read {
            inner,
            path: path_display.clone(),
        })?;

        toml::from_str(&content)
            .map(Some)
            .map_err(|inner| ConfigError::Parse {
                inner,
                path: path_display,
            })
    }

    /// Merges `other` on top of `self`: per-hook overrides win outright;
    /// read-write-guard rules from both layers accumulate, with same-filename
    /// precedence resolved later when [`Config`] collects them into a
    /// [`RuleSet`](inceptool_stages::read_write_guard::RuleSet) (later
    /// entries win there too, so layering order is preserved end to end).
    pub(super) fn merge(&mut self, other: Self) {
        self.hooks.extend(other.hooks);
        self.read_write_guard
            .rules
            .extend(other.read_write_guard.rules);
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
            read_write_guard_rules: raw.read_write_guard.rules.into_iter().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use inceptool_stages::read_write_guard::Access;

    use rstest::rstest;

    use core::assert_matches;
    use std::io;

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error(transparent)]
        Config(#[from] ConfigError),
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error("Test failure: {0}")]
        Failure(String),
    }

    mod load_layer {
        use super::*;

        #[rstest]
        fn missing_file_returns_none() -> Result<(), TestError> {
            let dir = tempfile::tempdir()?;

            assert_matches!(
                RawConfig::load_layer(&dir.path().join("missing.toml"))?,
                None
            );

            Ok(())
        }

        #[rstest]
        fn valid_file_parses() -> Result<(), TestError> {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("inceptool.toml");

            fs::write(&path, "[hooks.rtk]\nenabled = false\n")?;

            let raw = RawConfig::load_layer(&path)?
                .ok_or_else(|| TestError::Failure("expected a parsed layer".into()))?;

            assert_eq!(raw.hooks.get("rtk").map(|h| h.enabled), Some(false));

            Ok(())
        }

        #[rstest]
        fn malformed_file_is_err() -> Result<(), TestError> {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("inceptool.toml");

            fs::write(&path, "not valid toml {{")?;

            assert_matches!(RawConfig::load_layer(&path), Err(ConfigError::Parse { .. }));

            Ok(())
        }
    }

    mod merge {
        use super::*;

        #[rstest]
        fn later_layer_overrides_earlier_hook() {
            let mut base = RawConfig {
                hooks: BTreeMap::from([("rtk".to_owned(), RawHookConfig { enabled: true })]),
                read_write_guard: ReadWriteGuardRawConfig::default(),
            };

            base.merge(RawConfig {
                hooks: BTreeMap::from([("rtk".to_owned(), RawHookConfig { enabled: false })]),
                read_write_guard: ReadWriteGuardRawConfig::default(),
            });

            assert_eq!(base.hooks.get("rtk").map(|h| h.enabled), Some(false));
        }

        #[rstest]
        fn rules_from_both_layers_accumulate() {
            let mut base = RawConfig {
                hooks: BTreeMap::default(),
                read_write_guard: ReadWriteGuardRawConfig {
                    rules: vec![Rule::new(Cow::Borrowed("a.lock"), Access::Read)],
                },
            };

            base.merge(RawConfig {
                hooks: BTreeMap::default(),
                read_write_guard: ReadWriteGuardRawConfig {
                    rules: vec![Rule::new(Cow::Borrowed("b.lock"), Access::Read)],
                },
            });

            assert_eq!(base.read_write_guard.rules.len(), 2);
        }
    }

    mod parse_base_config {
        use super::*;

        #[rstest]
        fn embedded_config_is_valid() -> Result<(), TestError> {
            RawConfig::parse_base_config()?;
            Ok(())
        }

        #[rstest]
        fn includes_built_in_read_write_guard_rules() -> Result<(), TestError> {
            let raw = RawConfig::parse_base_config()?;

            assert!(!raw.read_write_guard.rules.is_empty());

            Ok(())
        }
    }

    mod try_from {
        use super::*;

        #[rstest]
        fn built_ins_are_present_by_default() -> Result<(), TestError> {
            let config = Config::try_from(RawConfig::parse_base_config()?)?;

            assert!(config.read_write_guard_rules().get("Cargo.lock").is_some());

            Ok(())
        }

        #[rstest]
        fn user_rule_overrides_built_in() -> Result<(), TestError> {
            let mut raw = RawConfig::parse_base_config()?;

            raw.merge(RawConfig {
                hooks: BTreeMap::default(),
                read_write_guard: ReadWriteGuardRawConfig {
                    rules: vec![Rule::new(Cow::Borrowed("Cargo.lock"), Access::Read)],
                },
            });

            let config = Config::try_from(raw)?;

            let (rule, _) = config
                .read_write_guard_rules()
                .get("Cargo.lock")
                .ok_or_else(|| TestError::Failure("expected Cargo.lock rule".into()))?;

            assert_eq!(rule.access(), &Access::Read);

            Ok(())
        }

        #[rstest]
        fn user_rule_adds_new_filename_alongside_built_ins() -> Result<(), TestError> {
            let mut raw = RawConfig::parse_base_config()?;

            raw.merge(RawConfig {
                hooks: BTreeMap::default(),
                read_write_guard: ReadWriteGuardRawConfig {
                    rules: vec![Rule::new(Cow::Borrowed("custom.lock"), Access::Read)],
                },
            });

            let config = Config::try_from(raw)?;

            assert!(config.read_write_guard_rules().get("custom.lock").is_some());
            assert!(config.read_write_guard_rules().get("Cargo.lock").is_some());

            Ok(())
        }

        #[rstest]
        #[case::generated_go_protobuf("/repo/api/service.pb.go")]
        #[case::generated_python_protobuf("/repo/api/service_pb2.py")]
        #[case::node_modules_anywhere("/repo/node_modules/lodash/index.js")]
        #[case::git_internals_anywhere("/repo/.git/HEAD")]
        fn built_in_glob_patterns_are_present(#[case] file_path: &str) -> Result<(), TestError> {
            let config = Config::try_from(RawConfig::parse_base_config()?)?;

            assert!(config.read_write_guard_rules().get(file_path).is_some());

            Ok(())
        }

        #[rstest]
        fn hooks_resolve_to_enabled_flags() -> Result<(), TestError> {
            let raw = RawConfig {
                hooks: BTreeMap::from([("rtk".to_owned(), RawHookConfig { enabled: false })]),
                read_write_guard: ReadWriteGuardRawConfig::default(),
            };

            let config = Config::try_from(raw)?;

            assert!(!config.is_hook_enabled("rtk"));
            assert!(config.is_hook_enabled("pre-commit-runner"));

            Ok(())
        }
    }
}
