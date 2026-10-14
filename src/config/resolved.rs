use super::{ConfigError, raw_config::RawConfig};

use inceptool_stages::read_write_guard::RuleSet;

use std::{
    borrow::Cow,
    collections::BTreeMap,
    env, mem,
    path::{Path, PathBuf},
};

/// Returns the platform-specific project directories for `inceptool` (config
/// dir, cache dir, etc.), or `None` if they cannot be determined (e.g. no
/// valid `$HOME`).
///
/// Shared by [`Config::load`] and
/// [`crate::logging::setup_logging`] so both resolve the same XDG-style
/// locations.
pub fn project_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "inceptool")
}

/// Discovers the git repository enclosing `start` and returns its working
/// directory, or `None` if `start` isn't inside a git repository (or that
/// repository is bare and has no working directory).
fn discover_git_root(start: &Path) -> Option<PathBuf> {
    gix::discover(start).ok()?.workdir().map(Path::to_path_buf)
}

/// Fully resolved configuration: built-in defaults merged with user
/// overrides, ready to hand to stage constructors.
#[derive(Debug)]
#[expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "shared within the config module hierarchy"
)]
pub struct Config {
    /// Resolved per-stage enable/disable flags, keyed by stage name (e.g.
    /// `"rtk"`). Looked up via [`Config::is_hook_enabled`].
    pub(super) hooks: BTreeMap<Cow<'static, str>, bool>,
    /// Resolved guarded-file rules for
    /// [`ReadWriteGuardStage`](inceptool_stages::ReadWriteGuardStage): the
    /// embedded built-ins merged with any user-supplied overrides. Looked
    /// up via [`Config::read_write_guard_rules`].
    pub(super) read_write_guard_rules: RuleSet,
    /// The enclosing git repository's working directory, discovered once by
    /// [`Config::load`] via [`discover_git_root`]. `None` for every
    /// intermediate per-layer `Config` built by [`Config::try_from`] — only
    /// `load` itself has the cwd needed to discover it. Looked up via
    /// [`Config::repo_root`].
    pub(super) repo_root: Option<PathBuf>,
}

impl Config {
    /// Loads configuration: starts from the embedded base config (built-in
    /// stage defaults), then layers the user config dir
    /// (`$XDG_CONFIG_HOME`), the enclosing git repository's root, and
    /// `inceptool.toml` in the current working directory on top, in that
    /// order (later layers win).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::EmbeddedConfig`] if the embedded base config
    /// fails to parse — this is the only failure mode, since it would
    /// indicate a bug in the shipped binary rather than a user error.
    pub fn load() -> Result<Self, ConfigError> {
        let mut config = Self::try_from(RawConfig::parse_base_config()?)?;

        if let Some(dirs) = project_dirs() {
            config.merge_layer_at(&dirs.config_dir().join("inceptool.toml"));
        }

        let cwd = env::current_dir().ok();
        let git_root = cwd.as_deref().and_then(discover_git_root);
        config.repo_root.clone_from(&git_root);

        if let Some(root) = &git_root {
            config.merge_layer_at(&root.join("inceptool.toml"));
        }

        if let Some(cwd) = cwd.filter(|cwd| git_root.as_deref() != Some(cwd.as_path())) {
            config.merge_layer_at(&cwd.join("inceptool.toml"));
        }

        Ok(config)
    }

    /// Loads and merges the `inceptool.toml` layer at `path` into `self`, if
    /// present. A missing file is silently skipped. A file that exists but
    /// fails to load (read/parse) or resolve (the [`TryFrom`] step) is
    /// logged and skipped too, rather than propagated — this binary runs as
    /// a hook on every tool call, so a malformed *user* config degrades to
    /// "no override from this layer" instead of failing every invocation.
    pub(super) fn merge_layer_at(&mut self, path: &Path) {
        let layer = match RawConfig::load_layer(path) {
            Ok(Some(raw)) => raw,
            Ok(None) => return,
            Err(error) => {
                tracing::error!(
                    path = %path.display(),
                    %error,
                    "failed to load inceptool.toml layer, ignoring it"
                );
                return;
            }
        };

        match Self::try_from(layer) {
            Ok(layer) => self.merge(layer, path),
            Err(error) => tracing::error!(
                path = %path.display(),
                %error,
                "failed to resolve inceptool.toml layer, ignoring it"
            ),
        }
    }

    /// Merges `other` (a more specific, later-loaded layer) on top of
    /// `self`: per-hook overrides win outright; read-write-guard rules from
    /// both layers accumulate, with overlapping rules reconciled by
    /// [`Rule::subsumes`](inceptool_stages::read_write_guard::Rule::subsumes) via [`RuleSet`]'s own fold — a later, broader rule
    /// drops an earlier, more specific one it now fully covers, and a
    /// same-pattern override is just the case where a rule subsumes its own
    /// exact pattern. Either way, layering order is preserved end to end,
    /// same as hooks. `path` identifies `other`'s source, for the override
    /// trace logged at debug level — `Config` itself stores only the merged
    /// result, not which layer contributed it (see
    /// [`From<Config> for RawConfig`]'s doc for why per-entry provenance
    /// isn't tracked further than this).
    pub(super) fn merge(&mut self, other: Self, path: &Path) {
        for (name, enabled) in other.hooks {
            if let Some(&previous) = self.hooks.get(&name)
                && previous != enabled
            {
                tracing::debug!(
                    path = %path.display(),
                    hook = %name,
                    previous,
                    now = enabled,
                    "hook override"
                );
            }

            self.hooks.insert(name, enabled);
        }

        for incoming in other.read_write_guard_rules.iter() {
            for existing in self.read_write_guard_rules.iter() {
                if !incoming.subsumes(existing) {
                    continue;
                }

                if incoming.filename() == existing.filename() {
                    if incoming.access() != existing.access() {
                        tracing::debug!(
                            path = %path.display(),
                            filename = %incoming.filename(),
                            previous_access = ?existing.access(),
                            new_access = ?incoming.access(),
                            "read-write-guard rule override"
                        );
                    }
                } else {
                    tracing::debug!(
                        path = %path.display(),
                        dropped_filename = %existing.filename(),
                        subsuming_pattern = %incoming.filename(),
                        previous_access = ?existing.access(),
                        new_access = ?incoming.access(),
                        "read-write-guard rule subsumed and dropped"
                    );
                }
            }
        }

        self.read_write_guard_rules = mem::take(&mut self.read_write_guard_rules)
            .into_iter()
            .chain(other.read_write_guard_rules)
            .collect();
    }

    /// Whether the named stage (e.g. `"guardrails"`) should be registered.
    /// Defaults to `true` if not explicitly configured.
    #[must_use = "returns whether the hook is enabled; has no side effects"]
    pub fn is_hook_enabled(&self, name: &str) -> bool {
        self.hooks.get(name).copied().unwrap_or(true)
    }

    /// The resolved guarded-file rules for
    /// [`ReadWriteGuardStage`](inceptool_stages::ReadWriteGuardStage).
    #[must_use = "returns the rule set; has no side effects"]
    pub const fn read_write_guard_rules(&self) -> &RuleSet {
        &self.read_write_guard_rules
    }

    /// The enclosing git repository's working directory, if [`Config::load`]
    /// found one — for
    /// [`ReadWriteGuardStage::new`](inceptool_stages::ReadWriteGuardStage::new)
    /// to anchor full-path glob rules to the repo root.
    #[must_use = "returns the discovered repo root; has no side effects"]
    pub fn repo_root(&self) -> Option<&Path> {
        self.repo_root.as_deref()
    }

    /// Renders the fully resolved configuration (built-in defaults merged
    /// with any user overrides) back into TOML text — the `git config
    /// --list` equivalent for `inceptool`'s merged hook/read-write-guard
    /// settings, printed by `inceptool config`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Serialize`] if the conversion back to TOML
    /// fails — this would indicate a bug rather than a user error, since
    /// `self` is already a validly resolved `Config`. Consumes `self`
    /// rather than borrowing it, so the conversion to [`RawConfig`] can
    /// move each hook name and rule instead of cloning them.
    #[must_use = "returns the rendered TOML text; has no side effects"]
    #[expect(
        clippy::wrong_self_convention,
        reason = "every caller is done with the Config afterward; consuming self lets \
                  From<Config> for RawConfig move each hook name and rule instead of \
                  cloning them"
    )]
    pub fn to_toml(self) -> Result<String, ConfigError> {
        RawConfig::from(self).to_toml()
    }
}
