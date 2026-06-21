use std::borrow::Cow;

use super::{Expr, Redirect, Spanned};

mod fmt;

/// One parsed Bash statement, as produced by [`crate::parser::parse_statement`].
#[derive(Clone, PartialEq, Eq)]
pub enum Statement<'a> {
    /// A simple command: a name followed by zero or more argument expressions.
    Command {
        /// The command's name word, taken directly from the lexed token without interpolation —
        /// parameter expansion of command names isn't implemented yet (see
        /// `parser::word::parse_literal`, which only `args` is run through).
        name: Cow<'a, str>,
        /// The command's argument expressions, each already split into literal/variable-
        /// reference parts.
        args: Vec<Spanned<Expr<'a>>>,
    },
    /// A `for NAME in ...; do ...; done` loop.
    ForLoop {
        /// The loop variable's name, as a word — usually a plain literal (`x`), but Bash also
        /// allows e.g. a command substitution here, so this isn't a bare `&str`.
        variable: Spanned<Expr<'a>>,
        /// The expressions iterated over, one loop body run per expression. Defaults to
        /// `["$@"]` when the source omits the `in` clause entirely (`for x; do ...; done`).
        iterable: Vec<Spanned<Expr<'a>>>,
        /// The statement run on each iteration.
        body: Box<Spanned<Self>>,
    },
    /// An `if ...; then ...; else ...; fi` conditional. `elif` has no field of its own — an
    /// `elif` clause is a nested [`Statement::If`] in `else_branch`, which is exactly the AST
    /// shape Bash's own `else if ...; fi` produces, so the two forms are indistinguishable (and
    /// both round-trip fine either way).
    If {
        /// The condition; the branch taken depends on its exit status.
        condition: Box<Spanned<Self>>,
        /// The statement run when `condition` succeeds.
        then_branch: Box<Spanned<Self>>,
        /// The statement run when `condition` fails, if an `else`/`elif` clause is present.
        else_branch: Option<Box<Spanned<Self>>>,
    },
    /// A `while ...; do ...; done` loop, repeating `body` for as long as `condition`
    /// succeeds.
    While {
        /// The statement re-evaluated before each iteration.
        condition: Box<Spanned<Self>>,
        /// The statement run on each iteration.
        body: Box<Spanned<Self>>,
    },
    /// An `until ...; do ...; done` loop — like [`Statement::While`], but repeats `body` for as
    /// long as `condition` fails.
    Until {
        /// The statement re-evaluated before each iteration.
        condition: Box<Spanned<Self>>,
        /// The statement run on each iteration.
        body: Box<Spanned<Self>>,
    },
    /// A `case <word> in ...; esac` pattern dispatch: `word` is matched against each arm's
    /// patterns in order, and the first match's body (if any) runs.
    Case {
        /// The word matched against each arm's patterns.
        word: Spanned<Expr<'a>>,
        /// The arms, tried in source order.
        arms: Vec<CaseArm<'a>>,
    },
    /// Two or more commands connected by `|`/`|&`, each command's stdout (and, for `|&`,
    /// stderr) feeding the next's stdin.
    Pipeline {
        /// The first command in the pipeline.
        head: Box<Spanned<Self>>,
        /// Each subsequent stage: the pipe operator that connects it from the previous stage,
        /// paired with the command itself. Always non-empty (a single-command pipeline is never
        /// built — the parser returns the bare command instead).
        tail: Vec<(PipeOp, Spanned<Self>)>,
    },
    /// A `(...)` subshell: `body` runs in a forked copy of the shell, so its side effects
    /// (variable assignments, `cd`, ...) don't affect the parent.
    Subshell {
        /// The statement run inside the subshell.
        body: Box<Spanned<Self>>,
    },
    /// A `{ ...; }` brace group: `body` runs in the current shell, unlike [`Statement::Subshell`]
    /// — used to group commands for a redirect or background job without forking.
    BraceGroup {
        /// The statement run inside the group.
        body: Box<Spanned<Self>>,
    },
    /// Two pipelines joined by `&&`/`||`: `right` runs only if `left`'s exit status satisfies
    /// `op`. Binds tighter than [`Statement::Sequence`]/[`Statement::Background`] (mirroring
    /// POSIX's `list`/`and_or` grammar split), built left-associatively by
    /// `parser::command::parse_and_or` — `a && b || c` is
    /// `AndOr(AndOr(a, And, b), Or, c)`.
    AndOr {
        /// The left-hand pipeline (or nested `AndOr`), evaluated first.
        left: Box<Spanned<Self>>,
        /// Which condition on `left`'s exit status gates running `right`.
        op: LogicalOp,
        /// The right-hand pipeline (or nested `AndOr`), evaluated per `op`.
        right: Box<Spanned<Self>>,
    },
    /// Two statements joined by `;` or a newline: `right` always runs after `left`, regardless of
    /// `left`'s exit status. A lone trailing `;`/newline carries no information and is never
    /// built into this variant — `parser::command::parse_list` returns the bare `left` instead.
    Sequence {
        /// The statement that runs first.
        left: Box<Spanned<Self>>,
        /// The statement that unconditionally runs after `left`.
        right: Box<Spanned<Self>>,
    },
    /// `left &` (or `left & right`): `left` runs asynchronously. Unlike a lone trailing `;`,
    /// backgrounding is meaningful even with nothing following — `right` is `None` exactly when
    /// `&` was the list's final token.
    Background {
        /// The statement that runs asynchronously.
        left: Box<Spanned<Self>>,
        /// The statement that runs after launching `left`, if the list continues.
        right: Option<Box<Spanned<Self>>>,
    },
    /// `inner` with one or more [`Redirect`]s attached, in source order. A single wrapper
    /// variant rather than a `redirects` field on every other variant: Bash's own grammar
    /// attaches a `redirect_list` to any `compound_command` (`{ ...; } > file`) and folds
    /// redirects interleaved with a simple command's own words into one flat list regardless of
    /// where they fall (`cat < in.txt -n` and `cat -n < in.txt` mean the same thing) — both
    /// shapes are exactly "some other statement, plus redirects", so one wrapper covers both.
    Redirected {
        /// The statement being redirected.
        inner: Box<Spanned<Self>>,
        /// The redirects, in source order. Order matters only for fd-duplication chains, where
        /// each redirect sees the fd table as left by the previous one (`2>&1 1>&2` differs from
        /// `1>&2 2>&1`).
        redirects: Vec<Redirect<'a>>,
    },
}

/// One `pattern) commands ;;` arm of a [`Statement::Case`].
#[derive(Clone, PartialEq, Eq)]
pub struct CaseArm<'a> {
    /// The patterns tried against the `case` word, in order — more than one when alternatives
    /// are joined by `|` (`a|b)`).
    pub patterns: Vec<Spanned<Expr<'a>>>,
    /// The statement run when one of `patterns` matches, or `None` for an empty arm (`a) ;;`).
    pub body: Option<Box<Spanned<Statement<'a>>>>,
}

/// Which exit-status condition on `left` gates running `right` in a [`Statement::AndOr`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    /// `&&` — run `right` only if `left` exits successfully.
    And,
    /// `||` — run `right` only if `left` exits with failure.
    Or,
}

/// Which pipe operator connects two adjacent stages in a [`Statement::Pipeline`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeOp {
    /// `|` — connects the left command's stdout to the right command's stdin.
    Stdout,
    /// `|&` — connects both stdout and stderr of the left command to the right command's stdin
    /// (Bash shorthand for `2>&1 |`).
    StdoutStderr,
}
