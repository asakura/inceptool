use super::{Expr, Spanned};

use std::borrow::Cow;
use std::fmt;

/// One I/O redirection attached to a [`crate::types::Statement`] via
/// [`crate::types::Statement::Redirected`].
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
    /// `<<` — opens a heredoc: `target` is a [`RedirectTarget::Heredoc`].
    Heredoc,
    /// `<<-` — like [`RedirectKind::Heredoc`], but the body's (and delimiter line's) leading tabs
    /// are stripped before matching.
    HeredocStripTabs,
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
    /// [`RedirectKind::Heredoc`]/[`RedirectKind::HeredocStripTabs`]'s delimiter and captured
    /// body. `delimiter` is the word exactly as written (quotes/escapes included, if any) —
    /// see `strip_delimiter_quoting` for the bare identifier actually matched against each
    /// candidate end-of-heredoc line.
    Heredoc {
        /// The delimiter word, verbatim.
        delimiter: Cow<'a, str>,
        /// The captured body, already tab-stripped (for [`RedirectKind::HeredocStripTabs`]) and
        /// line-spliced (unless the delimiter was quoted).
        body: Cow<'a, str>,
    },
}

/// Bash quote-removal for a heredoc delimiter word: strips `'...'`/`"..."` quote markers and
/// backslash escapes, producing the bare identifier matched against each candidate
/// end-of-heredoc line. Also reports whether anything was actually quoted/escaped — a quoted
/// delimiter takes its heredoc body completely literally, with no backslash-newline
/// line-continuation splicing.
///
/// Shared by `parser::redirect` (computing the inputs to
/// [`crate::stream::TokenStream::capture_heredoc`]) and this module's own round-trip `Display`
/// (regenerating the terminator line from the verbatim [`RedirectTarget::Heredoc::delimiter`]).
#[must_use = "stripping delimiter quoting has no effect unless the caller uses the result"]
pub fn strip_delimiter_quoting(raw: &str) -> (String, bool) {
    let mut out = String::new();
    let mut quoted = false;
    let mut chars = raw.chars();

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                quoted = true;

                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '\'' => {
                quoted = true;

                for inner in chars.by_ref() {
                    if inner == '\'' {
                        break;
                    }

                    out.push(inner);
                }
            }
            '"' => {
                quoted = true;

                for inner in chars.by_ref() {
                    if inner == '"' {
                        break;
                    }

                    out.push(inner);
                }
            }
            other => out.push(other),
        }
    }

    (out, quoted)
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
            Self::Heredoc { delimiter, body } => {
                write!(
                    f,
                    "(heredoc \"{delimiter}\" \"{}\")",
                    body.replace('\n', "\\n")
                )
            }
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
            Self::Heredoc => "<<",
            Self::HeredocStripTabs => "<<-",
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
            RedirectTarget::Heredoc { delimiter, .. } => write!(f, "{delimiter}"),
        }
    }
}

impl Redirect<'_> {
    /// Writes this redirect's heredoc body and terminator line (the body, then the
    /// quote-stripped delimiter, then a newline) — nothing for any other [`RedirectTarget`].
    /// Does *not* write the leading newline that separates this from whatever precedes it on the
    /// rendered line — the caller writes exactly one of those, shared across every pending
    /// heredoc on that line, since real Bash captures all of them back-to-back with no blank
    /// line in between.
    ///
    /// Deliberately *not* part of [`Display for Redirect`](Redirect)'s own `fmt`: real Bash
    /// defers every heredoc body on a line until that line's actual terminating newline, however
    /// many further tokens (`&&`, `;`, `do`/`done`, ...) render onto that same output line after
    /// the `<<DELIM` operator. `Statement`'s `Display` impl collects every heredoc redirect on
    /// the statement's line into a pending list as it writes that line, then calls this once per
    /// entry only after the whole line is written.
    #[must_use = "writing the heredoc body has no effect unless the caller propagates the result"]
    pub(crate) fn write_heredoc_body(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let RedirectTarget::Heredoc { delimiter, body } = &self.target else {
            return Ok(());
        };

        let (stripped, _) = strip_delimiter_quoting(delimiter);

        writeln!(f, "{body}{stripped}")
    }
}
