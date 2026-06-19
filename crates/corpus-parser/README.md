# inceptool-corpus-parser

Parser for the corpus `.tests` file format used by the `inceptool-parable`
test suite.

## Overview

Each `.tests` file declares a single test suite with a unique number and name,
contains one or more test groups (delimited by `# === Name ===` headers),
and within each group, one or more test cases with input/expected sections.

`TestSuite`, `TestGroup`, and `CorpusCase` (in `types`) are all
lifetime-parameterized: their string fields borrow zero-copy from the input
`&str` passed to `TestSuite::parse`, falling back to owned (`Cow::Owned`)
copies only when the input contains `\r\n` line endings that need
normalizing first, or a case's `input`/`expected` text contains an escaped
delimiter line (see below). On failure, `TestSuite::parse` returns a
`CorpusParseError` (in `error`) wrapping a `CorpusParseErrorKind` that
enumerates each specific malformed-input case (missing headers, empty
groups, orphan test cases, malformed comments, and so on).

A case's `input` or `expected` text can contain a literal `---` or `===`
line — e.g. a Bash heredoc body that happens to read `---` — by escaping it
as `\---`/`\===`; the parser unescapes it back to `---`/`===` once the real
section boundaries are found. A leading backslash is otherwise inert in
Bash, so this convention reads naturally inside Bash snippets.

The `ident` module exposes `to_ident_fragment`, a Rust-identifier sanitizer.
Given at least one ASCII-alphanumeric input character it always returns a
valid identifier fragment (lowercased, non-alphanumeric runs collapsed to
`_`, and prefixed with `_` if it would otherwise start with a digit).
`inceptool-parable`'s `build.rs` uses it together with `TestSuite::parse` to
turn each suite/group/case name into a valid `rstest` test function and case
identifier when generating integration tests from the corpus at build time.

## Usage

```rust
use inceptool_corpus_parser::TestSuite;

let content = std::fs::read_to_string("corpus/01_words.tests")?;
let suite = TestSuite::parse("01_words", &content)?;

for group in &suite.groups {
    println!("Group: {}", group.name);
    for case in &group.cases {
        println!("  Case: {}", case.name);
    }
}
```

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.
