//! One command's baseline rating and the flag/combo/operand rules that escalate or mitigate it,
//! plus its subcommands — see [`Command`] and [`Subcommand`].

use super::{ComboRule, Flag, FlagGrammar, OperandRule, Platform, ProfilePatch};
use crate::error::RiskDataError;

use serde::Deserialize;

use std::collections::BTreeSet;
use std::iter;

/// `true` — [`Command::case_sensitive`]'s default, since most commands match flag spellings
/// case-sensitively.
const fn default_true() -> bool {
    true
}

/// One command's baseline rating and the flag/combo/operand rules that can escalate or mitigate
/// it — see [`crate::types::Dataset`].
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    /// The command's canonical name, as it appears as a `Statement::Command`'s name.
    pub name: Box<str>,
    /// Alternate names resolving to this same entry (e.g. `readarray` for `mapfile`).
    #[serde(default)]
    pub aliases: Vec<Box<str>>,
    /// Where this command comes from — provenance/documentation only, never read by
    /// classification logic.
    #[expect(
        dead_code,
        reason = "deserialized from every data file for human provenance only; deliberately \
                  never read back, so the field doesn't carry through to codegen output"
    )]
    pub kind: CommandKind,
    /// Which concrete implementation this declaration models. The same name may be declared
    /// more than once, each under a different `Platform` — see [`crate::types::Platform`].
    /// Defaults to [`Platform::GnuLinux`], so existing data files need no edits.
    #[serde(default)]
    pub platform: Platform,
    /// Which flag-syntax family this command's tokens follow. Defaults to [`FlagGrammar::Gnu`],
    /// reproducing today's one hardcoded tokenization scheme exactly.
    #[serde(default)]
    pub grammar: FlagGrammar,
    /// Whether flag spellings match case-sensitively. Defaults to `true`.
    #[serde(default = "default_true")]
    pub case_sensitive: bool,
    /// This command's rating with no flags considered. Any axis left unset here defaults to its
    /// lowest value (`TrustImpact::None`/`Reversibility::Reversible`/`BlastRadius::Narrow`/
    /// `Disclosure::None`/`Persistence::Ephemeral`/`Privilege::None`/`Auditability::Intact`/
    /// `Exposure::Contained`/`Verification::Checked`).
    #[serde(default)]
    pub baseline: ProfilePatch,
    /// Why the baseline is what it is.
    pub baseline_reason: Box<str>,
    /// Whether this command's multi-letter single-dash flags combine (`-ex` is `-e` plus `-x`)
    /// rather than being atomic single flags. Defaults to `false` — a command must opt in,
    /// since this is a real getopt-grammar fact about the command, not something inferrable
    /// from a flag's spelling alone. Meaningless (and rejected by [`Command::validate`]) when
    /// `grammar` is [`FlagGrammar::Go`], which never clusters.
    #[serde(default)]
    pub short_flags_combinable: bool,
    /// This command's individually rated flags, in its global scope (applies regardless of
    /// whether a subcommand was identified, or when this command declares none at all).
    #[serde(default)]
    pub flag: Vec<Flag>,
    /// Rules that escalate or mitigate based on more than one global-scope flag being present at
    /// once.
    #[serde(default)]
    pub combo_rule: Vec<ComboRule>,
    /// Rules matched against every literal positional argument, regardless of whether it's
    /// flag-shaped (e.g. `kill`'s `-1` pid operand).
    #[serde(default)]
    pub operand_rule: Vec<OperandRule>,
    /// This command's subcommands (`git push`, `docker run`, ...) — one level deep only. Each
    /// has its own independent flag/combo/operand ruleset, layered on top of (not replacing) the
    /// fields above.
    #[serde(default)]
    pub subcommands: Vec<Subcommand>,
}

/// Where a [`Command`] comes from. Provenance/documentation only — never read by classification
/// logic, which treats every dataset entry identically regardless of origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    /// A Bash builtin (`:`, `eval`, `kill`, ...).
    Builtin,
    /// An external program found on `PATH` (`rm`, `git`, `docker`, ...).
    External,
}

/// One subcommand's own baseline rating and flag/combo/operand rules (`push` on `git`, `run` on
/// `docker`) — see [`Command::subcommand`].
///
/// Its flag/combo/operand fields are an independent namespace from the parent [`Command`]'s
/// global scope: the same literal spelling can mean something different globally vs. inside one
/// subcommand. Not a `#[serde(flatten)]` of a shared shape with `Command`, since `flatten` is
/// incompatible with `#[serde(deny_unknown_fields)]`, which every struct in this crate uses as a
/// hard convention.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subcommand {
    /// The subcommand's canonical name, as it appears as the first literal argument after the
    /// parent command's own flags.
    pub name: Box<str>,
    /// Alternate names resolving to this same subcommand.
    #[serde(default)]
    pub aliases: Vec<Box<str>>,
    /// This subcommand's rating with no flags considered, independent of the parent command's
    /// own baseline.
    #[serde(default)]
    pub baseline: ProfilePatch,
    /// Why the baseline is what it is.
    pub baseline_reason: Box<str>,
    /// Whether this subcommand's multi-letter single-dash flags combine, independent of the
    /// parent command's own `short_flags_combinable`.
    #[serde(default)]
    pub short_flags_combinable: bool,
    /// This subcommand's individually rated flags.
    #[serde(default)]
    pub flag: Vec<Flag>,
    /// Rules that escalate or mitigate based on more than one of this subcommand's flags (or one
    /// of the parent's global flags) being present at once — see [`Command::validate`].
    #[serde(default)]
    pub combo_rule: Vec<ComboRule>,
    /// Rules matched against every literal positional argument once this subcommand has been
    /// identified.
    #[serde(default)]
    pub operand_rule: Vec<OperandRule>,
}

impl Command {
    /// This command's own internal consistency checks — see [`crate::types::Dataset::validate`].
    pub(crate) fn validate(&self) -> Result<(), RiskDataError> {
        if matches!(self.grammar, FlagGrammar::Go) && self.short_flags_combinable {
            return Err(RiskDataError::CombinableUnderGoGrammar {
                command: self.name.clone(),
            });
        }

        let global_spellings = validate_ruleset(
            &self.name,
            None,
            self.case_sensitive,
            &self.flag,
            &self.combo_rule,
            &self.operand_rule,
            &BTreeSet::new(),
        )?;

        let mut seen_subcommands = BTreeSet::new();

        for subcommand in &self.subcommands {
            let names = iter::once(subcommand.name.as_ref())
                .chain(subcommand.aliases.iter().map(AsRef::as_ref));

            for name in names {
                if !seen_subcommands.insert(name) {
                    return Err(RiskDataError::DuplicateSubcommand {
                        command: self.name.clone(),
                        name: name.into(),
                    });
                }
            }

            validate_ruleset(
                &self.name,
                Some(&subcommand.name),
                self.case_sensitive,
                &subcommand.flag,
                &subcommand.combo_rule,
                &subcommand.operand_rule,
                &global_spellings,
            )?;
        }

        Ok(())
    }
}

/// Whether `spellings` already holds an entry equal to `needle` under `case_sensitive`'s rule —
/// mirrors `inceptool_parable::risk`'s runtime `spelling_eq`, so a dataset accepted here behaves
/// identically at lookup time.
#[must_use = "checking membership has no effect unless the caller uses the result"]
fn contains_spelling(spellings: &BTreeSet<&str>, needle: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        spellings.contains(needle)
    } else {
        spellings
            .iter()
            .any(|spelling| spelling.eq_ignore_ascii_case(needle))
    }
}

/// One ruleset's (a [`Command`]'s global scope, or one of its [`Subcommand`]s) internal
/// consistency checks: no repeated flag spelling within this scope, every `value_rule`/
/// `operand_rule` pattern compiles, and every combo rule's `requires` is satisfiable by this
/// scope's own spellings plus `extra_spellings` (the parent's global spellings, when validating a
/// subcommand; empty when validating the global scope itself). `case_sensitive` is the owning
/// [`Command`]'s own flag, so spelling comparisons here agree with the runtime's
/// `spelling_eq`-based lookup. Returns this scope's own spelling set on success, for the caller to
/// pass as `extra_spellings` when validating its subcommands.
fn validate_ruleset<'a>(
    command: &str,
    subcommand: Option<&str>,
    case_sensitive: bool,
    flags: &'a [Flag],
    combo_rules: &[ComboRule],
    operand_rules: &[OperandRule],
    extra_spellings: &BTreeSet<&str>,
) -> Result<BTreeSet<&'a str>, RiskDataError> {
    let mut spellings = BTreeSet::new();

    for flag in flags {
        for spelling in &flag.spellings {
            if contains_spelling(&spellings, spelling.as_ref(), case_sensitive) {
                return Err(RiskDataError::DuplicateFlagSpelling {
                    command: command.into(),
                    subcommand: subcommand.map(Into::into),
                    spelling: spelling.clone(),
                });
            }

            spellings.insert(spelling.as_ref());
        }

        for value_rule in &flag.value_rule {
            value_rule.validate(command, subcommand)?;
        }
    }

    for combo_rule in combo_rules {
        for required in &combo_rule.requires {
            if !contains_spelling(&spellings, required.as_ref(), case_sensitive)
                && !contains_spelling(extra_spellings, required.as_ref(), case_sensitive)
            {
                return Err(RiskDataError::UnknownComboFlag {
                    command: command.into(),
                    subcommand: subcommand.map(Into::into),
                    spelling: required.clone(),
                });
            }
        }
    }

    for operand_rule in operand_rules {
        operand_rule.validate(command, subcommand)?;
    }

    Ok(spellings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Dataset;

    use rstest::rstest;

    use core::assert_matches;

    #[derive(Debug, thiserror::Error)]
    enum TestError {
        #[error(transparent)]
        RiskData(#[from] RiskDataError),
    }

    /// `files`' `&str` literal `(path, contents)` pairs converted into the owned
    /// `Vec<(Box<str>, Box<str>)>` `Dataset::parse` expects, so fixtures can stay terse string
    /// literals.
    fn parse_files<const N: usize>(files: [(&str, &str); N]) -> Result<Dataset, RiskDataError> {
        Dataset::parse(
            files
                .into_iter()
                .map(|(path, contents)| (path.into(), contents.into()))
                .collect(),
        )
    }

    mod validate {
        use super::*;

        #[rstest]
        fn rejects_a_duplicate_flag_spelling_within_one_command() {
            let duplicated = r#"
                [[command]]
                name = "set"
                kind = "builtin"
                baseline_reason = "Sets shell options."

                  [[command.flag]]
                  spellings = ["-x"]
                  effect = "escalate"
                  reason = "Enables execution tracing."

                  [[command.flag]]
                  spellings = ["-x"]
                  effect = "escalate"
                  reason = "Duplicate row, same spelling."
            "#;

            let result = parse_files([("set.toml", duplicated)]);

            assert_matches!(result, Err(RiskDataError::DuplicateFlagSpelling { .. }));
        }

        #[rstest]
        fn rejects_flag_spellings_differing_only_by_case_when_case_insensitive() {
            let duplicated = r#"
                [[command]]
                name = "set"
                kind = "builtin"
                case_sensitive = false
                baseline_reason = "Sets shell options."

                  [[command.flag]]
                  spellings = ["-X"]
                  effect = "escalate"
                  reason = "Enables execution tracing."

                  [[command.flag]]
                  spellings = ["-x"]
                  effect = "escalate"
                  reason = "Same spelling under case-insensitive matching."
            "#;

            let result = parse_files([("set.toml", duplicated)]);

            assert_matches!(result, Err(RiskDataError::DuplicateFlagSpelling { .. }));
        }

        #[rstest]
        fn accepts_flag_spellings_differing_only_by_case_when_case_sensitive()
        -> Result<(), TestError> {
            let distinct = r#"
                [[command]]
                name = "set"
                kind = "builtin"
                baseline_reason = "Sets shell options."

                  [[command.flag]]
                  spellings = ["-X"]
                  effect = "escalate"
                  reason = "Enables execution tracing, uppercase spelling."

                  [[command.flag]]
                  spellings = ["-x"]
                  effect = "escalate"
                  reason = "Distinct lowercase spelling under case-sensitive matching."
            "#;

            parse_files([("set.toml", distinct)])?;

            Ok(())
        }

        #[rstest]
        fn accepts_a_combo_rule_requiring_a_flag_spelled_differently_in_case_when_case_insensitive()
        -> Result<(), TestError> {
            let cross_case_combo = r#"
                [[command]]
                name = "tool"
                kind = "external"
                case_sensitive = false
                baseline_reason = "A case-insensitive CLI."

                  [[command.flag]]
                  spellings = ["-x"]
                  effect = "escalate"
                  reason = "Lowercase declaration."

                  [[command.combo_rule]]
                  requires = ["-X"]
                  effect = "escalate"
                  reason = "Uppercase requirement, same spelling case-insensitively."
            "#;

            parse_files([("tool.toml", cross_case_combo)])?;

            Ok(())
        }

        #[rstest]
        fn rejects_a_combo_rule_requiring_an_undeclared_flag() {
            let bad_combo = r#"
                [[command]]
                name = "git"
                kind = "external"
                baseline_reason = "Version control."

                  [[command.flag]]
                  spellings = ["-f"]
                  effect = "escalate"
                  reason = "Force."

                  [[command.combo_rule]]
                  requires = ["-f", "-d"]
                  effect = "escalate"
                  reason = "-d isn't declared anywhere on this command."
            "#;

            let result = parse_files([("git.toml", bad_combo)]);

            assert_matches!(result, Err(RiskDataError::UnknownComboFlag { .. }));
        }

        #[rstest]
        fn rejects_a_flag_value_rule_with_an_invalid_pattern() {
            let bad_value_rule = r#"
                [[command]]
                name = "rsync"
                kind = "external"
                baseline_reason = "Synchronizes files, locally or over a remote shell."

                  [[command.flag]]
                  spellings = ["--password-file"]
                  effect = "escalate"
                  reason = "Reads a plaintext credential from a file."

                    [[command.flag.value_rule]]
                    pattern = "("
                    effect = "mitigate"
                    reason = "Unbalanced paren, should fail to compile."
            "#;

            let result = parse_files([("rsync.toml", bad_value_rule)]);

            assert_matches!(result, Err(RiskDataError::InvalidPattern { .. }));
        }

        #[rstest]
        fn rejects_short_flags_combinable_under_the_go_grammar() {
            let contradiction = r#"
                [[command]]
                name = "tool"
                kind = "external"
                grammar = "go"
                short_flags_combinable = true
                baseline_reason = "A Go-style CLI."
            "#;

            let result = parse_files([("tool.toml", contradiction)]);

            assert_matches!(result, Err(RiskDataError::CombinableUnderGoGrammar { .. }));
        }

        #[rstest]
        fn rejects_two_subcommands_sharing_a_name() {
            let duplicated = r#"
                [[command]]
                name = "git"
                kind = "external"
                baseline_reason = "Version control."

                  [[command.subcommands]]
                  name = "push"
                  baseline_reason = "Uploads commits."

                  [[command.subcommands]]
                  name = "push"
                  baseline_reason = "Duplicate subcommand name."
            "#;

            let result = parse_files([("git.toml", duplicated)]);

            assert_matches!(result, Err(RiskDataError::DuplicateSubcommand { .. }));
        }

        #[rstest]
        fn accepts_the_same_flag_spelling_in_two_different_subcommand_scopes()
        -> Result<(), TestError> {
            let same_spelling_different_scopes = r#"
                [[command]]
                name = "git"
                kind = "external"
                baseline_reason = "Version control."

                  [[command.subcommands]]
                  name = "push"
                  baseline_reason = "Uploads commits."

                    [[command.subcommands.flag]]
                    spellings = ["-f"]
                    effect = "escalate"
                    reason = "Force push."

                  [[command.subcommands]]
                  name = "clean"
                  baseline_reason = "Removes untracked files."

                    [[command.subcommands.flag]]
                    spellings = ["-f"]
                    effect = "escalate"
                    reason = "Force clean - same spelling, independent scope."
            "#;

            parse_files([("git.toml", same_spelling_different_scopes)])?;

            Ok(())
        }

        #[rstest]
        fn a_subcommand_combo_rule_may_require_a_parent_global_flag() -> Result<(), TestError> {
            let cross_scope_combo = r#"
                [[command]]
                name = "git"
                kind = "external"
                baseline_reason = "Version control."

                  [[command.flag]]
                  spellings = ["-C"]
                  effect = "escalate"
                  reason = "Runs as if started in the given directory."

                  [[command.subcommands]]
                  name = "push"
                  baseline_reason = "Uploads commits."

                    [[command.subcommands.flag]]
                    spellings = ["-f"]
                    effect = "escalate"
                    reason = "Force push."

                    [[command.subcommands.combo_rule]]
                    requires = ["-C", "-f"]
                    effect = "escalate"
                    reason = "-C is a parent-scope global flag, -f is the subcommand's own."
            "#;

            parse_files([("git.toml", cross_scope_combo)])?;

            Ok(())
        }

        #[rstest]
        fn rejects_a_subcommand_combo_rule_requiring_a_flag_declared_nowhere() {
            let bad_cross_scope_combo = r#"
                [[command]]
                name = "git"
                kind = "external"
                baseline_reason = "Version control."

                  [[command.subcommands]]
                  name = "push"
                  baseline_reason = "Uploads commits."

                    [[command.subcommands.combo_rule]]
                    requires = ["-f"]
                    effect = "escalate"
                    reason = "-f isn't declared in this subcommand or globally."
            "#;

            let result = parse_files([("git.toml", bad_cross_scope_combo)]);

            assert_matches!(result, Err(RiskDataError::UnknownComboFlag { .. }));
        }
    }
}
