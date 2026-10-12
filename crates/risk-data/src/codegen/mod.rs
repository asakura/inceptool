//! Recursively scans a directory for command-risk `.toml` files and code-generates the `phf`
//! lookup table a caller's runtime classification logic reads — see [`generate_command_table`].
//!
//! ## Core Design
//!
//! [`generate_command_table`] is the crate's one public entry point: filesystem discovery
//! (`scanner`), `crate::types`' TOML schema (parse + merge + validate), and rendering into
//! [`crate::entry::CommandEntry`]-shaped tokens (`render`/`rule`/`command`) all happen
//! behind it, so a caller's build script needs nothing more than a directory path and a place to
//! write the result. Every `Renderer` impl builds a [`proc_macro2::TokenStream`] via
//! `quote::quote!` rather than formatting text by hand, and every type it names resolves through
//! an absolute `render::crate_path` helper (e.g. `::inceptool_risk_data::CommandEntry`) — the
//! generated tokens never depend on a `use` statement at the call site, unlike a hand-formatted
//! `format!()` string would.
//!
//! ## Flow
//!
//! 1. `scanner::Scanner::new` walks the directory tree, depth-first, collecting every `.toml`
//!    file's `(path, content)` pair and sorting the flat result by path for a deterministic
//!    merge order regardless of the filesystem's own directory-listing order.
//! 2. `Dataset::try_from(scanner)` parses and merges every file's `command` list via the
//!    crate-internal `types::Dataset::parse`, which also validates the merged result —
//!    `types::Dataset::validate` catches every cross-reference the schema alone can't.
//! 3. Each `types::Command` is rendered into `CommandEntry`-shaped tokens via its `Renderer`
//!    impl, grouped by every name/alias it resolves to across all parsed files into a
//!    `BTreeMap<Box<str>, PlatformPairs>` (one `PlatformPair` per declared `Platform` variant),
//!    then each group's variants are rendered into one `&[PlatformEntry]` slice and handed to a
//!    `phf_codegen::Map` (stringified once, since its `entry` method takes text) —
//!    perfect-hash construction happens once, here, not at runtime.
//! 4. The built `phf_codegen::Map` implements `quote::ToTokens` (the crate's `quote` feature),
//!    so it's spliced directly into the final `static COMMANDS: ::phf::Map<...> = ...;` tokens
//!    rather than being formatted back into text by this crate.
//!
//! ## Edge Cases
//!
//! - A missing, or non-directory, root is reported via [`crate::RiskDataError::MissingRoot`]
//!   rather than silently treated as having no risk data to declare.
//! - A root that exists and is a directory, but contains no `.toml` file recursively, is
//!   reported via [`crate::RiskDataError::NoTomlFiles`] rather than silently producing an empty
//!   table.
//! - A non-UTF-8 file/directory name under the root is supported: it's lossily converted (`�`
//!   for invalid sequences) rather than rejected, since the name is only ever used as a sort key
//!   and a diagnostic label, never round-tripped back into a real filesystem path.

mod command;
mod generator;
mod platform_pair;
mod render;
mod rule;
mod scanner;

pub use generator::generate_command_table;

use self::render::Renderer;

use proc_macro2::TokenStream;

#[cfg(test)]
mod tests {
    use super::*;

    use core::assert_matches;
    use crate::error::RiskDataError;
    use rstest::rstest;

    use std::os::unix::fs::symlink;
    use std::{fs, io};

    #[derive(Debug, thiserror::Error)]
    enum TestError {
        #[error(transparent)]
        RiskData(#[from] RiskDataError),
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error(transparent)]
        Syn(#[from] syn::Error),
    }

    mod generate_command_table {
        use super::*;

        use tempfile::TempDir;

        #[rstest]
        fn recursively_discovers_toml_files_in_nested_subdirectories() -> Result<(), TestError> {
            let dir = TempDir::new()?;
            let sub = dir.path().join("sub");
            fs::create_dir_all(&sub)?;

            fs::write(
                dir.path().join("a.toml"),
                r#"
                    [[command]]
                    name = "eval"
                    kind = "builtin"
                    baseline_reason = "Executes a constructed string as a command."
                "#,
            )?;
            fs::write(
                sub.join("b.toml"),
                r#"
                    [[command]]
                    name = "kill"
                    kind = "builtin"
                    baseline_reason = "Sends a signal to another process."
                "#,
            )?;

            let rendered = generate_command_table(dir.path())?.to_string();

            assert!(rendered.contains("\"eval\""));
            assert!(rendered.contains("\"kill\""));
            Ok(())
        }

        #[rstest]
        fn follows_a_symlinked_subdirectory() -> Result<(), TestError> {
            let dir = TempDir::new()?;
            // Lives outside the scanned root, reachable only through the symlink below — proves
            // the walk follows the link rather than happening to find the same file twice.
            let real = TempDir::new()?;

            fs::write(
                real.path().join("b.toml"),
                r#"
                    [[command]]
                    name = "kill"
                    kind = "builtin"
                    baseline_reason = "Sends a signal to another process."
                "#,
            )?;

            symlink(real.path(), dir.path().join("linked"))?;

            let rendered = generate_command_table(dir.path())?.to_string();

            assert!(rendered.contains("\"kill\""));
            Ok(())
        }

        #[rstest]
        fn ignores_non_toml_files() -> Result<(), TestError> {
            let dir = TempDir::new()?;
            fs::write(dir.path().join("notes.txt"), "not toml")?;
            fs::write(
                dir.path().join("a.toml"),
                r#"
                    [[command]]
                    name = "eval"
                    kind = "builtin"
                    baseline_reason = "Executes a constructed string as a command."
                "#,
            )?;

            let rendered = generate_command_table(dir.path())?.to_string();
            assert!(rendered.contains("\"eval\""));
            Ok(())
        }

        #[rstest]
        fn renders_to_a_syntactically_valid_static_item() -> Result<(), TestError> {
            let dir = TempDir::new()?;
            fs::write(
                dir.path().join("a.toml"),
                r#"
                    [[command]]
                    name = "eval"
                    kind = "builtin"
                    baseline_reason = "Executes a constructed string as a command."

                    [[command.flag]]
                    spellings = ["-p"]
                    effect = "escalate"
                    profile = { trust = "arbitrary_execution" }
                    reason = "Treats the operand as a path."
                "#,
            )?;

            let rendered = generate_command_table(dir.path())?;
            syn::parse2::<syn::ItemStatic>(rendered)?;
            Ok(())
        }

        #[rstest]
        fn rendered_tokens_reference_no_bare_crate_type_names() -> Result<(), TestError> {
            let dir = TempDir::new()?;
            fs::write(
                dir.path().join("a.toml"),
                r#"
                    [[command]]
                    name = "eval"
                    kind = "builtin"
                    baseline_reason = "Executes a constructed string as a command."
                "#,
            )?;

            let rendered = generate_command_table(dir.path())?.to_string();

            assert!(rendered.contains(":: inceptool_risk_data :: CommandEntry"));
            assert!(rendered.contains(":: inceptool_risk_data :: PlatformEntry"));
            assert!(rendered.contains(":: phf :: Map"));
            Ok(())
        }

        #[rstest]
        fn renders_value_rules_takes_value_combo_rules_and_a_mitigating_effect()
        -> Result<(), TestError> {
            let dir = TempDir::new()?;
            fs::write(
                dir.path().join("a.toml"),
                r#"
                    [[command]]
                    name = "rsync"
                    kind = "external"
                    baseline_reason = "Synchronizes files, locally or over a remote shell."

                      [[command.flag]]
                      spellings = ["--password-file"]
                      effect = "escalate"
                      profile = { reversibility = "irreversible", blast_radius = "broad" }
                      reason = "Reads a plaintext credential from a file."
                      takes_value = "combined"

                        [[command.flag.value_rule]]
                        pattern = "^-$"
                        effect = "mitigate"
                        reason = "Reading from stdin avoids leaving a credential on disk."

                      [[command.flag]]
                      spellings = ["--port"]
                      effect = "escalate"
                      reason = "Targets a non-default port."

                      [[command.combo_rule]]
                      requires = ["--password-file", "--port"]
                      effect = "mitigate"
                      reason = "Combo rule coverage fixture."

                      [[command.operand_rule]]
                      pattern = "^--delete$"
                      effect = "mitigate"
                      reason = "Operand rule coverage fixture."
                "#,
            )?;

            let rendered = generate_command_table(dir.path())?.to_string();

            assert!(rendered.contains("ValueRuleEntry"));
            assert!(rendered.contains("ComboRuleEntry"));
            assert!(rendered.contains("OperandRuleEntry"));
            assert!(rendered.contains("Combined"));
            assert!(rendered.contains("Mitigate"));
            assert!(rendered.contains("Irreversible"));
            assert!(rendered.contains("Broad"));
            Ok(())
        }

        #[rstest]
        fn renders_every_platform_and_grammar_variant_plus_subcommands() -> Result<(), TestError> {
            let dir = TempDir::new()?;
            fs::write(
                dir.path().join("a.toml"),
                r#"
                    [[command]]
                    name = "kill"
                    kind = "builtin"
                    platform = "bsd"
                    grammar = "bsd"
                    baseline_reason = "Sends a signal to another process."

                    [[command]]
                    name = "kill"
                    kind = "builtin"
                    platform = "mac_os"
                    baseline_reason = "Sends a signal to another process."

                    [[command]]
                    name = "kill"
                    kind = "builtin"
                    platform = "busybox"
                    baseline_reason = "Sends a signal to another process."

                    [[command]]
                    name = "kubectl"
                    kind = "external"
                    grammar = "go"
                    baseline_reason = "Controls a Kubernetes cluster."

                    [[command]]
                    name = "git"
                    kind = "external"
                    baseline_reason = "Version control."

                      [[command.subcommands]]
                      name = "push"
                      aliases = ["p"]
                      baseline_reason = "Uploads commits to a remote."

                        [[command.subcommands.flag]]
                        spellings = ["-f", "--force"]
                        effect = "escalate"
                        reason = "Overwrites the remote history."
                "#,
            )?;

            let rendered = generate_command_table(dir.path())?.to_string();

            assert!(rendered.contains("Bsd"));
            assert!(rendered.contains("MacOs"));
            assert!(rendered.contains("Busybox"));
            assert!(rendered.contains("Go"));
            assert!(rendered.contains("\"push\""));
            assert!(rendered.contains("\"p\""));
            Ok(())
        }

        #[rstest]
        fn missing_directory_is_an_error() -> Result<(), TestError> {
            let parent = TempDir::new()?;
            let missing = parent.path().join("does-not-exist");

            let result = generate_command_table(&missing);
            assert_matches!(result, Err(RiskDataError::MissingRoot { .. }));
            Ok(())
        }

        #[rstest]
        fn non_directory_root_is_an_error() -> Result<(), TestError> {
            let dir = TempDir::new()?;
            let file = dir.path().join("not-a-dir.toml");
            fs::write(&file, "")?;

            let result = generate_command_table(&file);
            assert_matches!(result, Err(RiskDataError::MissingRoot { .. }));
            Ok(())
        }

        #[rstest]
        fn empty_directory_is_an_error() -> Result<(), TestError> {
            let dir = TempDir::new()?;

            let result = generate_command_table(dir.path());
            assert_matches!(result, Err(RiskDataError::NoTomlFiles { .. }));
            Ok(())
        }

        #[rstest]
        fn propagates_a_validation_error_across_subdirectories() -> Result<(), TestError> {
            let dir = TempDir::new()?;
            let sub = dir.path().join("sub");
            fs::create_dir_all(&sub)?;

            let kill_toml = r#"
                [[command]]
                name = "kill"
                kind = "builtin"
                baseline_reason = "Sends a signal to another process."
            "#;

            fs::write(dir.path().join("a.toml"), kill_toml)?;
            fs::write(sub.join("b.toml"), kill_toml)?;

            let result = generate_command_table(dir.path());
            assert_matches!(result, Err(RiskDataError::DuplicateCommand { .. }));
            Ok(())
        }
    }
}
