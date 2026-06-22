//! Captures a heredoc's body from raw, not-yet-lexed source text — see
//! [`crate::stream::TokenStream::capture_heredoc`] for why this runs eagerly, right when the
//! delimiter word is parsed, rather than waiting for the line's terminating newline to actually
//! be lexed.
//!
//! ## Core Design
//!
//! [`capture`] never advances the lexer's real cursor — it only reads a borrowed slice of the
//! still-unconsumed input. The redirect that calls it folds the byte count it reports into
//! [`crate::types::LexerState::heredoc_skip`], which the lexer spends later, once it actually
//! lexes the real newline ending the current line (see `crate::lexer::LexerStream::lex_token_with_start`).

use std::borrow::Cow;

/// Strips one leading run of tab characters from `line`, if `strip_tabs` (the `<<-` form).
#[must_use = "stripping leading tabs has no effect unless the caller uses the result"]
fn strip_leading_tabs(line: &str, strip_tabs: bool) -> &str {
    if strip_tabs {
        line.trim_start_matches('\t')
    } else {
        line
    }
}

/// Reads one logical heredoc line from the start of `text`. When `splice`, a physical line
/// ending in an odd run of trailing backslashes is joined directly onto the next physical line
/// (dropping just the final backslash and the newline between them), repeating across any
/// number of chained continuations — Bash's line-continuation rule, which a quoted heredoc
/// delimiter disables entirely (`splice: false`).
///
/// Returns the joined content (its own trailing newline excluded), the number of raw bytes
/// consumed (including every newline folded into a join, plus the final one if `text` had one),
/// and whether that final newline was present (`false` only at the unterminated end of `text`).
#[must_use = "reading a logical line has no effect unless the caller uses the result"]
fn read_logical_line(text: &str, splice: bool) -> (Cow<'_, str>, usize, bool) {
    let mut consumed = 0_usize;
    let mut owned: Option<String> = None;

    loop {
        let rest = text.get(consumed..).unwrap_or_default();
        let newline_at = rest.find('\n');
        let piece = newline_at.map_or(rest, |i| rest.get(..i).unwrap_or_default());

        let trailing_backslashes = piece.chars().rev().take_while(|&c| c == '\\').count();
        let should_splice =
            splice && newline_at.is_some() && !trailing_backslashes.is_multiple_of(2);

        if should_splice {
            let kept = piece
                .get(..piece.len().saturating_sub(1))
                .unwrap_or_default();

            match &mut owned {
                Some(joined) => joined.push_str(kept),
                None => owned = Some(kept.to_owned()),
            }

            consumed = consumed.saturating_add(piece.len()).saturating_add(1);
            continue;
        }

        let line = owned.take().map_or(Cow::Borrowed(piece), |mut joined| {
            joined.push_str(piece);
            Cow::Owned(joined)
        });

        consumed = consumed.saturating_add(piece.len());
        let had_newline = newline_at.is_some();

        if had_newline {
            consumed = consumed.saturating_add(1);
        }

        return (line, consumed, had_newline);
    }
}

/// Captures one heredoc's body from `text` — the lexer's remaining input, read right after the
/// delimiter word was lexed. The body itself starts `already_claimed` bytes past the end of the
/// *current* source line: bytes already spoken for by earlier heredocs queued on this same line
/// (`cmd <<EOF1 <<EOF2` reads `EOF1`'s body before `EOF2`'s, even though both operators precede
/// either body in the source). Reads logical lines (via [`read_logical_line`], splicing trailing
/// `\`-newline continuations when `splice`) until one, after `strip_tabs`'s tab-stripping, equals
/// `delimiter`, or `text` runs out (an unterminated heredoc simply ends at EOF, matching Bash's
/// own lenient, warning-only handling of that case).
///
/// Returns the body and how many bytes *this* heredoc itself consumed, starting right after the
/// current line ends and `already_claimed` bytes further in — the value the caller folds into
/// the next heredoc's own `already_claimed`, and ultimately into what the lexer skips once it
/// actually lexes the line's terminating newline.
#[must_use = "capturing a heredoc body has no effect unless the caller uses the result"]
pub fn capture<'a>(
    text: &'a str,
    already_claimed: usize,
    delimiter: &str,
    strip_tabs: bool,
    splice: bool,
) -> (Cow<'a, str>, usize) {
    let line_end = text.find('\n').map_or(text.len(), |i| i.saturating_add(1));
    let start = line_end.saturating_add(already_claimed);

    let mut body = String::new();
    let mut pos = start;

    loop {
        let rest = text.get(pos..).unwrap_or_default();

        if rest.is_empty() {
            break;
        }

        let (line, consumed, had_newline) = read_logical_line(rest, splice);
        let stripped = strip_leading_tabs(&line, strip_tabs);

        pos = pos.saturating_add(consumed);

        if stripped == delimiter {
            break;
        }

        body.push_str(stripped);

        if had_newline {
            body.push('\n');
        }
    }

    (Cow::Owned(body), pos.saturating_sub(start))
}
