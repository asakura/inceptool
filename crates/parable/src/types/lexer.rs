use std::borrow::Cow;

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
