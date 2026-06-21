use std::fmt;

use super::{Expr, Spanned};

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
