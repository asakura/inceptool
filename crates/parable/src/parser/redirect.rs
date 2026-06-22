//! Redirection operators (`<`, `>`, `>>`, `>|`, `<>`, `<&`, `>&`, `&>`, `&>>`, `<<`, `<<-`, `<<<`)
//! attached to a [`crate::Statement`] via [`crate::Statement::Redirected`] — see `parse_redirect`.
//!
//! Heredocs (`<<`/`<<-`) are the one redirect kind whose target isn't just the next word:
//! [`parse_heredoc_target`] reads the delimiter normally, then immediately captures the body via
//! [`crate::stream::TokenStream::capture_heredoc`] — see that method, and
//! [`crate::lexer::heredoc`], for how the body is read off raw, not-yet-lexed source text without
//! waiting for the line's terminating newline to actually be lexed.

use super::{Expected, ParserStream, expect_command, word::parse_literal};

use crate::types::{Redirect, RedirectKind, RedirectTarget, Token, strip_delimiter_quoting};

use winnow::{
    ModalResult, Parser as _, combinator::cut_err, error::ErrMode, stream::Stream as _, token::any,
};

/// Maps a lexed operator token to its [`RedirectKind`], or `None` if `token` isn't a supported
/// redirect operator. The single authoritative token-to-kind mapping, shared by [`parse_kind`]
/// (which consumes the token) and [`accepts_fd_prefix`]'s lookahead (which only peeks it).
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
        Token::LessLess => RedirectKind::Heredoc,
        Token::LessLessMinus => RedirectKind::HeredocStripTabs,
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
///
/// The [`parse_literal`] fallback is wrapped in [`cut_err`]: by the time this runs, [`parse_kind`]
/// has already committed to a redirect operator, so a missing target is a real syntax error
/// (`cat >` is invalid Bash), not "this wasn't a redirect after all" — without the cut, the
/// `Backtrack` [`parse_literal`] returns on a missing word would make whatever `repeat`/`alt`
/// invoked [`parse_redirect`] silently roll back past the operator it already consumed, dropping
/// it from the rendered output instead of reporting the error.
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

    let expr = cut_err(expect_command(
        Expected::Standalone("redirect target"),
        parse_literal,
    ))
    .parse_next(input)?;

    Ok(RedirectTarget::File(expr))
}

/// Parses a heredoc's delimiter word, then immediately captures its body via
/// [`crate::stream::TokenStream::capture_heredoc`] — see the module doc and that method for why
/// capturing now, rather than once the line's terminating newline is actually lexed, is safe.
///
/// The delimiter word itself is [`cut_err`]-wrapped for the same reason [`parse_target`]'s
/// fallback is: `<<`/`<<-` has already been committed by [`parse_kind`], so a missing delimiter
/// (`cat <<`) is a real syntax error, not a sign this wasn't a heredoc redirect.
#[must_use = "parses the heredoc target; discarding ignores syntax structures"]
fn parse_heredoc_target<'a>(
    input: &mut ParserStream<'a>,
    kind: RedirectKind,
) -> ModalResult<RedirectTarget<'a>> {
    let delimiter = cut_err(expect_command(
        Expected::Standalone("heredoc delimiter"),
        any.verify_map(|t: Token<'a>| match t {
            Token::Word(w) => Some(w),
            _ => None,
        }),
    ))
    .parse_next(input)?;

    let (effective, quoted) = strip_delimiter_quoting(&delimiter);
    let strip_tabs = matches!(kind, RedirectKind::HeredocStripTabs);
    let body = input.capture_heredoc(&effective, strip_tabs, !quoted);

    Ok(RedirectTarget::Heredoc { delimiter, body })
}

/// Parses one redirection — an optional leading fd, an operator, and its target — at the
/// cursor. Backtracks cleanly (no token consumed) if the cursor isn't at a redirect operator,
/// including the case where a leading digit word turned out not to be an fd prefix after all.
#[must_use = "parses a redirect; discarding ignores syntax structures"]
fn parse_redirect<'a>(input: &mut ParserStream<'a>) -> ModalResult<Redirect<'a>> {
    let fd = parse_fd_prefix(input);
    let kind = parse_kind(input)?;

    let target = match kind {
        RedirectKind::Heredoc | RedirectKind::HeredocStripTabs => {
            parse_heredoc_target(input, kind)?
        }
        _ => parse_target(kind, input)?,
    };

    Ok(Redirect { fd, kind, target })
}

/// Parses one redirect at the cursor like [`parse_redirect`], but turns its "no redirect here"
/// `Backtrack` into `Ok(None)` rather than an `Err` a caller's `while let Ok(..) = ...` loop would
/// otherwise have to (and, before this existed, did) treat identically to a `Cut` — silently
/// stopping the loop either way and dropping an already-committed operator like the `<<` in
/// `cat <<` instead of reporting its missing target as the syntax error it is. Every redirect-
/// collection loop in this crate calls this, never [`parse_redirect`] directly, so a `Cut` always
/// propagates via `?`.
#[must_use = "parses a redirect; discarding ignores syntax structures"]
pub(super) fn parse_redirect_opt<'a>(
    input: &mut ParserStream<'a>,
) -> ModalResult<Option<Redirect<'a>>> {
    match parse_redirect(input) {
        Ok(r) => Ok(Some(r)),
        Err(ErrMode::Backtrack(_)) => Ok(None),
        Err(e) => Err(e),
    }
}
