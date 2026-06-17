//! # Pre-commit Runner Architecture
//!
//! Runs each pre-commit hook's binary directly against a file after the LLM
//! edits it, feeding the results back as additional context.
//!
//! ## Core Design
//!
//! Rather than invoking the `pre-commit` CLI, this stage reads
//! `.pre-commit-config.yaml` via [`inceptool_parsers::pre_commit::PreCommitConfig`],
//! resolves which hooks apply to the edited file by matching their
//! `files` / `exclude` regex patterns, and spawns each hook's `entry` binary
//! directly via [`std::process::Command`]. This removes the dependency on the
//! `pre-commit` binary being installed.
//!
//! Hooks run sequentially against the same file, so a later hook sees an earlier
//! hook's output. Rather than reporting each hook's change in isolation, the stage
//! tracks the file's content before the first matching hook and after the last one,
//! and renders a single unified diff (via [`gix::diff::blob`]) between those two
//! endpoints, attributed to every hook that individually changed the content along
//! the way.
//!
//! ## Flow
//!
//! 1. **Guard**: Intercepts `PostToolUse` events for write tools only.
//! 2. **File path extraction**: Probes the tool input JSON via the shared
//!    [`inceptool_protocol::extract_file_path`] helper.
//! 3. **Config discovery**: Discovers the git repository root via [`gix`], then looks for
//!    `.pre-commit-config.yaml` there; falls back to the session cwd when the directory is
//!    not inside a git repository or the repository has no working directory (bare clone).
//! 4. **Hook dispatch**: Normalizes the file path to a repo-relative path using the discovered
//!    configuration root. For each hook that matches this relative path, splits the `entry`
//!    string into binary and leading args, then spawns the binary from the configuration root
//!    with the hook's configured args (appending the relative file path when `pass_filenames` is true).
//! 5. **Aggregation**: Folds each hook's outcome into a running total: the file's content
//!    before the first matching hook, its content after the last one, which hooks individually
//!    changed it, and which exited non-zero.
//! 6. **Feedback**: Injects `additional_context` with a single unified diff (when the net
//!    content changed) followed by one block per failing hook; passes through silently when
//!    neither happened.
//!
//! ## Edge Cases
//!
//! - Hooks with no `entry` field are skipped silently.
//! - If the binary cannot be spawned (not on `PATH`): logs via `tracing::error!`
//!   and continues to the next hook.
//! - `always_run` hooks bypass the file-pattern filter.
//! - A hook with no `files` pattern matches every file (pre-commit default).
//! - A hook that both modifies the file and exits non-zero is attributed in the diff
//!   *and* gets its own failure block.
//! - If an earlier hook's change is fully reverted by a later one, the net diff is
//!   empty and no modification is reported, even though individual hooks ran.

use gix::diff::blob::unified_diff::{ConsumeBinaryHunk, ContextSize};
use gix::diff::blob::{Algorithm, Diff, InternedInput, UnifiedDiff};
use inceptool_engine::{EngineError, Stage};
use inceptool_parsers::pre_commit::{Hook, PreCommitConfig};
use inceptool_protocol::{
    Conn, HookInputEvent, HookKind, HookOutputEvent, PostToolUseOutput, extract_file_path,
};

use serde_json::Value;

use std::{
    borrow::Cow,
    fmt, fs,
    path::Path,
    process::{Command, Output},
};

/// Name of the pre-commit configuration file looked up in the session cwd.
const PRE_COMMIT_CONFIG: &str = ".pre-commit-config.yaml";

/// Write tool names whose `PostToolUse` event should trigger pre-commit hook runs.
const WRITE_TOOLS: &[&str] = &["Write", "Edit", "MultiEdit", "write_file", "replace"];

/// Stage that runs pre-commit hook binaries directly after an LLM writes a file.
///
/// Reads `.pre-commit-config.yaml`, filters hooks whose `files` pattern matches
/// the edited path, spawns each hook's `entry` binary, and injects the results
/// as `additional_context` when a hook modifies the file or exits non-zero.
#[derive(Debug, Clone, Copy, Default)]
pub struct PreCommitRunnerStage;

/// Filtered summary of hook runs for a single file, rendered into `additional_context`.
///
/// Built via [`Self::new`] from a [`RunOutcome`]: a single net diff (when the file's
/// content changed across the whole hook chain) plus one entry per hook that exited
/// non-zero. Renders via [`fmt::Display`].
#[derive(Debug)]
struct PreCommitReport<'a> {
    file_path: &'a str,
    summary: Option<ModifiedSummary>,
    failures: Vec<HookFailure>,
}

/// The net change across every hook that modified a file, rendered as one unified diff.
#[derive(Debug)]
struct ModifiedSummary {
    hook_ids: Vec<String>,
    diff: String,
}

/// A single hook that exited non-zero.
#[derive(Debug)]
struct HookFailure {
    hook_id: String,
    output: Output,
}

/// Accumulates the net effect of running every matching hook against a file.
///
/// Built incrementally via [`Self::record`] as each hook runs in sequence, so
/// `initial_content`/`final_content` always span the *first* hook's "before" to the
/// *last* hook's "after", regardless of how many hooks ran in between.
#[derive(Debug, Default)]
struct RunOutcome {
    initial_content: Option<String>,
    final_content: Option<String>,
    modified_by: Vec<String>,
    failures: Vec<HookFailure>,
}

/// Outcome of running a single hook binary.
#[derive(Debug)]
struct HookOutcome {
    hook_id: String,
    modified: bool,
    content_before: String,
    content_after: String,
    output: Output,
}

impl RunOutcome {
    /// Folds a single hook's [`HookOutcome`] into the running totals.
    fn record(&mut self, outcome: HookOutcome) {
        let HookOutcome {
            hook_id,
            modified,
            content_before,
            content_after,
            output,
        } = outcome;

        if self.initial_content.is_none() {
            self.initial_content = Some(content_before);
        }

        self.final_content = Some(content_after);

        if modified {
            self.modified_by.push(hook_id.clone());
        }

        if !output.status.success() {
            self.failures.push(HookFailure { hook_id, output });
        }
    }
}

impl Stage for PreCommitRunnerStage {
    fn name(&self) -> &'static str {
        "pre-commit-runner"
    }

    fn hook(&self) -> HookKind {
        HookKind::PostToolUse
    }

    fn tool_names(&self) -> &'static [&'static str] {
        WRITE_TOOLS
    }

    fn run(&self, conn: &mut Conn<'_>) -> Result<Option<HookOutputEvent>, EngineError> {
        let HookInputEvent::PostToolUse(input) = &conn.event else {
            return Ok(None);
        };

        let parsed: Value = input.parse_tool_input()?;
        let Some(file_path) = extract_file_path(&parsed) else {
            return Ok(None);
        };

        tracing::debug!(file_path, "checking pre-commit hooks");

        let Some(cwd_str) = conn.session.cwd.as_deref() else {
            return Ok(None);
        };

        let cwd = Path::new(cwd_str);
        let git_repo = gix::discover(cwd).ok();
        let config_root = git_repo
            .as_ref()
            .and_then(gix::Repository::workdir)
            .unwrap_or(cwd);
        let config_path = config_root.join(PRE_COMMIT_CONFIG);

        let Ok(yaml) = fs::read_to_string(&config_path) else {
            return Ok(None);
        };

        let config = match PreCommitConfig::parse(&yaml) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "failed to parse .pre-commit-config.yaml");
                return Ok(None);
            }
        };

        let rel_file_path = Path::new(file_path)
            .strip_prefix(config_root)
            .map_or_else(|_| Cow::Borrowed(file_path), |p| p.to_string_lossy());

        let mut outcome = RunOutcome::default();

        for hook in config.hooks() {
            if !Self::file_matches_hook(hook, &rel_file_path) {
                tracing::trace!(
                    hook_id = hook.id(),
                    file_path = rel_file_path.as_ref(),
                    "hook pattern does not match"
                );

                continue;
            }

            if let Some(run) = Self::run_hook(config_root, hook, &rel_file_path) {
                outcome.record(run);
            }
        }

        let Some(report) = PreCommitReport::new(&rel_file_path, outcome) else {
            return Ok(None);
        };

        Ok(Some(HookOutputEvent::PostToolUse(PostToolUseOutput {
            additional_context: Some(Cow::Owned(report.to_string())),
            ..Default::default()
        })))
    }
}

impl PreCommitRunnerStage {
    /// Returns `true` when `hook` should run against `file_path`.
    ///
    /// Checks `always_run` first (bypasses patterns), then the `files` inclusion
    /// regex, then the `exclude` regex. Regex compile errors are logged and cause
    /// the hook to be skipped either way, so a malformed pattern fails closed
    /// instead of silently running (`files`) or silently not excluding (`exclude`).
    #[must_use = "discards whether the hook applies to this file, causing it to be silently skipped or always run"]
    fn file_matches_hook(hook: &Hook<'_>, file_path: &str) -> bool {
        if hook.always_run() {
            return true;
        }

        if let Some(regex_result) = hook.files_regex() {
            match regex_result {
                Ok(regex) => {
                    if !regex.is_match(file_path).unwrap_or(false) {
                        return false;
                    }
                }
                Err(e) => {
                    tracing::error!(hook_id = hook.id(), error = %e, "invalid files regex");
                    return false;
                }
            }
        }

        if let Some(regex_result) = hook.exclude_regex() {
            match regex_result {
                Ok(regex) => {
                    if regex.is_match(file_path).unwrap_or(false) {
                        return false;
                    }
                }
                Err(e) => {
                    tracing::error!(hook_id = hook.id(), error = %e, "invalid exclude regex");
                    return false;
                }
            }
        }

        true
    }

    /// Runs a single hook's entry binary against `file_path` in `config_root`.
    ///
    /// Snapshots file content before and after to detect modifications.
    /// Returns `None` when the hook has no `entry`, the binary cannot be
    /// spawned, or either file read fails — all failure modes are logged.
    #[must_use = "returns the hook's outcome; discarding it drops failing-hook detection"]
    fn run_hook(config_root: &Path, hook: &Hook<'_>, file_path: &str) -> Option<HookOutcome> {
        let entry = hook.entry()?;
        let mut parts = entry.split_whitespace();
        let binary = parts.next()?;
        let leading_args: Vec<&str> = parts.collect();
        let abs_path = config_root.join(file_path);

        let content_before = Self::read_snapshot(&abs_path, file_path, hook.id(), "before")?;
        let output = Self::spawn_hook(config_root, hook, file_path, binary, &leading_args)?;
        let content_after = Self::read_snapshot(&abs_path, file_path, hook.id(), "after")?;

        let modified = content_before != content_after;

        Some(HookOutcome {
            hook_id: hook.id().to_owned(),
            modified,
            content_before,
            content_after,
            output,
        })
    }

    /// Reads `path`'s content for before/after hook snapshotting, logging on failure.
    ///
    /// `phase` (`"before"` or `"after"`) is attached as a tracing field to
    /// distinguish which snapshot failed. Returns `None` on read failure so
    /// callers can propagate via `?`.
    #[must_use = "returns the snapshot content; discarding it loses the read needed to detect modification"]
    fn read_snapshot(
        path: &Path,
        file_path: &str,
        hook_id: &str,
        phase: &'static str,
    ) -> Option<String> {
        match fs::read_to_string(path) {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::error!(file_path, hook_id, phase, error = %e, "failed to read file for hook");
                None
            }
        }
    }

    /// Builds and spawns `hook`'s entry `binary` against `file_path`, rooted at `config_root`.
    ///
    /// `leading_args` are the words following the binary name in the `entry` string;
    /// `hook.args()` and (when `pass_filenames` is set) `file_path` are appended after them.
    /// Returns `None` and logs when the binary cannot be spawned (e.g. not on `PATH`).
    #[must_use = "returns the process output; discarding it silently drops the hook's result"]
    fn spawn_hook(
        config_root: &Path,
        hook: &Hook<'_>,
        file_path: &str,
        binary: &str,
        leading_args: &[&str],
    ) -> Option<Output> {
        let mut cmd = Command::new(binary);

        cmd.current_dir(config_root);
        cmd.args(leading_args);
        cmd.args(hook.args().iter().map(AsRef::as_ref));

        if hook.pass_filenames() {
            cmd.arg(file_path);
        }

        match cmd.output() {
            Ok(o) => Some(o),
            Err(e) => {
                tracing::error!(hook_id = hook.id(), binary, error = %e, "failed to spawn hook binary");
                None
            }
        }
    }

    /// Renders a unified diff between `before` and `after` using `gix`'s blob-diff machinery.
    ///
    /// Returns `None` only if hunk rendering fails, which cannot happen for valid `&str`
    /// input (the underlying error is a UTF-8 conversion of bytes that originated as
    /// `&str` tokens); logged via `tracing::error!` defensively.
    #[must_use = "returns the rendered diff; discarding it loses the comparison"]
    fn unified_diff(before: &str, after: &str) -> Option<String> {
        let input = InternedInput::new(before, after);
        let diff = Diff::compute(Algorithm::Histogram, &input);
        let sink = ConsumeBinaryHunk::new(String::new(), "\n");

        match UnifiedDiff::new(&diff, &input, sink, ContextSize::symmetrical(3)).consume() {
            Ok(diff_text) => Some(diff_text),
            Err(e) => {
                tracing::error!(error = %e, "failed to render unified diff");
                None
            }
        }
    }
}

impl<'a> PreCommitReport<'a> {
    /// Builds a report from `outcome`, computing a single net diff when the file's
    /// content changed and keeping every hook that exited non-zero.
    ///
    /// Returns `None` when the content is unchanged and no hook failed — callers
    /// should pass through silently in that case.
    #[must_use = "returns the filtered report; discarding it loses the hook results"]
    fn new(file_path: &'a str, outcome: RunOutcome) -> Option<Self> {
        let RunOutcome {
            initial_content,
            final_content,
            modified_by,
            failures,
        } = outcome;

        let summary = match (initial_content, final_content) {
            (Some(initial), Some(latest)) if initial != latest => {
                PreCommitRunnerStage::unified_diff(&initial, &latest).map(|diff| ModifiedSummary {
                    hook_ids: modified_by,
                    diff,
                })
            }
            _ => None,
        };

        if summary.is_none() && failures.is_empty() {
            return None;
        }

        Some(Self {
            file_path,
            summary,
            failures,
        })
    }
}

impl fmt::Display for PreCommitReport<'_> {
    /// Renders a header naming `file_path`, the [`ModifiedSummary`] (if any), then
    /// each [`HookFailure`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Pre-commit hooks ran on `{}`.", self.file_path)?;

        if let Some(summary) = &self.summary {
            write!(f, "{summary}")?;
        }

        for failure in &self.failures {
            write!(f, "{failure}")?;
        }

        Ok(())
    }
}

impl fmt::Display for ModifiedSummary {
    /// Renders the contributing hook ids followed by the net unified diff in a
    /// fenced `diff` code block.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\n**{}** modified the file. Diff:\n\n```diff\n",
            self.hook_ids.join(", ")
        )?;

        f.write_str(&self.diff)?;
        f.write_str("```\n")
    }
}

impl fmt::Display for HookFailure {
    /// Renders the hook id, exit code, and captured stdout/stderr.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\n**{}** failed", self.hook_id)?;

        if let Some(code) = self.output.status.code() {
            write!(f, " (exit {code})")?;
        }

        f.write_str(":\n")?;

        let stdout = String::from_utf8_lossy(&self.output.stdout);

        if !stdout.trim().is_empty() {
            writeln!(f, "{}", stdout.trim())?;
        }

        let stderr = String::from_utf8_lossy(&self.output.stderr);

        if !stderr.trim().is_empty() {
            writeln!(f, "{}", stderr.trim())?;
        }

        Ok(())
    }
}

#[cfg(test)]
#[expect(
    clippy::panic_in_result_fn,
    reason = "rstest cases return Result for ?-based setup but use assert_matches!/assert_eq! for assertions"
)]
mod tests {
    use super::*;

    use inceptool_protocol::{PostToolUseInput, RawJson, SessionMeta};

    use indoc::indoc;
    use rstest::rstest;
    use serde_json::value::RawValue;

    use core::assert_matches;
    use std::{io, os::unix::process::ExitStatusExt as _, process::ExitStatus};

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error(transparent)]
        Engine(#[from] EngineError),
        #[error(transparent)]
        Json(#[from] serde_json::Error),
        #[error(transparent)]
        Yaml(#[from] serde_saphyr::Error),
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error("Test failure: {0}")]
        Failure(String),
    }

    /// Builds a synthetic [`Output`] as if a process exited with `code`.
    fn make_output(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    /// Builds a [`HookOutcome`] that changed the file from `content_before` to `content_after`.
    fn hook_outcome(
        hook_id: &str,
        content_before: &str,
        content_after: &str,
        output: Output,
    ) -> HookOutcome {
        HookOutcome {
            hook_id: hook_id.to_owned(),
            modified: content_before != content_after,
            content_before: content_before.to_owned(),
            content_after: content_after.to_owned(),
            output,
        }
    }

    /// Parses `yaml` and returns its first hook, cloned to outlive the parsed config.
    fn first_hook(yaml: &str) -> Result<Hook<'_>, TestError> {
        let config = PreCommitConfig::parse(yaml)?;

        let repo = config
            .repos()
            .first()
            .ok_or_else(|| TestError::Failure("no repos".to_owned()))?;

        let hook = repo
            .hooks()
            .first()
            .ok_or_else(|| TestError::Failure("no hooks".to_owned()))?;

        Ok(hook.clone())
    }

    /// Converts `path` to `&str`, failing the test on non-UTF-8 paths.
    fn path_str(path: &Path) -> Result<&str, TestError> {
        path.to_str()
            .ok_or_else(|| TestError::Failure("invalid utf8 path".to_owned()))
    }

    /// Builds a [`SessionMeta`] with only `session_id`, `driver`, and `cwd` set.
    fn session_meta<'a>(
        session_id: &'a str,
        driver: &'a str,
        cwd: Option<&'a str>,
    ) -> SessionMeta<'a> {
        SessionMeta {
            session_id: Cow::Borrowed(session_id),
            transcript_path: None,
            cwd: cwd.map(Cow::Borrowed),
            timestamp: None,
            driver: Cow::Borrowed(driver),
            driver_meta: None,
            permission_mode: None,
            effort: None,
            agent_id: None,
            agent_type: None,
        }
    }

    /// Builds a [`Conn`] with a `PostToolUse` event for `tool_name`.
    fn post_tool_use_conn<'a>(
        tool_name: &'a str,
        cwd: Option<&'a str>,
        tool_input_raw: &'a RawValue,
        tool_output_raw: &'a RawValue,
    ) -> Conn<'a> {
        Conn {
            session: session_meta("test-session", "test", cwd),
            event: HookInputEvent::PostToolUse(PostToolUseInput {
                tool_name: Cow::Borrowed(tool_name),
                tool_input: RawJson(tool_input_raw),
                tool_output: RawJson(tool_output_raw),
                tool_output_source: None,
                mcp_context: None,
                original_request_name: None,
            }),
        }
    }

    mod pre_commit_runner_stage {
        use super::*;

        mod run {
            use super::*;

            #[rstest]
            fn non_post_tool_use_event_returns_none() -> Result<(), TestError> {
                let stage = PreCommitRunnerStage;
                let raw = RawValue::from_string("{}".to_owned())?;

                let mut conn = Conn {
                    session: session_meta("s", "t", None),
                    event: HookInputEvent::PreToolUse(inceptool_protocol::PreToolUseInput {
                        tool_name: Cow::Borrowed("Edit"),
                        tool_input: RawJson(&raw),
                        mcp_context: None,
                        original_request_name: None,
                    }),
                };

                assert!(stage.run(&mut conn)?.is_none());

                Ok(())
            }

            #[rstest]
            fn missing_file_path_in_input_returns_none() -> Result<(), TestError> {
                let stage = PreCommitRunnerStage;
                let raw_in = RawValue::from_string("{}".to_owned())?;
                let raw_out = RawValue::from_string("{}".to_owned())?;
                let mut conn = post_tool_use_conn("Edit", None, &raw_in, &raw_out);

                assert!(stage.run(&mut conn)?.is_none());

                Ok(())
            }

            #[rstest]
            fn no_cwd_returns_none() -> Result<(), TestError> {
                let stage = PreCommitRunnerStage;
                let raw_in = RawValue::from_string(r#"{"file_path": "src/main.rs"}"#.to_owned())?;
                let raw_out = RawValue::from_string("{}".to_owned())?;
                let mut conn = post_tool_use_conn("Edit", None, &raw_in, &raw_out);

                assert!(stage.run(&mut conn)?.is_none());

                Ok(())
            }

            #[rstest]
            fn no_config_file_returns_none() -> Result<(), TestError> {
                let stage = PreCommitRunnerStage;
                let dir = tempfile::TempDir::new()?;
                let cwd_owned = dir.path().to_string_lossy().into_owned();
                let raw_in = RawValue::from_string(r#"{"file_path": "src/main.rs"}"#.to_owned())?;
                let raw_out = RawValue::from_string("{}".to_owned())?;
                let mut conn = post_tool_use_conn("Edit", Some(&cwd_owned), &raw_in, &raw_out);

                assert!(stage.run(&mut conn)?.is_none());

                Ok(())
            }
        }
    }

    mod pre_commit_report {
        use super::*;

        mod new {
            use super::*;

            #[rstest]
            fn empty_outcome_returns_none() {
                assert!(PreCommitReport::new("src/foo.rs", RunOutcome::default()).is_none());
            }

            #[rstest]
            fn all_passed_no_change_returns_none() {
                let mut outcome = RunOutcome::default();

                outcome.record(hook_outcome(
                    "cargo-fmt",
                    "fn main() {}\n",
                    "fn main() {}\n",
                    make_output(0, "", ""),
                ));

                assert!(PreCommitReport::new("src/foo.rs", outcome).is_none());
            }

            #[rstest]
            fn modified_file_returns_context_with_diff() {
                let mut outcome = RunOutcome::default();

                outcome.record(hook_outcome(
                    "cargo-fmt",
                    "fn main(){}\n",
                    "fn main() {}\n",
                    make_output(0, "", ""),
                ));

                assert_matches!(
                    PreCommitReport::new("src/foo.rs", outcome).map(|r| r.to_string()),
                    Some(ctx) if ctx.contains("cargo-fmt")
                        && ctx.contains("-fn main(){}")
                        && ctx.contains("+fn main() {}")
                        && ctx.contains("src/foo.rs")
                );
            }

            #[rstest]
            fn failed_hook_returns_context_with_output() {
                let mut outcome = RunOutcome::default();

                outcome.record(hook_outcome(
                    "cargo-check",
                    "fn main() {}\n",
                    "fn main() {}\n",
                    make_output(1, "error: unused import", ""),
                ));

                assert_matches!(
                    PreCommitReport::new("src/foo.rs", outcome).map(|r| r.to_string()),
                    Some(ctx) if ctx.contains("cargo-check")
                        && ctx.contains("error: unused import")
                        && (ctx.contains("exit 1") || ctx.contains("failed"))
                );
            }

            #[rstest]
            fn mixed_pass_and_fail_includes_only_failing_hook() {
                let mut outcome = RunOutcome::default();

                outcome.record(hook_outcome(
                    "trim-whitespace",
                    "fn main() {}\n",
                    "fn main() {}\n",
                    make_output(0, "", ""),
                ));
                outcome.record(hook_outcome(
                    "cargo-check",
                    "fn main() {}\n",
                    "fn main() {}\n",
                    make_output(1, "error!", ""),
                ));

                assert_matches!(
                    PreCommitReport::new("src/foo.rs", outcome).map(|r| r.to_string()),
                    Some(ctx) if !ctx.contains("trim-whitespace") && ctx.contains("cargo-check")
                );
            }

            #[rstest]
            fn chained_modifications_render_single_diff_attributed_to_both_hooks() {
                let mut outcome = RunOutcome::default();

                outcome.record(hook_outcome(
                    "hook-a",
                    "original\n",
                    "step-one\n",
                    make_output(0, "", ""),
                ));
                outcome.record(hook_outcome(
                    "hook-b",
                    "step-one\n",
                    "final\n",
                    make_output(0, "", ""),
                ));

                let ctx = PreCommitReport::new("src/foo.rs", outcome)
                    .map(|r| r.to_string())
                    .unwrap_or_default();

                assert_eq!(ctx.matches("```diff").count(), 1);

                assert!(ctx.contains("hook-a, hook-b"));
                assert!(ctx.contains("-original"));
                assert!(ctx.contains("+final"));
                assert!(!ctx.contains("step-one"));
            }

            #[rstest]
            fn reverted_modification_with_no_net_change_returns_none() {
                let mut outcome = RunOutcome::default();

                outcome.record(hook_outcome(
                    "hook-a",
                    "original\n",
                    "changed\n",
                    make_output(0, "", ""),
                ));
                outcome.record(hook_outcome(
                    "hook-b",
                    "changed\n",
                    "original\n",
                    make_output(0, "", ""),
                ));

                assert!(PreCommitReport::new("src/foo.rs", outcome).is_none());
            }
        }
    }

    mod file_matches_hook {
        use super::*;

        #[rstest]
        // Full YAML per case: hook properties must be indented under the hook entry.
        #[case::no_pattern(
            indoc! {"
                repos:
                - repo: local
                  hooks:
                  - id: test
                "},
            "src/foo.rs",
            true
        )]
        #[case::pattern_matches(
            indoc! {r"
                repos:
                - repo: local
                  hooks:
                  - id: test
                    files: '\.rs$'
                "},
            "src/foo.rs",
            true
        )]
        #[case::pattern_no_match(
            indoc! {r"
                repos:
                - repo: local
                  hooks:
                  - id: test
                    files: '\.py$'
                "},
            "src/foo.rs",
            false
        )]
        #[case::exclude_matches(
            indoc! {"
                repos:
                - repo: local
                  hooks:
                  - id: test
                    exclude: 'src/'
                "},
            "src/foo.rs",
            false
        )]
        #[case::exclude_no_match(
            indoc! {"
                repos:
                - repo: local
                  hooks:
                  - id: test
                    exclude: 'tests/'
                "},
            "src/foo.rs",
            true
        )]
        #[case::invalid_files_regex_fails_closed(
            indoc! {"
                repos:
                - repo: local
                  hooks:
                  - id: test
                    files: '('
                "},
            "src/foo.rs",
            false
        )]
        #[case::invalid_exclude_regex_fails_closed(
            indoc! {"
                repos:
                - repo: local
                  hooks:
                  - id: test
                    exclude: '('
                "},
            "src/foo.rs",
            false
        )]
        fn hook_pattern_matching(
            #[case] yaml: &str,
            #[case] file: &str,
            #[case] expected: bool,
        ) -> Result<(), TestError> {
            let hook = first_hook(yaml)?;

            assert_eq!(
                PreCommitRunnerStage::file_matches_hook(&hook, file),
                expected
            );

            Ok(())
        }

        #[rstest]
        fn always_run_bypasses_pattern() -> Result<(), TestError> {
            let hook = first_hook(indoc! {r"
                repos:
                - repo: local
                  hooks:
                  - id: test
                    files: '\.py$'
                    always_run: true
                "})?;

            assert!(PreCommitRunnerStage::file_matches_hook(&hook, "src/foo.rs"));

            Ok(())
        }
    }

    mod run_hook {
        use super::*;
        use rstest::fixture;
        use std::fs;

        #[fixture]
        fn temp_repo() -> tempfile::TempDir {
            #[expect(clippy::expect_used, reason = "fixtures cannot return Result")]
            {
                let dir = tempfile::TempDir::new().expect("create temp dir");
                let src = dir.path().join("src");

                fs::create_dir_all(&src).expect("create src dir");
                fs::write(src.join("main.rs"), "fn main() {}").expect("write file");

                dir
            }
        }

        #[rstest]
        fn runs_from_config_root_and_uses_repo_relative_paths(
            temp_repo: tempfile::TempDir,
        ) -> Result<(), TestError> {
            let root = temp_repo.path();
            let hook = first_hook(indoc! {"
                repos:
                - repo: local
                  hooks:
                  - id: pwd-hook
                    name: pwd
                    entry: pwd
                    language: system
                    always_run: true
                    pass_filenames: true
                "})?;

            // Run hook simulating file_path="src/main.rs" inside config_root
            let run = PreCommitRunnerStage::run_hook(root, &hook, "src/main.rs")
                .ok_or_else(|| TestError::Failure("hook should run".to_owned()))?;

            // `pwd` should output the config_root
            let stdout = String::from_utf8_lossy(&run.output.stdout);

            assert_eq!(stdout.trim(), path_str(root)?);

            Ok(())
        }
    }

    mod read_snapshot {
        use super::*;
        use std::fs;

        #[rstest]
        fn existing_file_returns_content() -> Result<(), TestError> {
            let dir = tempfile::TempDir::new()?;
            let path = dir.path().join("file.txt");

            fs::write(&path, "hello")?;

            assert_eq!(
                PreCommitRunnerStage::read_snapshot(&path, "file.txt", "hook-id", "before"),
                Some("hello".to_owned())
            );

            Ok(())
        }

        #[rstest]
        fn missing_file_returns_none() -> Result<(), TestError> {
            let dir = tempfile::TempDir::new()?;
            let path = dir.path().join("missing.txt");

            assert!(
                PreCommitRunnerStage::read_snapshot(&path, "missing.txt", "hook-id", "before")
                    .is_none()
            );

            Ok(())
        }
    }

    mod spawn_hook {
        use super::*;

        /// Unwraps a [`PreCommitRunnerStage::spawn_hook`] result, failing the test if it didn't spawn.
        fn expect_spawned(output: Option<Output>) -> Result<Output, TestError> {
            output.ok_or_else(|| TestError::Failure("hook should spawn".to_owned()))
        }

        #[rstest]
        fn missing_binary_returns_none() -> Result<(), TestError> {
            let dir = tempfile::TempDir::new()?;

            let hook = first_hook(indoc! {"
                repos:
                - repo: local
                  hooks:
                  - id: test
                "})?;

            let result = PreCommitRunnerStage::spawn_hook(
                dir.path(),
                &hook,
                "src/main.rs",
                "definitely-not-a-real-binary-xyz",
                &[],
            );

            assert!(result.is_none());

            Ok(())
        }

        #[rstest]
        fn runs_in_config_root() -> Result<(), TestError> {
            let dir = tempfile::TempDir::new()?;

            let hook = first_hook(indoc! {"
                repos:
                - repo: local
                  hooks:
                  - id: pwd-hook
                    pass_filenames: false
                "})?;

            let output = expect_spawned(PreCommitRunnerStage::spawn_hook(
                dir.path(),
                &hook,
                "unused",
                "pwd",
                &[],
            ))?;

            let stdout = String::from_utf8_lossy(&output.stdout);

            assert_eq!(stdout.trim(), path_str(dir.path())?);

            Ok(())
        }

        #[rstest]
        #[case::pass_filenames_true(
            indoc! {"
                repos:
                - repo: local
                  hooks:
                  - id: test
                    pass_filenames: true
                "},
            "src/main.rs\n"
        )]
        #[case::pass_filenames_false(
            indoc! {"
                repos:
                - repo: local
                  hooks:
                  - id: test
                    pass_filenames: false
                "},
            "\n"
        )]
        fn pass_filenames_controls_trailing_arg(
            #[case] yaml: &str,
            #[case] expected_stdout: &str,
        ) -> Result<(), TestError> {
            let dir = tempfile::TempDir::new()?;
            let hook = first_hook(yaml)?;

            let output = expect_spawned(PreCommitRunnerStage::spawn_hook(
                dir.path(),
                &hook,
                "src/main.rs",
                "echo",
                &[],
            ))?;

            assert_eq!(String::from_utf8_lossy(&output.stdout), expected_stdout);

            Ok(())
        }

        #[rstest]
        fn appends_leading_args_then_hook_args() -> Result<(), TestError> {
            let dir = tempfile::TempDir::new()?;

            let hook = first_hook(indoc! {"
                repos:
                - repo: local
                  hooks:
                  - id: test
                    args: ['--world']
                    pass_filenames: false
                "})?;

            let output = expect_spawned(PreCommitRunnerStage::spawn_hook(
                dir.path(),
                &hook,
                "unused",
                "echo",
                &["hello"],
            ))?;

            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                "hello --world"
            );

            Ok(())
        }
    }
}
