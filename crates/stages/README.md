# Inceptool Stages

`inceptool-stages` (folder: `crates/stages`) provides the built-in
[`Stage`](../engine) implementations that are registered into the
`inceptool-engine` `Registry` to form the default `inceptool-rs` pipeline.

Each stage implements `Stage::hook` (the `HookKind` bucket it runs in),
`Stage::tool_names` (the tool names it filters on within that bucket), and
`Stage::run` (its `&mut Conn` middleware logic, à la Elixir Phoenix `Plug`).
A stage returns `Some(HookOutputEvent)` to short-circuit the pipeline with a
decision/context, or `None` to let the next stage run.

## Stages

### `RtkStage` (`rtk`)

- **Hook**: `PreToolUse`
- **Tools**: `Bash`, `run_shell_command`

Extracts the `command` from the tool input and pipes it through the external
`rtk rewrite <command>` binary. If `rtk` succeeds and produces a different,
non-empty command string, the stage rewrites the tool input's `command` field
and returns `Decision::Allow` with `reason: "RTK auto-rewrite"` and the
updated input. If `rtk` is missing, errors, or returns the same/empty output,
the stage is a no-op (`Ok(None)`) — failures to invoke `rtk` are only logged
via `tracing::error!`, never surfaced to the agent.

Note: the actual rewrite logic (e.g. preferring `rtk`-flavored `ls`/`tree`
equivalents) lives entirely in the external `rtk` binary; this stage is a thin
pass-through wrapper around it.

