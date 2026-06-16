# CLAUDE.md

## RULES

### Safety

`unsafe_code = "forbid"` in `[workspace.lints.rust]` — not a convention, a
compiler-enforced absolute. No `unsafe` block, `impl`, or trait can exist in
any crate. There is no exception and no escape hatch.

### No panics

- No `.unwrap()` or `.expect()` anywhere — including tests and examples. Use `?`
  propagating to the appropriate `Result` type. The `unwrap_used`, `expect_used`,
  and `panic` Clippy lints are denied at the workspace level; violations are hard
  errors.
- `todo!()`, `unimplemented!()`, and `dbg!()` are also denied by Clippy — they
  cannot appear in committed code. If a branch is genuinely unreachable, prove it
  with types or match exhaustiveness; if a feature is unfinished, don't commit it.

### Zero-copy by default

- `Cow<'a, str>` for every string field that can borrow from a source buffer.
  Always pair `#[serde(borrow, default)]` on such fields — `borrow` enables
  zero-copy deserialization, `default` handles missing fields gracefully instead
  of failing. Never default to `String` in protocol/deserialization types.
- `serde_json::value::RawValue` (`RawJson`) for opaque JSON blobs that shouldn't
  be materialized.
- Avoid `.to_owned()` unless you've exhausted borrowing options. Prefer
  `.as_ref()` / `.as_deref()` at call sites.
- `Cow::Borrowed("literal")` for `&'static str` constants embedded in structs
  (e.g. `Access` / `Rule` static tables).
- `str_to_string` is denied — `.to_string()` on `&str` is a hard error. Use
  `.to_owned()` or `.into()` instead.

### Constants

- Every magic literal (string sentinel, numeric limit, binary name, subcommand)
  gets a named `const`. Group related consts directly before the first type or
  function that uses them, e.g.:
  ```rust
  const SHORT_REV_LEN: usize = 7;
  const ROOT_NODE_NAME: &str = "root";
  ```

### Arithmetic

`arithmetic_side_effects` is denied — every `+`, `-`, `*`, or `/` that could
overflow or underflow is a hard error. Use the variant that matches the semantic:

- `saturating_*` when clamping at the boundary is correct behavior
- `checked_*` when overflow should produce `None` or an error
- `wrapping_*` only when modular arithmetic is explicitly intended

### Indexing

`indexing_slicing` is denied — `slice[n]` and `slice[a..b]` are hard errors.
Use `.get(n)` / `.get(a..b)` returning `Option`, then handle `None` via `?` or
an early return. Prefer iterator methods (`iter`, `windows`, `chunks`) when
traversing rather than random-accessing.

### Type design

- Use `BTreeMap` over `HashMap` whenever key ordering matters (output
  determinism, display, indexes keyed by filename/string).
- `LazyLock<T>` for expensive `static` initializations (rule sets, compiled
  tables).
- Express output variants as `HookOutputEvent` enum arms so invalid states are
  unrepresentable. Never use a bare struct where an enum variant exists.
- Newtype wrappers (e.g. `Summary<'a>`) to scope `fmt::Display` impls without
  polluting the inner type.
- `#[derive(Debug, Clone, Copy, Default)]` on every zero-size stage struct (all
  four — they're free and enable generic bounds downstream).
- `#[must_use]` on every pure constructor or builder method (e.g.
  `Registry::new()`).
- `missing_debug_implementations` is denied — every public type must implement
  or derive `Debug`, no exceptions.
- `missing_copy_implementations` is denied — every public type that *can* be
  `Copy` (no heap allocation, no `Drop` impl) must derive it. When unsure,
  attempt `#[derive(Copy, Clone)]` and let the compiler decide.

### Stage naming and implementation

- Stage structs are named `XxxStage` (e.g. `FlakeLockSummarizationStage`,
  `ReadWriteGuardStage`, `RtkStage`).
- Every `Stage` impl follows this guard order in `run()`:
  1. Match on the expected `HookInputEvent` variant; return `Ok(None)` for
     everything else.
  2. Parse tool input; extract the target field (file path, command, …).
  3. Guard conditions (wrong filename, empty value, unreadable file, …) — each
     returning `Ok(None)` early.
  4. Do the real work.
  5. Return `Ok(Some(HookOutputEvent::…(…)))`.
- Build output structs with struct-update syntax — set only the relevant fields
  and use `..Default::default()` for the rest:
  ```rust
  PreToolUseOutput {
      decision: Some(Decision::Deny),
      reason: Some(summary.into()),
      ..Default::default()
  }
  ```

### Function decomposition

Extract non-trivial logic into named private helpers rather than nesting it
inside `run()`. Follow the `RtkStage` pattern:
`rewrite()` → `invoke()` + `interpret()` — each with a single responsibility
and a `///` doc comment explaining the non-obvious contract.

### Display over format! for user-facing text

Types that render user-visible output implement `fmt::Display`. Do not
scatter `format!()` calls building the same shape of string across multiple
call sites.

### Errors

- Use `thiserror` in every crate — library crates, stages, protocol, engine,
  and drivers all define their own typed error enum with
  `#[derive(thiserror::Error)]`.
- `miette` is reserved for user-facing binaries only. Use it at the CLI
  boundary to render diagnostic output; never inside lib crates.
- Every crate exposes a single `Error` type (e.g. `EngineError`, `StagesError`)
  re-exported at the crate root. Callers wrap foreign errors with `#[from]`
  rather than mapping manually.
- `#[error("…")]` messages are the only user-visible explanation — write them
  for humans, not for `Debug` output.

### Tracing

Use `tracing` for all observability — `print_stdout` and `print_stderr` are
denied, so `println!` / `eprintln!` are hard errors:

- `tracing::error!` for non-surfaced failures (missing binary, bad process
  output, anything swallowed instead of propagated).
- `tracing::debug!` for significant pipeline events (stage produced output,
  pipeline halted).
- `tracing::trace!` for high-frequency skips (stage skipped due to tool-name
  mismatch).
- `#[tracing::instrument(skip_all, fields(…))]` on `run_pipeline` and any
  function where span fields are worth capturing in production traces.

### Code rot

AI writes fast, which means wrong abstractions, dead paths, and stale comments
accumulate faster than in human-paced development. Actively resist rot:

- **Delete dead code.** No commented-out blocks, no `#[expect(dead_code)]` on
  production items, no types or functions left behind when their only caller is
  removed. Rust warns on unused items — treat every `dead_code` warning as an
  instruction to delete.
- **No `// TODO` in committed code, and `todo!()` / `unimplemented!()` are
  denied by Clippy** — they are hard errors, not just convention. If a
  workaround is genuinely temporary, the removal condition goes in the commit
  message or a linked issue.
- **Abstractions require at least two real callers.** A trait, wrapper type, or
  helper with one caller is premature. Inline it. Generalize only when the
  second caller exists.
- **Update docs in the same change.** When a module's design changes, its `//!`
  block changes in the same commit. A doc that describes the old design is worse
  than no doc — it actively misleads.
- **Tests must guard intent, not output.** When the implementation changes, ask
  whether the test still covers the original invariant. If you find yourself
  updating an expected value without understanding *why* it changed, the test has
  drifted — rewrite it to pin the behavior it actually means to guard.
- **No defensive checks for impossible cases.** Don't add `if x.is_none() {
  return; }` for an `x` the type system guarantees is always `Some`. Every such
  check is a lie about the invariants and a maintenance burden.

### Clippy suppressions

The workspace runs at maximum Clippy severity: `all`, `pedantic`, `nursery`,
`cargo`, and `restriction` lint groups are all `deny`. Every warning in every
group is a hard error. New code must satisfy the most aggressive lint profile
possible; the compiler rejects patterns that are merely suboptimal, not just
wrong.

Three lints enforce the suppression discipline:

- `allow_attributes = "deny"` — `#[allow(...)]` is itself a hard error. There
  is no way to silence a lint with `allow`.
- `unfulfilled_lint_expectations = "deny"` — a stale `#[expect(...)]` that no
  longer matches any firing lint is also a hard error. `expect` is
  self-cleaning: remove the suppression when the underlying code no longer
  triggers the lint.
- `allow_attributes_without_reason = "deny"` — every suppression must carry
  `reason = "…"`.

The only valid suppression form is `#[expect(clippy::foo, reason = "…")]`.
Crate-level test suppressions go in `#![cfg_attr(test, expect(...))]` at the
top of `lib.rs`:

```rust
#![cfg_attr(
    test,
    expect(
        clippy::panic_in_result_fn,
        reason = "rstest cases return Result for `?`-based setup but use \
                  assert_eq!/assert_matches! for assertions per project convention"
    )
)]
```

---

## FILE LAYOUT

Every `.rs` file follows this top-to-bottom declaration order:

1. `#![cfg_attr(...)]` crate attributes (`lib.rs` only)
2. `//!` module doc comment
3. `use` imports (see **Import ordering** below)
4. Named `const` items
5. Type definitions — structs and enums, outermost/public first, then private
6. `impl Stage for XxxStage` (stages only)
7. Other `impl XxxType { … }` blocks
8. `impl fmt::Display for XxxType` blocks
9. Private free functions (helpers, `short_rev`, `trim_in_place`, etc.)
10. `mod tests { … }`

### Import ordering

Three groups, each separated by a blank line, in this order:

```rust
use inceptool_engine::{EngineError, Stage};       // 1. internal crates
use inceptool_protocol::{Conn, Decision, …};

use serde::Deserialize;                            // 2. external crates
use serde_json::Value;

use std::borrow::Cow;                              // 3. std
use std::collections::BTreeMap;
use std::fmt;
```

`absolute_paths` is denied — inline qualified paths like `std::fmt::Display` in
function bodies or type positions are hard errors. Every type must arrive via a
`use` declaration at the top of the file.

### lib.rs re-export pattern

Each `lib.rs` declares modules and immediately re-exports their primary public
type(s) by name — never wildcard-re-export from stages or engine:

```rust
pub mod flake_lock;
pub use flake_lock::FlakeLockSummarizationStage;

pub mod read_write_guard;
pub use read_write_guard::ReadWriteGuardStage;
```

Bare `pub mod foo;` with no outer `///` is correct — `missing_docs` is
satisfied by the `//!` inner doc inside each module file. Do not add a
redundant `///` above `pub mod` when the module already has a `//!` block.

Protocol is the exception: it owns all wire types and uses `pub use module::*`
to present a flat API surface.

---

## DEPENDENCIES

All workspace dependencies use `default-features = false` and declare only the
features they actually need:

```toml
# [workspace.dependencies] in root Cargo.toml
serde = { version = "1.0", default-features = false, features = ["derive"] }
serde_json = { version = "1.0", default-features = false, features = ["raw_value", "std"] }
```

When adding a dependency:

1. Add it to `[workspace.dependencies]` in the root `Cargo.toml` with
   `default-features = false` and an explicit feature list.
2. Reference it from the crate's `Cargo.toml` as `dep.workspace = true` — never
   repeat the version in a crate-level `Cargo.toml`.
3. Justify the addition: prefer extending an already-present crate over
   introducing a new one.

---

## TESTING

### Structure — nested submods, not a flat list

`mod tests` contains named sub-modules, one per type or function under test.
Each submod opens with `use super::*` to inherit the parent's imports:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    mod stage {
        use super::*;
        // tests for Stage::run
    }

    mod flake_lock {
        use super::*;

        mod diff {
            use super::*;
            // tests for FlakeLock::diff
        }
    }

    mod short_rev {
        use super::*;
        // tests for short_rev()
    }
}
```

Never dump all test functions flat into one `mod tests`.

### Test functions

- Every test function has `#[rstest]` — even parameterless ones.
- `#[case::descriptive_snake_name]` labels are mandatory. Bare `#[case]` is not
  allowed.
- `redundant_test_prefix` is denied — name tests `does_thing`, never
  `test_does_thing`. The `#[rstest]` attribute is sufficient context.
- Parametrize fixtures with `#[default(…)]` + `#[with(…)]` for the same data
  shape at different values.
- Prefer `core::assert_matches!` over manual `match`/`if let` when testing enum
  shape.
- One observable behavior per test. A test that asserts decision AND reason AND
  substring is three tests collapsed — split it.
- All fallible test functions return `Result<(), TestError>` and use `?` for
  error propagation — never `unwrap` inside a test body.

### TestError

Each `mod tests` defines its own private `TestError` (`thiserror`). Never reuse
domain errors for test logic. Always include a `Failure(String)` variant; add
`#[from]` variants for every foreign error the test helpers can produce:

```rust
#[derive(thiserror::Error, Debug)]
enum TestError {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("Test failure: {0}")]
    Failure(String),
}
```

### Test helpers

- Document with `///`.
- `const fn` for pure constructors (e.g. `const fn session_meta() ->
  SessionMeta<'static>`).
- Stub types are named `StubX`, live inside `mod tests`, and carry a `///` doc
  explaining what behavior they stand in for.

---

## DOCUMENTATION

Every module begins with a `//!` block. Non-trivial modules (any Stage, the
Registry, complex type hierarchies) must use this structure:

```
//! # <Name> Architecture
//!
//! One-paragraph overview of what this module does and why it exists.
//!
//! ## Core Design
//!
//! The key insight/constraint driving the implementation.
//!
//! ## Flow
//!
//! 1. **Step name**: What happens and why.
//! 2. …
//!
//! ## Edge Cases        ← or ## Implementation Details
//!
//! What can go wrong and how the module handles it.
```

Simple/leaf modules (e.g. `error.rs`, `types.rs`) need only a one-sentence `//!`
summary.

`missing_docs` is denied — all public types and functions must have `///` doc
comments; the compiler enforces this. Private helpers that encode non-obvious
invariants also get `///`. Omit comments that restate the name.

`rustdoc::all` is denied — all rustdoc lints are hard errors. Doc examples that
appear in `///` blocks must compile and pass (`cargo test` runs them).

---

## ARCH

- root: `./Cargo.toml` (workspace)
- protocol: `crates/protocol/` — zero-copy schema, `HookEvent` enum, `Conn`
- engine: `crates/engine/` — `Stage` trait, `Registry` pipeline
- drivers: `crates/driver-claude/`, `crates/driver-gemini/` — wire protocol
  parsing/mapping
- stages: `crates/stages/` — native user-hook implementations

Crate naming: inner crates are prefixed `inceptool-` (e.g.
`inceptool-protocol`); directory paths are unprefixed.

Pipeline model: Elixir Phoenix `Plug`, but with `&mut Conn` (mutable) for
performance and simplicity. A stage returns `Ok(None)` to pass through or
`Ok(Some(output))` to produce a result; `Deny`/`Block` are terminal.

---

## CMD

- build: `cargo build --workspace`
- test: `cargo test --workspace`
- fmt: `cargo fmt`
- check: `cargo check --workspace --all-targets && cargo clippy --workspace --all-targets`

---

## WORKFLOW

**Explore → Plan (when >3 files change) → Implement → check → fmt → test → Document**

- *Explore*: read the relevant code before writing any.
- *Plan*: write a brief plan for multi-file changes; get confirmation before
  implementing.
- *check*: run the full check command above; fix every warning before
  continuing.
- *fmt*: always after check, before committing.
- *test*: `cargo test --workspace`; new behavior requires new tests; regressions
  require a reproducing test added before the fix.
- *Document*: update module `//!` docs for every pattern or stage change.
