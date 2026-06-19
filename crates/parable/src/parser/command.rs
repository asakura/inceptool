//! Simple commands, pipelines, and command lists — [`parse_base_command`], [`parse_pipeline`],
//! [`parse_list`].

use super::redirect::parse_redirect;
use super::word::parse_literal;
use super::{ParserStream, parse_command};

use crate::types::{Statement, Token};

use winnow::ModalResult;
use winnow::Parser as _;
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

#[must_use = "parses a command list; discarding ignores syntax structures"]
pub(super) fn parse_list<'a>(input: &mut ParserStream<'a>) -> ModalResult<Statement<'a>> {
    let first = parse_pipeline(input)?;
    let mut items = Vec::new();
    let mut current = first;

    loop {
        let sep_result: ModalResult<Token<'_>> = any
            .verify(|t| {
                matches!(t, Token::Semi)
                    || matches!(t, Token::Amp)
                    || matches!(t, Token::AndAnd)
                    || matches!(t, Token::OrOr)
                    || matches!(t, Token::Newline)
            })
            .parse_next(input);

        if let Ok(sep) = sep_result {
            items.push((current, sep));

            // We got a separator. Let's see if there is another pipeline.
            if let Ok(next_pipe) = parse_pipeline(input) {
                current = next_pipe;
            } else {
                // Trailing separator
                break;
            }
        } else {
            items.push((current, Token::Newline)); // Implicit terminator if no more separators
            break;
        }
    }

    if items.len() == 1 && items.first().map(|i| &i.1) == Some(&Token::Newline) {
        Ok(items
            .pop()
            .ok_or_else(|| ErrMode::Backtrack(ContextError::new()))?
            .0)
    } else {
        Ok(Statement::List { items })
    }
}
