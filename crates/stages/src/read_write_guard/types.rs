//! The guarded-file rule shape consumed by [`super::ReadWriteGuardStage`].

use serde::Deserialize;

use std::borrow::Cow;
use std::collections::BTreeMap;

/// Access policy for a guarded file, driving how [`super::ReadWriteGuardStage`]
/// responds to reads and writes of it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Deny both reads and writes. Reads get a flat, no-frills reason
    /// pointing at `git diff`; writes get `hint` and `note`. The policy used
    /// by all built-in rules.
    No {
        /// The terminal command suggested when a write is denied.
        hint: Cow<'static, str>,
        /// Additional caveats about the scope of `hint` (e.g. "updates ALL
        /// packages").
        note: Cow<'static, str>,
    },
    /// Like [`Access::No`], but reads are intended to get a richer
    /// diff/summary instead of the flat reason, once diff-content generation
    /// exists. Currently degrades to [`Access::No`]'s read behavior.
    /// Reserved for a future upgrade of any rule, built-in or
    /// user-supplied.
    Diff {
        /// The terminal command suggested when a write is denied.
        hint: Cow<'static, str>,
        /// Additional caveats about the scope of `hint`.
        note: Cow<'static, str>,
    },
    /// Deny writes only (with `hint` and `note`); reads pass through
    /// untouched.
    Write {
        /// The terminal command suggested when a write is denied.
        hint: Cow<'static, str>,
        /// Additional caveats about the scope of `hint`.
        note: Cow<'static, str>,
    },
    /// Deny reads only (flat reason); writes pass through untouched.
    Read,
}

/// A guarded-file rule: how [`super::ReadWriteGuardStage`] responds to reads
/// and writes of files matching its `filename` pattern.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Rule {
    /// The pattern this rule matches, in one of three forms (see
    /// [`RuleSet`] for how each is matched):
    /// * an exact basename (`"Cargo.lock"`)
    /// * a basename glob, i.e. no `/` but glob metacharacters (`"*.pb.go"`)
    /// * a full-path glob, i.e. containing `/` (`"**/node_modules/**"`)
    filename: Cow<'static, str>,
    /// Which operations are intercepted, and how.
    access: Access,
}

impl Rule {
    /// Builds a rule matching `filename` with the given `access` policy.
    #[must_use = "constructs a new rule; discarding it does nothing"]
    pub const fn new(filename: Cow<'static, str>, access: Access) -> Self {
        Self { filename, access }
    }

    /// The access policy this rule applies.
    #[must_use = "returns the access policy; has no side effects"]
    pub const fn access(&self) -> &Access {
        &self.access
    }
}

/// A compiled glob-pattern rule: matched against just the candidate path's
/// basename when `match_basename` is `true` (the original pattern had no
/// `/`, e.g. `"*.pb.go"`), or against the full path otherwise (the original
/// pattern contained `/`, e.g. `"**/node_modules/**"`).
#[derive(Debug, Clone)]
struct GlobRule {
    /// The compiled pattern.
    pattern: glob::Pattern,
    /// Whether to match against the basename (`true`) or the full path.
    match_basename: bool,
    /// The rule to apply on a match.
    rule: Rule,
}

/// Index of [`Rule`]s, built via [`FromIterator`].
///
/// Exact filenames live in a `BTreeMap` (checked first — `O(log n)`, no
/// linear scan over the supported ecosystems); glob/path patterns are
/// compiled once up front and checked in a fallback linear scan.
#[derive(Debug, Clone)]
pub struct RuleSet {
    exact: BTreeMap<Cow<'static, str>, Rule>,
    globs: Vec<GlobRule>,
}

/// Whether `pattern` needs glob compilation rather than a literal basename
/// match: it contains `/` (full-path pattern) or a glob metacharacter
/// (`*`, `?`, `[`) with no `/` (basename pattern).
fn is_glob_pattern(pattern: &str) -> bool {
    pattern.contains('/') || pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
}

impl FromIterator<Rule> for RuleSet {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = Rule>,
    {
        // Dedup/override by raw pattern string first, exactly like before —
        // this is what makes a later rule with the same `filename` (exact
        // or glob) replace an earlier one.
        let canonical: BTreeMap<Cow<'static, str>, Rule> = iter
            .into_iter()
            .map(|rule| (rule.filename.clone(), rule))
            .collect();

        let mut exact = BTreeMap::new();
        let mut globs = Vec::new();

        for (pattern, rule) in canonical {
            if !is_glob_pattern(&pattern) {
                exact.insert(pattern, rule);
                continue;
            }

            match glob::Pattern::new(&pattern) {
                Ok(compiled) => globs.push(GlobRule {
                    match_basename: !pattern.contains('/'),
                    pattern: compiled,
                    rule,
                }),
                Err(error) => tracing::error!(
                    pattern = %pattern,
                    %error,
                    "invalid read-write-guard glob pattern, ignoring rule"
                ),
            }
        }

        Self { exact, globs }
    }
}

impl RuleSet {
    /// Looks up the [`Rule`] guarding `file_path`: tries an exact basename
    /// match first, then basename globs (e.g. `*.pb.go`), then full-path
    /// globs (e.g. `**/node_modules/**`), in that order. Returns the
    /// matched rule alongside the name to use in user-facing messages — the
    /// basename for exact/basename-glob matches, or the full `file_path`
    /// for a path-glob match (so e.g. a `**/node_modules/**` hit reports
    /// the actual nested file rather than an ambiguous basename like
    /// `package.json`).
    #[must_use = "returns the matching rule and display name; has no side effects"]
    pub fn get<'a>(&self, file_path: &'a str) -> Option<(&Rule, &'a str)> {
        let basename = file_path.split('/').next_back().unwrap_or(file_path);

        if let Some(rule) = self.exact.get(basename) {
            return Some((rule, basename));
        }

        for glob_rule in &self.globs {
            let candidate = if glob_rule.match_basename {
                basename
            } else {
                file_path
            };

            if glob_rule.pattern.matches(candidate) {
                return Some((&glob_rule.rule, candidate));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    use core::assert_matches;

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
                Access::No {
                    hint: Cow::Borrowed("Run `nix flake update` to update it."),
                    note: Cow::Borrowed("(NOTE: updates ALL flake inputs)"),
                },
            ),
            Rule::new(
                Cow::Borrowed("Cargo.lock"),
                Access::No {
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

    mod glob_patterns {
        use super::*;

        use std::iter::once;

        /// A small fixture [`RuleSet`] mixing a basename glob and a path
        /// glob, for exercising [`RuleSet::get`]'s non-exact match paths.
        fn fixture_rules() -> RuleSet {
            [
                Rule::new(
                    Cow::Borrowed("*.pb.go"),
                    Access::Write {
                        hint: Cow::Borrowed("Regenerate from the .proto file."),
                        note: Cow::Borrowed("(NOTE: fully regenerated on every protoc run)"),
                    },
                ),
                Rule::new(
                    Cow::Borrowed("**/node_modules/**"),
                    Access::No {
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
                Rule::new(Cow::Borrowed("*.pb.go"), Access::Read),
                Rule::new(
                    Cow::Borrowed("*.pb.go"),
                    Access::Write {
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

            assert_matches!(rule.access(), Access::Write { .. });

            Ok(())
        }

        #[rstest]
        fn malformed_glob_pattern_is_dropped_not_panicking() {
            let rules: RuleSet = once(Rule::new(Cow::Borrowed("[invalid"), Access::Read)).collect();

            assert!(rules.get("[invalid").is_none());
        }
    }
}
