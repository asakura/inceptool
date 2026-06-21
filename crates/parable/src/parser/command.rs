//! Simple commands, pipelines, and command lists — [`parse_base_command`], [`parse_pipeline`],
//! [`parse_and_or`], [`parse_list`].

use super::{
    redirect::parse_redirect,
    word::parse_literal,
    {ParserStream, attach_redirects, parse_command, skip_newlines, spanned},
};

use crate::types::{LogicalOp, PipeOp, Spanned, Statement, Token};

use winnow::{
    ModalResult, Parser as _,
    combinator::{alt, cut_err, opt},
    token::any,
};

/// Parses a simple command: a name, its arguments, and any redirects — interleaved with the
/// arguments in any order (`cat < in.txt -n` and `cat -n < in.txt` parse the same way), plus any
/// leading the command name itself (`< in.txt cat`). Wraps the result in
/// [`Statement::Redirected`] only when at least one redirect was found, so every redirect-free
/// command parses exactly as before this existed.
#[must_use = "parses a base command; discarding ignores syntax structures"]
pub(super) fn parse_base_command<'a>(
    input: &mut ParserStream<'a>,
) -> ModalResult<Spanned<Statement<'a>>> {
    let start_offset = input.current_span_start();
    let mut redirects = Vec::new();

    while let Ok(r) = parse_redirect(input) {
        redirects.push(r);
    }

    let name_start = input.current_span_start();
    let name = any
        .verify_map(|t| match t {
            Token::Word(word) => Some(word),
            _ => None,
        })
        .parse_next(input)?;

    let mut bare_end = input.previous_span_end();
    let mut args = Vec::new();

    loop {
        if let Ok(r) = parse_redirect(input) {
            redirects.push(r);
            continue;
        }

        let Ok(expr) = parse_literal(input) else {
            break;
        };

        bare_end = input.previous_span_end();
        args.push(expr);
    }

    // `bare_end` tracks only the name/args' own end, never a redirect's — so this span covers
    // exactly the bare command text, even when a redirect is interleaved before, between, or
    // after them (`cat < in.txt -n` and `echo a >file` both exclude the redirect here).
    let cmd = Spanned {
        inner: Statement::Command { name, args },
        span: name_start..bare_end,
    };

    Ok(attach_redirects(cmd, redirects, start_offset, input))
}

#[must_use = "parses a pipeline; discarding ignores syntax structures"]
fn parse_pipeline<'a>(input: &mut ParserStream<'a>) -> ModalResult<Spanned<Statement<'a>>> {
    let start_offset = input.current_span_start();
    let first = parse_command(input)?;
    let mut tail: Vec<(PipeOp, Spanned<Statement<'a>>)> = Vec::new();

    while let Ok(pipe_token) = {
        let res: ModalResult<Token<'_>> = any
            .verify(|t| matches!(t, Token::Pipe) | matches!(t, Token::PipeAmp))
            .parse_next(input);

        res
    } {
        let op = match pipe_token {
            Token::PipeAmp => PipeOp::StdoutStderr,
            _ => PipeOp::Stdout,
        };
        skip_newlines(input);
        let next_cmd = cut_err(super::expect_command(
            super::Expected::Command,
            super::parse_command,
        ))
        .parse_next(input)?;
        tail.push((op, next_cmd));
    }

    if tail.is_empty() {
        Ok(first)
    } else {
        Ok(spanned(
            start_offset,
            input,
            Statement::Pipeline {
                head: Box::new(first),
                tail,
            },
        ))
    }
}

/// Parses an `and_or` chain: one or more pipelines joined by `&&`/`||`, left-associative —
/// `a && b || c` becomes `AndOr(AndOr(a, And, b), Or, c)`. Bash's `&&`/`||` bind tighter than
/// `;`/`&`/newline, so this sits between [`parse_pipeline`] and [`parse_list`]: every `&&`/`||`
/// chain folds into a single [`Statement`] before [`parse_list`] ever sees a `;`/`&`/newline
/// separator, which matters for `&`'s scope — `a && b &` must background the whole `a && b`
/// conjunction, not just `b`.
#[must_use = "parses an and-or chain; discarding ignores syntax structures"]
fn parse_and_or<'a>(input: &mut ParserStream<'a>) -> ModalResult<Spanned<Statement<'a>>> {
    let start_offset = input.current_span_start();
    let mut current = parse_pipeline(input)?;

    loop {
        let op_result: ModalResult<LogicalOp> = alt((
            any.verify(|t| matches!(t, Token::AndAnd))
                .value(LogicalOp::And),
            any.verify(|t| matches!(t, Token::OrOr))
                .value(LogicalOp::Or),
        ))
        .parse_next(input);

        let Ok(op) = op_result else {
            break;
        };

        // Unlike the operator check above, a missing right-hand pipeline here isn't "no more
        // chain" — `&&`/`||` was already consumed, so Bash requires an operand to follow. Letting
        // this `?` propagate (rather than swallowing the error and silently ending the chain)
        // matters once it's a `Cut` error from a malformed compound command: see
        // `parser::control_flow`'s empty-body tests for what silently swallowing it used to do.
        skip_newlines(input);
        let right = cut_err(super::expect_command(
            super::Expected::Command,
            parse_pipeline,
        ))
        .parse_next(input)?;

        current = spanned(
            start_offset,
            input,
            Statement::AndOr {
                left: Box::new(current),
                op,
                right: Box::new(right),
            },
        );
    }

    Ok(current)
}

/// Parses a `list`: one or more `and_or` chains joined by `;`/`&`/newline, left-associative. A
/// lone trailing `;`/newline is dropped (`echo foo;` parses the same as `echo foo`) since neither
/// changes execution; a lone trailing `&` is kept as a unary [`Statement::Background`] since
/// backgrounding is meaningful even with nothing following it.
#[must_use = "parses a command list; discarding ignores syntax structures"]
pub(super) fn parse_list<'a>(input: &mut ParserStream<'a>) -> ModalResult<Spanned<Statement<'a>>> {
    parse_list_until(super::Expected::Command, input, |_| false)
}

/// Parses a `list`, like [`parse_list`], but stops folding once `stop` reports the next token
/// starts the caller's own closing delimiter — used by `grouping::parse_brace_group` so a
/// trailing `;` before `}` doesn't make this function try to parse one `and_or` too many and
/// hand `}` (which lexes as a plain [`Token::Word`]) to [`parse_base_command`] as a command name.
#[must_use = "parses a command list; discarding ignores syntax structures"]
pub(super) fn parse_list_until<'a, F>(
    expected: super::Expected,
    input: &mut ParserStream<'a>,
    stop: F,
) -> ModalResult<Spanned<Statement<'a>>>
where
    F: Fn(&ParserStream<'a>) -> bool,
{
    let start_offset = input.current_span_start();
    skip_newlines(input);

    let mut current = cut_err(super::expect_command(expected, parse_and_or)).parse_next(input)?;

    loop {
        let sep_result: ModalResult<Token<'_>> = any
            .verify(|t| {
                matches!(t, Token::Semi) || matches!(t, Token::Amp) || matches!(t, Token::Newline)
            })
            .parse_next(input);

        let Ok(sep) = sep_result else {
            break;
        };

        skip_newlines(input);

        // A backtrack here genuinely means "no further item" (the next token isn't anything
        // `parse_and_or` can start, e.g. the `stop` delimiter `at_keyword` didn't recognize, or
        // end of input) — `opt` swallows exactly that. A `Cut` error (a malformed compound
        // command that's already committed past its leading keyword) is not "no further item"
        // and must propagate, which is why this isn't the unconditional `.ok()` it used to be.
        let next = if stop(input) {
            None
        } else {
            opt(parse_and_or).parse_next(input)?
        };

        let inner = match (sep, next) {
            (Token::Amp, next) => Statement::Background {
                left: Box::new(current),
                right: next.map(Box::new),
            },
            (_, Some(next)) => Statement::Sequence {
                left: Box::new(current),
                right: Box::new(next),
            },
            (_, None) => break,
        };

        current = spanned(start_offset, input, inner);
    }

    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::stream::TokenStream;

    use core::assert_matches;

    #[derive(Debug, thiserror::Error)]
    enum TestError {
        #[error("Test failure: {0}")]
        Failure(String),
    }

    mod parse_base_command {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn redirected_command_span_excludes_the_redirect() -> Result<(), TestError> {
            let mut stream = TokenStream::new("echo a >file");
            let parsed = super::parse_base_command(&mut stream)
                .map_err(|e| TestError::Failure(e.to_string()))?;

            assert_eq!(parsed.span, 0..12);

            let Statement::Redirected { inner: bare, .. } = parsed.inner else {
                return Err(TestError::Failure("expected a Redirected statement".into()));
            };

            assert_eq!(bare.span, 0..6);

            Ok(())
        }

        #[rstest]
        fn leading_redirect_is_excluded_from_the_bare_command_span() -> Result<(), TestError> {
            let mut stream = TokenStream::new("< in.txt cat");
            let parsed = super::parse_base_command(&mut stream)
                .map_err(|e| TestError::Failure(e.to_string()))?;

            let Statement::Redirected { inner: bare, .. } = parsed.inner else {
                return Err(TestError::Failure("expected a Redirected statement".into()));
            };

            assert_eq!(bare.span, 9..12);

            Ok(())
        }
    }

    mod parse_pipeline {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn span_covers_head_through_the_last_stage() -> Result<(), TestError> {
            let mut stream = TokenStream::new("echo a | echo b");
            let parsed = super::parse_pipeline(&mut stream)
                .map_err(|e| TestError::Failure(e.to_string()))?;

            assert_matches!(parsed.inner, Statement::Pipeline { .. });
            assert_eq!(parsed.span, 0..15);

            Ok(())
        }
    }

    mod parse_and_or {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn span_covers_left_through_right() -> Result<(), TestError> {
            let mut stream = TokenStream::new("true && false");
            let parsed =
                super::parse_and_or(&mut stream).map_err(|e| TestError::Failure(e.to_string()))?;

            assert_matches!(parsed.inner, Statement::AndOr { .. });
            assert_eq!(parsed.span, 0..13);

            Ok(())
        }
    }

    mod parse_list {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn sequence_right_span_excludes_the_separator_and_its_whitespace() -> Result<(), TestError>
        {
            // The exact case the stream-level whitespace-skip bug (see `crate::stream`) was
            // caught from: before that fix, `right`'s span started at the `;` rather than at
            // `echo`.
            let mut stream = TokenStream::new("echo a; echo bbbb");
            let parsed =
                super::parse_list(&mut stream).map_err(|e| TestError::Failure(e.to_string()))?;

            assert_eq!(parsed.span, 0..17);

            let Statement::Sequence { right, .. } = parsed.inner else {
                return Err(TestError::Failure("expected a Sequence statement".into()));
            };

            assert_eq!(right.span, 8..17);

            Ok(())
        }

        #[rstest]
        fn background_span_covers_through_the_trailing_amp() -> Result<(), TestError> {
            let mut stream = TokenStream::new("sleep 1 &");
            let parsed =
                super::parse_list(&mut stream).map_err(|e| TestError::Failure(e.to_string()))?;

            assert_matches!(parsed.inner, Statement::Background { right: None, .. });
            assert_eq!(parsed.span, 0..9);

            Ok(())
        }
    }
}
