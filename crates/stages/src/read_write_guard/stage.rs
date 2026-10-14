use super::RuleSet;
use super::denial::dispatch;

use inceptool_engine::{EngineError, Stage};
use inceptool_protocol::{Conn, HookInputEvent, HookKind, HookOutputEvent, extract_file_path};

use serde_json::Value;

use std::path::{Path, PathBuf};

/// Tool names whose `PreToolUse` event reads a file's contents.
pub(super) const READ_TOOLS: &[&str] = &["Read", "view_file", "cat"];

/// Stage that denies direct reads and writes of ecosystem lockfiles.
#[derive(Debug, Clone)]
pub struct ReadWriteGuardStage {
    rules: RuleSet,
    /// The enclosing git repository's working directory, if known — see
    /// [`ReadWriteGuardStage::repo_relative`] for how it's used.
    repo_root: Option<PathBuf>,
}

impl Stage for ReadWriteGuardStage {
    fn name(&self) -> &'static str {
        "read-write-guard"
    }

    fn hook(&self) -> HookKind {
        HookKind::PreToolUse
    }

    fn tool_names(&self) -> &'static [&'static str] {
        &[
            "Write",
            "Edit",
            "MultiEdit",
            "write_file",
            "replace",
            "Read",
            "view_file",
            "cat",
        ]
    }

    fn run(&self, conn: &mut Conn<'_>) -> Result<Option<HookOutputEvent>, EngineError> {
        if let HookInputEvent::PreToolUse(input) = &conn.event {
            let parsed: Value = input.parse_tool_input()?;

            let Some(file_path) = extract_file_path(&parsed) else {
                return Ok(None);
            };

            let Some((rule, display_name)) = self.rules.get(self.repo_relative(file_path)) else {
                return Ok(None);
            };

            let is_read_tool = READ_TOOLS.contains(&input.tool_name.as_ref());

            let Some(output) = dispatch(display_name, rule, is_read_tool) else {
                return Ok(None);
            };

            return Ok(Some(HookOutputEvent::PreToolUse(output)));
        }
        Ok(None)
    }
}

impl ReadWriteGuardStage {
    /// Builds the stage from a fully resolved [`RuleSet`] (the caller is
    /// responsible for merging built-in defaults with any user overrides)
    /// and the enclosing git repository's working directory, if known — used
    /// to anchor full-path glob rules to the repo root instead of an
    /// arbitrary absolute path.
    #[must_use = "constructs a new stage; discarding it does nothing"]
    pub const fn new(rules: RuleSet, repo_root: Option<PathBuf>) -> Self {
        Self { rules, repo_root }
    }

    /// Returns `file_path` relative to the stored repo root, when one is
    /// known and `file_path` lies under it — letting a
    /// full-path glob rule like `"vendor/lib.rs"` mean what it looks like it
    /// means (gitignore's own convention: a slash-containing pattern with no
    /// leading `**/` is anchored to a root), rather than being anchored to
    /// whatever literal absolute path the tool happened to report. Falls
    /// back to `file_path` unchanged when there's no known repo root, or
    /// when `file_path` isn't under it (e.g. a file outside the repo) —
    /// [`RuleSet::get`] computes the basename from whichever string it's
    /// handed, so basename/exact matching is identical either way; only the
    /// full-path glob target is affected. Uses [`Path::strip_prefix`] rather
    /// than a string-prefix check, since it's component-aware: `/repo`
    /// doesn't falsely match a sibling like `/repository/...`.
    #[must_use = "returns file_path made relative to the repo root when known; has no side effects"]
    fn repo_relative<'a>(&self, file_path: &'a str) -> &'a str {
        self.repo_root
            .as_deref()
            .and_then(|root| Path::new(file_path).strip_prefix(root).ok())
            .and_then(Path::to_str)
            .unwrap_or(file_path)
    }
}
