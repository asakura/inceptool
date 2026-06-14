# Inceptool

[![CI](https://github.com/asakura/inceptool/actions/workflows/ci.yml/badge.svg)](https://github.com/asakura/inceptool/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://asakura.github.io/inceptool/coverage.json)](https://asakura.github.io/inceptool/)
[![cargo-deny](https://img.shields.io/endpoint?url=https://asakura.github.io/inceptool/cargo-deny.json)](https://asakura.github.io/inceptool/)

`inceptool` runs as a hook executor / interceptor proxy
between an AI coding agent (Claude Code or Gemini CLI) and the operating system,
orchestrating agent safety, linting, formatting, and context-optimization logic.

## How It Works

`inceptool` is invoked as `inceptool <driver> <hook>`. It reads a single
JSON hook payload from stdin, normalizes it into a protocol-level `Conn`, runs
it through a pipeline of stages, and writes a JSON response to stdout (or
exits with a code to signal a blocking decision back to the agent).

- **Zero-copy & low overhead**: Orchestration is built in Rust using Serde for
  rapid serialization/deserialization. Unlike the Nushell version, which relied
  on spawning external shell processes for basic orchestration (50-100ms
  cumulative latency hits), `inceptool` handles all routing and core logic
  in-memory.
- **Modular engine system**: Stages are discrete plugins implementing the
  `Stage` trait. Each stage declares the `HookKind` it runs for and the tool
  names it applies to (`"*"` for all), and executes via `run`. The
  `Engine`/`Registry` dispatches to the pipeline bucket for the `HookKind`
  selected by the CLI invocation, and returns early on the first stage that
  produces an output modification.
- **Driver abstraction layer**: Instead of hardcoding AI-CLI-specific schemas
  throughout the pipeline, driver implementations (the `Driver` trait) adapt raw
  JSON from the agent into a normalized protocol format (`Conn`). Stage
  developers only ever target this normalized `protocol`.
- **Native parsers**: Critical checks such as guardrails leverage
  high-performance Rust AST parsers (e.g. `flash` for bash shell scripts) rather
  than external binaries or brittle regex checks.

## Usage

```
inceptool <driver> <hook>
```

- `<driver>`: `claude` or `gemini` - selects the wire format used to parse
  stdin and format stdout.
- `<hook>`: the raw hook event name configured for this command in the agent's
  hook settings (e.g. `PreToolUse` for Claude, `BeforeTool` for Gemini). Each
  driver maps this, via `Driver::hook_kind`, to the canonical `HookKind` that
  selects which stage pipeline runs - dispatch is driven entirely by this CLI
  argument, never by inspecting the JSON payload.

## Configuration

Stages can be individually enabled or disabled via an `inceptool.toml` file
using a `[hooks.<name>]` table with an `enabled` boolean. Every hook defaults to
`enabled = true` if it is not mentioned in the config at all.

Configuration is loaded in two layers, merged together:

1. The user-level config from the XDG config directory (e.g.
   `~/.config/inceptool/inceptool.toml` on Linux), loaded first.
2. A project-local `inceptool.toml` in the current working directory, loaded
   second.

Entries from the local (CWD) config override entries from the user-level config
on a per-hook basis.

The valid hook names correspond to the stages above: `rtk`, `guardrails`,
`write-guard`, `format`, `lint`, `flake-lock-summarization`, and
`pre-commit-summarization`.

Example `inceptool.toml`:

```toml
# Disable the rtk rewrite and flake.lock summarization stages,
# leave everything else at its default (enabled).
[hooks.rtk]
enabled = false
```

## Installation

Prebuilt static Linux binaries (`x86_64` and `aarch64`) are attached to
[GitHub Releases](https://github.com/asakura/inceptool/releases). Releases
and crates.io publishing are automated by
[release-plz](https://release-plz.dev) from conventional commits on `main`.

Alternatively, install from crates.io:

```sh
cargo install inceptool
```

## Development

This project uses [Nix flakes](https://nixos.wiki/wiki/Flakes) to provide
a fully reproducible development environment. The Nix shell automatically
provisions the pinned Rust toolchain (via `rust-toolchain.toml`) along with
all external tools the `stages` crate relies on (`git`, `pre-commit`,
`nixfmt`, `shfmt`, `shellcheck`, `cargo-nextest`, `cargo-deny`).

To enter the development environment, run:

```sh
nix develop
```

Once inside the Nix shell, you have access to the full suite of tools:

```sh
cargo build --workspace
cargo nextest run    # fast test runner (replacing `cargo test`)
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check     # supply-chain / license / advisory checks
cargo llvm-cov nextest --workspace --open  # generate & view HTML coverage report
```

Without Nix, install the toolchain pinned in `rust-toolchain.toml` via
`rustup`; the same `cargo` commands work, aside from stages that shell out to
Nix-provided tools.

`nix flake check` runs the `fmt`/`clippy` checks used in CI. The static musl
binaries used for release artifacts can be built locally with:

```sh
nix build .#inceptool-x86_64-linux-musl
nix build .#inceptool-aarch64-linux-musl
```

## Workspace Layout

- [`crates/protocol`](crates/protocol/README.md) (`inceptool-protocol`) -
  canonical, zero-copy wire protocol and `Conn`/`HookEvent` data structures
  shared by every other crate.
- [`crates/engine`](crates/engine/README.md) (`inceptool-engine`) - the `Stage`
  trait and the `Registry` pipeline executor that dispatches hook events to the
  right stages.
- [`crates/driver-claude`](crates/driver-claude/README.md)
  (`inceptool-driver-claude`) - driver mapping Claude Code hook payloads to/from
  the protocol.
- [`crates/driver-gemini`](crates/driver-gemini/README.md)
  (`inceptool-driver-gemini`) - driver mapping Gemini CLI hook payloads to/from
  the protocol.
- [`crates/stages`](crates/stages/README.md) (`inceptool-stages`) - the built-in
  stage implementations (guardrails, formatting, linting, summarization, etc.)
  registered by the `inceptool` binary.

## Stage Pipeline

The `Registry` holds one pipeline per `HookKind`. Each stage is placed into the
pipeline for the `HookKind` it declares via `Stage::hook`, and runs in
registration order within that bucket, filtered by its `Stage::tool_names` list.

### PreToolUse

(tools: `Bash`, `run_shell_command`, `Write`, `Edit`, `MultiEdit`, `write_file`,
`replace`)

1. **RtkStage** (`rtk`) - rewrites `ls` and `tree` invocations to their `rtk`
   equivalents.

### PostToolUse

(tools: `Write`, `Edit`, `MultiEdit`, `write_file`, `replace`, `Read`,
`view_file`, `cat`, `Bash`, `run_shell_command`)
