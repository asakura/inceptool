use super::{Lookahead, TOKEN_SLICE_CAPACITY, TokenOffsets, TokenStream};
use crate::types::Token;

use smallvec::SmallVec;
use std::fmt;
use winnow::stream::{Location, Needed, Offset, Stream, StreamIsPartial};

#[expect(
    clippy::missing_trait_methods,
    reason = "default-provided Stream methods (the *_unchecked slicers winnow's own slice impls \
              override with unsafe, plus finish/peek_finish) are correct as-is and unsafe_code \
              is forbidden in this crate"
)]
impl<'a> Stream for TokenStream<'a> {
    type Token = Token<'a>;
    type Slice = SmallVec<[Token<'a>; TOKEN_SLICE_CAPACITY]>;
    type IterOffsets = TokenOffsets<'a>;
    type Checkpoint = Self;

    fn iter_offsets(&self) -> Self::IterOffsets {
        TokenOffsets {
            stream: self.clone(),
            index: 0,
        }
    }

    fn eof_offset(&self) -> usize {
        self.iter_offsets().count()
    }

    fn next_token(&mut self) -> Option<Self::Token> {
        if let Some(lookahead) = self.lookahead.get_mut().take() {
            return Some(lookahead.token);
        }

        if self.lex_failure.get_mut().is_some() {
            return None;
        }

        match self.lexer.get_mut().lex_token() {
            Ok(Token::Eof) => None,
            Ok(token) => Some(token),
            Err(_) => {
                *self.lex_failure.get_mut() = Some(Box::new(self.lexer.get_mut().clone()));
                None
            }
        }
    }

    fn peek_token(&self) -> Option<Self::Token> {
        if let Some(lookahead) = self.lookahead.borrow().as_ref() {
            return Some(lookahead.token.clone());
        }

        if self.lex_failure.borrow().is_some() {
            return None;
        }

        let remaining_before = self.lexer.borrow().eof_offset();
        let lexed = self.lexer.borrow_mut().lex_token_with_start();

        match lexed {
            Ok((_, Token::Eof)) => None,
            Ok((token_start_remaining, token)) => {
                *self.lookahead.borrow_mut() = Some(Lookahead {
                    token: token.clone(),
                    token_start_remaining,
                    remaining_before,
                });
                Some(token)
            }
            Err(_) => {
                *self.lex_failure.borrow_mut() = Some(Box::new(self.lexer.borrow().clone()));
                None
            }
        }
    }

    fn offset_for<P>(&self, predicate: P) -> Option<usize>
    where
        P: Fn(Self::Token) -> bool,
    {
        self.iter_offsets()
            .find(|(_, token)| predicate(token.clone()))
            .map(|(index, _)| index)
    }

    fn offset_at(&self, tokens: usize) -> Result<usize, Needed> {
        let mut remaining = self.clone();

        for consumed in 0..tokens {
            if remaining.next_token().is_none() {
                return Err(Needed::new(tokens.saturating_sub(consumed)));
            }
        }

        Ok(tokens)
    }

    fn next_slice(&mut self, offset: usize) -> Self::Slice {
        (0..offset).filter_map(|_| self.next_token()).collect()
    }

    fn peek_slice(&self, offset: usize) -> Self::Slice {
        self.clone().next_slice(offset)
    }

    fn checkpoint(&self) -> Self::Checkpoint {
        self.clone()
    }

    fn reset(&mut self, checkpoint: &Self::Checkpoint) {
        self.clone_from(checkpoint);
    }

    #[expect(
        clippy::use_debug,
        reason = "trace() exists purely to render a debug trace of the stream's state"
    )]
    fn trace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Offset for TokenStream<'_> {
    fn offset_from(&self, start: &Self) -> usize {
        start
            .logical_remaining_len()
            .saturating_sub(self.logical_remaining_len())
    }
}

impl Location for TokenStream<'_> {
    fn previous_token_end(&self) -> usize {
        self.original_len
            .saturating_sub(self.logical_remaining_len())
    }
    fn current_token_start(&self) -> usize {
        self.original_len
            .saturating_sub(self.token_start_remaining_len())
    }
}

#[expect(
    clippy::missing_trait_methods,
    reason = "is_partial's default (Self::is_partial_supported()) is correct as-is"
)]
impl StreamIsPartial for TokenStream<'_> {
    type PartialState = ();

    fn complete(&mut self) -> Self::PartialState {}

    fn restore_partial(&mut self, _state: Self::PartialState) {}

    fn is_partial_supported() -> bool {
        false
    }
}

#[expect(
    clippy::missing_trait_methods,
    reason = "only a minimal lexing-driven next() is needed; Iterator's other ~70 default-provided \
              combinators are correct as-is and reimplementing them all would dwarf this module"
)]
impl<'a> Iterator for TokenOffsets<'a> {
    type Item = (usize, Token<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.stream.next_token()?;
        let index = self.index;
        self.index = self.index.saturating_add(1);
        Some((index, token))
    }
}
