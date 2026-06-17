//! # Flake-Lock Parser Architecture
//!
//! Decodes `flake.lock` files into a zero-copy view of their `nodes` map and
//! provides a generic [`FlakeLock::diff`] over two parsed revisions.
//!
//! ## Core Design
//!
//! `flake.lock` files are large, dense JSON documents. Field values borrow
//! directly from the source buffer via `Cow<'a, str>` instead of allocating
//! a full `serde_json::Value` tree, so only the values a caller actually
//! asks for are copied.
//!
//! ## Flow
//!
//! 1. **Decode**: [`serde_json::from_str`] into [`FlakeLock`], retaining
//!    only the `nodes` map (the top-level `version` and `root` pointer are
//!    ignored).
//! 2. **Diff**: [`FlakeLock::diff`] compares two revisions' nodes by `rev`,
//!    producing one [`DiffEntry`] per non-root node.
//!
//! ## Edge Cases
//!
//! The root node (named `"root"`) carries no `locked` pin and is always
//! skipped by [`FlakeLock::diff`]. Callers decide what an empty node map or
//! empty diff result means for their use case (`inceptool-stages`'s
//! `FlakeLockSummarizationStage` treats both as "nothing to summarize").

use serde::Deserialize;

use std::borrow::Cow;
use std::collections::BTreeMap;

/// The synthetic root node present in every `flake.lock`, which carries no `locked` pin.
const ROOT_NODE_NAME: &str = "root";

/// Zero-copy view of the top-level structure of a `flake.lock` (format version 7).
///
/// Field values borrow directly from the source JSON via `Cow<'a, str>`. Only the
/// `nodes` map is retained; the top-level `version` and `root` pointer are ignored.
#[derive(Debug, Default, Deserialize)]
pub struct FlakeLock<'a> {
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

/// One non-root `flake.lock` input, as produced by [`FlakeLock::diff`].
#[derive(Debug)]
pub struct DiffEntry {
    /// The node's key in the `flake.lock`'s `nodes` map (e.g. `"nixpkgs"`).
    pub name: String,
    /// The formatted source, as produced by [`LockedRef::label`] (e.g.
    /// `NixOS/nixpkgs`, `git:<url>`, `path:<path>`).
    pub label: String,
    /// The pinned revision in the [`FlakeLock::diff`] receiver, or empty if unpinned.
    pub cur_rev: String,
    /// The pinned revision in [`FlakeLock::diff`]'s `head` argument, or empty if the
    /// input is new or absent from `head`.
    pub old_rev: String,
    /// `true` if [`Self::old_rev`] is non-empty and differs from [`Self::cur_rev`].
    pub changed: bool,
}

impl FlakeLock<'_> {
    /// Returns `true` if this lock has no nodes at all (not even `"root"`).
    #[must_use = "discards whether the parsed lock has any nodes"]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Compares this lock's nodes against `head`'s, producing one [`DiffEntry`] per
    /// non-root node. An entry's `changed` flag is set when `head` has a recorded
    /// revision for that node that differs from the current one.
    #[must_use = "returns the diff entries; discarding them loses the comparison"]
    pub fn diff(&self, head: &FlakeLock<'_>) -> Vec<DiffEntry> {
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

            entries.push(DiffEntry {
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

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::{fixture, rstest};
    use serde_json::json;

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error(transparent)]
        Json(#[from] serde_json::Error),
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

        #[rstest]
        fn is_empty_reflects_node_count(flake_lock_json: String) -> Result<(), TestError> {
            let lock: FlakeLock<'_> = serde_json::from_str(&flake_lock_json)?;
            assert!(!lock.is_empty());

            let empty: FlakeLock<'_> = serde_json::from_str("{}")?;
            assert!(empty.is_empty());

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
}
