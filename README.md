# Inceptool

`inceptool-rs` is a high-performance, native Rust replacement for the
Nushell-based "Inceptool". It runs as a hook executor / interceptor proxy
between an AI coding agent (Claude Code or Gemini CLI) and the operating system,
orchestrating agent safety, linting, formatting, and context-optimization logic.

## How It Works

`inceptool-rs` reads a single JSON hook payload from stdin, normalizes it into a
protocol-level `Conn`, runs it through a pipeline of stages, and writes a JSON
response to stdout (or exits with a code to signal a blocking decision back to
the agent).

- **Zero-copy & low overhead**: Orchestration is built in Rust using Serde for
  rapid serialization/deserialization. Unlike the Nushell version, which relied
  on spawning external shell processes for basic orchestration (50-100ms
  cumulative latency hits), `inceptool-rs` handles all routing and core logic
  in-memory.
- **Modular engine system**: Stages are discrete plugins implementing the
  `Stage` trait. Each stage declares the `HookKind` it runs for and the tool
  names it applies to (`"*"` for all), and executes via `run`. The
  `Engine`/`Registry` dispatches to the pipeline bucket matching the incoming
  hook and returns early on the first stage that produces an output
  modification.
- **Driver abstraction layer**: Instead of hardcoding AI-CLI-specific schemas
  throughout the pipeline, driver implementations (the `Driver` trait) adapt raw
  JSON from the agent into a normalized protocol format (`Conn`). Stage
  developers only ever target this normalized `protocol`.
- **Native parsers**: Critical checks such as guardrails leverage
  high-performance Rust AST parsers (e.g. `flash` for bash shell scripts) rather
  than external binaries or brittle regex checks.

The binary auto-selects a driver heuristically based on the incoming JSON:
payloads containing Claude Code hook names (`PreToolUse`, `PostToolUse`,
`UserPromptSubmit`) or a `permission_mode` field are routed through the Claude
driver; everything else falls back to the Gemini driver.

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

