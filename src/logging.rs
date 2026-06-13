//! Initializes the `tracing` subscriber for the CLI.

use crate::config::project_dirs;

use miette::{IntoDiagnostic as _, Result};
use std::fs;
use std::io;
use tracing_subscriber::fmt;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::prelude::*;

/// Initializes the `tracing` subscriber, writing to standard error and a
/// cached log file.
///
/// Errors at or above [`LevelFilter::ERROR`] are always written to stderr.
/// A second layer logs to `<cache_dir>/inceptool.log` (filtered by
/// `RUST_LOG`/[`EnvFilter::from_default_env`]) if [`project_dirs`] resolves
/// to a valid location; otherwise file logging is skipped entirely.
pub fn setup_logging() -> Result<()> {
    let stderr_layer = fmt::layer()
        .with_writer(io::stderr)
        .with_filter(LevelFilter::ERROR);

    let file_layer = project_dirs()
        .map(|dirs| -> Result<_> {
            let cache_dir = dirs.cache_dir();

            fs::create_dir_all(cache_dir).into_diagnostic()?;

            let log_path = cache_dir.join("inceptool.log");
            let log_file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .into_diagnostic()?;

            Ok(fmt::layer()
                .with_writer(log_file)
                .with_filter(EnvFilter::from_default_env()))
        })
        .transpose()?;

    tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer)
        .init();

    Ok(())
}
