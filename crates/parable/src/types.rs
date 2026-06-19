//! The AST and token types shared by [`crate::lexer`], [`crate::parser`], and downstream
//! analysis — see [`Token`], [`Expr`], and [`Statement`].

use std::{borrow::Cow, fmt};

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
    /// `&` — background-execution suffix, or (within a [`Statement::List`]) a separator that
    /// runs the preceding pipeline asynchronously.
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

/// A parsed word, with any `$NAME`/`${NAME}` references resolved out of the surrounding literal
/// text — see `parser::word`.
#[derive(Clone, PartialEq, Eq)]
pub enum Expr<'a> {
    /// Plain text, not subject to further expansion.
    Literal(Cow<'a, str>),
    /// A `$NAME`/`${NAME}` reference, not yet resolved to a value.
    VarRef(&'a str),
    /// A word containing one or more `$NAME`/`${NAME}` references mixed with literal text,
    /// e.g. `"prefix${x}suffix"` is `[Literal("prefix"), VarRef("x"), Literal("suffix")]`.
    Interpolated(Vec<Self>),
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
        args: Vec<Expr<'a>>,
    },
    /// A `for NAME in ...; do ...; done` loop.
    ForLoop {
        /// The loop variable's name.
        variable: &'a str,
        /// The expressions iterated over, one loop body run per expression.
        iterable: Vec<Expr<'a>>,
        /// The statements run on each iteration.
        body: Vec<Self>,
    },
    /// An `if ...; then ...; else ...; fi` conditional. `elif` is not yet supported — see
    /// [`crate::parser`]'s module doc.
    If {
        /// The condition's statements; the branch taken depends on the last one's exit status.
        condition: Vec<Self>,
        /// The statements run when `condition`'s last statement succeeds.
        then_branch: Vec<Self>,
        /// The statements run when `condition`'s last statement fails, if an `else` clause is
        /// present.
        else_branch: Option<Vec<Self>>,
    },
    /// A `while ...; do ...; done` loop, repeating `body` for as long as `condition`'s last
    /// statement succeeds.
    While {
        /// The statements re-evaluated before each iteration.
        condition: Vec<Self>,
        /// The statements run on each iteration.
        body: Vec<Self>,
    },
    /// An `until ...; do ...; done` loop — like [`Statement::While`], but repeats `body` for as
    /// long as `condition`'s last statement fails.
    Until {
        /// The statements re-evaluated before each iteration.
        condition: Vec<Self>,
        /// The statements run on each iteration.
        body: Vec<Self>,
    },
    /// Two or more commands connected by `|`/`|&`, each command's stdout (and, for `|&`,
    /// stderr) feeding the next's stdin.
    Pipeline {
        /// The piped commands, in left-to-right order.
        commands: Vec<Self>,
    },
    /// A `(...)` subshell: `body` runs in a forked copy of the shell, so its side effects
    /// (variable assignments, `cd`, ...) don't affect the parent.
    Subshell {
        /// The statements run inside the subshell.
        body: Vec<Self>,
    },
    /// A `{ ...; }` brace group: `body` runs in the current shell, unlike [`Statement::Subshell`]
    /// — used to group commands for a redirect or background job without forking.
    BraceGroup {
        /// The statements run inside the group.
        body: Vec<Self>,
    },
    /// A sequence of pipelines joined by `;`/`&`/`&&`/`||`/newline separators.
    List {
        /// Each pipeline paired with the [`Token`] that follows it (its separator), or
        /// [`Token::Newline`] as an implicit terminator when none was lexed.
        items: Vec<(Self, Token<'a>)>,
    },
    /// `inner` with one or more [`Redirect`]s attached, in source order. A single wrapper
    /// variant rather than a `redirects` field on every other variant: Bash's own grammar
    /// attaches a `redirect_list` to any `compound_command` (`{ ...; } > file`) and folds
    /// redirects interleaved with a simple command's own words into one flat list regardless of
    /// where they fall (`cat < in.txt -n` and `cat -n < in.txt` mean the same thing) — both
    /// shapes are exactly "some other statement, plus redirects", so one wrapper covers both.
    Redirected {
        /// The statement being redirected.
        inner: Box<Self>,
        /// The redirects, in source order. Order matters only for fd-duplication chains, where
        /// each redirect sees the fd table as left by the previous one (`2>&1 1>&2` differs from
        /// `1>&2 2>&1`).
        redirects: Vec<Redirect<'a>>,
    },
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
    File(Expr<'a>),
    /// A target file descriptor for [`RedirectKind::DuplicateInput`]/[`RedirectKind::DuplicateOutput`]
    /// (`2>&1`'s `1`).
    Fd(u32),
    /// `-`: closes the source file descriptor for [`RedirectKind::DuplicateInput`]/[`RedirectKind::DuplicateOutput`]
    /// (`2>&-`).
    Close,
}

impl fmt::Debug for Expr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Literal(s) => write!(f, "(word {s:?})"),
            Expr::VarRef(v) => write!(f, "(var_ref {v:?})"),
            Expr::Interpolated(parts) => {
                write!(f, "(interp")?;

                for part in parts {
                    write!(f, " {part:?}")?;
                }

                write!(f, ")")
            }
        }
    }
}

impl fmt::Debug for Statement<'_> {
    #[expect(
        clippy::too_many_lines,
        reason = "AST pretty-printer enumerates every Statement variant"
    )]
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
                write!(f, "(for {variable} (")?;

                for (i, iter) in iterable.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }

                    write!(f, "{iter:?}")?;
                }

                write!(f, ") (")?;

                for (i, stmt) in body.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }

                    write!(f, "{stmt:?}")?;
                }

                write!(f, "))")
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                write!(f, "(if (")?;

                for (i, stmt) in condition.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }

                    write!(f, "{stmt:?}")?;
                }

                write!(f, ") (")?;

                for (i, stmt) in then_branch.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }

                    write!(f, "{stmt:?}")?;
                }

                if let Some(else_b) = else_branch {
                    write!(f, ") (")?;

                    for (i, stmt) in else_b.iter().enumerate() {
                        if i > 0 {
                            write!(f, " ")?;
                        }

                        write!(f, "{stmt:?}")?;
                    }
                }

                write!(f, "))")
            }
            Statement::While { condition, body } => {
                write!(f, "(while (")?;

                for (i, stmt) in condition.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }

                    write!(f, "{stmt:?}")?;
                }

                write!(f, ") (")?;

                for (i, stmt) in body.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }

                    write!(f, "{stmt:?}")?;
                }

                write!(f, "))")
            }
            Statement::Until { condition, body } => {
                write!(f, "(until (")?;

                for (i, stmt) in condition.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }

                    write!(f, "{stmt:?}")?;
                }

                write!(f, ") (")?;

                for (i, stmt) in body.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }

                    write!(f, "{stmt:?}")?;
                }

                write!(f, "))")
            }
            Statement::Pipeline { commands } => {
                write!(f, "(pipeline")?;

                for cmd in commands {
                    write!(f, " {cmd:?}")?;
                }

                write!(f, ")")
            }
            Statement::Subshell { body } => {
                write!(f, "(subshell")?;

                for stmt in body {
                    write!(f, " {stmt:?}")?;
                }

                write!(f, ")")
            }
            Statement::BraceGroup { body } => {
                write!(f, "(brace_group")?;

                for stmt in body {
                    write!(f, " {stmt:?}")?;
                }

                write!(f, ")")
            }
            Statement::List { items } => {
                write!(f, "(list")?;

                for (stmt, _sep) in items {
                    write!(f, " {stmt:?}")?;
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
    #[expect(
        clippy::too_many_lines,
        reason = "AST pretty-printer enumerates every Statement variant"
    )]
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

                write!(f, "; do ")?;

                for (i, stmt) in body.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }

                    write!(f, "{stmt}")?;
                }

                write!(f, "; done")
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                write!(f, "if ")?;

                for (i, stmt) in condition.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }

                    write!(f, "{stmt}")?;
                }

                write!(f, "; then ")?;

                for (i, stmt) in then_branch.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }

                    write!(f, "{stmt}")?;
                }

                if let Some(else_b) = else_branch {
                    write!(f, "; else ")?;

                    for (i, stmt) in else_b.iter().enumerate() {
                        if i > 0 {
                            write!(f, "; ")?;
                        }

                        write!(f, "{stmt}")?;
                    }
                }

                write!(f, "; fi")
            }
            Statement::While { condition, body } => {
                write!(f, "while ")?;

                for (i, stmt) in condition.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }

                    write!(f, "{stmt}")?;
                }

                write!(f, "; do ")?;

                for (i, stmt) in body.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }

                    write!(f, "{stmt}")?;
                }

                write!(f, "; done")
            }
            Statement::Until { condition, body } => {
                write!(f, "until ")?;

                for (i, stmt) in condition.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }

                    write!(f, "{stmt}")?;
                }

                write!(f, "; do ")?;

                for (i, stmt) in body.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }

                    write!(f, "{stmt}")?;
                }

                write!(f, "; done")
            }
            Statement::Pipeline { commands } => {
                for (i, cmd) in commands.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }

                    write!(f, "{cmd}")?;
                }

                Ok(())
            }
            Statement::Subshell { body } => {
                write!(f, "(")?;

                for (i, stmt) in body.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }

                    write!(f, "{stmt}")?;
                }

                write!(f, ")")
            }
            Statement::BraceGroup { body } => {
                write!(f, "{{ ")?;

                for (i, stmt) in body.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }

                    write!(f, "{stmt}")?;
                }

                write!(f, "; }}")
            }
            Statement::List { items } => {
                let mut needs_space = false;

                for (stmt, sep) in items {
                    if needs_space {
                        write!(f, " ")?;
                    }

                    write!(f, "{stmt}")?;

                    match sep {
                        Token::Eof => {
                            needs_space = true;
                        }
                        Token::Newline => {
                            writeln!(f)?;
                            needs_space = false;
                        }
                        Token::Semi => {
                            write!(f, ";")?;
                            needs_space = true;
                        }
                        Token::Amp => {
                            write!(f, "&")?;
                            needs_space = true;
                        }
                        _ => {
                            write!(f, " {sep}")?;
                            needs_space = true;
                        }
                    }
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
