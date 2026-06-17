//! Errors produced while loading and resolving [`super::Config`].

use thiserror::Error;
use toml::de::Error as TomlError;

use std::{borrow::Cow, io};

/// Errors that can occur while loading or resolving configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// A config file exists but could not be read.
    #[error("failed to read config file {path}")]
    Read {
        /// The underlying I/O error.
        #[source]
        inner: io::Error,
        /// The file that failed to read.
        path: Cow<'static, str>,
    },
    /// A config file exists but could not be parsed as TOML.
    #[error("failed to parse config file {path} as TOML")]
    Parse {
        /// The underlying TOML parse error.
        #[source]
        inner: TomlError,
        /// The file that failed to parse.
        path: Cow<'static, str>,
    },
    /// The embedded base config (built-in stage defaults) failed to parse.
    /// Unlike [`ConfigError::Parse`], this isn't user data — it would
    /// indicate a bug in the embedded `base.toml`.
    #[error("embedded base config is invalid and could not be parsed")]
    EmbeddedConfig(#[source] TomlError),
}
