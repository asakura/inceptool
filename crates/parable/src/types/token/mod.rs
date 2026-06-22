use std::borrow::Cow;

mod fmt;

/// A single lexical token, as produced by [`crate::lexer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token<'a> {
    /// A run of word content, not yet classified as a keyword or split into an
    /// [`crate::types::Expr`] — see `parser::word`.
    Word(Cow<'a, str>),
    /// A line terminator. Significant as a statement separator, unlike inline whitespace (which
    /// the lexer simply skips without emitting a token).
    Newline,

    // Single-char operators
    /// `;` — sequential command separator (`cmd1; cmd2`).
    Semi,
    /// `|` — pipeline separator, connecting one command's stdout to the next's stdin.
    Pipe,
    /// `&` — background-execution suffix; see [`crate::types::Statement::Background`].
    Amp,
    /// `(` — opens a [`crate::types::Statement::Subshell`].
    LParen,
    /// `)` — closes a [`crate::types::Statement::Subshell`].
    RParen,
    /// `{` — opens a [`crate::types::Statement::BraceGroup`].
    LBrace,
    /// `}` — closes a [`crate::types::Statement::BraceGroup`].
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
