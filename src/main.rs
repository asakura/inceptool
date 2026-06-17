#![cfg_attr(
    test,
    expect(
        clippy::panic_in_result_fn,
        reason = "rstest cases return Result for `?`-based setup but use assert_eq!/assert_matches! \
                  for assertions per project convention"
    )
)]

//! Binary entrypoint for `inceptool`.
//!
//! Parses the `<driver> <hook>` CLI invocation, builds the stage
//! [`inceptool_engine::Registry`], normalizes the incoming hook payload into
//! a [`Conn`](inceptool_protocol::session::Conn) via the selected
//! [`Driver`], runs it through the pipeline, and writes the result to stdout.
//!
//! See the workspace `README.md` for the architecture overview, stage
//! pipeline, and `inceptool.toml` configuration format.
//!
//! The binary itself is organized into:
//!
//! - **[`cli`]**: Parses `<driver> <hook>` into a [`cli::Cli`].
//! - **[`config`]**: Loads `inceptool.toml` user configuration.
//! - **`error`**: CLI-specific error types.
//! - **[`logging`]**: Initializes the `tracing` subscriber.
//! - **[`registry`]**: Builds the stage [`inceptool_engine::Registry`] from configuration.
//! - **[`output`]**: Formats the engine's output and writes it to stdout.

mod cli;
mod config;
mod logging;
mod output;
mod registry;

use cli::{Cli, DriverKind};
use config::Config;

use inceptool_driver_claude::ClaudeDriver;
use inceptool_driver_gemini::GeminiDriver;
use inceptool_engine::Registry;
use inceptool_protocol::Driver;

use miette::{IntoDiagnostic as _, Result};

use std::io::{self, Read as _};

/// Executes the payload processing pipeline for the selected driver.
#[tracing::instrument(skip_all, fields(hook = ?kind), err)]
fn run_with_driver<D>(
    driver: &D,
    raw_json: &str,
    kind: inceptool_protocol::HookKind,
    registry: &Registry,
) -> Result<()>
where
    D: Driver,
{
    let mut conn = inceptool_protocol::from_wire(driver, raw_json).into_diagnostic()?;
    let final_output = registry.run_pipeline(kind, &mut conn).into_diagnostic()?;

    output::handle_output(final_output, driver)
}

fn main() -> Result<()> {
    let (cli, hook_kind) = Cli::parse_and_validate()?;

    logging::setup_logging(cli.verbose)?;

    tracing::info!(driver = ?cli.driver, hook = ?hook_kind, "processing hook");

    let mut raw_json = String::with_capacity(4 * 1024);

    io::stdin()
        .read_to_string(&mut raw_json)
        .into_diagnostic()?;

    let trimmed_json = raw_json.trim();

    if trimmed_json.is_empty() {
        return Ok(());
    }

    let config = Config::load().into_diagnostic()?;
    let registry = registry::build_registry(&config);

    match cli.driver {
        DriverKind::Claude => run_with_driver(&ClaudeDriver, trimmed_json, hook_kind, &registry),
        DriverKind::Gemini => run_with_driver(&GeminiDriver, trimmed_json, hook_kind, &registry),
    }
}
