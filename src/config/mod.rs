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

pub use error::ConfigError;
use raw_config::RawConfig;

use inceptool_stages::read_write_guard::RuleSet;

use std::{collections::BTreeMap, env};

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

/// Fully resolved configuration: built-in defaults merged with user
/// overrides, ready to hand to stage constructors.
#[derive(Debug)]
pub struct Config {
    /// Resolved per-stage enable/disable flags, keyed by stage name (e.g.
    /// `"rtk"`). Looked up via [`Config::is_hook_enabled`].
    hooks: BTreeMap<String, bool>,
    /// Resolved guarded-file rules for
    /// [`ReadWriteGuardStage`](inceptool_stages::ReadWriteGuardStage): the
    /// embedded built-ins merged with any user-supplied overrides. Looked
    /// up via [`Config::read_write_guard_rules`].
    read_write_guard_rules: RuleSet,
}

impl Config {
    /// Loads configuration: starts from the embedded base config (built-in
    /// stage defaults), then layers the user config dir
    /// (`$XDG_CONFIG_HOME`) and `inceptool.toml` in the current working
    /// directory on top, in that order (later layers win).
    ///
    /// Missing user files are silently skipped. A file that exists but
    /// fails to parse is an error.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::EmbeddedConfig`] if the embedded base config
    /// fails to parse, or [`ConfigError::Read`]/[`ConfigError::Parse`] if a
    /// user config file exists but cannot be read or parsed.
    pub fn load() -> Result<Self, ConfigError> {
        let mut raw = RawConfig::parse_base_config()?;

        if let Some(dirs) = project_dirs()
            && let Some(layer) = RawConfig::load_layer(&dirs.config_dir().join("inceptool.toml"))?
        {
            raw.merge(layer);
        }

        if let Ok(cwd) = env::current_dir()
            && let Some(layer) = RawConfig::load_layer(&cwd.join("inceptool.toml"))?
        {
            raw.merge(layer);
        }

        Self::try_from(raw)
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
}
