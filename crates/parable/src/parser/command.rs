//! Simple commands, pipelines, and command lists — [`parse_base_command`], [`parse_pipeline`],
//! [`parse_and_or`], [`parse_list`].

use super::redirect::parse_redirect;
use super::word::parse_literal;
use super::{ParserStream, parse_command};

use crate::types::{LogicalOp, Statement, Token};

use winnow::ModalResult;
use winnow::Parser as _;
use winnow::combinator::alt;
use winnow::error::{ContextError, ErrMode};
use winnow::token::any;

/// Parses a simple command: a name, its arguments, and any redirects — interleaved with the
/// arguments in any order (`cat < in.txt -n` and `cat -n < in.txt` parse the same way), plus any
/// leading the command name itself (`< in.txt cat`). Wraps the result in
/// [`Statement::Redirected`] only when at least one redirect was found, so every redirect-free
/// command parses exactly as before this existed.
#[must_use = "parses a base command; discarding ignores syntax structures"]
pub(super) fn parse_base_command<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let mut redirects = Vec::new();

    while let Ok(r) = parse_redirect(input) {
        redirects.push(r);
    }

    let name = any
        .verify_map(|t| match t {
            Token::Word(word) => Some(word.clone()),
            _ => None,
        })
        .parse_next(input)?;

    let mut args = Vec::new();

    loop {
        if let Ok(r) = parse_redirect(input) {
            redirects.push(r);
            continue;
        }

        let Ok(expr) = parse_literal(input) else {
            break;
        };

        args.push(expr);
    }

    let cmd = Statement::Command { name, args };

    if redirects.is_empty() {
        Ok(cmd)
    } else {
        Ok(Statement::Redirected {
            inner: Box::new(cmd),
            redirects,
        })
    }
}

#[must_use = "parses a pipeline; discarding ignores syntax structures"]
fn parse_pipeline<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let first = parse_command(input)?;
    let mut commands = vec![first];

    while let Ok(_pipe) = {
        let res: ModalResult<Token<'_>> = any
            .verify(|t| matches!(t, Token::Pipe) || matches!(t, Token::PipeAmp))
            .parse_next(input);

        res
    } {
        let next_cmd = parse_command(input)?;
        commands.push(next_cmd);
    }

    if commands.len() == 1 {
        Ok(commands
            .pop()
            .ok_or_else(|| ErrMode::Backtrack(ContextError::new()))?)
    } else {
        Ok(Statement::Pipeline { commands })
    }
}

/// Parses an `and_or` chain: one or more pipelines joined by `&&`/`||`, left-associative —
/// `a && b || c` becomes `AndOr(AndOr(a, And, b), Or, c)`. Bash's `&&`/`||` bind tighter than
/// `;`/`&`/newline, so this sits between [`parse_pipeline`] and [`parse_list`]: every `&&`/`||`
/// chain folds into a single [`Statement`] before [`parse_list`] ever sees a `;`/`&`/newline
/// separator, which matters for `&`'s scope — `a && b &` must background the whole `a && b`
/// conjunction, not just `b`.
#[must_use = "parses an and-or chain; discarding ignores syntax structures"]
fn parse_and_or<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
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

        let Ok(right) = parse_pipeline(input) else {
            break;
        };

        current = Statement::AndOr {
            left: Box::new(current),
            op,
            right: Box::new(right),
        };
    }

    Ok(current)
}

/// Parses a `list`: one or more `and_or` chains joined by `;`/`&`/newline, left-associative. A
/// lone trailing `;`/newline is dropped (`echo foo;` parses the same as `echo foo`) since neither
/// changes execution; a lone trailing `&` is kept as a unary [`Statement::Background`] since
/// backgrounding is meaningful even with nothing following it.
#[must_use = "parses a command list; discarding ignores syntax structures"]
pub(super) fn parse_list<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    parse_list_until(input, |_| false)
}

/// Parses a `list`, like [`parse_list`], but stops folding once `stop` reports the next token
/// starts the caller's own closing delimiter — used by `grouping::parse_brace_group` so a
/// trailing `;` before `}` doesn't make this function try to parse one `and_or` too many and
/// hand `}` (which lexes as a plain [`Token::Word`]) to [`parse_base_command`] as a command name.
#[must_use = "parses a command list; discarding ignores syntax structures"]
pub(super) fn parse_list_until<'a, F>(
    input: &mut ParserStream<'a>,
    stop: F,
) -> ModalResult<Statement<'a>>
where
    F: Fn(&ParserStream<'a>) -> bool,
{
    let mut current = parse_and_or(input)?;

    loop {
        let sep_result: ModalResult<Token<'_>> = any
            .verify(|t| {
                matches!(t, Token::Semi) || matches!(t, Token::Amp) || matches!(t, Token::Newline)
            })
            .parse_next(input);

        let Ok(sep) = sep_result else {
            break;
        };

        let next = if stop(input) {
            None
        } else {
            parse_and_or(input).ok()
        };

        current = match (sep, next) {
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
    }

    Ok(current)
}
