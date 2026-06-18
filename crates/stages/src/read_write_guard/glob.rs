//! Compiled glob/path-pattern rule entries — see [`GlobRule`].

use super::Rule;

use std::cmp::Reverse;

/// Which candidate string a [`GlobRule`] matches against, derived once from
/// whether its original pattern contained `/` (see [`super::rule::Rule`]'s
/// doc for the three accepted pattern forms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GlobTarget {
    /// Match against the candidate path's basename only.
    Basename,
    /// Match against the full candidate path.
    FullPath,
}

/// A compiled glob-pattern rule, paired with the [`Rule`] to apply on a
/// match and the [`GlobTarget`] it matches against. Stored in
/// `RuleSetInner::globs`, sorted by [`glob_specificity`] (most specific
/// first) — see that function for the ranking and
/// [`super::rule_set::RuleSet::get`] for how the order drives match
/// priority.
#[derive(Debug, Clone)]
pub(super) struct GlobRule {
    /// The compiled pattern.
    pattern: glob::Pattern,
    /// Pre-filter computed once by [`required_literal`] at construction;
    /// empty means "always run the full match" (see that function for when
    /// this happens).
    required_literal: Box<str>,
    /// Which candidate string this entry matches against.
    target: GlobTarget,
    /// The rule to apply on a match.
    rule: Rule,
}

impl GlobRule {
    /// Builds a compiled glob entry from its already-computed parts.
    #[must_use = "constructs a new glob entry; discarding it does nothing"]
    pub(super) fn new(pattern: glob::Pattern, target: GlobTarget, rule: Rule) -> Self {
        let filename = rule.filename().as_ref();
        let required_literal = required_literal(filename).into();

        Self {
            pattern,
            required_literal,
            target,
            rule,
        }
    }

    /// The compiled pattern.
    #[must_use = "returns the compiled pattern; has no side effects"]
    pub(super) const fn pattern(&self) -> &glob::Pattern {
        &self.pattern
    }

    /// The [`required_literal`] pre-filter for this entry.
    #[must_use = "returns the required-literal pre-filter; has no side effects"]
    pub(super) fn required_literal(&self) -> &str {
        &self.required_literal
    }

    /// Which candidate string this entry matches against.
    #[must_use = "returns the match target; has no side effects"]
    pub(super) const fn target(&self) -> GlobTarget {
        self.target
    }

    /// The rule to apply on a match.
    #[must_use = "returns the rule; has no side effects"]
    pub(super) const fn rule(&self) -> &Rule {
        &self.rule
    }

    /// Consumes the entry, returning its [`Rule`] — the owned counterpart to
    /// [`GlobRule::rule`], for moving a `Rule` out without cloning it.
    #[must_use = "consumes the entry; discarding the result loses the rule"]
    pub(super) fn into_rule(self) -> Rule {
        self.rule
    }
}

/// The longest run of literal (non-wildcard) characters in `pattern`, used
/// by [`super::rule_set::RuleSet::get`] as a cheap pre-filter: any string
/// the pattern can match must contain this substring verbatim, so a
/// `str::contains` check rules out the overwhelming majority of
/// non-matching candidates before paying for [`glob::Pattern::matches`].
///
/// Splits on `*`/`?` only — patterns containing a `[...]` character class
/// return `""` (no pre-filter, always run the full match) rather than
/// treating the class's contents as literal text, since `[ab]` matches a
/// single `a` or `b`, not the substring `"ab"`.
///
/// Strips any leading `**/` component(s) first: `glob`'s recursive
/// wildcard can match zero directories *and* the separator right after it
/// when it's the pattern's first component — `"**/Cargo.lock"` matches a
/// root-level `"Cargo.lock"` with no leading `/` at all — so a literal run
/// starting immediately after a leading `**/` can't count that `/` as
/// guaranteed to appear in every match. A trailing `/**` has no such
/// exemption (`"x/**"` does *not* match bare `"x"`), so it's left alone.
pub(super) fn required_literal(pattern: &str) -> &str {
    if pattern.contains('[') {
        return "";
    }

    let mut body = pattern;

    while let Some(rest) = body.strip_prefix("**/") {
        body = rest;
    }

    body.split(['*', '?'])
        .max_by_key(|run| run.len())
        .unwrap_or_default()
}

/// Specificity-ranking key for sorting `RuleSetInner::globs`: smaller is
/// more specific. Ranked by wildcard density — each `*` counts double
/// `?`/`[`, since `*` (and especially `**`) can match unboundedly more
/// text — tie-broken by a longer [`required_literal`] run (a more
/// constrained pattern matches fewer strings) and finally the raw pattern
/// string, for determinism. Exact (non-glob) patterns never go through
/// this: they're unconditionally more specific than any glob, and live in
/// `RuleSetInner::exact` instead.
pub(super) fn glob_specificity(pattern: &str) -> (usize, Reverse<usize>, &str) {
    let wildcard_score = pattern.chars().fold(0_usize, |score, c| {
        score.saturating_add(match c {
            '*' => 2,
            '?' | '[' => 1,
            _ => 0,
        })
    });

    (
        wildcard_score,
        Reverse(required_literal(pattern).len()),
        pattern,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    mod required_literal {
        use super::*;

        #[rstest]
        #[case::suffix_glob("*.pyc", ".pyc")]
        #[case::prefix_glob("npm-debug.log*", "npm-debug.log")]
        #[case::double_star_path_glob("**/node_modules/**", "node_modules/")]
        #[case::single_char_wildcard("?ile", "ile")]
        #[case::bare_wildcard_has_no_literal("*", "")]
        #[case::character_class_is_skipped("[ab]c", "")]
        #[case::leading_double_star_drops_separator("**/Cargo.lock", "Cargo.lock")]
        #[case::repeated_leading_double_star("**/**/x", "x")]
        fn extracts_expected_substring(#[case] pattern: &str, #[case] expected: &str) {
            assert_eq!(required_literal(pattern), expected);
        }
    }

    mod glob_specificity {
        use super::*;

        use std::cmp::Ordering;

        #[rstest]
        // Each pair can match the very same
        // real file (e.g. a `.pyc` living inside a `__pycache__` directory),
        // so the basename glob's narrower match shape must unconditionally
        // outrank the recursive path glob it overlaps with — regardless of
        // how much longer the path glob's `required_literal` run is (8/12
        // chars vs. 4/13 chars below).
        #[case::pyc_file_inside_pycache_dir("*.pyc", "**/__pycache__/**")]
        #[case::iml_file_inside_gradle_dir("*.iml", "**/.gradle/**")]
        #[case::xcuserstate_file_inside_xcuserdata_dir("*.xcuserstate", "**/xcuserdata/**")]
        #[case::generated_ts_file_inside_generated_dir("*.generated.ts", "**/__generated__/**")]
        #[case::tfplan_file_inside_terraform_dir("*.tfplan", "**/.terraform/**")]
        fn basename_glob_outranks_overlapping_path_glob(
            #[case] basename_glob: &str,
            #[case] path_glob: &str,
        ) {
            assert_eq!(
                glob_specificity(basename_glob).cmp(&glob_specificity(path_glob)),
                Ordering::Less
            );
        }

        #[rstest]
        // `*_grpc.pb.go` and `*.pb.go` both match
        // "service_grpc.pb.go" — same single-`*` wildcard score, so the
        // `required_literal`-length tie-break decides: the grpc variant's
        // longer literal run correctly makes it the more specific pattern.
        #[case::grpc_pb_go_vs_plain_pb_go("*_grpc.pb.go", "*.pb.go")]
        fn longer_required_literal_breaks_an_equal_wildcard_score_tie(
            #[case] longer_literal_glob: &str,
            #[case] shorter_literal_glob: &str,
        ) {
            assert_eq!(
                glob_specificity(longer_literal_glob).cmp(&glob_specificity(shorter_literal_glob)),
                Ordering::Less
            );
        }

        #[rstest]
        // Both pairs are patterns with identical wildcard
        // score *and* identical required-literal length (".git/"/".svn/"
        // are both 5 bytes; ".swo"/".swp" are both 4) — they never overlap
        // on a real file, so the final tie-break (the raw pattern string)
        // is purely for sort determinism, not because either is actually
        // "more specific" in any meaningful sense.
        #[case::git_vs_svn_cache_dir("**/.git/**", "**/.svn/**")]
        #[case::vim_swap_file_variants("*.swo", "*.swp")]
        fn equal_score_and_literal_length_falls_back_to_the_pattern_string(
            #[case] lexicographically_first: &str,
            #[case] lexicographically_second: &str,
        ) {
            assert_eq!(
                glob_specificity(lexicographically_first)
                    .cmp(&glob_specificity(lexicographically_second)),
                Ordering::Less
            );
        }

        #[rstest]
        // A `[...]` character class makes `required_literal` give up its
        // pre-filter entirely (see that function's doc), so `file[12].txt`
        // is scored as having *no* literal at all — even though `[12]` is
        // actually more constrained (one of two characters) than the bare
        // `?` in `?ile`, which keeps its "ile" literal. The heuristic gets
        // this synthetic pair backwards: `?ile` ranks more specific despite
        // matching strictly more strings, a known blind spot since no
        // built-in pattern uses a character class today (see
        // `required_literal`'s doc).
        #[case::question_mark_outranks_character_class("?ile", "file[12].txt")]
        fn character_class_pre_filter_loss_skews_ranking_below_a_lone_wildcard(
            #[case] question_mark_glob: &str,
            #[case] character_class_glob: &str,
        ) {
            assert_eq!(
                glob_specificity(question_mark_glob).cmp(&glob_specificity(character_class_glob)),
                Ordering::Less
            );
        }

        #[rstest]
        // Mixes basename globs (vim/protobuf/npm patterns) and path globs
        // (vcs cache directories), all scavenged from base.toml, into a
        // single sort to check the comparator's *overall* output order —
        // not just the pairwise relationships the cases above isolate.
        fn sorts_a_realistic_mix_of_patterns_into_the_documented_priority_order() {
            let mut patterns = [
                "*.pb.go",
                "**/.svn/**",
                "*.pyc",
                "npm-debug.log*",
                "**/__pycache__/**",
                "*_grpc.pb.go",
                "**/.git/**",
            ];

            patterns.sort_by_key(|&pattern| glob_specificity(pattern));

            assert_eq!(
                patterns,
                [
                    "npm-debug.log*",
                    "*_grpc.pb.go",
                    "*.pb.go",
                    "*.pyc",
                    "**/__pycache__/**",
                    "**/.git/**",
                    "**/.svn/**",
                ]
            );
        }
    }
}
