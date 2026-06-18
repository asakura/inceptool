//! Command-line argument parsing for `inceptool <command> [args]`, dispatched
//! git-style on the first token (`claude <hook>`, `gemini <hook>`, `config`).

use inceptool_driver_claude::ClaudeDriver;
use inceptool_driver_gemini::GeminiDriver;
use inceptool_protocol::{Driver as _, HookKind};

use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic as _, Result};

/// `inceptool`'s top-level subcommands.
///
/// - `Claude`/`Gemini` carry `hook`, the raw hook event name configured for
///   this command in the agent's hook settings (e.g. `"PreToolUse"` for
///   Claude, `"BeforeTool"` for Gemini). It is passed to
///   [`Driver::hook_kind`](inceptool_protocol::Driver::hook_kind) to
///   determine which stage pipeline bucket runs — dispatch is driven by this
///   CLI argument, not by inspecting the JSON payload.
/// - `Config` prints the fully resolved configuration (built-in defaults
///   merged with any user overrides) as TOML — the `git config --list`
///   equivalent for `inceptool`'s merged settings.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Process a hook event using Claude Code's wire format.
    Claude {
        /// The raw hook event name, in Claude Code's vocabulary.
        hook: String,
    },
    /// Process a hook event using Gemini CLI's wire format.
    Gemini {
        /// The raw hook event name, in Gemini CLI's vocabulary.
        hook: String,
    },
    /// Print the fully resolved configuration as TOML.
    Config,
}

/// What `main` should do with a parsed [`Cli`], resolved eagerly by
/// [`Cli::parse_and_validate`] so an invalid `hook` name for `claude`/
/// `gemini` fails fast, before stdin is even read.
///
/// The driver to dispatch with is the variant tag itself, rather than a
/// separate field alongside `hook_kind` — that would let a caller pair, say,
/// `Gemini`'s driver with a `hook_kind` resolved against Claude's
/// vocabulary, a combination [`Cli::parse_and_validate`] can never actually
/// produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Run the hook pipeline using Claude Code's wire format, dispatching
    /// via `hook_kind`.
    Claude {
        /// The stage pipeline bucket this hook event dispatches to.
        hook_kind: HookKind,
    },
    /// Run the hook pipeline using Gemini CLI's wire format, dispatching
    /// via `hook_kind`.
    Gemini {
        /// The stage pipeline bucket this hook event dispatches to.
        hook_kind: HookKind,
    },
    /// Print the resolved configuration as TOML.
    ShowConfig,
}

/// A parsed `inceptool <command>` invocation.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(version, about = "An extensible LLM agent hook architecture", long_about = None)]
pub struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,

    /// Increase verbosity of the `<cache_dir>/inceptool.log` file log
    /// (`-v` warn, `-vv` info, `-vvv` debug, `-vvvv` trace).
    ///
    /// Error-level logging to stderr is unaffected by this flag and cannot
    /// be disabled, as the host CLI depends on it for error reporting.
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
}

impl Cli {
    /// Parses argv and resolves it into the verbosity level and an
    /// [`Action`] to run.
    ///
    /// Returns `verbose` alongside `Action` rather than the whole [`Cli`] —
    /// once `Action` is resolved, `command` (the only other field) has
    /// nothing left to contribute, and keeping it around would let a caller
    /// hold a `Cli` and `Action` that name different commands.
    ///
    /// # Errors
    ///
    /// Returns an error if argument parsing fails, or if a `claude`/`gemini`
    /// invocation's `hook` name isn't recognized by that driver.
    pub fn parse_and_validate() -> Result<(u8, Action)> {
        let cli = <Self as clap::Parser>::parse();

        let action = match cli.command {
            Command::Claude { hook } => Action::Claude {
                hook_kind: ClaudeDriver.hook_kind(&hook).into_diagnostic()?,
            },
            Command::Gemini { hook } => Action::Gemini {
                hook_kind: GeminiDriver.hook_kind(&hook).into_diagnostic()?,
            },
            Command::Config => Action::ShowConfig,
        };

        Ok((cli.verbose, action))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use clap::error;
    use rstest::rstest;

    use core::assert_matches;

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error(transparent)]
        Clap(#[from] clap::Error),
    }

    #[rstest]
    #[case::claude("claude", "PreToolUse", Command::Claude { hook: "PreToolUse".to_owned() })]
    #[case::gemini("gemini", "BeforeTool", Command::Gemini { hook: "BeforeTool".to_owned() })]
    fn parse_valid(
        #[case] command_arg: &str,
        #[case] hook_arg: &str,
        #[case] expected: Command,
    ) -> Result<(), TestError> {
        let cli = Cli::try_parse_from(["inceptool", command_arg, hook_arg])?;
        assert_eq!(cli.command, expected);
        Ok(())
    }

    #[rstest]
    fn parse_config() -> Result<(), TestError> {
        let cli = Cli::try_parse_from(["inceptool", "config"])?;
        assert_matches!(cli.command, Command::Config);
        Ok(())
    }

    #[rstest]
    fn parse_missing_subcommand() {
        assert_matches!(
            Cli::try_parse_from(["inceptool"]),
            Err(err) if err.kind() == error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[rstest]
    fn parse_unknown_subcommand() {
        assert_matches!(
            Cli::try_parse_from(["inceptool", "nushell", "PreToolUse"]),
            Err(err) if err.kind() == error::ErrorKind::InvalidSubcommand
        );
    }

    #[rstest]
    fn parse_missing_hook() {
        assert_matches!(
            Cli::try_parse_from(["inceptool", "claude"]),
            Err(err) if err.kind() == error::ErrorKind::MissingRequiredArgument
        );
    }
}
