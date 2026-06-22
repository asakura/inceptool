use super::{CaseArm, LogicalOp, PipeOp, Redirect, Spanned, Statement};

use std::fmt;

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

impl<'a> Statement<'a> {
    /// Writes this statement exactly like [`fmt::Display`] would, except every heredoc body and
    /// terminator it would write is pushed onto `pending` instead of written inline. Real Bash
    /// defers a heredoc's body until the actual newline that ends its *entire* containing source
    /// line, no matter how many further tokens (`&&`, `;`, `do`/`done`, `then`/`fi`, ...) render
    /// onto that one output line after the `<<DELIM` operator — every recursive call here keeps
    /// writing onto that same line, sharing the one `pending` list, so only the top-level
    /// [`fmt::Display`] impl below ever needs to actually drain it.
    fn write_collecting<'s>(
        &'s self,
        f: &mut fmt::Formatter<'_>,
        pending: &mut Vec<&'s Redirect<'a>>,
    ) -> fmt::Result {
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

                write!(f, "; do ")?;
                body.inner.write_collecting(f, pending)?;
                write!(f, "; done")
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => Self::write_if_collecting(
                f,
                pending,
                condition,
                then_branch,
                else_branch.as_deref(),
            ),
            Statement::While { condition, body } => {
                Self::write_loop_collecting(f, pending, "while", condition, body)
            }
            Statement::Until { condition, body } => {
                Self::write_loop_collecting(f, pending, "until", condition, body)
            }
            Statement::Case { word, arms } => {
                write!(f, "case {word} in ")?;

                for arm in arms {
                    arm.write_collecting(f, pending)?;
                    write!(f, " ")?;
                }

                write!(f, "esac")
            }
            Statement::Pipeline { head, tail } => {
                head.inner.write_collecting(f, pending)?;

                for (pipe, cmd) in tail {
                    let op = match pipe {
                        PipeOp::Stdout => "|",
                        PipeOp::StdoutStderr => "|&",
                    };
                    write!(f, " {op} ")?;
                    cmd.inner.write_collecting(f, pending)?;
                }

                Ok(())
            }
            Statement::Subshell { body } => {
                write!(f, "(")?;
                body.inner.write_collecting(f, pending)?;
                write!(f, ")")
            }
            Statement::BraceGroup { body } => {
                write!(f, "{{ ")?;
                body.inner.write_collecting(f, pending)?;
                write!(f, "; }}")
            }
            Statement::AndOr { left, op, right } => {
                left.inner.write_collecting(f, pending)?;
                write!(f, " {op} ")?;
                right.inner.write_collecting(f, pending)
            }
            Statement::Sequence { left, right } => {
                left.inner.write_collecting(f, pending)?;
                write!(f, "; ")?;
                right.inner.write_collecting(f, pending)
            }
            Statement::Background { left, right } => {
                left.inner.write_collecting(f, pending)?;
                write!(f, " &")?;

                if let Some(right) = right {
                    write!(f, " ")?;
                    right.inner.write_collecting(f, pending)?;
                }

                Ok(())
            }
            Statement::Redirected { inner, redirects } => {
                inner.inner.write_collecting(f, pending)?;

                for redirect in redirects {
                    write!(f, " {redirect}")?;
                    pending.push(redirect);
                }

                Ok(())
            }
        }
    }

    /// The [`Statement::While`]/[`Statement::Until`] arms of [`Self::write_collecting`], split
    /// out to keep that function under Clippy's line-count limit — both loops render identically
    /// apart from `keyword`.
    fn write_loop_collecting<'s>(
        f: &mut fmt::Formatter<'_>,
        pending: &mut Vec<&'s Redirect<'a>>,
        keyword: &'static str,
        condition: &'s Spanned<Self>,
        body: &'s Spanned<Self>,
    ) -> fmt::Result {
        write!(f, "{keyword} ")?;
        condition.inner.write_collecting(f, pending)?;
        write!(f, "; do ")?;
        body.inner.write_collecting(f, pending)?;
        write!(f, "; done")
    }

    /// The [`Statement::If`] arm of [`Self::write_collecting`], split out to keep that function
    /// under Clippy's line-count limit — walks the `elif`-as-nested-`If` chain in `else_branch`
    /// (see that variant's doc comment) until a non-`If` `else` or no `else` at all ends it.
    fn write_if_collecting<'s>(
        f: &mut fmt::Formatter<'_>,
        pending: &mut Vec<&'s Redirect<'a>>,
        condition: &'s Spanned<Self>,
        then_branch: &'s Spanned<Self>,
        else_branch: Option<&'s Spanned<Self>>,
    ) -> fmt::Result {
        write!(f, "if ")?;
        condition.inner.write_collecting(f, pending)?;
        write!(f, "; then ")?;
        then_branch.inner.write_collecting(f, pending)?;

        let mut current = else_branch;
        while let Some(stmt) = current {
            match &stmt.inner {
                Statement::If {
                    condition: elif_condition,
                    then_branch: elif_then_branch,
                    else_branch: elif_else_branch,
                } => {
                    write!(f, "; elif ")?;
                    elif_condition.inner.write_collecting(f, pending)?;
                    write!(f, "; then ")?;
                    elif_then_branch.inner.write_collecting(f, pending)?;
                    current = elif_else_branch.as_deref();
                }
                other => {
                    write!(f, "; else ")?;
                    other.write_collecting(f, pending)?;
                    current = None;
                }
            }
        }

        write!(f, "; fi")
    }
}

/// Writes the single newline that opens the deferred-heredoc tail of a rendered line, then every
/// pending heredoc's body and terminator back-to-back — shared by [`Display for
/// Statement`](Statement) and [`Display for CaseArm`](CaseArm), the two self-contained rendering
/// entry points that drain a `pending` list built by [`Statement::write_collecting`]. A no-op
/// when `pending` is empty, so a heredoc-free statement's rendering is untouched.
#[must_use = "writing the pending heredoc tail has no effect unless the caller propagates the result"]
fn write_pending_heredocs(f: &mut fmt::Formatter<'_>, pending: &[&Redirect<'_>]) -> fmt::Result {
    if pending.is_empty() {
        return Ok(());
    }

    writeln!(f)?;

    for redirect in pending {
        redirect.write_heredoc_body(f)?;
    }

    Ok(())
}

impl fmt::Display for Statement<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut pending = Vec::new();
        self.write_collecting(f, &mut pending)?;
        write_pending_heredocs(f, &pending)
    }
}

impl<'a> CaseArm<'a> {
    /// Writes this arm exactly like [`fmt::Display`] would, deferring any heredoc body in
    /// `self.body` onto the same shared `pending` list its enclosing [`Statement::Case`] is
    /// collecting — see [`Statement::write_collecting`].
    fn write_collecting<'s>(
        &'s self,
        f: &mut fmt::Formatter<'_>,
        pending: &mut Vec<&'s Redirect<'a>>,
    ) -> fmt::Result {
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
            body.inner.write_collecting(f, pending)?;
        }

        write!(f, ";;")
    }
}

impl fmt::Display for CaseArm<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut pending = Vec::new();
        self.write_collecting(f, &mut pending)?;
        write_pending_heredocs(f, &pending)
    }
}
