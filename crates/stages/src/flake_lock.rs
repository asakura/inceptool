//! # Flake-Lock-Summarization Architecture
//!
//! The Flake-Lock Summarization stage optimizes context window usage when
//! the AI agent attempts to read a `flake.lock` file.
//!
//! ## Core Design
//!
//! `flake.lock` files are large, dense JSON documents. When an agent reads
//! one, it floods its context window with unnecessary metadata, diluting its
//! attention and rapidly burning through token limits. In most cases, the
//! agent only needs to know the names of the inputs, their sources, and
//! whether they have changed relative to the git repository's `HEAD`.
//!
//! This stage intercepts `Read` operations targeting `flake.lock` *before*
//! they execute. It parses the file directly from disk and optionally
//! compares it against the last committed version (`HEAD`), then denies the
//! read and returns a highly condensed, line-by-line summary as the denial
//! reason — the agent never sees the raw JSON.
//!
//! ## Flow
//! 1. **Event Filtering**: Triggers on `PreToolUse` for tools: `Read`,
//!    `view_file`, `cat`, etc., when the target is named `flake.lock`.
//! 2. **Current State**: Deserializes the current `flake.lock` from the
//!    filesystem into `FlakeLock`, which borrows field values directly from
//!    the source buffer via `Cow<'a, str>` instead of allocating a full
//!    `serde_json::Value` tree.
//! 3. **Head State**: Uses [`gix`] to discover the enclosing repository,
//!    walk `HEAD`'s tree to the `flake.lock` blob, and parse its content the
//!    same way.
//! 4. **Diffing**: Compares the revisions (`rev` fields) of inputs between
//!    `HEAD` and the current file.
//! 5. **Formatting**: Generates a compact summary, e.g.
//!    `nixpkgs: NixOS/nixpkgs@abcdef1 -> 1234567`.
//! 6. **Denial**: Returns `Decision::Deny` with the summary as `reason`,
//!    blocking the read before the raw JSON ever reaches the agent.
//!
//! ## Edge Cases
//!
//! If the file can't be read, isn't valid JSON, or has no nodes worth
//! summarizing (an empty `nodes` map, or only the `root` node), the stage is
//! a no-op (`Ok(None)`), letting the read proceed normally. If `HEAD` can't
//! be determined (not a git repository, the file is untracked, no commits
//! yet, etc.), the summary is still produced, but every entry is shown as
//! unchanged (no `->` diff).

use inceptool_engine::{EngineError, Stage};
use inceptool_protocol::{
    Conn, Decision, HookInputEvent, HookKind, HookOutputEvent, PreToolUseOutput,
};

use serde::Deserialize;
use serde_json::Value;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

/// The file name this stage triggers on.
const FLAKE_LOCK_FILE_NAME: &str = "flake.lock";

/// The synthetic root node present in every `flake.lock`, which carries no `locked` pin.
const ROOT_NODE_NAME: &str = "root";

/// Number of leading characters of a revision hash shown in summaries.
const SHORT_REV_LEN: usize = 7;

/// Stage that denies raw `flake.lock` reads, returning a condensed
/// per-input summary - diffed against `HEAD` where possible - as the denial
/// reason.
#[derive(Debug, Clone, Copy, Default)]
pub struct FlakeLockSummarizationStage;

/// Zero-copy view of the top-level structure of a `flake.lock` (format version 7).
///
/// Field values borrow directly from the source JSON via `Cow<'a, str>`. Only the
/// `nodes` map is retained; the top-level `version` and `root` pointer are ignored.
#[derive(Debug, Default, Deserialize)]
struct FlakeLock<'a> {
    /// All input nodes, keyed by their identifier (including the `"root"` node).
    #[serde(borrow, default)]
    nodes: BTreeMap<Cow<'a, str>, FlakeNode<'a>>,
}

/// A single entry in a `flake.lock`'s `nodes` map.
///
/// Only the `locked` pin is retained; `inputs` and `original` are ignored during
/// deserialization.
#[derive(Debug, Deserialize)]
struct FlakeNode<'a> {
    /// The pinned source information for this input, if present.
    #[serde(borrow, default)]
    locked: Option<LockedRef<'a>>,
}

/// The `locked` pin of a `flake.lock` node, with fields borrowed from the source JSON.
#[derive(Debug, Deserialize)]
struct LockedRef<'a> {
    /// The source type (e.g. `github`, `git`, `tarball`, `path`).
    #[serde(borrow, default, rename = "type")]
    node_type: Cow<'a, str>,
    /// The repository owner, for `github`/`gitlab` sources.
    #[serde(borrow, default)]
    owner: Cow<'a, str>,
    /// The repository name, for `github`/`gitlab` sources.
    #[serde(borrow, default)]
    repo: Cow<'a, str>,
    /// The pinned git revision (commit hash).
    #[serde(borrow, default)]
    rev: Cow<'a, str>,
    /// The source URL, for `git`/`tarball` sources.
    #[serde(borrow, default)]
    url: Cow<'a, str>,
    /// The source path, for `path` sources.
    #[serde(borrow, default)]
    path: Cow<'a, str>,
}

/// A single non-root `flake.lock` input, summarized for display.
struct SummaryEntry {
    /// The node's key in the `flake.lock`'s `nodes` map (e.g. `"nixpkgs"`).
    name: String,
    /// The formatted source, as produced by [`LockedRef::label`] (e.g.
    /// `NixOS/nixpkgs`, `git:<url>`, `path:<path>`).
    label: String,
    /// The pinned revision in the current `flake.lock`, or empty if unpinned.
    cur_rev: String,
    /// The pinned revision in `HEAD`'s `flake.lock`, or empty if the input is
    /// new or `HEAD` couldn't be determined.
    old_rev: String,
    /// `true` if [`Self::old_rev`] is non-empty and differs from [`Self::cur_rev`].
    changed: bool,
}

/// A rendered `flake.lock` summary: the header plus one line per [`SummaryEntry`],
/// as described in the module's "Flow" step 5.
///
/// Callers should treat an empty slice as "nothing to summarize" and return
/// `Ok(None)` instead of constructing this.
struct Summary<'a>(&'a [SummaryEntry]);

impl Stage for FlakeLockSummarizationStage {
    fn name(&self) -> &'static str {
        "flake-lock-summarization"
    }

    fn hook(&self) -> HookKind {
        HookKind::PreToolUse
    }

    fn tool_names(&self) -> &'static [&'static str] {
        &["Read", "view_file", "cat"]
    }

    fn run(&self, conn: &mut Conn<'_>) -> Result<Option<HookOutputEvent>, EngineError> {
        if let HookInputEvent::PreToolUse(input) = &conn.event {
            let parsed: Value = input.parse_tool_input()?;

            let file_path = parsed
                .get("file_path")
                .or_else(|| parsed.get("path"))
                .or_else(|| parsed.get("AbsolutePath"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if file_path.is_empty()
                || Path::new(file_path).file_name().and_then(|f| f.to_str())
                    != Some(FLAKE_LOCK_FILE_NAME)
            {
                return Ok(None);
            }

            let Ok(content) = fs::read_to_string(file_path) else {
                return Ok(None);
            };

            let Ok(current) = serde_json::from_str::<FlakeLock<'_>>(&content) else {
                return Ok(None);
            };

            if current.nodes.is_empty() {
                return Ok(None);
            }

            let head_content = get_head_content(file_path);
            let head = head_content
                .as_deref()
                .and_then(|c| serde_json::from_str::<FlakeLock<'_>>(c).ok())
                .unwrap_or_default();

            let entries = current.diff(&head);

            if entries.is_empty() {
                return Ok(None);
            }

            let summary = Summary(&entries).to_string();

            return Ok(Some(HookOutputEvent::PreToolUse(PreToolUseOutput {
                decision: Some(Decision::Deny),
                reason: Some(summary.into()),
                ..Default::default()
            })));
        }
        Ok(None)
    }
}

impl FlakeLock<'_> {
    /// Compares this lock's nodes against `head`'s, producing one [`SummaryEntry`] per
    /// non-root node. An entry's `changed` flag is set when `head` has a recorded
    /// revision for that node that differs from the current one.
    fn diff(&self, head: &FlakeLock<'_>) -> Vec<SummaryEntry> {
        let mut entries = Vec::new();

        for (name, node) in &self.nodes {
            if name.as_ref() == ROOT_NODE_NAME {
                continue;
            }

            let locked = node.locked.as_ref();
            let cur_rev = locked.map_or("", |l| l.rev.as_ref());

            let old_rev = head
                .nodes
                .get(name.as_ref())
                .and_then(|n| n.locked.as_ref())
                .map_or("", |l| l.rev.as_ref());

            let changed = !old_rev.is_empty() && old_rev != cur_rev;

            entries.push(SummaryEntry {
                name: name.as_ref().to_owned(),
                label: locked.map(LockedRef::label).unwrap_or_default(),
                cur_rev: cur_rev.to_owned(),
                old_rev: old_rev.to_owned(),
                changed,
            });
        }

        entries
    }
}

impl LockedRef<'_> {
    /// Formats this pin as a human-readable label, based on [`Self::node_type`]:
    /// `owner/repo` for `github`/`gitlab`, `<type>:<url>` for `git`/`tarball`,
    /// `path:<path>` for `path`, or the raw type name otherwise.
    fn label(&self) -> String {
        match self.node_type.as_ref() {
            "github" | "gitlab" => format!("{}/{}", self.owner, self.repo),
            "git" => format!("git:{}", self.url),
            "tarball" => format!("tarball:{}", self.url),
            "path" => format!("path:{}", self.path),
            other => other.to_owned(),
        }
    }
}

impl fmt::Display for SummaryEntry {
    /// Renders as `  <name>: <label>@<rev>`, or `  <name>: <label>@<old> -> <new>`
    /// when [`Self::changed`] is `true`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "  {}: {}@", self.name, self.label)?;

        if self.changed {
            write!(
                f,
                "{} -> {}",
                short_rev(&self.old_rev),
                short_rev(&self.cur_rev)
            )
        } else {
            write!(f, "{}", short_rev(&self.cur_rev))
        }
    }
}

/// Truncates `rev` to its first [`SHORT_REV_LEN`] characters, or returns `(none)`
/// for an empty revision.
fn short_rev(rev: &str) -> &str {
    if rev.is_empty() {
        "(none)"
    } else if rev.len() > SHORT_REV_LEN {
        rev.get(..SHORT_REV_LEN).unwrap_or(rev)
    } else {
        rev
    }
}

/// Returns the contents of `file_path` as committed at `HEAD`, or `None` if that
/// can't be determined (not a git repository, file untracked, no `HEAD`, etc.).
///
/// Discovers the enclosing repository via [`gix::discover`], then peels `HEAD`
/// down to its tree and looks up the blob at `file_path`'s location relative to
/// the repository's working directory.
fn get_head_content(file_path: &str) -> Option<String> {
    let dir = Path::new(file_path).parent()?;

    let repo = gix::discover(dir).ok()?;
    let workdir = repo.workdir()?;

    let abs_path = fs::canonicalize(file_path).ok()?;
    let rel_path = abs_path.strip_prefix(workdir).ok()?;

    let tree = repo.head_commit().ok()?.tree().ok()?;
    let entry = tree.lookup_entry_by_path(rel_path).ok()??;

    String::from_utf8(entry.object().ok()?.detach().data).ok()
}

impl fmt::Display for Summary<'_> {
    /// Renders the header line (`flake.lock read blocked — use this summary
    /// instead (N inputs[, M changed vs HEAD]):`) followed by one line per
    /// entry, formatted via [`SummaryEntry`]'s `Display` impl.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total = self.0.len();
        let changed_count = self.0.iter().filter(|e| e.changed).count();

        write!(
            f,
            "flake.lock read blocked \u{2014} use this summary instead ("
        )?;

        if changed_count > 0 {
            write!(f, "{total} inputs, {changed_count} changed vs HEAD):")?;
        } else {
            write!(f, "{total} inputs):")?;
        }

        for entry in self.0 {
            write!(f, "\n{entry}")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use inceptool_protocol::{PreToolUseInput, RawJson, SessionMeta};

    use rstest::{fixture, rstest};
    use serde_json::json;
    use serde_json::value::RawValue;

    use std::io;

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error(transparent)]
        Engine(#[from] EngineError),
        #[error(transparent)]
        Json(#[from] serde_json::Error),
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error("Test failure: {0}")]
        Failure(String),
    }

    // A `flake.lock` with a single non-root `nixpkgs` input pinned to `abc1234`.
    #[fixture]
    fn flake_lock_json() -> String {
        json!({
            "nodes": {
                "nixpkgs": {
                    "locked": {
                        "type": "github",
                        "owner": "NixOS",
                        "repo": "nixpkgs",
                        "rev": "abc1234",
                        "narHash": "sha256-abc...",
                        "lastModified": 1_700_000_000
                    },
                    "original": {
                        "type": "github",
                        "owner": "NixOS",
                        "repo": "nixpkgs",
                        "ref": "nixos-unstable"
                    }
                },
                "root": {
                    "inputs": {"nixpkgs": "nixpkgs"}
                }
            },
            "root": "root",
            "version": 7
        })
        .to_string()
    }

    // A `flake.lock` with a single non-root `nixpkgs` input, pinned to `rev` via the
    // `github:NixOS/nixpkgs` source.
    #[fixture]
    fn nixpkgs_flake_json(#[default("abc1234")] rev: &str) -> String {
        json!({
            "nodes": {
                "nixpkgs": {
                    "locked": {
                        "type": "github",
                        "owner": "NixOS",
                        "repo": "nixpkgs",
                        "rev": rev
                    }
                },
                "root": {"inputs": {"nixpkgs": "nixpkgs"}}
            },
            "root": "root",
            "version": 7
        })
        .to_string()
    }

    // Tests for [`FlakeLockSummarizationStage::run`].
    mod stage {
        use super::*;

        use std::process::Command;

        /// Runs [`FlakeLockSummarizationStage`] against a synthetic `PreToolUse` `Read` of
        /// `tool_input_json` (e.g. `{"file_path": "/tmp/x/flake.lock"}`).
        fn run_stage_with_tool_input(
            tool_input_json: &str,
        ) -> Result<Option<HookOutputEvent>, TestError> {
            let stage = FlakeLockSummarizationStage;
            let tool_input = RawValue::from_string(tool_input_json.to_owned())?;

            let mut conn = Conn {
                session: SessionMeta {
                    session_id: Cow::Borrowed("session-1"),
                    transcript_path: None,
                    cwd: None,
                    timestamp: None,
                    driver: Cow::Borrowed("Test"),
                    driver_meta: None,
                    permission_mode: None,
                    effort: None,
                    agent_id: None,
                    agent_type: None,
                },
                event: HookInputEvent::PreToolUse(PreToolUseInput {
                    tool_name: Cow::Borrowed("Read"),
                    tool_input: RawJson(&tool_input),
                    mcp_context: None,
                    original_request_name: None,
                }),
            };

            Ok(stage.run(&mut conn)?)
        }

        /// Runs [`FlakeLockSummarizationStage`] against a synthetic `PreToolUse` `Read` of `file_path`.
        fn run_stage(file_path: &str) -> Result<Option<HookOutputEvent>, TestError> {
            run_stage_with_tool_input(&json!({"file_path": file_path}).to_string())
        }

        /// Writes `content` to `<dir>/flake.lock` and returns its path as a `String`.
        fn write_flake_lock(dir: &Path, content: &str) -> Result<String, TestError> {
            let file_path = dir.join(FLAKE_LOCK_FILE_NAME);

            fs::write(&file_path, content)?;

            file_path
                .to_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| TestError::Failure("non-utf8 temp path".into()))
        }

        /// Runs `git -C <dir> <args>`, returning an error if it doesn't exit successfully.
        fn run_git(dir: &Path, args: &[&str]) -> Result<(), TestError> {
            let status = Command::new("git").arg("-C").arg(dir).args(args).status()?;

            if status.success() {
                Ok(())
            } else {
                Err(TestError::Failure(format!("git {args:?} failed: {status}")))
            }
        }

        /// Initializes `dir` as a git repository configured for committing in tests.
        fn init_git_repo(dir: &Path) -> Result<(), TestError> {
            run_git(dir, &["init", "--quiet"])?;
            run_git(dir, &["config", "user.email", "test@example.com"])?;
            run_git(dir, &["config", "user.name", "Test"])?;
            run_git(dir, &["config", "commit.gpgsign", "false"])
        }

        /// Stages and commits all changes in `dir`.
        fn commit_all(dir: &Path, message: &str) -> Result<(), TestError> {
            run_git(dir, &["add", "-A"])?;
            run_git(dir, &["commit", "--quiet", "-m", message])
        }

        #[rstest]
        fn denies_flake_lock_read_with_summary(flake_lock_json: String) -> Result<(), TestError> {
            let dir = tempfile::tempdir()?;
            let file_path = write_flake_lock(dir.path(), &flake_lock_json)?;

            let output = run_stage(&file_path)?
                .ok_or_else(|| TestError::Failure("expected a hook output".into()))?;

            let HookOutputEvent::PreToolUse(pre) = output else {
                return Err(TestError::Failure("expected a PreToolUse output".into()));
            };

            assert_eq!(pre.decision, Some(Decision::Deny));

            let reason = pre
                .reason
                .ok_or_else(|| TestError::Failure("missing reason".into()))?;

            assert!(reason.starts_with(
                "flake.lock read blocked \u{2014} use this summary instead (1 inputs):"
            ));
            assert!(reason.contains("nixpkgs: NixOS/nixpkgs@abc1234"));

            Ok(())
        }

        #[rstest]
        fn returns_none_when_file_path_missing_from_tool_input() -> Result<(), TestError> {
            assert!(run_stage_with_tool_input("{}")?.is_none());
            Ok(())
        }

        #[rstest]
        fn returns_none_for_non_flake_lock_file() -> Result<(), TestError> {
            assert!(run_stage("/tmp/somewhere/flake.json")?.is_none());
            Ok(())
        }

        #[rstest]
        #[case::missing_file(None)]
        #[case::invalid_json(Some("not json".to_owned()))]
        #[case::empty_nodes(Some(json!({"nodes": {}}).to_string()))]
        #[case::root_only(Some(json!({"nodes": {"root": {"inputs": {}}}}).to_string()))]
        fn returns_none_for_non_summarizable_flake_lock(
            #[case] content: Option<String>,
        ) -> Result<(), TestError> {
            let dir = tempfile::tempdir()?;

            let file_path = match content {
                Some(content) => write_flake_lock(dir.path(), &content)?,
                None => dir
                    .path()
                    .join(FLAKE_LOCK_FILE_NAME)
                    .to_str()
                    .ok_or_else(|| TestError::Failure("non-utf8 temp path".into()))?
                    .to_owned(),
            };

            assert!(run_stage(&file_path)?.is_none());

            Ok(())
        }

        #[rstest]
        fn reports_changed_revisions_against_head(
            #[from(nixpkgs_flake_json)]
            #[with("1111111111111111111111111111111111111111")]
            old_json: String,
            #[from(nixpkgs_flake_json)]
            #[with("2222222222222222222222222222222222222222")]
            new_json: String,
        ) -> Result<(), TestError> {
            let dir = tempfile::tempdir()?;
            init_git_repo(dir.path())?;

            let file_path = write_flake_lock(dir.path(), &old_json)?;
            commit_all(dir.path(), "initial")?;

            fs::write(&file_path, new_json)?;

            let output = run_stage(&file_path)?
                .ok_or_else(|| TestError::Failure("expected a hook output".into()))?;

            let HookOutputEvent::PreToolUse(pre) = output else {
                return Err(TestError::Failure("expected a PreToolUse output".into()));
            };

            let reason = pre
                .reason
                .ok_or_else(|| TestError::Failure("missing reason".into()))?;

            assert!(reason.starts_with(
                "flake.lock read blocked \u{2014} use this summary instead (1 inputs, 1 changed vs HEAD):"
            ));
            assert!(reason.contains("nixpkgs: NixOS/nixpkgs@1111111 -> 2222222"));

            Ok(())
        }
    }

    // Tests for [`FlakeLock`] deserialization and [`FlakeLock::diff`].
    mod flake_lock {
        use super::*;

        #[rstest]
        fn deserialization_keeps_all_nodes(flake_lock_json: String) -> Result<(), TestError> {
            let lock: FlakeLock<'_> = serde_json::from_str(&flake_lock_json)?;
            assert_eq!(lock.nodes.len(), 2);
            Ok(())
        }

        #[rstest]
        fn deserialization_ignores_inputs_and_original(
            flake_lock_json: String,
        ) -> Result<(), TestError> {
            let lock: FlakeLock<'_> = serde_json::from_str(&flake_lock_json)?;

            let root = lock
                .nodes
                .get("root")
                .ok_or_else(|| TestError::Failure("missing root node".into()))?;

            assert!(root.locked.is_none());

            Ok(())
        }

        #[rstest]
        fn deserialization_defaults_to_empty_without_nodes() -> Result<(), TestError> {
            let lock: FlakeLock<'_> = serde_json::from_str("{}")?;
            assert!(lock.nodes.is_empty());
            Ok(())
        }

        // Tests for [`FlakeLock::diff`].
        mod diff {
            use super::*;

            #[rstest]
            fn skips_root_node(flake_lock_json: String) -> Result<(), TestError> {
                let lock: FlakeLock<'_> = serde_json::from_str(&flake_lock_json)?;
                let entries = lock.diff(&FlakeLock::default());

                let entry = entries
                    .first()
                    .ok_or_else(|| TestError::Failure("expected one entry".into()))?;

                assert_eq!(entries.len(), 1);
                assert_eq!(entry.name, "nixpkgs");
                assert_eq!(entry.label, "NixOS/nixpkgs");
                assert_eq!(entry.cur_rev, "abc1234");

                assert!(!entry.changed);

                Ok(())
            }

            #[rstest]
            fn detects_rev_change(
                #[from(nixpkgs_flake_json)]
                #[with("2222222")]
                current_json: String,
                #[from(nixpkgs_flake_json)]
                #[with("1111111")]
                head_json: String,
            ) -> Result<(), TestError> {
                let current: FlakeLock<'_> = serde_json::from_str(&current_json)?;
                let head: FlakeLock<'_> = serde_json::from_str(&head_json)?;
                let entries = current.diff(&head);

                let entry = entries
                    .first()
                    .ok_or_else(|| TestError::Failure("expected one entry".into()))?;

                assert_eq!(entries.len(), 1);
                assert_eq!(entry.old_rev, "1111111");
                assert_eq!(entry.cur_rev, "2222222");
                assert!(entry.changed);

                Ok(())
            }

            #[rstest]
            fn unchanged_when_rev_matches_head(
                nixpkgs_flake_json: String,
            ) -> Result<(), TestError> {
                let current: FlakeLock<'_> = serde_json::from_str(&nixpkgs_flake_json)?;
                let head: FlakeLock<'_> = serde_json::from_str(&nixpkgs_flake_json)?;
                let entries = current.diff(&head);

                let entry = entries
                    .first()
                    .ok_or_else(|| TestError::Failure("expected one entry".into()))?;

                assert_eq!(entries.len(), 1);
                assert!(!entry.changed);

                Ok(())
            }
        }
    }

    // Tests for [`LockedRef`] deserialization and [`LockedRef::label`].
    mod locked_ref {
        use super::*;

        #[rstest]
        fn deserialization_borrows_from_source() -> Result<(), TestError> {
            let flake_json =
                json!({"type":"github","owner":"NixOS","repo":"nixpkgs","rev":"abc1234"})
                    .to_string();
            let locked: LockedRef<'_> = serde_json::from_str(&flake_json)?;

            assert!(matches!(locked.node_type, Cow::Borrowed("github")));
            assert!(matches!(locked.owner, Cow::Borrowed("NixOS")));
            assert!(matches!(locked.rev, Cow::Borrowed("abc1234")));

            Ok(())
        }

        #[rstest]
        #[case::github(
            json!({"type":"github","owner":"NixOS","repo":"nixpkgs"}).to_string(),
            "NixOS/nixpkgs"
        )]
        #[case::gitlab(json!({"type":"gitlab","owner":"foo","repo":"bar"}).to_string(), "foo/bar")]
        #[case::git(
            json!({"type":"git","url":"https://example.com/repo.git"}).to_string(),
            "git:https://example.com/repo.git"
        )]
        #[case::tarball(
            json!({"type":"tarball","url":"https://example.com/x.tar.gz"}).to_string(),
            "tarball:https://example.com/x.tar.gz"
        )]
        #[case::path(json!({"type":"path","path":"/nix/store/abc"}).to_string(), "path:/nix/store/abc")]
        #[case::unknown(json!({"type":"mercurial"}).to_string(), "mercurial")]
        fn label_formats_by_node_type(
            #[case] json: String,
            #[case] expected: &str,
        ) -> Result<(), TestError> {
            let locked: LockedRef<'_> = serde_json::from_str(&json)?;
            assert_eq!(locked.label(), expected);
            Ok(())
        }
    }

    // Tests for [`short_rev`].
    mod short_rev {
        use super::*;

        #[rstest]
        #[case("", "(none)")]
        #[case("abc", "abc")]
        #[case("abc1234", "abc1234")]
        #[case("abcdef1234567890", "abcdef1")]
        fn truncates_to_seven_chars(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(short_rev(input), expected);
        }
    }

    // Tests for [`Summary`]'s `Display` impl.
    mod summary {
        use super::*;

        #[rstest]
        fn display_lists_unchanged_input_without_arrow() {
            let entries = vec![SummaryEntry {
                name: "nixpkgs".to_owned(),
                label: "NixOS/nixpkgs".to_owned(),
                cur_rev: "abc1234".to_owned(),
                old_rev: String::new(),
                changed: false,
            }];

            assert_eq!(
                Summary(&entries).to_string(),
                "flake.lock read blocked \u{2014} use this summary instead (1 inputs):\n  nixpkgs: NixOS/nixpkgs@abc1234"
            );
        }

        #[rstest]
        fn display_lists_changed_input_with_arrow_and_count() {
            let entries = vec![SummaryEntry {
                name: "nixpkgs".to_owned(),
                label: "NixOS/nixpkgs".to_owned(),
                cur_rev: "2222222".to_owned(),
                old_rev: "1111111".to_owned(),
                changed: true,
            }];

            assert_eq!(
                Summary(&entries).to_string(),
                "flake.lock read blocked \u{2014} use this summary instead (1 inputs, 1 changed vs HEAD):\n  nixpkgs: NixOS/nixpkgs@1111111 -> 2222222"
            );
        }
    }
}
