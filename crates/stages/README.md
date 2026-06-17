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

### `ReadWriteGuardStage` (`read-write-guard`)

- **Hook**: `PreToolUse`
- **Tools**: `Write`, `Edit`, `MultiEdit`, `write_file`, `replace`, `Read`,
  `view_file`, `cat`

This crate has no notion of "built-in" guarded files — `ReadWriteGuardStage`
is constructed from a fully resolved `RuleSet` (`ReadWriteGuardStage::new`)
that the binary's `src/config` layer assembles from its embedded base config
(built-in defaults for the usual ecosystem lockfile/manifests, e.g.
`flake.lock`, `package-lock.json`, `Cargo.lock` — see `src/config/base.toml`
for the full list) merged with any user-supplied `[[read-write-guard.rules]]`
overrides from `inceptool.toml` — see the top-level `README.md` for the
override syntax.

Triggers when the tool input's `file_path`/`path`/`AbsolutePath` points at a
filename present in that `RuleSet`.

For modifying tools (`Write`, `Edit`, `MultiEdit`, `write_file`, `replace`),
the stage returns `Decision::Deny` with a `reason` pointing at the correct
ecosystem-native command (e.g. `nix flake update`, `cargo update`,
`npm install`) plus a note about its blast radius (e.g. "updates ALL Rust
dependencies").

For reading tools (`Read`, `view_file`, `cat`), the stage also returns
`Decision::Deny`, but with a generic reason noting that the file is
machine-generated noise and suggesting `git diff <file>` to see what changed.

Since `Decision::Deny` is terminal and stages run in registration order,
`FlakeLockSummarizationStage` (registered first) still wins for `flake.lock`
reads when it has a useful diff summary — this stage's generic read-deny is
the fallback for everything else (and for `flake.lock` when
`FlakeLockSummarizationStage` is a no-op).

The read-deny path only fires if Claude Code's `PreToolUse` hook matcher
covers `Read`/`view_file`/`cat` (the default `inceptool` config only wires
`Bash`) — see the project's `~/.claude/settings.json`.

### `PreCommitRunnerStage` (`pre-commit-runner`)

- **Hook**: `PostToolUse`
- **Tools**: `Write`, `Edit`, `MultiEdit`, `write_file`, `replace`

Runs after the agent writes a file. Reads `.pre-commit-config.yaml` from the
discovered git repository root (falling back to the session cwd outside a
git repository or in a bare clone), filters the configured hooks down to
those whose `files`/`exclude` regex matches the edited file (or that set
`always_run`), and spawns each matching hook's `entry` binary directly via
`std::process::Command` — bypassing the `pre-commit` CLI entirely.

Hooks run sequentially against the same file, so the stage snapshots the
file's content before the first matching hook and after the last one, and
folds the chain into `additional_context`: a single unified diff (rendered
via `gix::diff::blob`) attributed to every hook that changed the content
along the way, followed by one block per hook that exited non-zero. If
nothing changed and nothing failed, the stage is a no-op (`Ok(None)`).

A hook with a malformed `files` or `exclude` regex is logged via
`tracing::error!` and skipped (fails closed), rather than silently running
against — or failing to exclude — files it wasn't meant to touch.

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
