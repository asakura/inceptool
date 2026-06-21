use super::Token;
use std::fmt;

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Token::Word(w) => w.as_ref(),
            Token::Newline => "\n",
            Token::Semi => ";",
            Token::Pipe => "|",
            Token::Amp => "&",
            Token::LParen => "(",
            Token::RParen => ")",
            Token::LBrace => "{",
            Token::RBrace => "}",
            Token::Less => "<",
            Token::Greater => ">",
            Token::AndAnd => "&&",
            Token::OrOr => "||",
            Token::SemiSemi => ";;",
            Token::SemiAmp => ";&",
            Token::SemiSemiAmp => ";;&",
            Token::LessLess => "<<",
            Token::GreaterGreater => ">>",
            Token::LessAmp => "<&",
            Token::GreaterAmp => ">&",
            Token::LessGreater => "<>",
            Token::GreaterPipe => ">|",
            Token::LessLessMinus => "<<-",
            Token::LessLessLess => "<<<",
            Token::AmpGreater => "&>",
            Token::AmpGreaterGreater => "&>>",
            Token::PipeAmp => "|&",
            Token::AssignmentWord(w) => w,
            Token::Number(n) => n,
            Token::Eof => "",
        };

        write!(f, "{s}")
    }
}
