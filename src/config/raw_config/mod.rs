//! The TOML-facing mirror of [`Config`](crate::config::Config): every type in this module and its
//! submodules ([`access`], [`raw_hook_config`], [`read_write_guard_raw_config`],
//! [`rule`]) exists only to describe what a user may write in
//! `inceptool.toml` (or the embedded base config) and to convert that,
//! field by field, into the stage-facing types the rest of the binary uses
//! ([`Config`](crate::config::Config), [`inceptool_stages::read_write_guard::Rule`],
//! [`inceptool_stages::read_write_guard::Access`]).
//!
//! That conversion boundary is deliberate, not incidental:
//! [`Rule`](inceptool_stages::read_write_guard::Rule) and
//! [`Access`](inceptool_stages::read_write_guard::Access) are policy types
//! owned by the `inceptool-stages` crate, used by
//! [`ReadWriteGuardStage`](inceptool_stages::ReadWriteGuardStage) to decide
//! what to do with a read or write. They are *not* `Deserialize`, and
//! should stay that way.
//!
//! So instead, [`rule::RawRule`]/[`access::RawAccess`] mirror the TOML
//! shape exactly (with `deny_unknown_fields` living *there*, where it's
//! actually about catching user typos) and convert via [`From`] into
//! [`Rule`](inceptool_stages::read_write_guard::Rule)/[`Access`](inceptool_stages::read_write_guard::Access).
//! [`RawConfig`] itself follows the same pattern one level up, converting
//! into [`Config`](crate::config::Config) via [`TryFrom`].
//!
//! [`RawConfig`] only ever describes a single layer (the embedded base
//! config, or one `inceptool.toml`): it has no merge logic of its own.
//! Combining layers is [`Config::merge`](crate::config::Config::merge)'s job, once each layer has already
//! been resolved into the stage-facing shape — see that method's doc for
//! why multi-layer merging belongs there rather than here.

mod access;
mod document;
mod raw_hook_config;
mod read_write_guard_raw_config;
mod rule;

pub(super) use document::RawConfig;

#[cfg(test)]
mod tests {
    use super::*;

    use super::{
        access::RawAccess, raw_hook_config::RawHookConfig,
        read_write_guard_raw_config::ReadWriteGuardRawConfig, rule::RawRule,
    };
    use crate::config::{Config, ConfigError};

    use inceptool_stages::read_write_guard::Access;

    use rstest::rstest;
    use toml::de::Error as TomlDeError;

    use core::assert_matches;
    use std::{borrow::Cow, collections::BTreeMap, fs, io};

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error(transparent)]
        Config(#[from] ConfigError),
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error(transparent)]
        TomlDe(#[from] TomlDeError),
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

        #[rstest]
        fn typo_d_rules_table_name_is_rejected_not_silently_dropped() -> Result<(), TestError> {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("inceptool.toml");

            // "rule" (singular) instead of "rules" — without
            // #[serde(deny_unknown_fields)] this parses fine and silently
            // produces zero rules instead of the one the user wrote.
            fs::write(
                &path,
                "[[read-write-guard.rule]]\nfilename = \"flake.lock\"\n",
            )?;

            assert_matches!(RawConfig::load_layer(&path), Err(ConfigError::Parse { .. }));

            Ok(())
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

            assert_ne!(raw.read_write_guard.rules, Vec::<RawRule>::new());

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

            raw.read_write_guard.rules.push(RawRule {
                filename: Cow::Borrowed("Cargo.lock"),
                access: RawAccess::DenyRead,
            });

            let config = Config::try_from(raw)?;

            let (rule, _) = config
                .read_write_guard_rules()
                .get("Cargo.lock")
                .ok_or_else(|| TestError::Failure("expected Cargo.lock rule".into()))?;

            assert_eq!(rule.access(), &Access::DenyRead);

            Ok(())
        }

        #[rstest]
        #[case::nix("flake.lock")]
        #[case::npm("package-lock.json")]
        #[case::yarn("yarn.lock")]
        #[case::pnpm("pnpm-lock.yaml")]
        #[case::bun_lock("bun.lock")]
        #[case::bun_lockb("bun.lockb")]
        #[case::cargo("Cargo.lock")]
        #[case::poetry("poetry.lock")]
        #[case::pipenv("Pipfile.lock")]
        #[case::uv("uv.lock")]
        #[case::go("go.sum")]
        #[case::bundler("Gemfile.lock")]
        #[case::composer("composer.lock")]
        #[case::mix("mix.lock")]
        #[case::pub_dart("pubspec.lock")]
        #[case::terraform_lock(".terraform.lock.hcl")]
        fn original_built_in_lockfiles_are_present(
            #[case] file_name: &str,
        ) -> Result<(), TestError> {
            let config = Config::try_from(RawConfig::parse_base_config()?)?;

            assert!(config.read_write_guard_rules().get(file_name).is_some());

            Ok(())
        }

        #[rstest]
        fn user_rule_adds_new_filename_alongside_built_ins() -> Result<(), TestError> {
            let mut raw = RawConfig::parse_base_config()?;

            raw.read_write_guard.rules.push(RawRule {
                filename: Cow::Borrowed("custom.lock"),
                access: RawAccess::DenyRead,
            });

            let config = Config::try_from(raw)?;

            assert!(config.read_write_guard_rules().get("custom.lock").is_some());
            assert!(config.read_write_guard_rules().get("Cargo.lock").is_some());

            Ok(())
        }

        #[rstest]
        #[case::generated_go_protobuf("/repo/api/service.pb.go")]
        #[case::generated_go_grpc("/repo/api/service_grpc.pb.go")]
        #[case::generated_python_protobuf("/repo/api/service_pb2.py")]
        #[case::generated_python_grpc("/repo/api/service_pb2_grpc.py")]
        #[case::node_modules_anywhere("/repo/node_modules/lodash/index.js")]
        #[case::git_internals_anywhere("/repo/.git/HEAD")]
        #[case::svn_internals_anywhere("/repo/.svn/entries")]
        #[case::hg_internals_anywhere("/repo/.hg/store/x")]
        #[case::next_cache("/repo/.next/cache/x.json")]
        #[case::nuxt_cache("/repo/.nuxt/dist/x.js")]
        #[case::svelte_kit_cache("/repo/.svelte-kit/output/x.js")]
        #[case::angular_cache("/repo/.angular/cache/x")]
        #[case::turbo_cache("/repo/.turbo/cache/x")]
        #[case::nx_cache("/repo/.nx/cache/x")]
        #[case::ts_build_info("/repo/tsconfig.tsbuildinfo")]
        #[case::eslintcache("/repo/.eslintcache")]
        #[case::vercel_state("/repo/.vercel/output/x")]
        #[case::netlify_state("/repo/.netlify/state.json")]
        #[case::coverage_output("/repo/coverage/lcov.info")]
        #[case::nyc_output("/repo/.nyc_output/x.json")]
        #[case::playwright_report("/repo/playwright-report/index.html")]
        #[case::test_results("/repo/test-results/x.png")]
        #[case::cypress_videos("/repo/cypress/videos/x.mp4")]
        #[case::cypress_screenshots("/repo/cypress/screenshots/x.png")]
        #[case::pytest_cache("/repo/.pytest_cache/v/cache/x")]
        #[case::tox_cache("/repo/.tox/py311/x")]
        #[case::nox_cache("/repo/.nox/x")]
        #[case::mypy_cache("/repo/.mypy_cache/x")]
        #[case::ruff_cache("/repo/.ruff_cache/x")]
        #[case::pycache_dir("/repo/pkg/__pycache__/x.pyc")]
        #[case::pyc_file("/repo/x.pyc")]
        #[case::gradle_cache("/repo/.gradle/caches/x")]
        #[case::intellij_module("/repo/module.iml")]
        #[case::intellij_project("/repo/.idea/workspace.xml")]
        #[case::android_keystore("/repo/release.keystore")]
        #[case::java_keystore("/repo/release.jks")]
        #[case::vs_user_settings("/repo/MyApp.csproj.user")]
        #[case::vs_local_state("/repo/.vs/x")]
        #[case::cocoapods_pods("/repo/ios/Pods/Alamofire/x")]
        #[case::xcode_user_state("/repo/x.xcuserstate")]
        #[case::xcode_user_data("/repo/x.xcodeproj/xcuserdata/u/x")]
        #[case::xcode_derived_data("/repo/DerivedData/Build/x")]
        #[case::graphql_codegen_file("/repo/api.generated.ts")]
        #[case::graphql_codegen_dir("/repo/src/__generated__/x.ts")]
        #[case::wasm_binary("/repo/pkg/app.wasm")]
        #[case::terraform_cache("/repo/.terraform/providers/x")]
        #[case::terraform_plan("/repo/plan.tfplan")]
        #[case::ansible_retry("/repo/playbook.retry")]
        #[case::pulumi_state("/repo/.pulumi/x")]
        #[case::vim_swap_p("/repo/.main.rs.swp")]
        #[case::vim_swap_o("/repo/.main.rs.swo")]
        #[case::macos_ds_store("/repo/.DS_Store")]
        #[case::pem_key("/repo/server.pem")]
        #[case::p12_bundle("/repo/cert.p12")]
        #[case::pfx_bundle("/repo/cert.pfx")]
        #[case::ssh_rsa_key("/home/user/.ssh/id_rsa")]
        #[case::ssh_dsa_key("/home/user/.ssh/id_dsa")]
        #[case::ssh_ecdsa_key("/home/user/.ssh/id_ecdsa")]
        #[case::ssh_ed25519_key("/home/user/.ssh/id_ed25519")]
        #[case::npm_debug_log("/repo/npm-debug.log.12345")]
        #[case::yarn_error_log("/repo/yarn-error.log")]
        #[case::lerna_debug_log("/repo/lerna-debug.log")]
        #[case::crash_dump("/repo/crash.dmp")]
        #[case::parquet_data("/repo/data.parquet")]
        #[case::avro_data("/repo/data.avro")]
        #[case::orc_data("/repo/data.orc")]
        #[case::safetensors_model("/repo/model.safetensors")]
        #[case::onnx_model("/repo/model.onnx")]
        #[case::pytorch_model("/repo/model.pt")]
        #[case::generic_binary("/repo/model.bin")]
        #[case::sqlite_db("/repo/cache.sqlite")]
        #[case::sqlite3_db("/repo/cache.sqlite3")]
        #[case::generic_db("/repo/cache.db")]
        fn built_in_glob_patterns_are_present(#[case] file_path: &str) -> Result<(), TestError> {
            let config = Config::try_from(RawConfig::parse_base_config()?)?;

            assert!(config.read_write_guard_rules().get(file_path).is_some());

            Ok(())
        }

        #[rstest]
        fn hooks_resolve_to_enabled_flags() -> Result<(), TestError> {
            let raw = RawConfig {
                hooks: BTreeMap::from([(Cow::Borrowed("rtk"), RawHookConfig { enabled: false })]),
                read_write_guard: ReadWriteGuardRawConfig::default(),
            };

            let config = Config::try_from(raw)?;

            assert!(!config.is_hook_enabled("rtk"));
            assert!(config.is_hook_enabled("pre-commit-runner"));

            Ok(())
        }
    }

    mod from_config {
        use super::*;

        #[rstest]
        fn preserves_hook_flags() -> Result<(), TestError> {
            let raw = RawConfig {
                hooks: BTreeMap::from([(Cow::Borrowed("rtk"), RawHookConfig { enabled: false })]),
                read_write_guard: ReadWriteGuardRawConfig::default(),
            };

            let config = Config::try_from(raw)?;
            let round_tripped = RawConfig::from(config);

            assert_eq!(
                round_tripped.hooks.get("rtk").map(|hook| hook.enabled),
                Some(false)
            );

            Ok(())
        }

        #[rstest]
        fn preserves_every_rule() -> Result<(), TestError> {
            let config = Config::try_from(RawConfig::parse_base_config()?)?;
            let base_rule_count = RawConfig::parse_base_config()?.read_write_guard.rules.len();

            let round_tripped = RawConfig::from(config);

            assert_eq!(round_tripped.read_write_guard.rules.len(), base_rule_count);

            Ok(())
        }

        #[rstest]
        fn round_trips_through_toml_text() -> Result<(), TestError> {
            let config = Config::try_from(RawConfig::parse_base_config()?)?;

            let toml_text = RawConfig::from(config).to_toml()?;
            let reparsed: RawConfig = toml::from_str(&toml_text)?;
            let reparsed_config = Config::try_from(reparsed)?;

            let (rule, _) = reparsed_config
                .read_write_guard_rules()
                .get("Cargo.lock")
                .ok_or_else(|| TestError::Failure("expected Cargo.lock rule".into()))?;

            assert_matches!(rule.access(), Access::DenyAll { .. });

            Ok(())
        }
    }
}
