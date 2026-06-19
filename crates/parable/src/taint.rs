//! Best-effort taint tracking over resolved [`Expr`]s — see [`SymbolicValue`] and [`Environment`].

use crate::parser::{Segment, interpolation_segments};
use crate::types::{Expr, Statement};

use std::collections::BTreeMap;

/// Best-effort, flow-insensitive approximation of a Bash variable's possible value.
///
/// This is not full symbolic execution: subshells, the contents of command substitutions, and
/// control flow (branches/loops) are not modeled precisely — see [`Environment`]'s
/// documentation. The goal is to catch the common "tainted value reaches a dangerous sink"
/// shape, erring toward false positives rather than silently missing a real one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolicValue {
    /// A value known exactly at analysis time.
    Constant(String),
    /// A value that can be influenced by the script's caller.
    Tainted(TaintSource),
    /// Concatenation of parts, e.g. from `"prefix${x}suffix"`; tainted if any part is.
    Concat(Vec<Self>),
    /// A value this analysis can't resolve: command-substitution output, an indirect or
    /// unrecognized expansion, or a variable that was never assigned and isn't a positional
    /// parameter.
    Unknown,
}

/// Where a [`SymbolicValue::Tainted`] value is presumed to originate.
///
/// Only one source is modeled today; this is expected to grow (environment variables, `read`,
/// command-substitution output) as the parser gains the grammar to recognize them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaintSource {
    /// `$1`..`$9`, `$@`, `$*`, or `$#`: arguments supplied by the script's caller.
    PositionalParam,
}

impl SymbolicValue {
    /// Whether this value, or (for [`Self::Concat`]) any part of it, is tainted.
    #[must_use = "checking taint has no effect unless the caller acts on the result"]
    pub fn is_tainted(&self) -> bool {
        match self {
            Self::Tainted(_) => true,
            Self::Concat(parts) => parts.iter().any(Self::is_tainted),
            Self::Constant(_) | Self::Unknown => false,
        }
    }
}

/// Tracks each Bash variable's [`SymbolicValue`] as rules walk a script's statements in source
/// order, and resolves [`Expr`]s against that state.
///
/// # Limitations
///
/// - Pipeline and subshell components run in a copy of the environment in real Bash, but
///   assignments inside them are folded into the same [`Environment`] here.
/// - An `if`/`while`/`for`'s branches and bodies are folded into one environment in source
///   order (last write wins), not modeled as alternate paths or loop fixpoints.
/// - Command-substitution (`` $(...) ``/`` `...` ``) and arithmetic-expansion (`$((...))`)
///   contents are recognized by the parser only well enough to skip over them, so they
///   resolve to [`SymbolicValue::Unknown`], not whatever the substituted command would
///   actually produce.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    vars: BTreeMap<String, SymbolicValue>,
}

impl Environment {
    /// An environment with no variables assigned yet.
    #[must_use = "constructs the environment; discarding it tracks nothing"]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `name`'s value, overwriting any prior value (last write wins).
    pub fn assign(&mut self, name: &str, value: SymbolicValue) {
        self.vars.insert(name.to_owned(), value);
    }

    /// Looks up `name`. A name that was never assigned is inferred as caller-supplied taint
    /// when it's a positional/special parameter (`$1`, `$@`, ...), and [`SymbolicValue::Unknown`]
    /// otherwise (most unassigned names are inherited environment variables, not attacker input).
    #[must_use = "looking up a variable has no effect unless the caller uses the result"]
    pub fn lookup(&self, name: &str) -> SymbolicValue {
        if let Some(value) = self.vars.get(name) {
            return value.clone();
        }

        if is_positional_param(name) {
            SymbolicValue::Tainted(TaintSource::PositionalParam)
        } else {
            SymbolicValue::Unknown
        }
    }

    /// Resolves `expr` against this environment.
    #[must_use = "resolving an expression has no effect unless the caller uses the result"]
    pub fn resolve_expr(&self, expr: &Expr<'_>) -> SymbolicValue {
        match expr {
            Expr::Literal(text) => {
                // The parser folds command/arithmetic substitution and fancy `${...}` forms
                // back into plain literal text rather than fragmenting the AST over them (see
                // `parser::word::ReferenceOutcome::NotAReference`), so a literal containing one of
                // those constructs isn't actually a known-safe constant — treat it as Unknown
                // rather than silently dropping the taint this analysis can't see through.
                if contains_unresolved_construct(text) {
                    SymbolicValue::Unknown
                } else {
                    SymbolicValue::Constant(text.as_ref().to_owned())
                }
            }
            Expr::VarRef(name) => self.lookup(name),
            Expr::Interpolated(parts) => {
                SymbolicValue::Concat(parts.iter().map(|part| self.resolve_expr(part)).collect())
            }
        }
    }

    /// If `stmt` is a bare `NAME=value` assignment — a [`Statement::Command`] with no args
    /// whose name has assignment-word shape — records it. Other statement shapes are ignored;
    /// recursing into nested bodies is the caller's responsibility (see
    /// [`crate::rules::Engine`]).
    ///
    /// The assignment's value never passes through `parser::word::parse_literal` (the
    /// lexer has no notion of assignment words, so `x=$1` is just one opaque command-name
    /// token), so its `$NAME` references are split out here directly via
    /// `interpolation_segments` instead of walking an already-structured [`Expr`].
    pub fn apply_statement(&mut self, stmt: &Statement<'_>) {
        let Statement::Command { name, args } = stmt else {
            return;
        };

        if !args.is_empty() {
            return;
        }

        let Some((var_name, value_text)) = split_assignment(name) else {
            return;
        };

        let value = resolve_text(value_text, |n| self.lookup(n));
        self.assign(var_name, value);
    }
}

/// Resolves raw word text (not yet split into an [`Expr`]) by splitting out `$NAME`/`${NAME}`
/// references via `interpolation_segments` and resolving each through `lookup`.
fn resolve_text(text: &str, lookup: impl Fn(&str) -> SymbolicValue) -> SymbolicValue {
    let mut parts = interpolation_segments(text)
        .into_iter()
        .map(|segment| match segment {
            Segment::Literal(s) if contains_unresolved_construct(s) => SymbolicValue::Unknown,
            Segment::Literal(s) => SymbolicValue::Constant(s.to_owned()),
            Segment::VarRef(name) => lookup(name),
        });

    match (parts.next(), parts.next()) {
        (None, _) => SymbolicValue::Constant(String::new()),
        (Some(only), None) => only,
        (Some(first), Some(second)) => {
            let mut all = vec![first, second];
            all.extend(parts);
            SymbolicValue::Concat(all)
        }
    }
}

/// Whether `text` contains a command substitution or arithmetic expansion span (`$(`, `` ` ``,
/// or `$((`) that `interpolation_segments` folded into plain literal text rather than
/// decomposing — see [`Environment::resolve_expr`] for why that makes the text unsafe to treat
/// as a known-safe constant.
#[must_use = "checking for an unresolved construct has no effect unless the caller uses the result"]
fn contains_unresolved_construct(text: &str) -> bool {
    text.contains("$(") || text.contains('`')
}

/// `NAME=value` -> `Some(("NAME", "value"))` when `NAME` is a valid Bash identifier
/// (`[A-Za-z_][A-Za-z0-9_]*`) immediately followed by `=`. `None` for anything else, including
/// an ordinary command that merely contains a later `=`.
#[must_use = "splitting an assignment has no effect unless the caller uses the result"]
fn split_assignment(word: &str) -> Option<(&str, &str)> {
    let (name, value) = word.split_once('=')?;
    let mut chars = name.chars();
    let first = chars.next()?;

    if !(first.is_alphabetic() || first == '_') {
        return None;
    }

    if !chars.all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }

    Some((name, value))
}

/// Whether `name` is a positional or special parameter set by the script's caller
/// (`$1`..`$9`, `$@`, `$*`, `$#`), as opposed to `$?`/`$$`/`$!`/`$-` (set by the shell itself)
/// or an ordinary variable name.
#[must_use = "checking a name has no effect unless the caller uses the result"]
fn is_positional_param(name: &str) -> bool {
    matches!(name, "@" | "*" | "#")
        || (!name.is_empty() && name.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    enum TestError {}

    mod split_assignment {
        use super::*;

        use rstest::rstest;

        #[rstest]
        #[case::simple("x=5", Some(("x", "5")))]
        #[case::underscore_prefixed("_foo=bar", Some(("_foo", "bar")))]
        #[case::empty_value("x=", Some(("x", "")))]
        #[case::not_an_assignment("echo hello", None)]
        #[case::name_cannot_start_with_digit("5x=foo", None)]
        #[case::no_equals_sign("plainword", None)]
        fn matches_expected_shape(
            #[case] input: &str,
            #[case] expected: Option<(&str, &str)>,
        ) -> Result<(), TestError> {
            assert_eq!(super::split_assignment(input), expected);
            Ok(())
        }
    }

    mod environment {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn unassigned_positional_param_is_tainted() -> Result<(), TestError> {
            let env = Environment::new();
            assert_eq!(
                env.lookup("1"),
                SymbolicValue::Tainted(TaintSource::PositionalParam)
            );
            Ok(())
        }

        #[rstest]
        fn unassigned_ordinary_name_is_unknown() -> Result<(), TestError> {
            let env = Environment::new();
            assert_eq!(env.lookup("HOME"), SymbolicValue::Unknown);
            Ok(())
        }

        #[rstest]
        fn assignment_from_positional_param_taints_the_variable() -> Result<(), TestError> {
            let mut env = Environment::new();
            env.apply_statement(&Statement::Command {
                name: "x=$1".into(),
                args: vec![],
            });
            assert_eq!(
                env.lookup("x"),
                SymbolicValue::Tainted(TaintSource::PositionalParam)
            );
            Ok(())
        }

        #[rstest]
        fn assignment_from_constant_is_not_tainted() -> Result<(), TestError> {
            let mut env = Environment::new();
            env.apply_statement(&Statement::Command {
                name: "x=hello".into(),
                args: vec![],
            });
            assert!(!env.lookup("x").is_tainted());
            Ok(())
        }

        #[rstest]
        fn taint_propagates_through_concatenation() -> Result<(), TestError> {
            let mut env = Environment::new();
            env.apply_statement(&Statement::Command {
                name: "x=$1".into(),
                args: vec![],
            });
            assert!(
                env.resolve_expr(&Expr::Interpolated(vec![
                    Expr::Literal("prefix-".into()),
                    Expr::VarRef("x"),
                ]))
                .is_tainted()
            );
            Ok(())
        }

        #[rstest]
        fn command_with_args_is_not_an_assignment() -> Result<(), TestError> {
            let mut env = Environment::new();
            env.apply_statement(&Statement::Command {
                name: "x=5".into(),
                args: vec![Expr::Literal("extra".into())],
            });
            assert_eq!(env.lookup("x"), SymbolicValue::Unknown);
            Ok(())
        }
    }
}
