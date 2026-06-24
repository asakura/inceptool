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
//! 4. **Outcome Classification**: `RewriteOutcome` distinguishes three cases -
//!    a usable rewrite, the documented "no RTK equivalent" case (exit
//!    `RTK_EXIT_NO_EQUIVALENT` with empty `stdout`), and an unexpected failure
//!    (missing binary, non-UTF-8 `stdout`, or any other non-rewrite exit
//!    status). `stdout` content is the authoritative signal for a rewrite,
//!    not the exit code, since `rtk rewrite` exits `3` rather than `0` for
//!    rewritten compound commands (joined by `&&`, `;`, or `||`).
//! 5. **Response Generation**: A usable rewrite returns `Decision::Allow` with
//!    the rewritten `command` and reason `REWRITE_REASON`. The "no
//!    equivalent" case returns `Ok(None)` (pure pass-through). An unexpected
//!    failure is logged via `tracing::error!` *and* returned with
//!    `system_message` set to a human-readable description - shown to the
//!    user in the transcript, but with `decision`/`reason`/`updated_input`
//!    left unset so the tool call proceeds exactly as if this stage had not
//!    run at all.
//!
//! ## Edge Cases
//!
//! A later stage in the same `PreToolUse`/`Bash` pipeline bucket that also
//! returns `Some(_)` would replace this stage's output wholesale (see
//! `Registry::run_pipeline`), silently dropping the `system_message`. No
//! other stage currently matches `Bash`/`run_shell_command`, so this is a
//! latent risk rather than an active bug.

use inceptool_engine::{EngineError, Stage};
use inceptool_protocol::{
    Conn, Decision, HookInputEvent, HookKind, HookOutputEvent, PreToolUseOutput,
};

use serde_json::Value;

use std::{
    fmt, io,
    process::{Command, ExitStatus, Output},
};

/// Binary invoked to rewrite shell commands.
const RTK_BINARY: &str = "rtk";

/// Subcommand passed to [`RTK_BINARY`] that performs the rewrite.
const RTK_SUBCOMMAND: &str = "rewrite";

/// Reason supplied to the agent when a command is rewritten.
const REWRITE_REASON: &str = "RTK auto-rewrite";

/// Exit code `rtk rewrite` uses for "no RTK equivalent for this command".
const RTK_EXIT_NO_EQUIVALENT: i32 = 1;

/// Stage that intercepts shell commands and uses `rtk rewrite` to enforce standard practices.
#[derive(Debug, Clone, Copy, Default)]
pub struct RtkStage;

/// Outcome of attempting `rtk rewrite` for a single command.
#[derive(Debug, PartialEq, Eq)]
enum RewriteOutcome {
    /// `rtk` produced a different, non-empty command.
    Rewritten(String),
    /// `rtk` ran successfully but reported no equivalent for the command.
    NoEquivalent,
    /// `rtk` could not be invoked, or exited with an unexpected status; the
    /// payload is a user-facing description of why.
    Failed(String),
}

/// Why a [`RewriteOutcome::Failed`] occurred, for a specific `cmd`.
///
/// Implements [`fmt::Display`] to render one human-readable line, used both
/// for the `tracing::error!` log and the user-facing `system_message`.
struct RewriteFailure<'a> {
    /// The command that `rtk rewrite` was invoked with.
    cmd: &'a str,
    /// The specific reason the rewrite attempt failed.
    cause: FailureCause<'a>,
}

/// The specific reason a [`RewriteFailure`] occurred.
#[derive(Clone, Copy)]
enum FailureCause<'a> {
    /// `rtk` could not be spawned at all (e.g. the binary is missing).
    Spawn(&'a io::Error),
    /// `rtk` exited with `stdout` that was not valid UTF-8.
    InvalidOutput,
    /// `rtk` exited with an unexpected, non-rewrite status.
    UnexpectedExit {
        /// The exit status `rtk rewrite` terminated with.
        status: ExitStatus,
        /// The trimmed `stderr` `rtk rewrite` produced, if any.
        stderr: &'a str,
    },
}

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

        match Self::rewrite(cmd) {
            RewriteOutcome::Rewritten(rewritten) => {
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
            RewriteOutcome::NoEquivalent => Ok(None),
            RewriteOutcome::Failed(message) => {
                Ok(Some(HookOutputEvent::PreToolUse(PreToolUseOutput {
                    system_message: Some(message),
                    ..Default::default()
                })))
            }
        }
    }
}

impl RtkStage {
    /// Pipes `cmd` through `rtk rewrite` and classifies the result.
    #[must_use = "returns the rewrite outcome; discarding it drops the classification entirely"]
    fn rewrite(cmd: &str) -> RewriteOutcome {
        match Self::invoke(cmd) {
            Ok(output) => Self::interpret(cmd, output),
            Err(message) => RewriteOutcome::Failed(message),
        }
    }

    /// Classifies `rtk rewrite`'s `output` for the command it was invoked with.
    ///
    /// `rtk rewrite` exits `0` for a single rewritten command, `RTK_EXIT_NO_EQUIVALENT`
    /// with empty output when `cmd` has no RTK equivalent, and `3` for a
    /// rewritten *compound* command (e.g. one joined by `&&`, `;`, or `||`) -
    /// in all three cases `stdout` is the authoritative signal, so this only
    /// inspects `output.status` once `stdout` is unchanged/empty, to decide
    /// whether that is the documented "no equivalent" case or an unexpected
    /// failure worth logging and surfacing to the user.
    #[must_use = "returns the classified outcome; discarding it loses the rewrite/failure decision"]
    fn interpret(cmd: &str, output: Output) -> RewriteOutcome {
        let Ok(mut rewritten) = String::from_utf8(output.stdout) else {
            tracing::error!(cmd, "rtk rewrite produced non-UTF-8 output");

            return RewriteOutcome::Failed(
                RewriteFailure {
                    cmd,
                    cause: FailureCause::InvalidOutput,
                }
                .to_string(),
            );
        };

        trim_in_place(&mut rewritten);

        if !rewritten.is_empty() && rewritten != cmd {
            return RewriteOutcome::Rewritten(rewritten);
        }

        if output.status.success() || output.status.code() == Some(RTK_EXIT_NO_EQUIVALENT) {
            return RewriteOutcome::NoEquivalent;
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();

        tracing::error!(cmd, status = %output.status, stderr, "rtk rewrite exited unexpectedly");

        RewriteOutcome::Failed(
            RewriteFailure {
                cmd,
                cause: FailureCause::UnexpectedExit {
                    status: output.status,
                    stderr,
                },
            }
            .to_string(),
        )
    }

    /// Runs `rtk rewrite <cmd>`, logging and returning a user-facing message
    /// if the process cannot be spawned at all.
    #[must_use = "returns the process output; discarding it silently drops the rewrite result"]
    fn invoke(cmd: &str) -> Result<Output, String> {
        Command::new(RTK_BINARY)
            .arg(RTK_SUBCOMMAND)
            .arg(cmd)
            .output()
            .map_err(|e| {
                tracing::error!(cmd, error = %e, "rtk rewrite failed to execute");

                RewriteFailure {
                    cmd,
                    cause: FailureCause::Spawn(&e),
                }
                .to_string()
            })
    }
}

impl fmt::Display for RewriteFailure<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rtk rewrite failed for `{}`: ", self.cmd)?;

        match self.cause {
            FailureCause::Spawn(source) => write!(f, "{source}"),
            FailureCause::InvalidOutput => write!(f, "produced non-UTF-8 output"),
            FailureCause::UnexpectedExit { status, stderr: "" } => {
                write!(f, "exited with {status}")
            }
            FailureCause::UnexpectedExit { status, stderr } => {
                write!(f, "exited with {status}: {stderr}")
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
        RewriteOutcome::Rewritten("rtk git status".to_owned())
    )]
    // Exit 1 with empty stdout: the documented "no RTK equivalent" case.
    #[case::no_equivalent("foobarbaz qux", output(1, "", ""), RewriteOutcome::NoEquivalent)]
    // Exit 3: rtk's signal for a rewritten *compound* command - still a
    // usable rewrite, not a failure.
    #[case::compound_command_rewritten(
        "git status && git log",
        output(3, "rtk git status && rtk git log", ""),
        RewriteOutcome::Rewritten("rtk git status && rtk git log".to_owned())
    )]
    // Exit 0 but stdout echoes the input unchanged: treated as no rewrite.
    #[case::unchanged_output_is_not_a_rewrite(
        "git status",
        output(0, "git status", ""),
        RewriteOutcome::NoEquivalent
    )]
    // Unexpected non-zero exit with no usable stdout: surfaced as a failure.
    #[case::unexpected_failure(
        "git status",
        output(2, "", "boom"),
        RewriteOutcome::Failed("rtk rewrite failed for `git status`: exited with exit status: 2: boom".to_owned())
    )]
    fn interpret_extracts_rewrite_from_output(
        #[case] cmd: &str,
        #[case] output: Output,
        #[case] expected: RewriteOutcome,
    ) {
        assert_eq!(RtkStage::interpret(cmd, output), expected);
    }

    #[rstest]
    fn interpret_reports_non_utf8_output_as_failure() {
        let output = Output {
            status: ExitStatus::from_raw(0_i32 << 8),
            stdout: vec![0xFF_u8, 0xFE_u8],
            stderr: Vec::new(),
        };

        assert_eq!(
            RtkStage::interpret("git status", output),
            RewriteOutcome::Failed(
                "rtk rewrite failed for `git status`: produced non-UTF-8 output".to_owned()
            )
        );
    }
}
