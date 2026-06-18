# Inceptool

[![CI](https://github.com/asakura/inceptool/actions/workflows/ci.yml/badge.svg)](https://github.com/asakura/inceptool/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://asakura.github.io/inceptool/coverage.json)](https://asakura.github.io/inceptool/)
[![cargo-deny](https://img.shields.io/endpoint?url=https://asakura.github.io/inceptool/cargo-deny.json)](https://asakura.github.io/inceptool/)

So you've got an AI coding agent — Claude Code, Gemini CLI, take your pick —
wired up with hooks in your repo. `inceptool` sits in the middle of those
hooks and quietly decides what gets through, what gets rewritten, and what
gets blocked outright. Think of it as a small, fast proxy that gives you
programmable guardrails, auto-formatting, linting, and context-trimming for
your agent, without having to babysit it yourself.

## How it works

`inceptool` is invoked as `inceptool <driver> <hook>`. It reads a single JSON
hook payload from stdin, normalizes it into a protocol-level `Conn`, runs that
through a pipeline of stages, and writes a JSON response to stdout (or exits
with a code that tells the agent "nope, not this one").

A few things shape how it's built:

- **Zero-copy & fast** — it's Rust + Serde end to end, so
  (de)serialization, routing, and the core logic all happen in memory without
  unnecessary copying. You shouldn't notice it's there.
- **Stages as plugins** — each stage is a small struct implementing the
  `Stage` trait, declaring which hook kind and tool names it cares about. The
  engine routes each event to the right pipeline bucket and stops at the
  first stage that actually changes something.
- **Drivers translate, stages don't have to care** — instead of every stage
  knowing the quirks of Claude's or Gemini's JSON, a driver layer adapts the
  raw payload into one normalized protocol. Stage authors only ever deal with
  that normalized shape.
- **Real parsers, not regex hacks** — where it matters (guardrails and the
  like), `inceptool` uses proper parsers rather than shelling out to external
  binaries or hoping a regex holds up.

## Usage

```
inceptool <command> [args]
```

Commands:

- `inceptool claude <hook>` / `inceptool gemini <hook>` — process a hook
  event. `<hook>` is the raw hook event name as configured in your agent's
  settings (e.g. `PreToolUse` for Claude, `BeforeTool` for Gemini).
- `inceptool config` — print the fully resolved configuration (built-in
  defaults merged with any user overrides) as TOML.

## Configuration

Stages can be switched on or off via an `inceptool.toml` file, using a
`[hooks.<name>]` table with an `enabled` flag. If a hook isn't mentioned at
all, it's enabled by default.

Config is loaded in two layers and merged together:

1. User-level config from your XDG config dir (e.g.
   `~/.config/inceptool/inceptool.toml` on Linux), loaded first.
2. A project-local `inceptool.toml` in the current working directory, loaded
   second and overriding the user-level config on a per-hook basis.

The stages you can toggle are: `rtk`, `guardrails`, `read-write-guard`,
`format`, `lint`, `flake-lock-summarization`, and `pre-commit-runner`.

Here's an example that just disables the rtk rewrite stage, leaving
everything else at its default:

```toml
[hooks.rtk]
enabled = false
```

`read-write-guard`'s guarded-file rules can be extended or overridden the
same way, via a `[[read-write-guard.rules]]` array: a rule whose `filename`
matches a built-in (exactly, as a string — see below) replaces it; any other
filename is added alongside the built-ins.

```toml
[[read-write-guard.rules]]
filename = "Cargo.lock"
[read-write-guard.rules.access.deny_all]
hint = "Run `cargo update` to update it, then review the diff before committing."
note = "(NOTE: overridden by project config)"

[[read-write-guard.rules]]
filename = "my-tool.lock"
[read-write-guard.rules.access.deny_all]
hint = "Run `my-tool lock` to update it."
note = "(NOTE: this updates ALL my-tool dependencies)"
```

`filename` doesn't have to be an exact name — it supports glob/path patterns
too, matched in this order:

1. **Exact basename** (no `/`, no glob characters), e.g. `Cargo.lock` —
   matches a file with that exact name in any directory.
2. **Basename glob** (no `/`, has `*`/`?`/`[`), e.g. `*.pb.go` — matches by
   basename, e.g. `api/service.pb.go`.
3. **Full-path glob** (contains `/`), e.g. `**/node_modules/**` — matched
   against the full path given by the tool call. Use a leading `**/` to
   match at any depth, gitignore-style.

```toml
[[read-write-guard.rules]]
filename = "*.pb.go"
[read-write-guard.rules.access.deny_write]
hint = "Edit the `.proto` file and regenerate with `protoc-gen-go` instead."
note = "(NOTE: this file is fully regenerated on every protoc run)"

[[read-write-guard.rules]]
filename = "**/node_modules/**"
[read-write-guard.rules.access.deny_all]
hint = "Run `npm install` to manage dependencies — never hand-edit installed packages."
note = "(NOTE: regenerated entirely by the package manager)"
```

## Installation

Easiest path: grab a prebuilt static Linux binary (`x86_64` or `aarch64`) from
[GitHub Releases](https://github.com/asakura/inceptool/releases). Releases and
crates.io publishing are handled automatically by
[release-plz](https://release-plz.dev), driven by conventional commits on
`main`.

Or build it yourself from crates.io:

```sh
cargo install inceptool
```

### NixOS / Home Manager

If you're on Nix, you can consume the flake directly from your own
`flake.nix`. The package exposes a couple of overrideable knobs for the Rust
toolchain and crane setup:

```nix
# Inside a downstream flake.nix
outputs = { nixpkgs, inceptool-flake, ... }: {
  packages.x86_64-linux.default = inceptool-flake.packages.x86_64-linux.default.override {

    # Use the default rustc/cargo from nixpkgs (skip rust-overlay):
    rustToolchain = null;

    # OR pin a specific Rust version:
    # rustToolchain = pkgs.rust-bin.nightly.latest.default;

    # OR bring your own heavily-customized craneLib:
    # craneLib = myCustomCraneLib;
  };
};
```

## Development

This project leans on [Nix flakes](https://nixos.wiki/wiki/Flakes) for a fully
reproducible dev environment. The Nix shell brings the pinned Rust toolchain
(from `rust-toolchain.toml`) along with every external tool the `stages` crate
shells out to (`git`, `pre-commit`, `nixfmt`, `shfmt`, `shellcheck`,
`cargo-nextest`, `cargo-deny`) — all ready to go.

Jump in with:

```sh
nix develop
```

From there, the usual commands all work:

```sh
cargo build --workspace
cargo nextest run    # fast test runner (replacing `cargo test`)
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check     # supply-chain / license / advisory checks
cargo llvm-cov nextest --workspace --open  # generate & view HTML coverage report
```

No Nix? No problem — install the toolchain pinned in `rust-toolchain.toml` via
`rustup`, and the same `cargo` commands work, aside from the stages that lean
on Nix-provided tools.

`nix flake check` runs the same `fmt`/`clippy` checks CI does. If you need the
static musl binaries used for release artifacts, you can build them locally
with:

```sh
nix build .#inceptool-x86_64-linux-musl
nix build .#inceptool-aarch64-linux-musl
```

## Workspace layout

- [`crates/protocol`](crates/protocol/README.md) (`inceptool-protocol`) — the
  canonical, zero-copy wire protocol: `Conn`, `HookEvent`, and friends, shared
  by everything else.
- [`crates/engine`](crates/engine/README.md) (`inceptool-engine`) — the
  `Stage` trait and the `Registry` pipeline executor that dispatches hook
  events to the right stages.
- [`crates/driver-claude`](crates/driver-claude/README.md)
  (`inceptool-driver-claude`) — maps Claude Code hook payloads to and from the
  protocol.
- [`crates/driver-gemini`](crates/driver-gemini/README.md)
  (`inceptool-driver-gemini`) — same job, for Gemini CLI.
- [`crates/stages`](crates/stages/README.md) (`inceptool-stages`) — the
  built-in stages (guardrails, formatting, linting, summarization, and more)
  that ship with the `inceptool` binary.
