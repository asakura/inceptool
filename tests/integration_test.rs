#![allow(
    clippy::panic_in_result_fn,
    clippy::tests_outside_test_module,
    reason = "rstest cases return Result for `?`-based setup but use assert_eq!/assert_matches! \
              for assertions per project convention"
)]

//! End-to-end tests that invoke the built `inceptool` binary as a subprocess,
//! feeding it driver-specific hook payloads on stdin and asserting on its
//! stdout/stderr/exit-code contract.

use assert_cmd::Command;
use miette::{IntoDiagnostic as _, Result};
use predicates::prelude::*;
use rstest::{fixture, rstest};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::process::Command as StdCommand;
use tempfile::TempDir;

struct TestEnv {
    cmd: Command,
    // We hold the TempDir so it isn't dropped (and deleted) until the test finishes
    _temp_dir: TempDir,
}

#[fixture]
fn inceptool_cmd() -> Result<TestEnv> {
    let temp_dir = tempfile::tempdir().into_diagnostic()?;
    // Create an isolated XDG_CONFIG_HOME structure
    let config_dir = temp_dir.path().join("inceptool");

    fs::create_dir_all(&config_dir).into_diagnostic()?;

    // Provide a completely empty config to guarantee all stages use their default state
    fs::write(config_dir.join("inceptool.toml"), "").into_diagnostic()?;

    let data_home = temp_dir.path().join("data");
    fs::create_dir_all(&data_home).into_diagnostic()?;

    // `rtk` crashes if `HOME/.claude/settings.json` doesn't exist but the project has `.claude`
    let claude_dir = temp_dir.path().join(".claude");
    fs::create_dir_all(&claude_dir).into_diagnostic()?;
    let bin_dir = temp_dir.path().join("bin");
    fs::create_dir_all(&bin_dir).into_diagnostic()?;
    let dummy_hook = bin_dir.join("inceptool");
    fs::write(&dummy_hook, "#!/bin/sh\nexit 0").into_diagnostic()?;
    fs::set_permissions(&dummy_hook, fs::Permissions::from_mode(0o755)).into_diagnostic()?;
    let dummy_hook_path = dummy_hook.display().to_string();

    let settings_json = format!(
        r#"{{
  "hooks": {{
    "PreToolUse": [
      {{
        "hooks": [
          {{
            "command": "{dummy_hook_path} claude PreToolUse",
            "type": "command"
          }}
        ],
        "matcher": "Bash"
      }}
    ]
  }}
}}"#
    );
    fs::write(claude_dir.join("settings.json"), settings_json).into_diagnostic()?;

    // `rtk` also crashes (exit code 3) if run outside of a git repository
    StdCommand::new("git")
        .arg("init")
        .current_dir(temp_dir.path())
        .output()
        .into_diagnostic()?;

    let mut cmd = Command::cargo_bin("inceptool").into_diagnostic()?;

    cmd.env("RUST_LOG", "debug")
        .env("XDG_CONFIG_HOME", temp_dir.path()) // isolate user config
        .env("XDG_DATA_HOME", &data_home) // isolate rtk data
        .env("HOME", temp_dir.path()) // fully isolate everything else
        .current_dir(temp_dir.path()); // isolate local config and file operations

    Ok(TestEnv {
        cmd,
        _temp_dir: temp_dir,
    })
}

#[rstest]
#[case::claude_rtk_rewrite("claude_rtk_rewrite", "claude", "PreToolUse", serde_json::json!({
    "session_id": "test",
    "hook_event_name": "PreToolUse",
    "tool_name": "Bash",
    "tool_input": { "command": "ls" }
}))]
#[case::claude_skip_non_bash("claude_skip_non_bash", "claude", "PreToolUse", serde_json::json!({
    "session_id": "test",
    "hook_event_name": "PreToolUse",
    "tool_name": "Read",
    "tool_input": { "file_path": "foo" }
}))]
#[case::claude_skip_null_command("claude_skip_null_command", "claude", "PreToolUse", serde_json::json!({
    "session_id": "test",
    "hook_event_name": "PreToolUse",
    "tool_name": "Bash",
    "tool_input": {}
}))]
#[case::gemini_rtk_rewrite("gemini_rtk_rewrite", "gemini", "BeforeTool", serde_json::json!({
    "session_id": "test",
    "hook_event_name": "BeforeTool",
    "tool_name": "run_shell_command",
    "tool_input": { "command": "ls" }
}))]
#[case::gemini_skip_null_command("gemini_skip_null_command", "gemini", "BeforeTool", serde_json::json!({
    "session_id": "test",
    "hook_event_name": "BeforeTool",
    "tool_name": "run_shell_command",
    "tool_input": {}
}))]
fn integration_happy_path(
    inceptool_cmd: Result<TestEnv>,
    #[case] test_name: &str,
    #[case] driver: &str,
    #[case] stage: &str,
    #[case] input: Value,
) -> Result<()> {
    let mut inceptool_cmd = inceptool_cmd?;

    let assert = inceptool_cmd
        .cmd
        .args([driver, stage])
        .write_stdin(input.to_string())
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).into_diagnostic()?;
    let parsed: Value = serde_json::from_str(&stdout).into_diagnostic()?;

    insta::with_settings!({ snapshot_suffix => test_name }, {
        insta::assert_json_snapshot!(parsed, {
            ".session_id" => "[REDACTED_SESSION_ID]",
        });
    });

    Ok(())
}

#[rstest]
fn integration_failure_invalid_json(inceptool_cmd: Result<TestEnv>) -> Result<()> {
    let mut inceptool_cmd = inceptool_cmd?;

    inceptool_cmd
        .cmd
        .args(["claude", "PreToolUse"])
        .write_stdin("{ invalid json payload")
        .assert()
        .failure()
        .stderr(predicate::str::is_empty().not());

    Ok(())
}

#[rstest]
fn integration_empty_stdin(inceptool_cmd: Result<TestEnv>) -> Result<()> {
    let mut inceptool_cmd = inceptool_cmd?;

    let assert = inceptool_cmd
        .cmd
        .args(["claude", "PreToolUse"])
        .write_stdin("   \n \t ") // Send only whitespace
        .assert()
        .success()
        .stderr(predicate::str::is_empty());

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).into_diagnostic()?;

    if !stdout.is_empty() {
        return Err(miette::miette!(
            "Expected completely empty stdout for empty stdin, got: {stdout:?}"
        ));
    }

    Ok(())
}

#[rstest]
fn integration_invalid_hook_fails_fast(inceptool_cmd: Result<TestEnv>) -> Result<()> {
    let mut inceptool_cmd = inceptool_cmd?;

    inceptool_cmd
        .cmd
        .args(["claude", "SomeInvalidHook"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unsupported hook event"));

    Ok(())
}

#[rstest]
fn integration_worktree_create_special_case(inceptool_cmd: Result<TestEnv>) -> Result<()> {
    let mut inceptool_cmd = inceptool_cmd?;

    let input = serde_json::json!({
        "session_id": "test",
        "hook_event_name": "WorktreeCreate",
        "subagent_name": "explorer",
        "worktree_id": "wt-1",
        "git_root": "/repo",
        "parent_path": "/repo/.worktrees/main"
    });

    let assert = inceptool_cmd
        .cmd
        .args(["claude", "WorktreeCreate"])
        .env("INCEPTOOL_TEST_MOCK_WORKTREE", "1")
        .write_stdin(input.to_string())
        .assert()
        .success()
        .stderr(predicate::str::is_empty());

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).into_diagnostic()?;

    // Verify it prints ONLY the raw string path, without any JSON envelope!
    if stdout != "/mock/worktree/path" {
        return Err(miette::miette!(
            "expected raw worktree path on stdout, got: {stdout:?}"
        ));
    }

    Ok(())
}
