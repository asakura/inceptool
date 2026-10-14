//! Platform-aware user configuration: which stages are enabled, and
//! stage-specific overrides, loaded from `inceptool.toml`.
//!
//! Configuration is resolved in two layers: [`raw_config::RawConfig`] (the
//! user-facing shape that maps directly onto the TOML file, never exposed
//! outside this module — also used to parse the embedded base config) and
//! [`Config`] (the stage-provided shape: built-in defaults merged with user
//! overrides, ready to hand to [`crate::registry::build_registry`]).

mod error;
mod raw_config;
mod resolved;

pub use error::ConfigError;
pub use resolved::{Config, project_dirs};

#[cfg(test)]
mod tests {
    use super::*;

    use super::raw_config::RawConfig;

    use inceptool_stages::read_write_guard::{Access, Rule};

    use rstest::rstest;

    use core::assert_matches;
    use std::{
        borrow::Cow,
        fs, io,
        path::{Path, PathBuf},
    };

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error(transparent)]
        Config(#[from] ConfigError),
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error("Test failure: {0}")]
        Failure(String),
    }

    /// Builds a [`Config`] with exactly the given hooks and rules, for
    /// exercising [`Config::merge`] without going through a TOML layer.
    fn config_with(
        hooks: impl IntoIterator<Item = (&'static str, bool)>,
        rules: impl IntoIterator<Item = Rule>,
    ) -> Config {
        Config {
            hooks: hooks
                .into_iter()
                .map(|(name, enabled)| (Cow::Borrowed(name), enabled))
                .collect(),
            read_write_guard_rules: rules.into_iter().collect(),
            repo_root: None,
        }
    }

    mod merge {
        use super::*;

        #[rstest]
        fn later_layer_overrides_earlier_hook() {
            let mut base = config_with([("rtk", true)], []);
            let other = config_with([("rtk", false)], []);

            base.merge(other, Path::new("layer.toml"));

            assert_eq!(base.hooks.get("rtk"), Some(&false));
        }

        #[rstest]
        fn rules_from_both_layers_accumulate() {
            let mut base = config_with([], [Rule::new(Cow::Borrowed("a.lock"), Access::DenyRead)]);
            let other = config_with([], [Rule::new(Cow::Borrowed("b.lock"), Access::DenyRead)]);

            base.merge(other, Path::new("layer.toml"));

            assert!(base.read_write_guard_rules.get("a.lock").is_some());
            assert!(base.read_write_guard_rules.get("b.lock").is_some());
        }

        #[rstest]
        fn later_layer_rule_overrides_earlier_for_same_filename() -> Result<(), TestError> {
            let mut base = config_with(
                [],
                [Rule::new(Cow::Borrowed("Cargo.lock"), Access::DenyRead)],
            );

            let other = config_with(
                [],
                [Rule::new(
                    Cow::Borrowed("Cargo.lock"),
                    Access::DenyWrite {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                )],
            );

            base.merge(other, Path::new("layer.toml"));

            let (rule, _) = base
                .read_write_guard_rules
                .get("Cargo.lock")
                .ok_or_else(|| TestError::Failure("expected Cargo.lock rule".into()))?;

            assert_matches!(rule.access(), Access::DenyWrite { .. });

            Ok(())
        }

        #[rstest]
        fn later_broader_glob_rule_drops_earlier_more_specific_rule() -> Result<(), TestError> {
            let mut base = config_with(
                [],
                [Rule::new(Cow::Borrowed("Cargo.lock"), Access::DenyRead)],
            );

            let other = config_with(
                [],
                [Rule::new(
                    Cow::Borrowed("*.lock"),
                    Access::DenyWrite {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                )],
            );

            base.merge(other, Path::new("layer.toml"));

            let (rule, _) = base
                .read_write_guard_rules
                .get("Cargo.lock")
                .ok_or_else(|| {
                    TestError::Failure("expected a match via the broader glob".into())
                })?;

            assert_matches!(rule.access(), Access::DenyWrite { .. });

            Ok(())
        }

        #[rstest]
        fn reasserted_specific_rule_overrides_a_broader_glob_again() -> Result<(), TestError> {
            let mut base = config_with(
                [],
                [Rule::new(
                    Cow::Borrowed("*.lock"),
                    Access::DenyWrite {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                )],
            );

            let other = config_with(
                [],
                [Rule::new(Cow::Borrowed("Cargo.lock"), Access::DenyRead)],
            );

            base.merge(other, Path::new("layer.toml"));

            let (rule, _) = base
                .read_write_guard_rules
                .get("Cargo.lock")
                .ok_or_else(|| TestError::Failure("expected the reasserted exact rule".into()))?;

            assert_matches!(rule.access(), Access::DenyRead);

            Ok(())
        }
    }

    mod merge_layer_at {
        use super::*;

        #[rstest]
        fn missing_layer_is_skipped() -> Result<(), TestError> {
            let dir = tempfile::tempdir()?;
            let mut config = Config::try_from(RawConfig::default())?;

            config.merge_layer_at(&dir.path().join("missing.toml"));

            assert!(config.hooks.is_empty());

            Ok(())
        }

        #[rstest]
        fn malformed_layer_is_skipped_not_propagated() -> Result<(), TestError> {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("inceptool.toml");

            fs::write(&path, "not valid toml {{")?;

            let mut config = Config::try_from(RawConfig::default())?;
            config.merge_layer_at(&path);

            assert!(config.hooks.is_empty());

            Ok(())
        }

        #[rstest]
        fn valid_layer_is_merged() -> Result<(), TestError> {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("inceptool.toml");

            fs::write(&path, "[hooks.rtk]\nenabled = false\n")?;

            let mut config = Config::try_from(RawConfig::default())?;
            config.merge_layer_at(&path);

            assert_eq!(config.hooks.get("rtk"), Some(&false));

            Ok(())
        }
    }

    mod repo_root {
        use super::*;

        #[rstest]
        fn reflects_whatever_was_discovered() -> Result<(), TestError> {
            let mut config = Config::try_from(RawConfig::default())?;
            config.repo_root = Some(PathBuf::from("/repo"));

            assert_eq!(config.repo_root(), Some(Path::new("/repo")));

            Ok(())
        }

        #[rstest]
        fn is_none_when_nothing_was_discovered() -> Result<(), TestError> {
            let config = Config::try_from(RawConfig::default())?;

            assert_eq!(config.repo_root(), None);

            Ok(())
        }
    }
}
