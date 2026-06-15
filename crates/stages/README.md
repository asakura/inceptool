# Inceptool Stages

`inceptool-stages` (folder: `crates/stages`) provides the built-in
[`Stage`](../engine) implementations that are registered into the
`inceptool-engine` `Registry` to form the default `inceptool` pipeline.

Each stage implements `Stage::hook` (the `HookKind` bucket it runs in),
`Stage::tool_names` (the tool names it filters on within that bucket), and
`Stage::run` (its `&mut Conn` middleware logic, à la Elixir Phoenix `Plug`).
A stage returns `Some(HookOutputEvent)` to short-circuit the pipeline with a
decision/context, or `None` to let the next stage run.

## Stages

### `FlakeLockSummarizationStage` (`flake_lock`)

- **Hook**: `PreToolUse`
- **Tools**: `Read`, `view_file`, `cat`

Triggers when the tool input's `file_path`/`path`/`AbsolutePath` points at a
file named `flake.lock`. Deserializes the file in a zero-copy fashion (field
values borrow `Cow<'a, str>` slices directly from the source buffer via
`#[serde(borrow)]`) into a minimal view of the `nodes` map, ignoring unused
fields like `narHash`, `lastModified`, `original`, and `inputs`.

It also uses `gix` to discover the enclosing repository, walk `HEAD`'s tree to
the `flake.lock` blob, and fetch the last committed version of the file
(parsed the same way), diffing each input's `rev` against its `HEAD`
counterpart. The stage then returns `Decision::Deny`, blocking the read before
the raw JSON ever reaches the agent, with `reason` set to a summary listing
each input's source (`owner/repo`, `git:<url>`, `tarball:<url>`, or
`path:<path>`) and its (possibly truncated) revision, e.g.:

```
flake.lock read blocked — use this summary instead (2 inputs, 1 changed vs HEAD):
  nixpkgs: NixOS/nixpkgs@1111111 -> 2222222
  flake-utils: numtide/flake-utils@abc1234
```

If the file can't be read, isn't valid JSON, or has no `nodes`, the stage is a
no-op (`Ok(None)`), letting the read proceed normally. If `HEAD` can't be
determined (not a git repository, file untracked, etc.), the summary is
produced without the "changed vs HEAD" diff.

This stage only fires if Claude Code's `PreToolUse` hook matcher covers
`Read`/`view_file`/`cat` (the default `inceptool` config only wires `Bash`) —
see the project's `~/.claude/settings.json`.

### `RtkStage` (`rtk`)

- **Hook**: `PreToolUse`
- **Tools**: `Bash`, `run_shell_command`

Extracts the `command` from the tool input and pipes it through the external
`rtk rewrite <command>` binary. If `rtk` produces a different, non-empty
command string on `stdout`, the stage rewrites the tool input's `command`
field and returns `Decision::Allow` with `reason: "RTK auto-rewrite"` and the
updated input — regardless of `rtk`'s exit code, since `rtk rewrite` exits `3`
(not `0`) for a rewritten *compound* command (one joined by `&&`, `;`, or
`||`), while still printing the rewrite to `stdout`. If `rtk` is missing, or
exits non-zero with no usable `stdout` (and that exit code isn't the
documented `1` for "no RTK equivalent"), the stage is a no-op (`Ok(None)`) —
these failures are only logged via `tracing::error!`, never surfaced to the
agent.

Note: the actual rewrite logic (e.g. preferring `rtk`-flavored `ls`/`tree`
equivalents) lives entirely in the external `rtk` binary; this stage is a thin
pass-through wrapper around it.
