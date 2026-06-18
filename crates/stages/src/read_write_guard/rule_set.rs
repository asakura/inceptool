//! The resolved rule index — see [`RuleSet`].

use super::Rule;
use super::glob::{GlobRule, GlobTarget, glob_specificity};
use super::rule::is_glob_pattern;
use crate::path_utils::basename;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::vec::IntoIter;

/// The actual rule index backing [`RuleSet`].
///
/// Exact filenames live in a `BTreeMap` (checked first — `O(log n)`, no
/// linear scan over the supported ecosystems, and unconditionally the most
/// specific possible match); glob/path patterns are compiled once up front
/// into a single list sorted by [`glob_specificity`] (most specific first)
/// and checked as a fallback linear scan in that order, so a narrower glob
/// always gets first refusal over a broader one it overlaps with. Each
/// entry carries a [`GlobRule::required_literal`] pre-filter so non-matching
/// entries are rejected with a `str::contains` check rather than a full
/// [`glob::Pattern::matches`] call. The scan itself is intentionally a
/// simple `Vec` rather than a secondary index (e.g. extension-keyed
/// buckets): the rule count is small (low hundreds at most) and
/// [`RuleSet::get`] runs once per CLI invocation, not in a hot loop, so
/// anything beyond the pre-filter isn't justified. `RuleSet` is cloned
/// exactly once in the whole codebase (at startup, to hand a `RuleSet` to
/// [`super::ReadWriteGuardStage::new`]), so a plain derived [`Clone`] —
/// deep-copying the `BTreeMap`/`Vec` — is cheap enough; no `Arc` needed.
#[derive(Debug, Clone, Default)]
struct RuleSetInner {
    exact: BTreeMap<Cow<'static, str>, Rule>,
    globs: Vec<GlobRule>,
}

/// Index of [`Rule`]s, built via [`FromIterator`]. See `RuleSetInner` (this
/// module's private inner type) for the actual structure.
#[derive(Debug, Clone, Default)]
pub struct RuleSet(RuleSetInner);

impl FromIterator<Rule> for RuleSet {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Rule>,
    {
        // Fold the input in order: each incoming rule first drops every
        // already-accumulated rule it subsumes (see `Rule::subsumes`), then
        // gets added itself. A same-pattern override (the common case) is
        // just the special case where a rule subsumes its own exact
        // pattern; a later, broader glob dropping an earlier, narrower rule
        // it now fully covers falls out of the same fold. Later items thus
        // always have override power over earlier ones — exactly what
        // layering later `inceptool.toml` rules on top of earlier ones (or
        // built-ins) needs — while the relative order of rules that never
        // conflict doesn't affect the result.
        let mut resolved: Vec<Rule> = Vec::new();

        for rule in iter {
            resolved.retain(|existing| !rule.subsumes(existing));
            resolved.push(rule);
        }

        let mut exact = BTreeMap::new();
        let mut globs = Vec::new();

        for rule in resolved {
            let filename = rule.filename().clone();

            if !is_glob_pattern(filename.as_ref()) {
                exact.insert(filename, rule);
                continue;
            }

            match glob::Pattern::new(filename.as_ref()) {
                Ok(compiled) => {
                    let target = if filename.contains('/') {
                        GlobTarget::FullPath
                    } else {
                        GlobTarget::Basename
                    };

                    globs.push(GlobRule::new(compiled, target, rule));
                }
                Err(error) => tracing::error!(
                    pattern = %filename,
                    %error,
                    "invalid read-write-guard glob pattern, ignoring rule"
                ),
            }
        }

        globs.sort_by(|a, b| {
            glob_specificity(a.rule().filename()).cmp(&glob_specificity(b.rule().filename()))
        });

        Self(RuleSetInner { exact, globs })
    }
}

impl RuleSet {
    /// Looks up the [`Rule`] guarding `file_path`: tries an exact basename
    /// match first (unconditionally the most specific possible match), then
    /// scans the glob/path patterns in most-specific-first order (see
    /// `glob_specificity`) — so when more than one pattern matches the
    /// same file, the narrowest one wins, letting a user's specific
    /// override actually take effect over a broader rule it overlaps with.
    /// Returns the matched rule alongside the name to use in user-facing
    /// messages — the basename for exact/basename-glob matches, or the full
    /// `file_path` for a path-glob match (so e.g. a `**/node_modules/**`
    /// hit reports the actual nested file rather than an ambiguous
    /// basename like `package.json`).
    #[must_use = "returns the matching rule and display name; has no side effects"]
    pub fn get<'a>(&self, file_path: &'a str) -> Option<(&Rule, &'a str)> {
        let inner = &self.0;
        // `file_path` itself (rather than e.g. an empty string) is the
        // correct fallback for the rare paths with no well-defined basename
        // (see `super::path_utils::basename`'s doc, e.g. a trailing `/`):
        // an exact/basename-glob match then simply fails to match it (the
        // common case for such paths), and a path-glob entry still gets the
        // real path to match against either way.
        let basename = basename(file_path).unwrap_or(file_path);

        if let Some(rule) = inner.exact.get(basename) {
            return Some((rule, basename));
        }

        for glob_rule in &inner.globs {
            let candidate = match glob_rule.target() {
                GlobTarget::Basename => basename,
                GlobTarget::FullPath => file_path,
            };

            if !glob_rule.required_literal().is_empty()
                && !candidate.contains(glob_rule.required_literal())
            {
                continue;
            }

            if glob_rule.pattern().matches(candidate) {
                return Some((glob_rule.rule(), candidate));
            }
        }

        None
    }

    /// Iterates over every rule in the set, in the *opposite* of
    /// [`RuleSet::get`]'s match-priority order: least-specific glob first,
    /// most-specific (including every exact basename) last. This is for
    /// enumeration/serialization (the binary's config layer uses it to
    /// rebuild a serializable `RawConfig` from a resolved
    /// [`super::ReadWriteGuardStage`]'s rules) rather than matching, so it
    /// intentionally doesn't track `get()`'s order — no caller depends on a
    /// particular sequence here, only on every rule being present once.
    #[must_use = "returns an iterator over the rule set's rules; has no side effects"]
    pub fn iter(&self) -> impl Iterator<Item = &Rule> {
        let inner = &self.0;
        inner
            .globs
            .iter()
            .rev()
            .map(GlobRule::rule)
            .chain(inner.exact.values())
    }
}

impl IntoIterator for RuleSet {
    type Item = Rule;
    type IntoIter = IntoIter<Rule>;

    /// Owned counterpart to [`RuleSet::iter`]: consumes the set and yields
    /// every [`Rule`] by value, in the same (match-priority-reversed) order
    /// — for callers that need to move rules into another owned collection
    /// (e.g. merging two rule sets) without cloning each one. `GlobRule` is
    /// private, so unlike [`RuleSet::iter`] this can't return a borrowed
    /// `Chain` over it directly; building a `Vec` first keeps the
    /// associated type a plain, nameable `IntoIter<Rule>` instead of
    /// leaking that private type into the public signature.
    fn into_iter(self) -> Self::IntoIter {
        let RuleSetInner { exact, globs } = self.0;

        let mut rules: Vec<Rule> = Vec::with_capacity(globs.len().saturating_add(exact.len()));
        rules.extend(globs.into_iter().rev().map(GlobRule::into_rule));
        rules.extend(exact.into_values());

        rules.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::super::Access;
    use super::*;

    use rstest::rstest;

    use core::assert_matches;
    use std::{collections::BTreeSet, iter::once};

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error("Test failure: {0}")]
        Failure(String),
    }

    /// A small fixture [`RuleSet`] for exercising [`RuleSet::get`] directly.
    fn fixture_rules() -> RuleSet {
        [
            Rule::new(
                Cow::Borrowed("flake.lock"),
                Access::DenyAll {
                    hint: Cow::Borrowed("Run `nix flake update` to update it."),
                    note: Cow::Borrowed("(NOTE: updates ALL flake inputs)"),
                },
            ),
            Rule::new(
                Cow::Borrowed("Cargo.lock"),
                Access::DenyAll {
                    hint: Cow::Borrowed("Run `cargo update` to update it."),
                    note: Cow::Borrowed("(NOTE: updates ALL Rust dependencies)"),
                },
            ),
        ]
        .into_iter()
        .collect()
    }

    #[rstest]
    #[case::flake_lock("flake.lock")]
    #[case::cargo_lock("Cargo.lock")]
    fn rules_recognize_fixture_lockfiles(#[case] file_name: &str) {
        assert!(fixture_rules().get(file_name).is_some());
    }

    #[rstest]
    fn rules_return_none_for_unknown_file() {
        assert!(fixture_rules().get("main.rs").is_none());
    }

    mod iter {
        use super::*;

        #[rstest]
        fn yields_every_exact_rule_once() {
            let rules = fixture_rules();
            let filenames: BTreeSet<&str> =
                rules.iter().map(|rule| rule.filename().as_ref()).collect();

            assert_eq!(filenames, BTreeSet::from(["flake.lock", "Cargo.lock"]));
        }

        #[rstest]
        fn yields_basename_and_path_glob_rules_too() {
            let rules: RuleSet = [
                Rule::new(Cow::Borrowed("Cargo.lock"), Access::DenyRead),
                Rule::new(
                    Cow::Borrowed("*.pb.go"),
                    Access::DenyWrite {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                ),
                Rule::new(Cow::Borrowed("**/node_modules/**"), Access::DenyRead),
            ]
            .into_iter()
            .collect();

            let filenames: BTreeSet<&str> =
                rules.iter().map(|rule| rule.filename().as_ref()).collect();

            assert_eq!(
                filenames,
                BTreeSet::from(["Cargo.lock", "*.pb.go", "**/node_modules/**"])
            );
        }
    }

    mod glob_patterns {
        use super::*;

        /// A small fixture [`RuleSet`] mixing a basename glob and a path
        /// glob, for exercising [`RuleSet::get`]'s non-exact match paths.
        fn fixture_rules() -> RuleSet {
            [
                Rule::new(
                    Cow::Borrowed("*.pb.go"),
                    Access::DenyWrite {
                        hint: Cow::Borrowed("Regenerate from the .proto file."),
                        note: Cow::Borrowed("(NOTE: fully regenerated on every protoc run)"),
                    },
                ),
                Rule::new(
                    Cow::Borrowed("**/node_modules/**"),
                    Access::DenyAll {
                        hint: Cow::Borrowed("Run `npm install` to manage dependencies."),
                        note: Cow::Borrowed("(NOTE: regenerated entirely by the package manager)"),
                    },
                ),
            ]
            .into_iter()
            .collect()
        }

        #[rstest]
        fn matches_basename_glob_anywhere() -> Result<(), TestError> {
            let rules = fixture_rules();

            let (_, display_name) = rules
                .get("/repo/api/service.pb.go")
                .ok_or_else(|| TestError::Failure("expected a glob match".into()))?;

            assert_eq!(display_name, "service.pb.go");

            Ok(())
        }

        #[rstest]
        fn basename_glob_does_not_match_unrelated_file() {
            assert!(fixture_rules().get("/repo/service.go").is_none());
        }

        #[rstest]
        fn matches_path_glob_anywhere_in_tree() -> Result<(), TestError> {
            let rules = fixture_rules();
            let path = "/repo/node_modules/lodash/index.js";

            let (_, display_name) = rules
                .get(path)
                .ok_or_else(|| TestError::Failure("expected a glob match".into()))?;

            assert_eq!(display_name, path);

            Ok(())
        }

        #[rstest]
        fn path_glob_requires_a_real_path_component_match() {
            // "node_modules" appearing as part of a different component
            // name must not match `**/node_modules/**` — matching is
            // component-aware, not a substring search.
            assert!(
                fixture_rules()
                    .get("/repo/src/node_modules_helper.rs")
                    .is_none()
            );
        }

        #[rstest]
        fn later_duplicate_glob_pattern_overrides_earlier() -> Result<(), TestError> {
            let rules: RuleSet = [
                Rule::new(Cow::Borrowed("*.pb.go"), Access::DenyRead),
                Rule::new(
                    Cow::Borrowed("*.pb.go"),
                    Access::DenyWrite {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                ),
            ]
            .into_iter()
            .collect();

            let (rule, _) = rules
                .get("/repo/service.pb.go")
                .ok_or_else(|| TestError::Failure("expected a glob match".into()))?;

            assert_matches!(rule.access(), Access::DenyWrite { .. });

            Ok(())
        }

        #[rstest]
        fn malformed_glob_pattern_is_dropped_not_panicking() {
            let rules: RuleSet =
                once(Rule::new(Cow::Borrowed("[invalid"), Access::DenyRead)).collect();

            assert!(rules.get("[invalid").is_none());
        }

        #[rstest]
        fn basename_glob_takes_precedence_over_overlapping_path_glob() -> Result<(), TestError> {
            // A real-world overlap from base.toml: a `.pyc` file living
            // inside a `__pycache__` directory matches both a basename glob
            // (`*.pyc`) and a path glob (`**/__pycache__/**`). Neither
            // subsumes the other (glob-vs-glob containment isn't computed —
            // see `Rule::subsumes`), so both survive; the narrower basename
            // glob ranks more specific (see `glob_specificity`) and must
            // still win regardless of how the two pattern strings happen to
            // sort.
            let rules: RuleSet = [
                Rule::new(
                    Cow::Borrowed("*.pyc"),
                    Access::DenyWrite {
                        hint: Cow::Borrowed("<basename-hint>"),
                        note: Cow::Borrowed("<basename-note>"),
                    },
                ),
                Rule::new(Cow::Borrowed("**/__pycache__/**"), Access::DenyRead),
            ]
            .into_iter()
            .collect();

            let (rule, display_name) = rules
                .get("/repo/pkg/__pycache__/foo.pyc")
                .ok_or_else(|| TestError::Failure("expected a glob match".into()))?;

            assert_matches!(rule.access(), Access::DenyWrite { .. });
            assert_eq!(display_name, "foo.pyc");

            Ok(())
        }

        #[rstest]
        fn character_class_pattern_still_matches_without_pre_filter() {
            // `[12]` makes `required_literal` give up its pre-filter (see
            // that function's docs) — confirm matching still falls through
            // to `glob::Pattern::matches` correctly rather than silently
            // never matching.
            let rules: RuleSet =
                once(Rule::new(Cow::Borrowed("file[12].txt"), Access::DenyRead)).collect();

            assert!(rules.get("file1.txt").is_some());
            assert!(rules.get("file3.txt").is_none());
        }
    }

    mod subsumption {
        use super::*;

        #[rstest]
        fn later_broader_glob_drops_earlier_exact_rule() -> Result<(), TestError> {
            let rules: RuleSet = [
                Rule::new(Cow::Borrowed("Cargo.lock"), Access::DenyRead),
                Rule::new(
                    Cow::Borrowed("*.lock"),
                    Access::DenyWrite {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                ),
            ]
            .into_iter()
            .collect();

            let (rule, _) = rules
                .get("Cargo.lock")
                .ok_or_else(|| TestError::Failure("expected the broader glob to match".into()))?;

            assert_matches!(rule.access(), Access::DenyWrite { .. });

            Ok(())
        }

        #[rstest]
        fn reasserted_exact_rule_takes_precedence_again() -> Result<(), TestError> {
            let rules: RuleSet = [
                Rule::new(
                    Cow::Borrowed("*.lock"),
                    Access::DenyWrite {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                ),
                Rule::new(Cow::Borrowed("Cargo.lock"), Access::DenyRead),
            ]
            .into_iter()
            .collect();

            let (rule, _) = rules
                .get("Cargo.lock")
                .ok_or_else(|| TestError::Failure("expected the reasserted exact rule".into()))?;

            assert_matches!(rule.access(), Access::DenyRead);

            Ok(())
        }

        #[rstest]
        fn nonoverlapping_glob_rules_both_survive() -> Result<(), TestError> {
            let rules: RuleSet = [
                Rule::new(
                    Cow::Borrowed("*.pyc"),
                    Access::DenyWrite {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                ),
                Rule::new(Cow::Borrowed("**/__pycache__/**"), Access::DenyRead),
            ]
            .into_iter()
            .collect();

            let (pyc_rule, _) = rules
                .get("/repo/x.pyc")
                .ok_or_else(|| TestError::Failure("expected the basename glob to match".into()))?;
            let (dir_rule, _) = rules
                .get("/repo/__pycache__/data.bin")
                .ok_or_else(|| TestError::Failure("expected the path glob to match".into()))?;

            assert_matches!(pyc_rule.access(), Access::DenyWrite { .. });
            assert_matches!(dir_rule.access(), Access::DenyRead);

            Ok(())
        }

        #[rstest]
        #[case::filesystem_root("/Cargo.lock")]
        #[case::no_separator_at_all("Cargo.lock")]
        fn zero_depth_path_glob_drops_bare_exact_rule(
            #[case] file_path: &str,
        ) -> Result<(), TestError> {
            // A leading `**/` matches zero or more directories, so
            // `"**/Cargo.lock"` subsumes a root-level `"Cargo.lock"` too,
            // not just nested copies — both a `"/Cargo.lock"` path
            // (filesystem root) and a bare `"Cargo.lock"` with no separator
            // at all (see `required_literal`'s doc for why the prefilter
            // doesn't reject the latter).
            let rules: RuleSet = [
                Rule::new(Cow::Borrowed("Cargo.lock"), Access::DenyRead),
                Rule::new(
                    Cow::Borrowed("**/Cargo.lock"),
                    Access::DenyWrite {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                ),
            ]
            .into_iter()
            .collect();

            let (rule, _) = rules
                .get(file_path)
                .ok_or_else(|| TestError::Failure("expected the path glob to match".into()))?;

            assert_matches!(rule.access(), Access::DenyWrite { .. });

            Ok(())
        }

        #[rstest]
        fn bare_double_star_glob_drops_every_exact_rule() -> Result<(), TestError> {
            // With no `/` in the pattern at all, `"**"` is classified as a
            // basename glob and behaves like a single `*` — it matches any
            // basename, so it subsumes every exact rule already present,
            // not just the ones it was presumably meant to cover.
            let rules: RuleSet = [
                Rule::new(
                    Cow::Borrowed("Cargo.lock"),
                    Access::DenyAll {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                ),
                Rule::new(
                    Cow::Borrowed("id_rsa"),
                    Access::DenyAll {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                ),
                Rule::new(Cow::Borrowed("**"), Access::DenyRead),
            ]
            .into_iter()
            .collect();

            let (cargo_rule, _) = rules
                .get("Cargo.lock")
                .ok_or_else(|| TestError::Failure("expected the catch-all glob to match".into()))?;
            let (key_rule, _) = rules
                .get("id_rsa")
                .ok_or_else(|| TestError::Failure("expected the catch-all glob to match".into()))?;

            assert_matches!(cargo_rule.access(), Access::DenyRead);
            assert_matches!(key_rule.access(), Access::DenyRead);

            Ok(())
        }

        #[rstest]
        fn subsumption_is_case_sensitive() -> Result<(), TestError> {
            // `"*.lock"` does not match `"FOO.LOCK"` (different case), so it
            // must not subsume an exact rule for that literal name — both
            // survive and are matched independently.
            let rules: RuleSet = [
                Rule::new(
                    Cow::Borrowed("FOO.LOCK"),
                    Access::DenyAll {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                ),
                Rule::new(Cow::Borrowed("*.lock"), Access::DenyRead),
            ]
            .into_iter()
            .collect();

            let (exact_rule, _) = rules
                .get("FOO.LOCK")
                .ok_or_else(|| TestError::Failure("expected the exact rule to survive".into()))?;
            let (glob_rule, _) = rules.get("bar.lock").ok_or_else(|| {
                TestError::Failure("expected the glob to match a different file".into())
            })?;

            assert_matches!(exact_rule.access(), Access::DenyAll { .. });
            assert_matches!(glob_rule.access(), Access::DenyRead);

            Ok(())
        }

        #[rstest]
        fn exact_rule_never_subsumes_a_slash_containing_pattern() {
            // `"vendor/lib.rs"` has no wildcard characters but does contain
            // `/`, so it's classified as a (literal) path glob, not an
            // exact pattern — an exact `"lib.rs"` rule can never subsume it
            // (only a glob can subsume anything), regardless of which is
            // defined first, so both survive in the set. Checked via
            // `iter()` rather than `get()`: any candidate whose full path
            // literally matches `"vendor/lib.rs"` necessarily has basename
            // `"lib.rs"` too, so `get()`'s basename-first exact lookup would
            // always intercept it before the path glob is ever reached —
            // that's an orthogonal, intentional property of lookup order,
            // not what this test is about.
            let rules: RuleSet = [
                Rule::new(
                    Cow::Borrowed("vendor/lib.rs"),
                    Access::DenyWrite {
                        hint: Cow::Borrowed("<hint>"),
                        note: Cow::Borrowed("<note>"),
                    },
                ),
                Rule::new(Cow::Borrowed("lib.rs"), Access::DenyRead),
            ]
            .into_iter()
            .collect();

            let filenames: BTreeSet<&str> =
                rules.iter().map(|rule| rule.filename().as_ref()).collect();

            assert_eq!(filenames, BTreeSet::from(["vendor/lib.rs", "lib.rs"]));
        }
    }
}
