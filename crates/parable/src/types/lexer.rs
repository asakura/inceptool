/// Tracks the current parsing context of the Bash script.
#[derive(Debug, Clone, Copy, Default)]
pub struct LexerState {
    /// Whether the lexer is currently inside a `$((...))`/`((...))` arithmetic context, where
    /// metacharacters that would otherwise end a word (e.g. `<`, `>`) are plain operators.
    pub in_arithmetic: bool,
    /// Total bytes of heredoc body (+ delimiter line) material, for every `<<`/`<<-` already
    /// registered while scanning the current source line, that the next-lexed
    /// [`crate::types::Token::Newline`] must skip over before resuming normal tokenization — see
    /// `crate::stream::TokenStream::capture_heredoc` (which accumulates this) and
    /// `crate::lexer::LexerStream::lex_token_with_start` (which spends it).
    pub heredoc_skip: usize,
}
