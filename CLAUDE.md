# CLAUDE.md

## RULES

- errors: `thiserror` enums in lib crates (protocol/engine/drivers/hooks);
  `miette` for cli terminal output. All errors MUST be wrapped by the crate's
  `Error` type (e.g. `ProtocolError` in error.rs).
- testing: no `.unwrap()`/`.expect()` anywhere, incl. tests; use `?` -> crate's
  `Result` (or `miette::Result` in integration tests); prefer
  `core::assert_matches!` over manual matches.
- test-errors: each `mod tests` defines its own private `TestError` (`thiserror`),
  not a shared/global one; don't reuse domain errors (e.g. `ProtocolError`) for
  test logic; include `Failure(String)` for test-specific panics (note:
  thiserror can't `#[from] String`, since `String` has no `std::error::Error`
  impl).
- test-structure: colocate tests w/ units; keep hyper-specific - split broad
  tests via `rstest` + fixtures/cases/matricies.
- naming: inner crates prefixed `inceptool-` (e.g. `inceptool-protocol`); folder
  paths unchanged.
- design: zero-copy by default; use `Cow<'a, str>` and
  `serde_json::value::RawValue` (`RawJson`) extensively.
- arch: middleware follows Elixir Phoenix `Plug`, but with `&mut Conn` (mutable,
  not immutable) for perf/simplicity in Rust.
- output-builder: use `HookOutput` enum variants so invalid states are
  unrepresentable.
- doc: update `docs/` per pattern/feature change; document all modules/types
  fully.

## ARCH

- root: `./Cargo.toml` (workspace)
- protocol: `crates/protocol/` - zero-copy schema, `HookEvent` enum
- engine: `crates/engine/` - stage execution/pipeline
- drivers: `crates/driver-claude/`, `crates/driver-gemini/` - wire protocol
  parsing/mapping
- stages: `crates/stages/` - native user-hook impls

## CMD

- build: `cargo build --workspace`
- test: `cargo test --workspace`
- fmt: `cargo fmt`
- check: `cargo check --workspace --all-targets && cargo clippy --workspace --all-targets -- -D warnings`

## WORKFLOW

Explore > Plan (>3 files) > Implement > check > fmt > test > Document
