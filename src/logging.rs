//! Initializes the `tracing` subscriber for the CLI.

use crate::config::project_dirs;

use miette::{IntoDiagnostic as _, Result};
use std::fs;
use std::io;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

/// Initializes the `tracing` subscriber, writing to standard error and a
/// cached log file.
///
/// Errors at or above [`LevelFilter::ERROR`] are always written to stderr,
/// independent of `verbosity` — the host CLI depends on this for error
/// reporting and it cannot be raised or disabled.
///
/// `verbosity` is the number of times the CLI's `-v`/`--verbose` flag was
/// repeated, and sets the default level (see [`verbosity_to_level_filter`])
/// for the second layer, which logs to `<cache_dir>/inceptool.log` if
/// [`project_dirs`] resolves to a valid location (otherwise file logging is
/// skipped entirely). `RUST_LOG` takes precedence over `verbosity` for this
/// layer when set.
pub fn setup_logging(verbosity: u8) -> Result<()> {
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

            let env_filter = EnvFilter::builder()
                .with_default_directive(verbosity_to_level_filter(verbosity).into())
                .from_env_lossy();

            Ok(fmt::layer().with_writer(log_file).with_filter(env_filter))
        })
        .transpose()?;

    tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer)
        .init();

    Ok(())
}

/// Maps a `-v`/`--verbose` repeat count to a [`LevelFilter`] used as the
/// default directive for the file log layer.
const fn verbosity_to_level_filter(verbosity: u8) -> LevelFilter {
    match verbosity {
        0 => LevelFilter::ERROR,
        1 => LevelFilter::WARN,
        2 => LevelFilter::INFO,
        3 => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    }
}
