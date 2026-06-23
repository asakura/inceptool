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
mod render;
mod rule;
mod scanner;

use self::{
    render::{Renderer, crate_path},
    scanner::Scanner,
};

use crate::{
    error::RiskDataError,
    types::{Dataset, Platform},
};

use proc_macro2::TokenStream;
use quote::quote;

use std::{collections::BTreeMap, iter, mem, path::Path};

/// One name/alias's rendered [`crate::entry::CommandEntry`] tokens, tagged with the [`Platform`]
/// it models — one per [`PlatformPairs`] entry.
struct PlatformPair {
    variant: (Platform, TokenStream),
}

impl PlatformPair {
    #[must_use = "constructing a pair has no effect unless the caller stores it"]
    pub const fn new(platform: Platform, entry: TokenStream) -> Self {
        Self {
            variant: (platform, entry),
        }
    }
}

/// Every [`Platform`]-tagged rendered [`crate::entry::CommandEntry`] one command name/alias
/// resolves to, accumulated by [`generate_command_table`]'s grouping pass before being rendered
/// into a single `&[PlatformEntry]` slice.
#[derive(Default)]
struct PlatformPairs {
    variants: Vec<PlatformPair>,
}

impl Renderer for PlatformPairs {
    /// `self` rendered as the `&'static [::inceptool_risk_data::PlatformEntry]` slice that one
    /// command name/alias maps to in the generated `COMMANDS` table. Table-structure-specific,
    /// not a single schema value's own rendering, so this stays its own impl rather than living
    /// alongside the schema-type impls in [`render`].
    fn render(self) -> TokenStream {
        let platform_entry = crate_path("PlatformEntry");

        let rendered = self.variants.iter().map(
            |PlatformPair {
                 variant: (platform, entry),
             }| {
                let platform = platform.render();

                quote! { #platform_entry { platform: #platform, entry: #entry } }
            },
        );

        quote! { &[#(#rendered),*] }
    }
}

/// Recursively walks `dir` for `.toml` files, merges and validates them, and renders the result
/// into tokens for a `COMMANDS` lookup table declaration.
///
/// The declaration takes the form `static COMMANDS: ::phf::Map<&'static str, &'static
/// [::inceptool_risk_data::PlatformEntry]> = ...;` — every type the generated tokens reference
/// (`PlatformEntry`, `CommandEntry`, `RuleSetEntry`, `FlagEntry`, `ComboRuleEntry`,
/// `OperandRuleEntry`, `ValueRuleEntry`, `ProfilePatch`, `Effect`, `TakesValue`, `FlagGrammar`,
/// `Platform`, `TrustImpact`, `Reversibility`, `BlastRadius`, `Disclosure`, `Persistence`,
/// `Privilege`, `Auditability`, `Exposure`, `Verification`, and `::phf::Map` itself) is named
/// by an absolute path, so the caller needs no `use` statement at the point it `include!`s this
/// text — it only needs `inceptool-risk-data` and `phf` as dependencies.
///
/// A missing, or non-directory, `dir` is an error — see [`RiskDataError::MissingRoot`] — as is a
/// `dir` that contains no `.toml` file recursively — see [`RiskDataError::NoTomlFiles`].
///
/// # Examples
///
/// ```
/// # use inceptool_risk_data::generate_command_table;
/// # use std::fs;
/// # use tempfile::TempDir;
/// let dir = TempDir::new()?;
/// fs::write(
///     dir.path().join("a.toml"),
///     r#"
///         [[command]]
///         name = "eval"
///         kind = "builtin"
///         baseline_reason = "Executes a constructed string as a command."
///     "#,
/// )?;
/// let source = generate_command_table(dir.path())?.to_string();
/// assert!(source.contains("PlatformEntry"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
///
/// Returns [`RiskDataError::MissingRoot`] if `dir` doesn't exist or isn't a directory,
/// [`RiskDataError::NoTomlFiles`] if it contains no `.toml` file recursively,
/// [`RiskDataError::Io`] if the directory can't be walked, [`RiskDataError::Toml`] if a file's
/// content doesn't match the schema, or
/// [`RiskDataError::DuplicateCommand`], [`RiskDataError::DuplicateSubcommand`],
/// [`RiskDataError::DuplicateFlagSpelling`], [`RiskDataError::UnknownComboFlag`],
/// [`RiskDataError::CombinableUnderGoGrammar`], or [`RiskDataError::InvalidPattern`] if the
/// merged data fails a cross-reference check.
#[must_use = "generating has no effect unless the caller writes the result somewhere"]
pub fn generate_command_table<P>(dir: P) -> Result<TokenStream, RiskDataError>
where
    P: AsRef<Path>,
{
    let scanner = Scanner::new(dir)?;
    let dataset = Dataset::try_from(scanner)?;

    let mut grouped: BTreeMap<Box<str>, PlatformPairs> = BTreeMap::new();

    for command in dataset.command {
        let platform = command.platform;
        let names: Vec<Box<str>> = iter::once(command.name.clone())
            .chain(command.aliases.clone())
            .collect();
        let mut rendered = command.render();

        let last = names.len().saturating_sub(1);

        for (index, name) in names.into_iter().enumerate() {
            // Every alias entry shares the one rendering: moved into the last entry rather than
            // cloned, since this iteration has no further use for it.
            let entry = if index == last {
                mem::take(&mut rendered)
            } else {
                rendered.clone()
            };

            grouped
                .entry(name)
                .or_default()
                .variants
                .push(PlatformPair::new(platform, entry));
        }
    }

    let values = grouped
        .into_iter()
        .map(|(key, variants)| (key, variants.render()))
        .collect::<Vec<_>>();

    let mut map = phf_codegen::Map::new();

    for (key, value) in &values {
        map.entry(key.as_ref(), value.to_string());
    }

    Ok(map.render())
}

impl Renderer for phf_codegen::Map<'_, &str> {
    fn render(self) -> TokenStream {
        let built = self.build();
        let phf_map = quote! { ::phf::Map };
        let platform_entry = crate_path("PlatformEntry");

        quote! {
            static COMMANDS: #phf_map<&'static str, &'static [#platform_entry]> = #built;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::assert_matches;
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
