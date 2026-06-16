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
//!    `run_shell_command` tools.
//! 2. **Command Extraction**: Reads the `command` field from the tool input;
//!    bails out if it is absent or empty.
//! 3. **Rewrite Invocation**: Pipes the command through `rtk rewrite <command>`
//!    via [`std::process::Command`].
//! 4. **Diff Checking**: Compares the rewritten output against the original
//!    command.
//! 5. **Response Generation**: If `rtk` produced a different, non-empty command
//!    on `stdout` - regardless of exit code, since `rtk rewrite` exits `3`
//!    rather than `0` for rewritten compound commands (joined by `&&`, `;`,
//!    or `||`) - returns `Decision::Allow` with the rewritten `command` and
//!    reason `REWRITE_REASON`. Otherwise returns `Ok(None)` — failures to
//!    invoke `rtk` (missing binary, unexpected non-zero exit with no usable
//!    output) are only logged via `tracing::error!`, never surfaced to the
//!    agent.

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

        tracing::info!(original = %cmd, %rewritten, "command rewritten by rtk");

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
    /// Returns `None` if `rtk` is missing, fails to execute, or returns the
    /// same/empty output. In the failure cases, details are logged via
    /// `tracing::error!` rather than surfaced to the agent.
    fn rewrite(cmd: &str) -> Option<String> {
        let output = Self::invoke(cmd)?;

        Self::interpret(cmd, output)
    }

    /// Extracts a rewritten command from `rtk rewrite`'s `output`, if any.
    ///
    /// `rtk rewrite` exits `0` for a single rewritten command, `1` with empty
    /// output when `cmd` has no RTK equivalent, and `3` for a rewritten
    /// compound command (e.g. one joined by `&&`, `;`, or `||`) - in all
    /// three cases `stdout` is the authoritative signal, so this only
    /// inspects `output.status` to decide whether an empty/unchanged `stdout`
    /// is the documented "no equivalent" case (exit `1`) or an unexpected
    /// failure worth logging.
    fn interpret(cmd: &str, output: Output) -> Option<String> {
        let Ok(mut rewritten) = String::from_utf8(output.stdout) else {
            return None;
        };
        trim_in_place(&mut rewritten);

        if !rewritten.is_empty() && rewritten != cmd {
            return Some(rewritten);
        }

        if !output.status.success() && output.status.code() != Some(1_i32) {
            let stderr = String::from_utf8_lossy(&output.stderr);

            tracing::error!(
                "rtk rewrite failed with exit code: {} - {}",
                output.status,
                stderr.trim()
            );
        }

        None
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::os::unix::process::ExitStatusExt as _;
    use std::process::ExitStatus;

    use rstest::rstest;

    /// Builds a synthetic `rtk rewrite` [`Output`] as if the process exited
    /// normally with `code`, writing `stdout`/`stderr`.
    fn output(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[rstest]
    // Exit 0: a single command was rewritten.
    #[case::single_command_rewritten(
        "git status",
        output(0, "rtk git status", ""),
        Some("rtk git status")
    )]
    // Exit 1 with empty stdout: the documented "no RTK equivalent" case.
    #[case::no_equivalent("foobarbaz qux", output(1, "", ""), None)]
    // Exit 3: rtk's signal for a rewritten *compound* command - still a
    // usable rewrite, not a failure.
    #[case::compound_command_rewritten(
        "git status && git log",
        output(3, "rtk git status && rtk git log", ""),
        Some("rtk git status && rtk git log")
    )]
    // Exit 0 but stdout echoes the input unchanged: treated as no rewrite.
    #[case::unchanged_output_is_not_a_rewrite("git status", output(0, "git status", ""), None)]
    // Unexpected non-zero exit with no usable stdout: no rewrite.
    #[case::unexpected_failure("git status", output(2, "", "boom"), None)]
    fn interpret_extracts_rewrite_from_output(
        #[case] cmd: &str,
        #[case] output: Output,
        #[case] expected: Option<&str>,
    ) {
        assert_eq!(
            RtkStage::interpret(cmd, output),
            expected.map(str::to_owned)
        );
    }
}
