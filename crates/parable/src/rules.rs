//! Security-rule scanning over a parsed script — see [`Engine`] and [`Rule`].

use crate::taint::Environment;
use crate::types::Statement;

use std::borrow::Cow;
use std::fmt;

/// How seriously a [`Finding`] should be treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Worth a human's attention, but not necessarily wrong.
    Warning,
    /// A pattern that's almost always a real defect.
    Error,
}

/// What a [`Rule`] found.
///
/// Structured rather than a pre-formatted message, so [`Finding`]'s [`Display`](fmt::Display)
/// impl can render it via `write!` — see `[types] newtype` in this crate's style policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingDetail<'a> {
    /// A dangerous command (`eval`, `source`, `.`) received an argument influenced by the
    /// script's caller.
    TaintedDangerousCommand {
        /// The command name that received the tainted argument.
        command: Cow<'a, str>,
    },
}

/// One diagnostic produced by a [`Rule`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding<'a> {
    /// Identifies which [`Rule`] produced this finding.
    pub rule_id: &'static str,
    /// How seriously this finding should be treated.
    pub severity: Severity,
    /// The specifics of what was found.
    pub detail: FindingDetail<'a>,
}

impl fmt::Display for Finding<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.detail {
            FindingDetail::TaintedDangerousCommand { command } => write!(
                f,
                "`{command}` receives a value influenced by the script's caller"
            ),
        }
    }
}

/// A single check run against every [`Statement`] in a script by [`Engine::run`].
///
/// Rules see one statement at a time, plus the [`Environment`] as of just before that
/// statement executes; the [`Engine`] owns walking into nested bodies (`if`/`for`/... blocks),
/// so a `Rule` only needs to pattern-match the shapes it cares about.
pub trait Rule {
    /// Identifies this rule in any [`Finding`] it produces.
    fn id(&self) -> &'static str;

    /// Checks `stmt`, pushing a [`Finding`] for each problem found.
    fn check<'a>(&self, stmt: &Statement<'a>, env: &Environment, findings: &mut Vec<Finding<'a>>);
}

/// Runs a set of [`Rule`]s over a script's statements, threading one [`Environment`] through
/// the walk so rules can see prior assignments.
#[derive(Default)]
pub struct Engine<'r> {
    rules: Vec<&'r dyn Rule>,
}

impl fmt::Debug for Engine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field(
                "rule_ids",
                &self.rules.iter().map(|rule| rule.id()).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl<'r> Engine<'r> {
    /// An engine with no rules registered yet.
    #[must_use = "constructs the engine; discarding it runs no rules"]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `rule` to the set this engine runs.
    pub fn register(&mut self, rule: &'r dyn Rule) {
        self.rules.push(rule);
    }

    /// Walks `statements` (and their nested bodies) in source order, running every registered
    /// rule against each one, and returns every finding produced.
    #[must_use = "running the engine has no effect unless the caller inspects the findings"]
    pub fn run<'a>(&self, statements: &[Statement<'a>]) -> Vec<Finding<'a>> {
        let mut env = Environment::new();
        let mut findings = Vec::new();
        self.visit_all(statements, &mut env, &mut findings);
        findings
    }

    fn visit_all<'a>(
        &self,
        statements: &[Statement<'a>],
        env: &mut Environment,
        findings: &mut Vec<Finding<'a>>,
    ) {
        for stmt in statements {
            self.visit(stmt, env, findings);
        }
    }

    fn visit<'a>(
        &self,
        stmt: &Statement<'a>,
        env: &mut Environment,
        findings: &mut Vec<Finding<'a>>,
    ) {
        for rule in &self.rules {
            rule.check(stmt, env, findings);
        }

        env.apply_statement(stmt);

        match stmt {
            Statement::Command { .. } => {}
            Statement::ForLoop { body, .. } => self.visit_all(body, env, findings),
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.visit_all(condition, env, findings);
                self.visit_all(then_branch, env, findings);

                if let Some(else_b) = else_branch {
                    self.visit_all(else_b, env, findings);
                }
            }
            Statement::While { condition, body } | Statement::Until { condition, body } => {
                self.visit_all(condition, env, findings);
                self.visit_all(body, env, findings);
            }
            Statement::Pipeline { commands } => self.visit_all(commands, env, findings),
            Statement::Subshell { body } | Statement::BraceGroup { body } => {
                self.visit_all(body, env, findings);
            }
            Statement::List { items } => {
                for (inner, _separator) in items {
                    self.visit(inner, env, findings);
                }
            }
            Statement::Redirected { inner, .. } => self.visit(inner, env, findings),
        }
    }
}

const DANGEROUS_COMMANDS: [&str; 3] = ["eval", "source", "."];

/// Flags `eval`/`source`/`.` (the dot command) when an argument resolves to a value
/// influenced by the script's caller — the canonical shape of a shell-injection bug.
#[derive(Debug, Clone, Copy, Default)]
pub struct TaintedDangerousCommand;

impl Rule for TaintedDangerousCommand {
    fn id(&self) -> &'static str {
        "tainted-dangerous-command"
    }

    fn check<'a>(&self, stmt: &Statement<'a>, env: &Environment, findings: &mut Vec<Finding<'a>>) {
        let Statement::Command { name, args } = stmt else {
            return;
        };

        if !DANGEROUS_COMMANDS.contains(&name.as_ref()) {
            return;
        }

        if args.iter().any(|arg| env.resolve_expr(arg).is_tainted()) {
            findings.push(Finding {
                rule_id: self.id(),
                severity: Severity::Error,
                detail: FindingDetail::TaintedDangerousCommand {
                    command: name.clone(),
                },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::types::Expr;

    #[derive(Debug, thiserror::Error)]
    enum TestError {}

    mod tainted_dangerous_command {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn flags_eval_of_tainted_positional_param() -> Result<(), TestError> {
            let statements = vec![Statement::Command {
                name: "eval".into(),
                args: vec![Expr::VarRef("1")],
            }];

            let rule = TaintedDangerousCommand;
            let mut engine = Engine::new();
            engine.register(&rule);

            let findings = engine.run(&statements);

            assert_eq!(findings.len(), 1);
            assert_eq!(
                findings.first().map(|f| f.rule_id),
                Some("tainted-dangerous-command")
            );
            Ok(())
        }

        #[rstest]
        fn does_not_flag_eval_of_constant() -> Result<(), TestError> {
            let statements = vec![Statement::Command {
                name: "eval".into(),
                args: vec![Expr::Literal("echo hi".into())],
            }];

            let rule = TaintedDangerousCommand;
            let mut engine = Engine::new();
            engine.register(&rule);

            assert!(engine.run(&statements).is_empty());
            Ok(())
        }

        #[rstest]
        fn flags_eval_of_variable_assigned_from_positional_param() -> Result<(), TestError> {
            let statements = vec![
                Statement::Command {
                    name: "cmd=$1".into(),
                    args: vec![],
                },
                Statement::Command {
                    name: "eval".into(),
                    args: vec![Expr::VarRef("cmd")],
                },
            ];

            let rule = TaintedDangerousCommand;
            let mut engine = Engine::new();
            engine.register(&rule);

            assert_eq!(engine.run(&statements).len(), 1);
            Ok(())
        }

        #[rstest]
        fn does_not_flag_ordinary_commands() -> Result<(), TestError> {
            let statements = vec![Statement::Command {
                name: "echo".into(),
                args: vec![Expr::VarRef("1")],
            }];

            let rule = TaintedDangerousCommand;
            let mut engine = Engine::new();
            engine.register(&rule);

            assert!(engine.run(&statements).is_empty());
            Ok(())
        }

        #[rstest]
        fn finds_taint_inside_nested_if_body() -> Result<(), TestError> {
            let statements = vec![Statement::If {
                condition: vec![Statement::Command {
                    name: "true".into(),
                    args: vec![],
                }],
                then_branch: vec![Statement::Command {
                    name: "eval".into(),
                    args: vec![Expr::VarRef("1")],
                }],
                else_branch: None,
            }];

            let rule = TaintedDangerousCommand;
            let mut engine = Engine::new();
            engine.register(&rule);

            assert_eq!(engine.run(&statements).len(), 1);
            Ok(())
        }
    }
}
