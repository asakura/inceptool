# Inceptool Parable Crate (`inceptool-parable`)

## Public API ([`lib.rs`](src/lib.rs))

The root module orchestrates parsing and exports the public API.

- **`fn parse_program(input: &str) -> ModalResult<Vec<Spanned<Statement<'_>>>>`**:
  Lexes and parses a complete script into a list of spanned top-level statements.
- **`fn render_program_ast(input: &str) -> ModalResult<String>`**:
  Thin wrapper around `parse_program` used by the corpus test suite — renders the
  resulting AST in the `{:?}`-debug form corpus fixtures compare against.
- **Exports**: `Expr`, `LexerState`, `LogicalOp`, `PipeOp`, `Redirect`,
  `RedirectKind`, `RedirectTarget`, `Spanned`, `SpecialParam`, `Statement`,
  `Token`, `TokenStream`, `ParseError`, `ParseErrorDisplay`.

## AST & Types ([`types/`](src/types/))

Contains the core Abstract Syntax Tree representations, split across several
submodules but re-exported from `types/mod.rs`.

- **`enum Token<'a>`**: Raw tokens produced by the lexer (e.g., `Word`, `Pipe`,
  `AndAnd`, `LBrace`). Reserved words (`if`, `done`, `in`, ...) are not their own
  variant — they lex as plain `Word`s and are recognized by grammar position in
  `parser`.
- **`enum Statement<'a>`**: High-level execution nodes (`Command`,
  `ForLoop`, `If`, `While`, `Until`, `Case`, `Pipeline`, `Subshell`, `BraceGroup`,
  `AndOr`, `Sequence`, `Background`, `Redirected`).
- **`enum Expr<'a>`**: `Literal`, `VarRef`, `Positional`, `SpecialParam`, or
  `Interpolated(Vec<Spanned<Self>>)` — each part of an interpolated word
  (e.g. `"prefix${x}suffix"`) carries its own byte span, set once in
  `parser::word::interpolate`, rather than reusing the whole word's span for
  every part.
- **`struct Spanned<T>`**: Wraps an AST node with its exact byte `span: Range<usize>`.
  - `From<(T, Range<usize>)>` builds one from winnow's `.with_span()` shape
    (`.with_span().map(Spanned::from)`); there is no separate `Spanned::new`.
  - `Display`/`Debug` both delegate transparently to `inner` via one shared
    `transparent_fmt!` macro, so `{:?}` AST snapshots aren't polluted with spans.
- **Enums**:
  - `LogicalOp`: `&&` vs `||`.
  - `PipeOp`: `|` vs `|&`.
  - `CaseArm`: Patterns and body for a `case` block.
  - `Redirect`, `RedirectKind`, `RedirectTarget`:
    Shell redirections (`>`, `<&`, etc.).

## Lexer & Token Stream

### 1. Lexer ([`lexer.rs`](src/lexer.rs))

- **`fn LexerStream::lex_token(&mut self) -> ModalResult<Token<'_>>`**:
  The core tokenizer that yields a single token, skipping leading whitespace
  first (newlines excluded — they're a significant `Token::Newline`).
- **`fn LexerStream::lex_token_with_start(&mut self) -> ModalResult<(usize, Token<'_>)>`**:
  Same, but also returns `eof_offset()` measured right after that whitespace
  skip and before the token itself — the position `stream::TokenStream` needs
  to report a token's true _start_, as opposed to the _end of the previous
  token_ (which still includes the next token's upcoming whitespace). `lex_token`
  is a thin wrapper that discards the offset.

### 2. Token Stream ([`stream.rs`](src/stream.rs))

- **`struct TokenStream<'a>`**: A lazy iterator wrapping the lexer that
  implements `winnow::stream::Stream`/`Location`. It produces tokens on-demand,
  caching one token of lookahead (`peek_token`) so the parser can peek a
  keyword without re-lexing it.
- **`impl TokenStream<'a>`**:
  - `current_span_start(&self)`: Start offset of the next token — forces a
    peek if none is buffered yet, since whitespace is skipped lazily.
  - `previous_span_end(&self)`: End offset of the previously consumed token.
  - These two diff against _different_ internal lengths (`token_start_remaining`
    vs. `remaining_before` on the buffered `Lookahead`) precisely because
    whitespace can separate them; conflating the two was a real bug (see git
    history) where a node following whitespace reported the wrong start.

## Parser Submodules (`parser/`)

The parser is split into specialized `winnow` combinator submodules under [`parser/mod.rs`](src/parser/mod.rs).

- **`mod.rs`**:
  `parse_statement`, `parse_command` (the compound/base-command dispatch
  point). Also hosts two helpers shared across every sibling module:
  - `spanned(start, input, inner) -> Spanned<T>`: builds
    `Spanned { inner, span: start..input.previous_span_end() }`,
    replacing the hand-rolled version of that arithmetic at every call site.
  - `attach_redirects(stmt, trailing, start, input) -> Spanned<Statement>`:
    the one place that merges trailing redirects into an existing
    `Statement::Redirected` or wraps a fresh one — shared by `parse_command`
    and `command::parse_base_command` so the merge rule can't diverge between
    the two.
- **[`command.rs`](src/parser/command.rs)**:
  `parse_base_command` (simple commands — its `Statement::Command` span
  excludes any interleaved/leading/trailing redirect text, even though the
  outer `Redirected` span includes it), `parse_pipeline`, `parse_and_or`,
  `parse_list`/`parse_list_until`.
- **[`control_flow.rs`](src/parser/control_flow.rs)**:
  Parses `if/then/elif/else`, `for`, `while`, and `until`. A `for` loop with no
  `in` clause synthesizes an implicit `"$@"` iterable with a zero-width span
  anchored immediately after the loop variable — this crate's convention for a
  synthetic AST node with no corresponding source text (anchor where the
  absent construct would have appeared, not at an unrelated token).
- **[`case.rs`](src/parser/case.rs)**:
  Parses `case` statements and their `pattern)` arms.
- **[`grouping.rs`](src/parser/grouping.rs)**:
  Parses subshells `(...)` and brace groups `{ ...; }`.
- **[`redirect.rs`](src/parser/redirect.rs)**:
  Handles file descriptor redirection operators (`>`, `<&`, etc.).
- **[`word.rs`](src/parser/word.rs)**:
  Parses expressions, string literals, interpolations (`${VAR}`), and handles
  variable expansion structures. `interpolation_segments` returns each
  segment paired with its byte span within the word's text; `interpolate`
  shifts those by the word's absolute start offset to build `Expr::Interpolated`'s
  `Spanned` parts.

## Taint Tracking ([`taint.rs`](src/taint.rs))

Provides rudimentary symbolic execution to track whether values or positional
parameters flow into dangerous evaluations.

- **`struct Environment`**: A registry mapping variable names to their `SymbolicValue`.
  - `apply_statement(&mut self, stmt: &Spanned<Statement<'_>>)`: Updates
    the environment for simple assignments (e.g., `x=$1`).
  - `resolve_expr(&self, expr: &Spanned<Expr<'_>>)`: Resolves an expression
    into a `SymbolicValue`. `Expr::Interpolated`'s parts are already
    `Spanned`, so this recurses on each part directly rather than fabricating
    one.
- **`enum SymbolicValue`**:
  - `Constant`: Known safe string data.
  - `Tainted`: Untrusted user input (e.g., positional parameter `$1`).
  - `Concat`: A mix of parts.
  - `Unknown`: Unresolvable state.

## Security Rules Engine ([`rules.rs`](src/rules.rs))

A customizable engine for traversing the AST and flagging
suspicious or insecure patterns.

- **`trait Rule`**:
  Implementations define an `id()` and a `check()` method to inspect
  `Spanned<Statement>` and push violations.
- **`struct Engine<'a>`**: The rule runner.
  - `register(&mut self, rule: &'a dyn Rule)`: Adds a rule to the engine.
  - `run(&self, statements: &[Spanned<Statement<'_>>]) -> Vec<Finding<'_>>`:
    Traverses the AST recursively, updating a local `Environment` and
    executing rules.
- **`struct Finding<'a>`**: Emitted when a rule flags a problem
  (contains the rule ID and the offending node span).

## Error Handling ([`error.rs`](src/error.rs))

- **`struct ParseError<'a>`**:
  The error type returned by the parser when it fails. Retains a reference to
  the source input for rich reporting.
- **`struct ParseErrorDisplay<'a>`**:
  A wrapper used to implement `miette::Diagnostic` or human-readable formatting
  around a `ParseError`.
