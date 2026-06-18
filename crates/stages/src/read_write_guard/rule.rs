//! The guarded-file rule shape — see [`Rule`].

use super::Access;

use std::borrow::Cow;

/// A guarded-file rule: how [`super::ReadWriteGuardStage`] responds to reads
/// and writes of files matching its `filename` pattern.
///
/// Like [`Access`], deliberately not `serde::Deserialize` — see that type's
/// doc for why the TOML-facing shape is a separate `RawRule` owned by the
/// consuming binary's config layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The pattern this rule matches, in one of three forms (see
    /// [`super::rule_set::RuleSet`] for how each is matched):
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

    /// The pattern this rule matches — see the struct doc for the three
    /// accepted forms.
    #[must_use = "returns the rule's filename pattern; has no side effects"]
    pub const fn filename(&self) -> &Cow<'static, str> {
        &self.filename
    }

    /// The access policy this rule applies.
    #[must_use = "returns the access policy; has no side effects"]
    pub const fn access(&self) -> &Access {
        &self.access
    }

    /// Consumes the rule, returning its pattern and access policy —
    /// the owned counterpart to [`Rule::filename`]/[`Rule::access`], for
    /// converting an owned `Rule` into another owned shape (e.g.
    /// `RawRule`) without cloning either field.
    #[must_use = "consumes the rule; discarding the result loses both fields"]
    pub fn into_parts(self) -> (Cow<'static, str>, Access) {
        (self.filename, self.access)
    }

    /// Whether `self`'s pattern matches every string `other`'s pattern can
    /// match — i.e. `other` is redundant once `self` is also present.
    /// True for identical patterns (the same-key override case: a rule
    /// always subsumes its own exact pattern), or when `self` is a glob and
    /// `other` is a non-glob exact pattern that `self`'s compiled glob
    /// matches (e.g. `"*.lock"` subsumes `"Cargo.lock"`).
    ///
    /// Glob-vs-glob containment is deliberately not computed: deciding
    /// whether one glob's match set is a superset of another's in general
    /// has no cheap procedure, and no built-in or known rule shape needs it
    /// — two distinct, non-identical glob patterns are never considered to
    /// subsume one another, even if their match sets happen to overlap
    /// (e.g. `"*.pyc"` and `"**/__pycache__/**"` both stay).
    #[must_use = "returns whether self's pattern subsumes other's; has no side effects"]
    pub fn subsumes(&self, other: &Self) -> bool {
        if self.filename == other.filename {
            return true;
        }

        is_glob_pattern(&self.filename)
            && !is_glob_pattern(&other.filename)
            && glob::Pattern::new(&self.filename)
                .is_ok_and(|pattern| pattern.matches(&other.filename))
    }
}

/// Whether `pattern` needs glob compilation rather than a literal basename
/// match: it contains `/` (full-path pattern) or a glob metacharacter
/// (`*`, `?`, `[`) with no `/` (basename pattern).
pub(super) fn is_glob_pattern(pattern: &str) -> bool {
    pattern.contains(['/', '*', '?', '['])
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    /// Builds a [`Rule`] with the given pattern and an arbitrary access
    /// policy — [`Rule::subsumes`] never inspects `access`, so every case
    /// below uses this to keep the pattern under test the only thing that
    /// varies.
    const fn rule(pattern: &'static str) -> Rule {
        Rule::new(Cow::Borrowed(pattern), Access::DenyRead)
    }

    mod subsumes {
        use super::*;

        #[rstest]
        #[case::identical_exact_patterns("Cargo.lock", "Cargo.lock")]
        #[case::identical_glob_patterns("*.pb.go", "*.pb.go")]
        fn identical_patterns_always_subsume(
            #[case] self_pattern: &'static str,
            #[case] other_pattern: &'static str,
        ) {
            assert!(rule(self_pattern).subsumes(&rule(other_pattern)));
        }

        #[rstest]
        fn glob_subsumes_a_matching_exact_pattern() {
            // The doc's own canonical example.
            assert!(rule("*.lock").subsumes(&rule("Cargo.lock")));
        }

        #[rstest]
        fn glob_does_not_subsume_a_nonmatching_exact_pattern() {
            // Both real base.toml entries — "*.pb.go" has nothing to do
            // with "go.sum".
            assert!(!rule("*.pb.go").subsumes(&rule("go.sum")));
        }

        #[rstest]
        #[case::different_exact_patterns("Cargo.lock", "yarn.lock")]
        #[case::exact_pattern_textually_containing_the_other("lib.rs", "vendor/lib.rs")]
        fn exact_self_never_subsumes_a_different_pattern(
            #[case] exact_self: &'static str,
            #[case] other: &'static str,
        ) {
            assert!(!rule(exact_self).subsumes(&rule(other)));
        }

        #[rstest]
        // Both real base.toml entries, called out in `subsumes`' own doc as
        // the canonical "overlap survives" example: a `.pyc` living inside
        // a `__pycache__` directory matches both patterns, but glob-vs-glob
        // containment is deliberately never computed, so neither subsumes
        // the other regardless of which is `self`.
        #[case::pyc_then_pycache_dir("*.pyc", "**/__pycache__/**")]
        #[case::pycache_dir_then_pyc("**/__pycache__/**", "*.pyc")]
        fn distinct_overlapping_globs_never_subsume_each_other(
            #[case] self_pattern: &'static str,
            #[case] other_pattern: &'static str,
        ) {
            assert!(!rule(self_pattern).subsumes(&rule(other_pattern)));
        }

        #[rstest]
        fn glob_self_never_subsumes_a_different_glob_other_even_if_it_would_match_as_text() {
            // "*" would textually match the literal string "*.pb.go", but
            // `other` being itself a glob pattern short-circuits the check
            // before `self`'s compiled pattern is ever consulted.
            assert!(!rule("*").subsumes(&rule("*.pb.go")));
        }

        #[rstest]
        fn malformed_self_glob_pattern_does_not_subsume_anything_or_panic() {
            assert!(!rule("[invalid").subsumes(&rule("Cargo.lock")));
        }

        #[rstest]
        fn subsumption_is_case_sensitive() {
            assert!(!rule("*.lock").subsumes(&rule("FOO.LOCK")));
        }
    }
}
