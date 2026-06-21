use std::fmt;

use super::{CaseArm, LogicalOp, PipeOp, Statement};

impl fmt::Debug for Statement<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Statement::Command { name, args } => {
                write!(f, "(command (word {name:?})")?;

                for arg in args {
                    write!(f, " {arg:?}")?;
                }

                write!(f, ")")
            }
            Statement::ForLoop {
                variable,
                iterable,
                body,
            } => {
                write!(f, "(for {variable:?} (in")?;

                for iter in iterable {
                    write!(f, " {iter:?}")?;
                }

                write!(f, ") {body:?})")
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                write!(f, "(if {condition:?} {then_branch:?}")?;

                if let Some(else_b) = else_branch {
                    write!(f, " {else_b:?}")?;
                }

                write!(f, ")")
            }
            Statement::While { condition, body } => write!(f, "(while {condition:?} {body:?})"),
            Statement::Until { condition, body } => write!(f, "(until {condition:?} {body:?})"),
            Statement::Case { word, arms } => {
                write!(f, "(case {word:?}")?;

                for arm in arms {
                    write!(f, " {arm:?}")?;
                }

                write!(f, ")")
            }
            Statement::Pipeline { head, tail } => {
                write!(f, "(pipeline {head:?}")?;

                for (pipe, cmd) in tail {
                    let op = match pipe {
                        PipeOp::Stdout => "|",
                        PipeOp::StdoutStderr => "|&",
                    };
                    write!(f, " ({op} {cmd:?})")?;
                }

                write!(f, ")")
            }
            Statement::Subshell { body } => write!(f, "(subshell {body:?})"),
            Statement::BraceGroup { body } => write!(f, "(brace-group {body:?})"),
            Statement::AndOr { left, op, right } => {
                let tag = match op {
                    LogicalOp::And => "and",
                    LogicalOp::Or => "or",
                };

                write!(f, "({tag} {left:?} {right:?})")
            }
            Statement::Sequence { left, right } => write!(f, "(semi {left:?} {right:?})"),
            Statement::Background { left, right } => {
                write!(f, "(bg {left:?}")?;

                if let Some(right) = right {
                    write!(f, " {right:?}")?;
                }

                write!(f, ")")
            }
            Statement::Redirected { inner, redirects } => {
                write!(f, "(redirected {inner:?}")?;

                for redirect in redirects {
                    write!(f, " {redirect:?}")?;
                }

                write!(f, ")")
            }
        }
    }
}

impl fmt::Debug for CaseArm<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(pattern (")?;

        for (i, pattern) in self.patterns.iter().enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }

            write!(f, "{pattern:?}")?;
        }

        write!(f, ") ")?;

        match &self.body {
            Some(body) => write!(f, "{body:?}")?,
            None => write!(f, "()")?,
        }

        write!(f, ")")
    }
}

impl fmt::Display for LogicalOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::And => "&&",
            Self::Or => "||",
        };

        write!(f, "{s}")
    }
}

impl fmt::Display for Statement<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Statement::Command { name, args } => {
                write!(f, "{name}")?;

                for arg in args {
                    write!(f, " {arg}")?;
                }

                Ok(())
            }
            Statement::ForLoop {
                variable,
                iterable,
                body,
            } => {
                write!(f, "for {variable} in")?;

                for iter in iterable {
                    write!(f, " {iter}")?;
                }

                write!(f, "; do {body}; done")
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                write!(f, "if {condition}; then {then_branch}")?;

                let mut current = else_branch.as_ref();
                while let Some(stmt) = current {
                    match &stmt.inner {
                        Statement::If {
                            condition: elif_condition,
                            then_branch: elif_then_branch,
                            else_branch: elif_else_branch,
                        } => {
                            write!(f, "; elif {elif_condition}; then {elif_then_branch}")?;
                            current = elif_else_branch.as_ref();
                        }
                        other => {
                            write!(f, "; else {other}")?;
                            current = None;
                        }
                    }
                }

                write!(f, "; fi")
            }
            Statement::While { condition, body } => {
                write!(f, "while {condition}; do {body}; done")
            }
            Statement::Until { condition, body } => {
                write!(f, "until {condition}; do {body}; done")
            }
            Statement::Case { word, arms } => {
                write!(f, "case {word} in ")?;

                for arm in arms {
                    write!(f, "{arm} ")?;
                }

                write!(f, "esac")
            }
            Statement::Pipeline { head, tail } => {
                write!(f, "{head}")?;

                for (pipe, cmd) in tail {
                    let op = match pipe {
                        PipeOp::Stdout => "|",
                        PipeOp::StdoutStderr => "|&",
                    };
                    write!(f, " {op} {cmd}")?;
                }

                Ok(())
            }
            Statement::Subshell { body } => write!(f, "({body})"),
            Statement::BraceGroup { body } => write!(f, "{{ {body}; }}"),
            Statement::AndOr { left, op, right } => write!(f, "{left} {op} {right}"),
            Statement::Sequence { left, right } => write!(f, "{left}; {right}"),
            Statement::Background { left, right } => {
                write!(f, "{left} &")?;

                if let Some(right) = right {
                    write!(f, " {right}")?;
                }

                Ok(())
            }
            Statement::Redirected { inner, redirects } => {
                write!(f, "{inner}")?;

                for redirect in redirects {
                    write!(f, " {redirect}")?;
                }

                Ok(())
            }
        }
    }
}

impl fmt::Display for CaseArm<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Always emits the leading `(`, even though the parser accepts it as optional: omitting
        // it would be ambiguous when the first pattern is itself the bare word `esac` (the
        // closing keyword is only recognized at the position a new arm's pattern would
        // otherwise start) — re-parsing `esac) ...` there hits the closing-`esac` rule instead
        // of this pattern. Always printing `(` sidesteps needing to special-case that one
        // pattern value; it's valid Bash for every arm, not just that one.
        write!(f, "(")?;

        for (i, pattern) in self.patterns.iter().enumerate() {
            if i > 0 {
                write!(f, "|")?;
            }

            write!(f, "{pattern}")?;
        }

        write!(f, ") ")?;

        if let Some(body) = &self.body {
            write!(f, "{body};;")
        } else {
            write!(f, ";;")
        }
    }
}
