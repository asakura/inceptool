//! # Pre-commit Config Parser Architecture
//!
//! Parses `.pre-commit-config.yaml` files produced by [pre-commit](https://pre-commit.com)
//! into typed Rust structs for use by pipeline stages.
//!
//! ## Core Design
//!
//! String fields use `Cow<'a, str>` with `#[serde(borrow)]`. Unlike the legacy
//! `serde_yaml` crate, `serde-saphyr` properly calls `visit_borrowed_str` for
//! plain YAML scalars, so `Cow::Borrowed` is produced wherever the source text
//! can be used directly without transformation; quoted scalars that require
//! unescaping fall back to `Cow::Owned`.
//!
//! Required fields (`repo` in [`Repo`], `id` in [`Hook`]) omit
//! `#[serde(default)]` so that a config entry missing those keys produces a
//! parse error instead of silently defaulting to an empty string.
//!
//! ## Flow
//!
//! 1. **Parse**: [`PreCommitConfig::parse`] deserializes a YAML string into
//!    config → repos → hooks.
//! 2. **Inspect**: Accessor methods surface each field. Optional strings return
//!    `Option<&str>` via [`Option::as_deref`]; string lists return
//!    `&[Cow<'a, str>]`.
//! 3. **Pattern matching**: [`Hook::files_regex`] and [`Hook::exclude_regex`]
//!    compile patterns with [`fancy_regex`], which supports the Python `re`
//!    syntax (including lookaheads and lookbehinds) used by pre-commit.
//!    [`Hook::files_pattern`] and [`Hook::exclude_pattern`] return the raw
//!    string for callers that need it.
//!
//! ## Edge Cases
//!
//! - [`Repo::local_path`] returns `None` for remote URLs (those containing
//!   `://`) because casting a URL such as `https://github.com/…` to a [`Path`]
//!   is semantically wrong.
//! - `pass_filenames` defaults to `true` per the pre-commit schema; all other
//!   boolean flags default to `false`.

use serde::Deserialize;

use std::borrow::Cow;
use std::path::Path;
use std::slice::Iter;

/// Top-level structure of a `.pre-commit-config.yaml` file.
///
/// A pre-commit config lists one or more [`Repo`]s, each of which provides
/// [`Hook`]s that run at specific Git lifecycle stages (e.g. `pre-commit`,
/// `commit-msg`). `default_stages` sets the fallback when a hook does not
/// declare its own [`Hook::stages`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PreCommitConfig<'a> {
    #[serde(borrow, default)]
    default_stages: Vec<Cow<'a, str>>,
    #[serde(borrow, default)]
    repos: Vec<Repo<'a>>,
}

/// A source of hooks within a pre-commit configuration.
///
/// The `repo` field is either a remote Git URL (e.g.
/// `https://github.com/pre-commit/pre-commit-hooks`), the special value
/// `"local"` for hooks defined in the same repository, or `"meta"` for
/// built-in pre-commit hooks. Remote repos require a `rev` pinning them to a
/// specific Git tag or commit; local and meta repos omit `rev`.
///
/// Each repo lists one or more [`Hook`]s to run from that source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Repo<'a> {
    #[serde(rename = "repo", borrow)]
    url: Cow<'a, str>,
    #[serde(borrow, default)]
    rev: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    hooks: Vec<Hook<'a>>,
}

/// An individual tool to run as part of a pre-commit check.
///
/// A hook is identified by its `id` (e.g. `"cargo-check"`), which must be
/// unique within its [`Repo`]. The `entry` field is the executable or command
/// to invoke, and `language` describes how pre-commit should set up the
/// environment (e.g. `"system"` to use the host PATH, `"python"` to create a
/// virtualenv).
///
/// File selection works in three layers applied in order:
/// 1. **`files` / `exclude`** — Python `re` patterns matched against each
///    staged file path; use [`Hook::files_regex`] / [`Hook::exclude_regex`] to
///    compile them.
/// 2. **`types` / `types_or` / `exclude_types`** — file-type filters based on
///    the [identify](https://github.com/chriskuehl/identify) library's tags
///    (e.g. `"python"`, `"rust"`, `"shell"`).
/// 3. **`always_run`** — when `true`, the hook runs even if no files pass the
///    above filters.
///
/// `pass_filenames` controls whether the matched file paths are appended to the
/// command; set to `false` for hooks that operate on the whole repo. `stages`
/// restricts the hook to specific Git lifecycle events, overriding
/// [`PreCommitConfig::default_stages`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "pre-commit schema explicitly defines these as distinct boolean flags"
)]
pub struct Hook<'a> {
    #[serde(borrow)]
    id: Cow<'a, str>,
    #[serde(borrow, default)]
    alias: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    name: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    entry: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    language: Option<Cow<'a, str>>,
    // Note: pre-commit uses Python regex for `files` and `exclude`, not file globs.
    // Compile with fancy_regex to support lookaheads/lookbehinds — see files_regex().
    #[serde(borrow, default)]
    files: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    exclude: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    types: Vec<Cow<'a, str>>,
    #[serde(borrow, default)]
    types_or: Vec<Cow<'a, str>>,
    #[serde(borrow, default)]
    exclude_types: Vec<Cow<'a, str>>,
    #[serde(borrow, default)]
    args: Vec<Cow<'a, str>>,
    #[serde(borrow, default)]
    stages: Vec<Cow<'a, str>>,
    #[serde(default = "default_true")]
    pass_filenames: bool,
    #[serde(default)]
    always_run: bool,
    #[serde(default)]
    fail_fast: bool,
    #[serde(default)]
    require_serial: bool,
    #[serde(default)]
    verbose: bool,
}

impl<'a> PreCommitConfig<'a> {
    /// Parses a YAML string into a [`PreCommitConfig`].
    ///
    /// # Errors
    ///
    /// Returns a [`serde_saphyr::Error`] if the string is malformed or invalid YAML.
    #[must_use = "returns the parsed configuration; original string is unchanged"]
    pub fn parse(yaml: &'a str) -> Result<Self, serde_saphyr::Error> {
        serde_saphyr::from_str(yaml)
    }

    /// Returns the global default stages.
    #[must_use = "discards the parsed default stages list"]
    pub fn default_stages(&self) -> &[Cow<'a, str>] {
        &self.default_stages
    }

    /// Returns the repositories containing hooks.
    #[must_use = "discards the parsed list of repositories"]
    pub fn repos(&self) -> &[Repo<'a>] {
        &self.repos
    }

    /// Returns an iterator over every hook across all repositories.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn hooks(&self) -> impl Iterator<Item = &Hook<'a>> {
        self.repos.iter().flatten()
    }
}

impl<'a> Repo<'a> {
    /// Returns the repository URL or path (e.g. `local`).
    #[must_use = "discards the repository URL or path"]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the repository as a local filesystem path, or `None` for remote URLs.
    ///
    /// Returns `None` when the URL contains `://` (e.g. `https://github.com/…`)
    /// because those strings are not filesystem paths.
    #[must_use = "discards the local-path classification; recomputing it duplicates the same string check"]
    pub fn local_path(&self) -> Option<&Path> {
        if self.url.contains("://") {
            None
        } else {
            Some(Path::new(self.url.as_ref()))
        }
    }

    /// Returns the repository revision, if any.
    #[must_use = "discards the repository's pinned revision, if any"]
    pub fn rev(&self) -> Option<&str> {
        self.rev.as_deref()
    }

    /// Returns the hooks defined in this repository.
    #[must_use = "discards the repo's parsed hook list"]
    pub fn hooks(&self) -> &[Hook<'a>] {
        &self.hooks
    }

    /// Returns an iterator over the hooks defined in this repository.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn iter(&self) -> Iter<'_, Hook<'a>> {
        self.hooks.iter()
    }
}

impl<'b, 'a> IntoIterator for &'b Repo<'a> {
    type Item = &'b Hook<'a>;
    type IntoIter = Iter<'b, Hook<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> Hook<'a> {
    /// Returns the id of the hook.
    #[must_use = "discards the hook's id"]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the alias of the hook, if any.
    #[must_use = "discards the hook's alias, if any"]
    pub fn alias(&self) -> Option<&str> {
        self.alias.as_deref()
    }

    /// Returns the name of the hook, if any.
    #[must_use = "discards the hook's display name, if any"]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the entry (executable) of the hook, if any.
    #[must_use = "discards the hook's entry command, if any"]
    pub fn entry(&self) -> Option<&str> {
        self.entry.as_deref()
    }

    /// Returns the language of the hook, if any.
    #[must_use = "discards the hook's language, if any"]
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Returns the raw file inclusion pattern string, if any.
    ///
    /// Pre-commit uses Python `re` syntax; use [`Hook::files_regex`] to obtain
    /// a compiled [`fancy_regex::Regex`] that handles lookaheads and lookbehinds.
    #[must_use = "discards the raw files-inclusion pattern"]
    pub fn files_pattern(&self) -> Option<&str> {
        self.files.as_deref()
    }

    /// Compiles and returns the file inclusion regex, if any.
    ///
    /// Uses [`fancy_regex`] to support the Python `re` syntax pre-commit uses
    /// for this field (lookaheads, lookbehinds, etc.).
    ///
    /// # Errors
    ///
    /// Returns a [`fancy_regex::Error`] if the pattern is syntactically invalid.
    #[must_use = "discards the compiled regex (or compile error); recompiling repeats the work"]
    pub fn files_regex(&self) -> Option<Result<fancy_regex::Regex, fancy_regex::Error>> {
        self.files.as_deref().map(fancy_regex::Regex::new)
    }

    /// Returns the raw file exclusion pattern string, if any.
    ///
    /// See [`Hook::files_pattern`] for notes on Python `re` syntax.
    #[must_use = "discards the raw files-exclusion pattern"]
    pub fn exclude_pattern(&self) -> Option<&str> {
        self.exclude.as_deref()
    }

    /// Compiles and returns the file exclusion regex, if any.
    ///
    /// See [`Hook::files_regex`] for notes on Python `re` syntax and engine choice.
    ///
    /// # Errors
    ///
    /// Returns a [`fancy_regex::Error`] if the pattern is syntactically invalid.
    #[must_use = "discards the compiled exclude regex (or compile error); recompiling repeats the work"]
    pub fn exclude_regex(&self) -> Option<Result<fancy_regex::Regex, fancy_regex::Error>> {
        self.exclude.as_deref().map(fancy_regex::Regex::new)
    }

    /// Returns the list of file types to run on.
    #[must_use = "discards the hook's required file types"]
    pub fn types(&self) -> &[Cow<'a, str>] {
        &self.types
    }

    /// Returns the list of file types (OR) to run on.
    #[must_use = "discards the hook's alternative file types"]
    pub fn types_or(&self) -> &[Cow<'a, str>] {
        &self.types_or
    }

    /// Returns the list of file types to exclude.
    #[must_use = "discards the hook's excluded file types"]
    pub fn exclude_types(&self) -> &[Cow<'a, str>] {
        &self.exclude_types
    }

    /// Returns additional arguments to pass to the hook.
    #[must_use = "discards the hook's extra arguments"]
    pub fn args(&self) -> &[Cow<'a, str>] {
        &self.args
    }

    /// Returns the specific stages to run this hook in.
    #[must_use = "discards the hook's git lifecycle stages"]
    pub fn stages(&self) -> &[Cow<'a, str>] {
        &self.stages
    }

    /// Returns whether to pass filenames to the hook. Defaults to `true`.
    #[must_use = "discards whether filenames are passed to the hook"]
    pub const fn pass_filenames(&self) -> bool {
        self.pass_filenames
    }

    /// Returns whether to run the hook always, even if no files match. Defaults to `false`.
    #[must_use = "discards whether the hook always runs regardless of file matches"]
    pub const fn always_run(&self) -> bool {
        self.always_run
    }

    /// Returns whether the run fails immediately on the first error. Defaults to `false`.
    #[must_use = "discards whether the hook run fails fast"]
    pub const fn fail_fast(&self) -> bool {
        self.fail_fast
    }

    /// Returns whether to run sequentially instead of in parallel. Defaults to `false`.
    #[must_use = "discards whether the hook requires serial execution"]
    pub const fn require_serial(&self) -> bool {
        self.require_serial
    }

    /// Returns whether to force verbose output. Defaults to `false`.
    #[must_use = "discards whether the hook forces verbose output"]
    pub const fn verbose(&self) -> bool {
        self.verbose
    }
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    use std::fs;
    use std::io;
    use std::path::PathBuf;

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error(transparent)]
        Yaml(#[from] serde_saphyr::Error),
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error("Test failure: {0}")]
        Failure(String),
    }

    mod pre_commit_config {
        use super::*;

        mod parse {
            use super::*;

            #[rstest]
            fn parses_local_repo_fixture() -> Result<(), TestError> {
                let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/pre_commit_local_repo/.pre-commit-config.yaml");

                let content = fs::read_to_string(&path)?;
                let config = PreCommitConfig::parse(&content)?;

                assert!(!config.repos().is_empty(), "should have at least one repo");

                let local_repo = config
                    .repos()
                    .iter()
                    .find(|r| r.url() == "local")
                    .ok_or_else(|| TestError::Failure("should have a 'local' repo".into()))?;
                assert_eq!(local_repo.local_path(), Some(Path::new("local")));

                assert!(
                    !local_repo.hooks().is_empty(),
                    "local repo should have hooks"
                );

                let cargo_check = local_repo
                    .hooks()
                    .iter()
                    .find(|h| h.id() == "cargo-check")
                    .ok_or_else(|| TestError::Failure("should have cargo-check hook".into()))?;

                assert_eq!(cargo_check.language(), Some("system"));
                assert!(!cargo_check.always_run(), "cargo check does not always run");

                Ok(())
            }
        }
    }
}
