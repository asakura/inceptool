use super::{ParserStream, attach_redirects, case, command, control_flow, grouping, redirect};
use crate::types::{Spanned, Statement};

use winnow::{ModalResult, Parser as _, combinator::alt};

#[must_use = "parses a compound command or base command; discarding ignores syntax structures"]
pub(super) fn parse_command<'a>(
    input: &mut ParserStream<'a>,
) -> ModalResult<Spanned<Statement<'a>>> {
    let start_offset = input.current_span_start();
    let stmt = alt((
        control_flow::parse_for_loop,
        control_flow::parse_if,
        control_flow::parse_while,
        control_flow::parse_until,
        case::parse_case,
        grouping::parse_subshell,
        grouping::parse_brace_group,
        command::parse_base_command,
    ))
    .parse_next(input)?;

    let mut trailing = Vec::new();

    while let Some(r) = redirect::parse_redirect_opt(input)? {
        trailing.push(r);
    }

    Ok(attach_redirects(stmt, trailing, start_offset, input))
}

/// Parses the statement.
///
/// # Errors
/// Returns an error if the statement cannot be parsed.
#[must_use = "parses a statement; discarding ignores syntax structures"]
pub fn parse_statement<'a>(input: &mut ParserStream<'a>) -> ModalResult<Spanned<Statement<'a>>> {
    command::parse_list(input)
}
