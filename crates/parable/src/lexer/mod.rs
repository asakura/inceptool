//! # Lexer Architecture
//!
//! Tokenizes a bash script's raw `&str` into the [`Token`](crate::types::Token)s [`crate::parser`] consumes, one
//! token at a time via [`LexerStream::lex_token`] rather than pre-splitting the whole input up front. Lexing
//! is on-demand because bash's grammar is heavily context-sensitive (e.g. whether a reserved word
//! is a keyword or a plain argument depends on grammar position), so [`crate::parser`] is the
//! only thing that can decide what comes after this token before it's even lexed.
//!
//! ## Core Design
//!
//! [`LexerStream::lex_token`] always tries `parse_operator` before `parse_word`: operators are a small,
//! fixed set of multi-character punctuation runs (`;;&`, `<<-`, `&&`, ...) that would otherwise
//! lex as ordinary word characters, so they must win whenever they match. Within
//! `parse_operator` itself, the `alt` branches are ordered longest-prefix-first (`;;&` before
//! `;;` before `;`) for the same reason — `alt` commits to the first alternative that matches, so
//! listing a short operator before a longer one that starts with it would shadow the longer one.
//!
//! `parse_word` does not classify reserved words (`if`, `done`, `in`, ...): whether such a word
//! is a keyword or a plain argument depends on grammar position (e.g. `in` after a `for` NAME vs.
//! `in` as a command argument), which only [`crate::parser`] knows. The lexer always emits
//! [`Token::Word`](crate::types::Token::Word) and leaves that decision to the parser.
//!
//! ## Flow
//!
//! 1. **Skip whitespace**: `skip_whitespace` consumes runs of inline spaces/tabs. Newlines are
//!    not whitespace here — they are a significant [`Token::Newline`](crate::types::Token::Newline) produced by
//!    `parse_operator`, since bash treats them as statement separators.
//! 2. **Check for EOF**: if nothing remains after skipping whitespace, [`LexerStream::lex_token`] returns
//!    [`Token::Eof`](crate::types::Token::Eof) directly rather than trying either parser.
//! 3. **Try an operator, then a word**: `parse_operator` matches the fixed set of punctuation
//!    tokens; on backtrack, `parse_word` scans forward by hand (not via winnow combinators) to
//!    find the end of the next word, then slices it off in one shot with `take`.
//!
//! ## Edge Cases
//!
//! - **Quoting suspends metacharacter recognition**: inside `'...'` or `"..."`, characters that
//!   would otherwise end a word (whitespace, `;`, `|`, `&`, `(`, `)`, `<`, `>`) are just word
//!   content. `parse_word`'s `in_single`/`in_double` flags track this.
//! - **Nested expansions can contain anything**: `$(...)`, `${...}`, and `` `...` `` can contain
//!   whitespace, operators, even further nested expansions, without ending the word —
//!   `parse_word`'s `paren_depth`/`brace_depth`/`backtick_depth` counters track nesting so the
//!   word only ends once every expansion opened inside it has closed. Depth is tracked even
//!   inside double quotes (where `$(...)`/`${...}` still expand) but not inside single quotes
//!   (where bash performs no expansion at all).
//! - **Escaping consumes exactly one character verbatim**: a backslash and the character after it
//!   are both folded into the word unconditionally, so an escaped metacharacter, quote, or even a
//!   backslash itself never flips quoting state or changes expansion depth.
//! - **An empty match is a parse failure, not an empty word**: if the scan above consumes zero
//!   characters (e.g. the cursor sits on a character `parse_operator` already rejected),
//!   `parse_word` backtracks instead of returning [`Token::Word`](crate::types::Token::Word) with empty content.

#![expect(
    clippy::multiple_inherent_impl,
    reason = "methods are split across files by category"
)]

pub(crate) mod heredoc;
mod operator;
mod stream;
mod traits;
mod word;

pub use stream::LexerStream;
