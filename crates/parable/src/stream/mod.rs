//! # Token Stream Architecture
//!
//! [`TokenStream`] adapts the crate's lazy, on-demand lexer ([`crate::lexer::LexerStream::lex_token`]) to winnow's
//! [`winnow::stream::Stream`] trait, so [`crate::parser`] can drive parsing token-by-token without
//! pre-collecting the input into a `Vec<Token>` ahead of time.
//!
//! ## Core Design
//!
//! Bash reserved words (`done`, `fi`, `then`, ...) lex as plain `Word` tokens and are only
//! recognized as keywords by grammar position, so the parser routinely peeks the very same
//! upcoming token several times in a row — once per candidate keyword — before anything
//! actually consumes it. [`winnow::stream::Stream::peek_token`] takes `&self`, so caching that peeked token
//! across calls (to avoid re-lexing the same bytes on every peek) needs interior mutability:
//! `lexer`, `lookahead`, and `lex_failure` are all wrapped in [`std::cell::RefCell`] for exactly this
//! reason — not because a `TokenStream` is ever shared across threads.
//!
//! ## Flow
//!
//! 1. **Cache hit**: [`winnow::stream::Stream::peek_token`] returns the buffered token directly, no lexing.
//! 2. **Cache miss**: [`winnow::stream::Stream::peek_token`] lexes one token ahead — physically advancing
//!    `lexer` — and stashes it, along with the lexer's remaining-input length from just before
//!    that lex, so [`winnow::stream::Offset::offset_from`] can still report the parser's true *logical*
//!    position even though `lexer` is now ahead of it.
//! 3. **Consume**: [`winnow::stream::Stream::next_token`] drains the buffered token first, with no relexing;
//!    it only drives the lexer for real once the buffer is empty.
//!
//! ## Edge Cases
//!
//! - A lex failure discovered during [`winnow::stream::Stream::peek_token`] (not just [`winnow::stream::Stream::next_token`])
//!   must still land in `lex_failure` on `self`, never on a throwaway clone — otherwise
//!   [`TokenStream::take_lex_error`] would silently miss it.
//! - [`winnow::stream::Offset::offset_from`] must diff the parser's *logical* remaining length on both sides,
//!   not `lexer`'s raw remaining length: either side may hold a buffered lookahead that has
//!   already physically advanced `lexer` past a token the parser hasn't logically consumed.
//! - [`winnow::stream::Stream::checkpoint`]/[`winnow::stream::Stream::reset`] clone/restore the whole struct, including any
//!   buffered lookahead, so checkpointing right after a peek and resetting later correctly
//!   replays that same peek rather than losing it.
//! - [`winnow::stream::Location::current_token_start`] and [`winnow::stream::Location::previous_token_end`] diff against
//!   different lengths, not the same one: whitespace before a token is skipped lazily inside
//!   `crate::lexer::LexerStream::lex_token_with_start`, so "end of the previous token" and "start of the
//!   next one" are different positions whenever whitespace separates them. Reporting both from
//!   the same length would place the next token's start back at the previous token's end,
//!   underlining the gap between them instead of the token itself.

#![expect(
    clippy::field_scoped_visibility_modifiers,
    reason = "fields are shared within the module hierarchy"
)]

mod token_stream;
mod traits;

#[cfg(test)]
mod tests;

pub(crate) use token_stream::{Lookahead, TOKEN_SLICE_CAPACITY};
pub use token_stream::{TokenOffsets, TokenStream};
