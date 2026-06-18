//! Errors produced while loading and resolving [`super::Config`].

use thiserror::Error;
use toml::de::Error as TomlDeError;
use toml::ser::Error as TomlSerError;

use std::{borrow::Cow, io};

/// Errors that can occur while loading or resolving configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// A config file exists but could not be read. Returned by
    /// `RawConfig::load_layer`; callers loading a *user* layer use
    /// `Config::merge_layer_at` instead, which logs and recovers from
    /// this rather than propagating it out of [`super::Config::load`].
    #[error("failed to read config file {path}")]
    Read {
        /// The underlying I/O error.
        #[source]
        inner: io::Error,
        /// The file that failed to read.
        path: Cow<'static, str>,
    },
    /// A config file exists but could not be parsed as TOML. Returned by
    /// `RawConfig::load_layer`; see [`ConfigError::Read`] for why this
    /// doesn't propagate out of [`super::Config::load`].
    #[error("failed to parse config file {path} as TOML")]
    Parse {
        /// The underlying TOML parse error.
        #[source]
        inner: TomlDeError,
        /// The file that failed to parse.
        path: Cow<'static, str>,
    },
    /// The embedded base config (built-in stage defaults) failed to parse.
    /// Unlike [`ConfigError::Parse`], this isn't user data — it would
    /// indicate a bug in the embedded `base.toml`.
    #[error("embedded base config is invalid and could not be parsed")]
    EmbeddedConfig(#[source] TomlDeError),
    /// The resolved configuration could not be serialized back into TOML
    /// for `inceptool config`. Would indicate a bug in `RawConfig`'s
    /// `Serialize` derive output, not a user error.
    #[error("failed to serialize the resolved configuration as TOML")]
    Serialize(#[source] TomlSerError),
}
