//! # RTK Architecture
//!
//! The RTK stage is a thin pass-through wrapper around the external `rtk` binary,
//! which rewrites shell commands to their `rtk`-flavored equivalents.
//!
//! ## Core Design
//!
//! Rather than re-implementing command-rewriting heuristics in Rust, this stage
//! delegates entirely to `rtk rewrite <command>`. The rewrite rules (e.g. preferring
//! `rtk`-aware `ls`/`tree` equivalents) live in the external binary and can evolve
//! independently of `inceptool`.
//!
//! ## Flow
//!
//! 1. **Event Filtering**: Intercepts `PreToolUse` events for the `Bash` or
//!   `run_shell_command` tools.
//! 2. **Command Extraction**: Reads the `command` field from the tool input;
//!    bails out if it is absent or empty.
//! 3. **Rewrite Invocation**: Pipes the command through `rtk rewrite <command>`
//!    via [`std::process::Command`].
//! 4. **Diff Checking**: Compares the rewritten output against the original
//!    command.
//! 5. **Response Generation**: If `rtk` produced a different, non-empty command,
//!    returns `Decision::Allow` with the rewritten `command` and reason
//!    [`REWRITE_REASON`]. Otherwise returns `Ok(None)` — failures to invoke
//!    `rtk` (missing binary, non-zero exit, etc.) are only logged via
//!    `tracing::error!`, never surfaced to the agent.

use inceptool_engine::{EngineError, Stage};
use inceptool_protocol::{
    Conn, Decision, HookInputEvent, HookKind, HookOutputEvent, PreToolUseOutput,
};

use serde_json::Value;
use std::process::{Command, Output};

/// Binary invoked to rewrite shell commands.
const RTK_BINARY: &str = "rtk";

/// Subcommand passed to [`RTK_BINARY`] that performs the rewrite.
const RTK_SUBCOMMAND: &str = "rewrite";

/// Reason supplied to the agent when a command is rewritten.
const REWRITE_REASON: &str = "RTK auto-rewrite";

/// Stage that intercepts shell commands and uses `rtk rewrite` to enforce standard practices.
#[derive(Debug, Clone, Copy, Default)]
pub struct RtkStage;

impl Stage for RtkStage {
    fn name(&self) -> &'static str {
        "rtk"
    }

    fn hook(&self) -> HookKind {
        HookKind::PreToolUse
    }

    fn tool_names(&self) -> &'static [&'static str] {
        &["Bash", "run_shell_command"]
    }

    fn run(&self, conn: &mut Conn<'_>) -> Result<Option<HookOutputEvent>, EngineError> {
        let HookInputEvent::PreToolUse(input) = &conn.event else {
            return Ok(None);
        };

        let mut parsed: Value = input.parse_tool_input()?;

        let Some(cmd) = parsed.get("command").and_then(Value::as_str) else {
            return Ok(None);
        };

        if cmd.trim().is_empty() {
            return Ok(None);
        }

        let Some(rewritten) = Self::rewrite(cmd) else {
            return Ok(None);
        };

        if let Some(cmd_val) = parsed.get_mut("command") {
            *cmd_val = Value::String(rewritten);
        }

        Ok(Some(HookOutputEvent::PreToolUse(PreToolUseOutput {
            decision: Some(Decision::Allow),
            reason: Some(REWRITE_REASON.into()),
            updated_input: Some(parsed),
            ..Default::default()
        })))
    }
}

impl RtkStage {
    /// Pipes `cmd` through `rtk rewrite` and returns the rewritten command.
    ///
    /// Returns `None` if `rtk` is missing, fails to execute, exits with a
    /// non-zero status, or returns the same/empty output. In the failure
    /// cases, details are logged via `tracing::error!` rather than surfaced
    /// to the agent.
    fn rewrite(cmd: &str) -> Option<String> {
        let output = Self::invoke(cmd)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            tracing::error!(
                "rtk rewrite failed with exit code: {} - {}",
                output.status,
                stderr.trim()
            );

            return None;
        }

        let Ok(mut rewritten) = String::from_utf8(output.stdout) else {
            return None;
        };
        trim_in_place(&mut rewritten);

        if rewritten.is_empty() || rewritten == cmd {
            None
        } else {
            Some(rewritten)
        }
    }

    /// Runs `rtk rewrite <cmd>`, logging and returning `None` if the process
    /// cannot be spawned at all.
    fn invoke(cmd: &str) -> Option<Output> {
        match Command::new(RTK_BINARY)
            .arg(RTK_SUBCOMMAND)
            .arg(cmd)
            .output()
        {
            Ok(output) => Some(output),
            Err(e) => {
                tracing::error!("rtk rewrite failed to execute: {}", e);
                None
            }
        }
    }
}

/// Trims leading and trailing whitespace from `s` in place, shifting the
/// existing bytes rather than allocating a new buffer.
fn trim_in_place(s: &mut String) {
    let trimmed = s.trim();
    let start = (trimmed.as_ptr().addr()).saturating_sub(s.as_ptr().addr());
    let end = start.saturating_add(trimmed.len());

    s.truncate(end);

    if start > 0 {
        s.drain(..start);
    }
}
