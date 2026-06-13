//! User configuration: which stages are enabled, loaded from
//! `inceptool.toml`.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

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

/// User-configurable settings, merged from the user config dir and the
/// current working directory.
#[derive(Debug, Deserialize, Default)]
pub struct Config {
    /// Per-stage enable/disable overrides, keyed by stage name (e.g.
    /// `"rtk"`).
    #[serde(default)]
    pub hooks: HashMap<String, HookConfig>,
}

/// Per-stage configuration.
#[derive(Debug, Deserialize, Default)]
pub struct HookConfig {
    /// Whether this stage should be registered. Defaults to `true`.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

const fn default_true() -> bool {
    true
}

impl Config {
    /// Loads configuration, merging the user config dir (`$XDG_CONFIG_HOME`)
    /// with `inceptool.toml` in the current working directory (which takes
    /// precedence). Missing or invalid files are silently ignored, yielding
    /// [`Config::default`].
    pub fn load() -> Self {
        let mut config = Self::default();

        // Load from XDG_CONFIG_HOME
        if let Some(dirs) = project_dirs() {
            let user_config = dirs.config_dir().join("inceptool.toml");

            if let Ok(c) = Self::from_file(&user_config) {
                config.merge(c);
            }
        }

        // Load from CWD
        if let Ok(cwd) = std::env::current_dir() {
            let local_config = cwd.join("inceptool.toml");

            if let Ok(c) = Self::from_file(&local_config) {
                config.merge(c);
            }
        }

        config
    }

    fn from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;

        Ok(config)
    }

    fn merge(&mut self, other: Self) {
        for (k, v) in other.hooks {
            self.hooks.insert(k, v);
        }
    }

    /// Whether the named stage (e.g. `"guardrails"`) should be registered.
    /// Defaults to `true` if not explicitly configured.
    pub fn is_hook_enabled(&self, name: &str) -> bool {
        self.hooks.get(name).map(|h| h.enabled).unwrap_or(true)
    }
}
