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
//! Parses a git-style `<command> [args]` CLI invocation. The `claude
//! <hook>`/`gemini <hook>` commands build the stage
//! [`inceptool_engine::Registry`], normalize the incoming hook payload into
//! a [`Conn`](inceptool_protocol::session::Conn) via the selected
//! [`Driver`], run it through the pipeline, and write the result to stdout.
//! The `config` command instead prints the fully resolved configuration as
//! TOML and exits, without touching stdin.
//!
//! See the workspace `README.md` for the architecture overview, stage
//! pipeline, and `inceptool.toml` configuration format.
//!
//! The binary itself is organized into:
//!
//! - **[`cli`]**: Parses `<command> [args]` into a [`cli::Cli`] and a
//!   resolved [`cli::Action`].
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

use cli::{Action, Cli};
use config::Config;

use inceptool_driver_claude::ClaudeDriver;
use inceptool_driver_gemini::GeminiDriver;
use inceptool_engine::Registry;
use inceptool_protocol::{Driver, HookKind};

use miette::{IntoDiagnostic as _, Result};

use std::io::{self, Read as _, Write as _};

/// Executes the payload processing pipeline for the selected driver.
#[tracing::instrument(skip_all, fields(hook = ?kind), err)]
fn run_with_driver<D>(driver: &D, raw_json: &str, kind: HookKind, registry: &Registry) -> Result<()>
where
    D: Driver,
{
    let mut conn = inceptool_protocol::from_wire(driver, raw_json).into_diagnostic()?;
    let final_output = registry.run_pipeline(kind, &mut conn).into_diagnostic()?;

    output::handle_output(final_output, driver)
}

/// Reads the stdin JSON payload, builds the registry from `inceptool.toml`,
/// and dispatches it through `run_with_driver` for `driver`/`hook_kind`.
/// Shared by the `claude` and `gemini` subcommands.
fn process_hook<D>(driver_name: &'static str, driver: &D, hook_kind: HookKind) -> Result<()>
where
    D: Driver,
{
    tracing::info!(driver = driver_name, ?hook_kind, "processing hook");

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

    run_with_driver(driver, trimmed_json, hook_kind, &registry)
}

/// Prints the fully resolved configuration (built-in defaults merged with
/// user overrides) as TOML, for `inceptool config`.
fn show_config() -> Result<()> {
    let config = Config::load().into_diagnostic()?;
    let toml_text = config.to_toml().into_diagnostic()?;

    io::stdout()
        .write_all(toml_text.as_bytes())
        .into_diagnostic()?;
    io::stdout().flush().into_diagnostic()
}

fn main() -> Result<()> {
    let (verbose, action) = Cli::parse_and_validate()?;

    logging::setup_logging(verbose)?;

    match action {
        Action::Claude { hook_kind } => process_hook("claude", &ClaudeDriver, hook_kind),
        Action::Gemini { hook_kind } => process_hook("gemini", &GeminiDriver, hook_kind),
        Action::ShowConfig => show_config(),
    }
}
