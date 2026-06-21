//! `case`/`in`/`esac` — [`parse_case`].

use super::{
    command::parse_list_until,
    word::{parse_literal, parse_pattern_word},
    {ParserStream, at_keyword, consume_keyword, skip_newlines, spanned},
};

use crate::types::{CaseArm, Spanned, Statement, Token};

use winnow::{
    ModalResult, Parser as _,
    combinator::{cut_err, opt},
    error::StrContext,
    stream::Stream as _,
    token::any,
};

const KW_CASE: &str = "case";
const KW_IN: &str = "in";
const KW_ESAC: &str = "esac";

/// Whether the next token is one of the arm-terminating operators (`;;`/`;&`/`;;&`).
#[must_use = "peeking has no effect unless the caller acts on the result"]
fn at_arm_terminator(input: &ParserStream<'_>) -> bool {
    matches!(
        input.peek_token(),
        Some(Token::SemiSemi | Token::SemiAmp | Token::SemiSemiAmp)
    )
}

#[must_use = "parses a case statement; discarding ignores syntax structures"]
pub(super) fn parse_case<'a>(input: &mut ParserStream<'a>) -> ModalResult<Spanned<Statement<'a>>> {
    let start_offset = input.current_span_start();
    consume_keyword(input, KW_CASE)?;

    cut_err(|rest: &mut ParserStream<'a>| {
        let word = parse_literal(rest)?;

        skip_newlines(rest);
        consume_keyword(rest, KW_IN)?;
        skip_newlines(rest);

        let mut arms = Vec::new();

        while !at_keyword(rest, KW_ESAC) {
            arms.push(parse_case_arm(rest)?);
            skip_newlines(rest);
        }

        consume_keyword(rest, KW_ESAC)?;

        Ok(spanned(start_offset, rest, Statement::Case { word, arms }))
    })
    .context(StrContext::Label("case statement"))
    .parse_next(input)
}

/// Parses one `[(] pattern (| pattern)* ) [commands] [;;|;&|;;&]` arm.
fn parse_case_arm<'a>(input: &mut ParserStream<'a>) -> ModalResult<CaseArm<'a>> {
    // An optional leading `(` before the pattern list — purely cosmetic, so its presence or
    // absence isn't recorded.
    opt(any.verify(|t: &Token<'_>| matches!(t, Token::LParen))).parse_next(input)?;

    let mut patterns = vec![parse_pattern_word(input)?];

    loop {
        let pipe: ModalResult<Token<'_>> = any
            .verify(|t: &Token<'_>| matches!(t, Token::Pipe))
            .parse_next(input);

        if pipe.is_err() {
            break;
        }

        patterns.push(parse_pattern_word(input)?);
    }

    let _: Token<'_> = any
        .verify(|t: &Token<'_>| matches!(t, Token::RParen))
        .parse_next(input)?;

    skip_newlines(input);

    let body = if at_arm_terminator(input) || at_keyword(input, KW_ESAC) {
        None
    } else {
        Some(Box::new(parse_list_until(input, |inp| {
            at_keyword(inp, KW_ESAC)
        })?))
    };

    skip_newlines(input);

    // The closing terminator is optional on the arm immediately before `esac`. `;&` falls
    // through to the next arm's body unconditionally and `;;&` falls through but still tests the
    // next pattern — neither distinction is recorded here, matching how the optional leading `(`
    // just above is accepted without being recorded: the resulting `CaseArm` is the same shape
    // either way, since this crate's analyses don't yet model fallthrough's runtime effect on
    // which arms execute.
    opt(any.verify(|t: &Token<'_>| {
        matches!(t, Token::SemiSemi | Token::SemiAmp | Token::SemiSemiAmp)
    }))
    .parse_next(input)?;

    skip_newlines(input);

    Ok(CaseArm { patterns, body })
}

// `parse_case`/`parse_case_arm` behavior (the `esac`-as-pattern ambiguity, `;&`/`;;&`
// fallthrough) is covered by the "Syntax errors" and fallthrough groups in
// `corpus/09_case.tests`, rather than duplicated here.

#[cfg(test)]
mod tests {
    use super::*;

    use crate::stream::TokenStream;

    #[derive(Debug, thiserror::Error)]
    enum TestError {
        #[error("Test failure: {0}")]
        Failure(String),
    }

    mod parse_case {
        use super::*;

        use rstest::rstest;

        #[rstest]
        fn span_covers_case_through_esac_and_each_arm_keeps_its_own() -> Result<(), TestError> {
            let mut stream = TokenStream::new("case x in a) echo hi ;; esac");
            let parsed =
                super::parse_case(&mut stream).map_err(|e| TestError::Failure(e.to_string()))?;

            assert_eq!(parsed.span, 0..28);

            let Statement::Case { arms, .. } = parsed.inner else {
                return Err(TestError::Failure("expected a Case statement".into()));
            };

            let [arm] = arms.as_slice() else {
                return Err(TestError::Failure("expected exactly one arm".into()));
            };

            let [pattern] = arm.patterns.as_slice() else {
                return Err(TestError::Failure("expected exactly one pattern".into()));
            };

            assert_eq!(pattern.span, 10..11);

            let body = arm
                .body
                .as_ref()
                .ok_or_else(|| TestError::Failure("expected a non-empty arm body".into()))?;

            assert_eq!(body.span, 13..20);

            Ok(())
        }
    }
}
