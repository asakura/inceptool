use super::LexerStream;
use crate::types::LexerState;

use std::fmt;
use winnow::stream::{Compare, CompareResult, Needed, Offset, Stateful, Stream, StreamIsPartial};

#[expect(
    clippy::missing_trait_methods,
    reason = "default-provided Stream methods (the *_unchecked slicers winnow's own slice impls \
              override with unsafe, plus finish/peek_finish) are correct as-is and unsafe_code \
              is forbidden in this crate"
)]
impl<'a> Stream for LexerStream<'a> {
    type Token = <Stateful<&'a str, LexerState<'a>> as Stream>::Token;
    type Slice = <Stateful<&'a str, LexerState<'a>> as Stream>::Slice;
    type IterOffsets = <Stateful<&'a str, LexerState<'a>> as Stream>::IterOffsets;
    type Checkpoint = Self;

    fn iter_offsets(&self) -> Self::IterOffsets {
        self.0.iter_offsets()
    }

    fn eof_offset(&self) -> usize {
        self.0.eof_offset()
    }

    fn next_token(&mut self) -> Option<Self::Token> {
        self.0.next_token()
    }

    fn peek_token(&self) -> Option<Self::Token> {
        self.0.peek_token()
    }

    fn offset_for<P>(&self, predicate: P) -> Option<usize>
    where
        P: Fn(Self::Token) -> bool,
    {
        self.0.offset_for(predicate)
    }

    fn offset_at(&self, tokens: usize) -> Result<usize, Needed> {
        self.0.offset_at(tokens)
    }

    fn next_slice(&mut self, offset: usize) -> Self::Slice {
        self.0.next_slice(offset)
    }

    fn peek_slice(&self, offset: usize) -> Self::Slice {
        self.0.peek_slice(offset)
    }

    fn checkpoint(&self) -> Self::Checkpoint {
        self.clone()
    }

    fn reset(&mut self, checkpoint: &Self::Checkpoint) {
        self.clone_from(checkpoint);
    }

    fn trace(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.trace(f)
    }
}

impl Offset for LexerStream<'_> {
    fn offset_from(&self, start: &Self) -> usize {
        self.0.offset_from(&start.0)
    }
}

#[expect(
    clippy::missing_trait_methods,
    reason = "is_partial's default (Self::is_partial_supported()) and is_partial's default \
              (false) are correct as-is"
)]
impl<'a> StreamIsPartial for LexerStream<'a> {
    type PartialState = <Stateful<&'a str, LexerState<'a>> as StreamIsPartial>::PartialState;

    #[expect(
        clippy::semicolon_if_nothing_returned,
        reason = "the trailing expression is this fn's must_use return value, not a discarded \
                  statement; a trailing `;` would make it one and trip unused_must_use"
    )]
    fn complete(&mut self) -> Self::PartialState {
        self.0.complete()
    }

    fn restore_partial(&mut self, state: Self::PartialState) {
        self.0.restore_partial(state);
    }

    fn is_partial_supported() -> bool {
        <Stateful<&'a str, LexerState<'a>> as StreamIsPartial>::is_partial_supported()
    }
}

impl<'a, U> Compare<U> for LexerStream<'a>
where
    &'a str: Compare<U>,
{
    fn compare(&self, t: U) -> CompareResult {
        self.0.compare(t)
    }
}
