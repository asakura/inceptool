//! The AST and token types shared by [`crate::lexer`], [`crate::parser`], and downstream
//! analysis — see [`Token`], [`Expr`], and [`Statement`].

use std::{borrow::Cow, fmt, ops::Range};

/// Tracks the current parsing context of the Bash script.
#[derive(Debug, Clone, Default)]
pub struct LexerState<'a> {
    /// Whether the lexer is currently inside a `$((...))`/`((...))` arithmetic context, where
    /// metacharacters that would otherwise end a word (e.g. `<`, `>`) are plain operators.
    pub in_arithmetic: bool,
    /// The heredoc terminator the lexer is scanning for, once a `<<`/`<<-` has been seen, until
    /// a line consisting of exactly that delimiter is found.
    pub heredoc_delimiter: Option<Cow<'a, str>>,
}

/// A single lexical token, as produced by [`crate::lexer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token<'a> {
    /// A run of word content, not yet classified as a keyword or split into an [`Expr`] — see
    /// `parser::word`.
    Word(Cow<'a, str>),
    /// A line terminator. Significant as a statement separator, unlike inline whitespace (which
    /// the lexer simply skips without emitting a token).
    Newline,

    // Single-char operators
    /// `;` — sequential command separator (`cmd1; cmd2`).
    Semi,
    /// `|` — pipeline separator, connecting one command's stdout to the next's stdin.
    Pipe,
    /// `&` — background-execution suffix; see [`Statement::Background`].
    Amp,
    /// `(` — opens a [`Statement::Subshell`].
    LParen,
    /// `)` — closes a [`Statement::Subshell`].
    RParen,
    /// `{` — opens a [`Statement::BraceGroup`].
    LBrace,
    /// `}` — closes a [`Statement::BraceGroup`].
    RBrace,
    /// `<` — input redirection (`cmd < file`).
    Less,
    /// `>` — output redirection, truncating the target (`cmd > file`).
    Greater,

    // Multi-char operators
    /// `&&` — runs the next pipeline only if the previous one succeeded.
    AndAnd,
    /// `||` — runs the next pipeline only if the previous one failed.
    OrOr,
    /// `;;` — ends a `case` pattern's command list.
    SemiSemi,
    /// `;&` — ends a `case` pattern's command list and falls through to the next pattern's
    /// commands unconditionally, without testing that pattern.
    SemiAmp,
    /// `;;&` — ends a `case` pattern's command list and falls through to the next pattern, but
    /// still tests it before running its commands.
    SemiSemiAmp,
    /// `<<` — opens a heredoc (`cmd <<DELIM`).
    LessLess,
    /// `>>` — output redirection, appending to the target.
    GreaterGreater,
    /// `<&` — duplicates or moves a file descriptor for input (`cmd <&N`).
    LessAmp,
    /// `>&` — duplicates or moves a file descriptor for output (`cmd >&N`).
    GreaterAmp,
    /// `<>` — opens the target for both reading and writing (`cmd <> file`).
    LessGreater,
    /// `>|` — output redirection that forces truncation even under `noclobber`.
    GreaterPipe,
    /// `<<-` — opens a heredoc whose body's leading tabs are stripped before matching the
    /// delimiter.
    LessLessMinus,
    /// `<<<` — here-string: feeds a single expanded word to the command's stdin.
    LessLessLess,
    /// `&>` — redirects both stdout and stderr to the target, truncating it.
    AmpGreater,
    /// `&>>` — redirects both stdout and stderr to the target, appending to it.
    AmpGreaterGreater,
    /// `|&` — pipeline separator that also connects the previous command's stderr to the next's
    /// stdin (shorthand for `2>&1 |`).
    PipeAmp,

    // Special
    // Reserved words (if, for, done, ...) are not distinct variants: they
    // lex as plain `Word`s and are only recognized as keywords by the
    // parser, at the specific grammar positions where Bash expects them.
    /// A `NAME=value`-shaped word, recognized as an assignment rather than a command name or
    /// argument. Reserved for assignment-word recognition, not yet constructed by the lexer.
    AssignmentWord(&'a str),
    /// A run of digits in a position where Bash expects a number (e.g. a file descriptor before
    /// a redirect operator). Reserved for that recognition, not yet constructed by the lexer.
    Number(&'a str),

    /// End of input.
    Eof,
}

/// A wrapper attaching a byte span (start and end offset) to an AST node or token.
///
/// If the `miette` feature is enabled, this type implements conversions to
/// [`miette::SourceSpan`], allowing seamless integration with `miette` diagnostics.
#[derive(Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    /// The inner parsed node.
    pub inner: T,
    /// The byte span in the source string.
    pub span: Range<usize>,
}

impl<T> From<(T, Range<usize>)> for Spanned<T> {
    /// Builds a `Spanned` from a `(node, span)` tuple — the shape winnow's `.with_span()`
    /// combinator produces, so a parser can finish with `.with_span().map(Spanned::from)`.
    fn from((inner, span): (T, Range<usize>)) -> Self {
        Self { inner, span }
    }
}

/// Delegates `$trait` straight through to a `Spanned<T>`'s `inner`, so a `Spanned` reads exactly
/// like the node it wraps. In particular, this keeps `{:?}` corpus-test AST snapshots from being
/// polluted with spans, without [`fmt::Debug`] and [`fmt::Display`] each needing their own
/// hand-written pass-through.
macro_rules! transparent_fmt {
    ($trait:ident) => {
        impl<T: fmt::$trait> fmt::$trait for Spanned<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::$trait::fmt(&self.inner, f)
            }
        }
    };
}

transparent_fmt!(Display);
transparent_fmt!(Debug);

#[cfg(feature = "miette")]
impl<T> From<Spanned<T>> for miette::SourceSpan {
    fn from(spanned: Spanned<T>) -> Self {
        Self::from(spanned.span)
    }
}

#[cfg(feature = "miette")]
impl<T> From<&Spanned<T>> for miette::SourceSpan {
    /// Delegates to the owned [`From<Spanned<T>>`] impl above via a `start..end` reconstructed
    /// from copies of `span`'s two `usize` fields, rather than `Range::clone`-ing the whole span
    /// — cheaper, and `T` need not be `Clone` for it (`Spanned<T>` itself can't be reused here
    /// without one).
    fn from(spanned: &Spanned<T>) -> Self {
        Self::from(spanned.span.start..spanned.span.end)
    }
}

/// One of Bash's punctuation-named special parameters — `$@`, `$*`, `$#`, `$?`, `$$`, `$!`, `$-`.
///
/// These don't carry a name to borrow the way `$NAME`/`$1` do, so they're classified into this
/// enum at the point they're recognized (`parser::word::classify_param`) rather than passed
/// around as a bare symbol string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialParam {
    /// `$@` — positional parameters, each a separate word.
    AllArgs,
    /// `$*` — positional parameters as a single word.
    AllArgsStar,
    /// `$#` — number of positional parameters.
    ArgCount,
    /// `$?` — exit status of the last command.
    ExitStatus,
    /// `$$` — the shell's own PID.
    ShellPid,
    /// `$!` — PID of the last backgrounded command.
    LastBgPid,
    /// `$-` — current shell option flags.
    Flags,
}

impl fmt::Display for SpecialParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let symbol = match self {
            Self::AllArgs => "@",
            Self::AllArgsStar => "*",
            Self::ArgCount => "#",
            Self::ExitStatus => "?",
            Self::ShellPid => "$",
            Self::LastBgPid => "!",
            Self::Flags => "-",
        };

        write!(f, "{symbol}")
    }
}

/// A parsed word, with any `$NAME`/`${NAME}` references resolved out of the surrounding literal
/// text — see `parser::word`.
#[derive(Clone, PartialEq, Eq)]
pub enum Expr<'a> {
    /// Plain text, not subject to further expansion.
    Literal(Cow<'a, str>),
    /// A `$NAME`/`${NAME}` reference, not yet resolved to a value.
    VarRef(&'a str),
    /// A `$1`/`$12`/`${10}` positional parameter reference, holding the digit text.
    Positional(&'a str),
    /// A `$@`/`$*`/`$#`/`$?`/`$$`/`$!`/`$-` special parameter reference.
    SpecialParam(SpecialParam),
    /// A word containing one or more `$NAME`/`${NAME}` references mixed with literal text,
    /// e.g. `"prefix${x}suffix"` is `[Literal("prefix"), VarRef("x"), Literal("suffix")]`. Each
    /// part carries its own byte span (set once in `parser::word::interpolate`), rather than the
    /// whole word's span being reused for every part.
    Interpolated(Vec<Spanned<Self>>),
}

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

/// One I/O redirection attached to a [`Statement`] via [`Statement::Redirected`].
#[derive(Clone, PartialEq, Eq)]
pub struct Redirect<'a> {
    /// The explicit file descriptor the operator targets (`2` in `2>file`), if one was written.
    /// `None` means the operator's implicit default — fd 0 for the `<`-family, fd 1 for the
    /// `>`-family; meaningless for [`RedirectKind::Both`]/[`RedirectKind::BothAppend`], which
    /// always cover both 1 and 2 and never take a leading fd.
    pub fd: Option<u32>,
    /// Which redirection operator this is.
    pub kind: RedirectKind,
    /// What the operator points at.
    pub target: RedirectTarget<'a>,
}

/// A redirection operator — see [`Redirect::kind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectKind {
    /// `<` — redirect input from `target`.
    Input,
    /// `>` — redirect output to `target`, truncating it.
    Output,
    /// `>>` — redirect output to `target`, appending to it.
    Append,
    /// `>|` — like [`RedirectKind::Output`], but overrides `noclobber`.
    Clobber,
    /// `<>` — open `target` for both reading and writing.
    InputOutput,
    /// `<&` — duplicate or close an input file descriptor (`target` is an fd or `-`), or, as a
    /// Bash extension, redirect input from `target` as a plain file when it's neither.
    DuplicateInput,
    /// `>&` — duplicate or close an output file descriptor (`target` is an fd or `-`), or, as a
    /// Bash extension, redirect output to `target` as a plain file when it's neither.
    DuplicateOutput,
    /// `&>` — redirect both stdout and stderr to `target`, truncating it.
    Both,
    /// `&>>` — redirect both stdout and stderr to `target`, appending to it.
    BothAppend,
    /// `<<<` — feed `target`, expanded, to the command's stdin as a single line.
    HereString,
}

/// What a [`Redirect`] points at.
#[derive(Clone, PartialEq, Eq)]
pub enum RedirectTarget<'a> {
    /// An ordinary word — a file path for most [`RedirectKind`]s, or, for
    /// [`RedirectKind::HereString`], the literal text fed to stdin.
    File(Spanned<Expr<'a>>),
    /// A target file descriptor for [`RedirectKind::DuplicateInput`]/[`RedirectKind::DuplicateOutput`]
    /// (`2>&1`'s `1`).
    Fd(u32),
    /// `-`: closes the source file descriptor for [`RedirectKind::DuplicateInput`]/[`RedirectKind::DuplicateOutput`]
    /// (`2>&-`).
    Close,
}

impl Expr<'_> {
    /// Renders this expression's structural fragment, without the enclosing `(word ...)` —
    /// the piece [`fmt::Debug`] wraps once at the top, and [`Self::Interpolated`] reuses
    /// unwrapped for each of its parts so a mixed word reads as one flat node, e.g.
    /// `(interp "pid=" (special "$"))`, rather than nesting a `(word ...)` per part.
    #[expect(
        clippy::use_debug,
        reason = "quoting/escaping text the way Debug-for-str does is exactly the corpus rendering this fn produces, not a debugging remnant"
    )]
    fn fmt_fragment(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(s) => write!(f, "{s:?}"),
            Self::VarRef(name) => write!(f, "(var {name:?})"),
            Self::Positional(digits) => write!(f, "(positional {digits:?})"),
            Self::SpecialParam(param) => {
                use fmt::Write as _;

                let mut symbol = String::new();
                write!(symbol, "{param}")?;
                write!(f, "(special {symbol:?})")
            }
            Self::Interpolated(parts) => {
                write!(f, "(interp")?;

                for part in parts {
                    write!(f, " ")?;
                    part.inner.fmt_fragment(f)?;
                }

                write!(f, ")")
            }
        }
    }
}

impl fmt::Debug for Expr<'_> {
    /// Renders every variant as a `(word ...)` node, exposing `VarRef`/`Positional`/
    /// `SpecialParam`/`Interpolated` as their own structural shapes (see [`Self::fmt_fragment`])
    /// rather than collapsing back to reconstructed source text — corpus snapshots pin which
    /// kind of reference a word holds, not just what it reads back as.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(word ")?;
        self.fmt_fragment(f)?;
        write!(f, ")")
    }
}

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

impl fmt::Debug for Redirect<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(redirect")?;

        if let Some(fd) = self.fd {
            write!(f, " (fd {fd})")?;
        }

        write!(f, " \"{}\" {:?})", self.kind, self.target)
    }
}

impl fmt::Debug for RedirectTarget<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(expr) => write!(f, "{expr:?}"),
            Self::Fd(n) => write!(f, "(fd {n})"),
            Self::Close => write!(f, "(close)"),
        }
    }
}

impl fmt::Display for RedirectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Input => "<",
            Self::Output => ">",
            Self::Append => ">>",
            Self::Clobber => ">|",
            Self::InputOutput => "<>",
            Self::DuplicateInput => "<&",
            Self::DuplicateOutput => ">&",
            Self::Both => "&>",
            Self::BothAppend => "&>>",
            Self::HereString => "<<<",
        };

        write!(f, "{s}")
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

impl fmt::Display for Redirect<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(fd) = self.fd {
            write!(f, "{fd}")?;
        }

        write!(f, "{}", self.kind)?;

        match &self.target {
            RedirectTarget::File(expr) => write!(f, "{expr}"),
            RedirectTarget::Fd(n) => write!(f, "{n}"),
            RedirectTarget::Close => write!(f, "-"),
        }
    }
}

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Token::Word(w) => w.as_ref(),
            Token::Newline => "\n",
            Token::Semi => ";",
            Token::Pipe => "|",
            Token::Amp => "&",
            Token::LParen => "(",
            Token::RParen => ")",
            Token::LBrace => "{",
            Token::RBrace => "}",
            Token::Less => "<",
            Token::Greater => ">",
            Token::AndAnd => "&&",
            Token::OrOr => "||",
            Token::SemiSemi => ";;",
            Token::SemiAmp => ";&",
            Token::SemiSemiAmp => ";;&",
            Token::LessLess => "<<",
            Token::GreaterGreater => ">>",
            Token::LessAmp => "<&",
            Token::GreaterAmp => ">&",
            Token::LessGreater => "<>",
            Token::GreaterPipe => ">|",
            Token::LessLessMinus => "<<-",
            Token::LessLessLess => "<<<",
            Token::AmpGreater => "&>",
            Token::AmpGreaterGreater => "&>>",
            Token::PipeAmp => "|&",
            Token::AssignmentWord(w) => w,
            Token::Number(n) => n,
            Token::Eof => "",
        };

        write!(f, "{s}")
    }
}

impl fmt::Display for Expr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Literal(s) => write!(f, "{s}"),
            Expr::VarRef(v) => write!(f, "${v}"),
            Expr::Positional(digits) => write!(f, "${digits}"),
            Expr::SpecialParam(param) => write!(f, "${param}"),
            Expr::Interpolated(parts) => {
                for part in parts {
                    write!(f, "{part}")?;
                }

                Ok(())
            }
        }
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
