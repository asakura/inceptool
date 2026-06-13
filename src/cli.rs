//! Command-line argument parsing for `inceptool <driver> <hook>`.

use clap::{Parser, ValueEnum};
use inceptool_driver_claude::ClaudeDriver;
use inceptool_driver_gemini::GeminiDriver;
use inceptool_protocol::{Driver, HookKind};
use miette::{IntoDiagnostic, Result};

/// Which agent's wire format to parse stdin as and format stdout as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DriverKind {
    /// Claude Code's hook wire format.
    Claude,
    /// Gemini CLI's hook wire format.
    Gemini,
}

/// A parsed `inceptool <driver> <hook>` invocation.
///
/// - `driver` selects the wire format (Claude or Gemini) used to interpret
///   stdin and format stdout.
/// - `hook` is the raw hook event name configured for this command in the
///   agent's hook settings (e.g. `"PreToolUse"` for Claude, `"BeforeTool"`
///   for Gemini). It is passed to
///   [`Driver::hook_kind`](inceptool_protocol::Driver::hook_kind) to
///   determine which stage pipeline bucket runs — dispatch is driven by this
///   CLI argument, not by inspecting the JSON payload.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(version, about = "An extensible LLM agent hook architecture", long_about = None)]
pub struct Cli {
    /// The agent driver to use.
    pub driver: DriverKind,

    /// The raw hook event name, in the selected driver's vocabulary.
    pub hook: String,
}

impl Cli {
    pub fn parse_and_validate() -> Result<(Self, HookKind)> {
        let cli = <Self as clap::Parser>::parse();

        let hook_kind = match cli.driver {
            DriverKind::Claude => ClaudeDriver.hook_kind(&cli.hook).into_diagnostic()?,
            DriverKind::Gemini => GeminiDriver.hook_kind(&cli.hook).into_diagnostic()?,
        };

        Ok((cli, hook_kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use clap::Parser;
    use rstest::rstest;

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error(transparent)]
        Clap(#[from] clap::Error),
    }

    #[rstest]
    #[case::claude("claude", "PreToolUse", DriverKind::Claude, "PreToolUse")]
    #[case::gemini("gemini", "BeforeTool", DriverKind::Gemini, "BeforeTool")]
    fn test_parse_valid(
        #[case] driver_arg: &str,
        #[case] hook_arg: &str,
        #[case] expected_driver: DriverKind,
        #[case] expected_hook: &str,
    ) -> Result<(), TestError> {
        let cli = Cli::try_parse_from(["inceptool", driver_arg, hook_arg])?;

        assert_eq!(cli.driver, expected_driver);
        assert_eq!(cli.hook, expected_hook);

        Ok(())
    }

    #[rstest]
    fn test_parse_missing_driver() {
        assert!(Cli::try_parse_from(["inceptool"]).is_err());
    }

    #[rstest]
    fn test_parse_unknown_driver() {
        assert!(Cli::try_parse_from(["inceptool", "nushell", "PreToolUse"]).is_err());
    }

    #[rstest]
    fn test_parse_missing_hook() {
        assert!(Cli::try_parse_from(["inceptool", "claude"]).is_err());
    }
}
