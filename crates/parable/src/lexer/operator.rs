use super::LexerStream;
use crate::types::Token;

use winnow::{ModalResult, Parser as _, combinator::alt};

impl<'a> LexerStream<'a> {
    /// Matches the fixed-punctuation operator token at the cursor, if any.
    ///
    /// Branches are ordered longest-prefix-first (e.g. `;;&` before `;;` before `;`); see the
    /// module-level Core Design note for why that order is load-bearing.
    #[must_use = "constructs the operator token; failure to use drops the parsed stream state"]
    pub(super) fn parse_operator(&mut self) -> ModalResult<Token<'a>> {
        alt((
            alt((
                ";;&".value(Token::SemiSemiAmp),
                "<<-".value(Token::LessLessMinus),
                "<<<".value(Token::LessLessLess),
                "&>>".value(Token::AmpGreaterGreater),
            )),
            alt((
                "&&".value(Token::AndAnd),
                "||".value(Token::OrOr),
                ";;".value(Token::SemiSemi),
                ";&".value(Token::SemiAmp),
                "<<".value(Token::LessLess),
                ">>".value(Token::GreaterGreater),
            )),
            alt((
                "<&".value(Token::LessAmp),
                ">&".value(Token::GreaterAmp),
                "<>".value(Token::LessGreater),
                ">|".value(Token::GreaterPipe),
                "&>".value(Token::AmpGreater),
                "|&".value(Token::PipeAmp),
            )),
            alt((
                ";".value(Token::Semi),
                "|".value(Token::Pipe),
                "&".value(Token::Amp),
                "(".value(Token::LParen),
                ")".value(Token::RParen),
                "<".value(Token::Less),
                ">".value(Token::Greater),
                "\n".value(Token::Newline),
            )),
        ))
        .parse_next(self)
    }
}
