//! Redirection operators (`<`, `>`, `>>`, `>|`, `<>`, `<&`, `>&`, `&>`, `&>>`, `<<<`) attached to
//! a [`Statement`] via [`Statement::Redirected`] — see [`parse_redirect`].
//!
//! Heredocs (`<<`, `<<-`) are deliberately not handled here: capturing their body requires the
//! lexer to switch into a line-scanning mode keyed off the delimiter word
//! ([`LexerState::heredoc_delimiter`](crate::types::LexerState::heredoc_delimiter) is reserved
//! for exactly that, but unimplemented) — until it exists, `<<`/`<<-` are left unparsed, same as
//! before this module existed.

use super::{ParserStream, word::parse_literal};

use crate::types::{Redirect, RedirectKind, RedirectTarget, Token};

use winnow::{ModalResult, Parser as _, stream::Stream as _, token::any};

/// Maps a lexed operator token to its [`RedirectKind`], or `None` if `token` isn't a supported
/// redirect operator — in particular, `<<`/`<<-` (heredocs) are deliberately excluded; see the
/// module doc. The single authoritative token-to-kind mapping, shared by [`parse_kind`] (which
/// consumes the token) and [`accepts_fd_prefix`]'s lookahead (which only peeks it).
#[must_use = "mapping a token has no effect unless the caller uses the result"]
const fn token_to_kind(token: &Token<'_>) -> Option<RedirectKind> {
    Some(match token {
        Token::Less => RedirectKind::Input,
        Token::Greater => RedirectKind::Output,
        Token::GreaterGreater => RedirectKind::Append,
        Token::GreaterPipe => RedirectKind::Clobber,
        Token::LessGreater => RedirectKind::InputOutput,
        Token::LessAmp => RedirectKind::DuplicateInput,
        Token::GreaterAmp => RedirectKind::DuplicateOutput,
        Token::AmpGreater => RedirectKind::Both,
        Token::AmpGreaterGreater => RedirectKind::BothAppend,
        Token::LessLessLess => RedirectKind::HereString,
        _ => return None,
    })
}

/// Whether `kind` accepts a leading file-descriptor number (`2` in `2>file`). Bash allows one
/// before every supported operator except [`RedirectKind::Both`]/[`RedirectKind::BothAppend`]
/// (`&>`/`&>>`), where the `&` already implies both fd 1 and fd 2.
#[must_use = "checking a kind has no effect unless the caller uses the result"]
const fn accepts_fd_prefix(kind: RedirectKind) -> bool {
    !matches!(kind, RedirectKind::Both | RedirectKind::BothAppend)
}

/// Whether `word` is non-empty and every character is an ASCII digit.
#[must_use = "checking a word has no effect unless the caller uses the result"]
fn is_all_digits(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_digit())
}

/// Consumes a leading file-descriptor number (`2` in `2>file`) if the token at the cursor is an
/// all-digit word immediately followed by an operator that [`accepts_fd_prefix`]. Peeks rather
/// than committing: a bare digit word that *isn't* followed by such an operator is an ordinary
/// argument, not part of a redirect, so it's left untouched (stream unchanged) for the caller to
/// parse as one.
#[must_use = "peeking the fd prefix has no effect unless the caller uses the result"]
fn parse_fd_prefix(input: &mut ParserStream<'_>) -> Option<u32> {
    let checkpoint = input.checkpoint();

    let result: ModalResult<Token<'_>> = any
        .verify(|t: &Token<'_>| matches!(t, Token::Word(w) if is_all_digits(w)))
        .parse_next(input);

    let Ok(Token::Word(word)) = result else {
        return None;
    };

    let next_accepts = input
        .peek_token()
        .and_then(|t| token_to_kind(&t))
        .is_some_and(accepts_fd_prefix);

    if !next_accepts {
        input.reset(&checkpoint);
        return None;
    }

    word.parse().ok()
}

/// Consumes the redirect-operator token at the cursor and maps it to a [`RedirectKind`].
/// Backtracks (no token consumed) if the cursor isn't at a supported operator.
#[must_use = "parses the operator; discarding ignores syntax structures"]
fn parse_kind<'a>(input: &mut ParserStream<'a>) -> ModalResult<RedirectKind> {
    any.verify_map(|t: Token<'a>| token_to_kind(&t))
        .parse_next(input)
}

/// Parses the fd-or-`-` target of a [`RedirectKind::DuplicateInput`]/[`RedirectKind::DuplicateOutput`]
/// redirect (`1` in `>&1`, `-` in `>&-`). Backtracks if the next word is neither, leaving it for
/// [`parse_target`]'s [`RedirectTarget::File`] fallback (Bash's `>&filename` extension).
#[must_use = "parses the dup target; discarding ignores syntax structures"]
fn parse_dup_target<'a>(input: &mut ParserStream<'a>) -> ModalResult<RedirectTarget<'a>> {
    any.verify_map(|t: Token<'a>| match t {
        Token::Word(w) if w.as_ref() == "-" => Some(RedirectTarget::Close),
        Token::Word(w) if is_all_digits(&w) => w.parse().ok().map(RedirectTarget::Fd),
        _ => None,
    })
    .parse_next(input)
}

/// Parses what `kind`'s operator points at: an fd or `-` for the fd-duplication operators, an
/// ordinary word (via [`parse_literal`]) otherwise.
#[must_use = "parses the target; discarding ignores syntax structures"]
fn parse_target<'a>(
    kind: RedirectKind,
    input: &mut ParserStream<'a>,
) -> ModalResult<RedirectTarget<'a>> {
    if matches!(
        kind,
        RedirectKind::DuplicateInput | RedirectKind::DuplicateOutput
    ) && let Ok(target) = parse_dup_target(input)
    {
        return Ok(target);
    }

    Ok(RedirectTarget::File(parse_literal(input)?))
}

/// Parses one redirection — an optional leading fd, an operator, and its target — at the
/// cursor. Backtracks cleanly (no token consumed) if the cursor isn't at a redirect operator,
/// including the case where a leading digit word turned out not to be an fd prefix after all.
#[must_use = "parses a redirect; discarding ignores syntax structures"]
pub(super) fn parse_redirect<'a>(input: &mut ParserStream<'a>) -> ModalResult<Redirect<'a>> {
    let fd = parse_fd_prefix(input);
    let kind = parse_kind(input)?;
    let target = parse_target(kind, input)?;

    Ok(Redirect { fd, kind, target })
}
