# Inceptool Parsers

`inceptool-parsers` (folder: `crates/parsers`) provides zero-copy parsers
for the external file formats that `inceptool-stages` stages build policy
on top of. A parser only decodes raw file content into typed Rust structs —
it has no awareness of `Stage`, `Decision`, or any other engine/protocol
concept.

## Parsers

### `flake_lock`

Decodes a `flake.lock` file (Nix flake lockfile, format version 7) into a
zero-copy view of its `nodes` map: field values borrow `Cow<'a, str>`
slices directly from the source buffer via `#[serde(borrow)]`, and only the
`locked` pin is retained per node (`inputs`/`original` are ignored).

`FlakeLock::diff` compares two parsed revisions (e.g. the working copy
against the version committed at `HEAD`) and produces one `DiffEntry` per
non-root input, recording its formatted source label, current/previous
pinned revision, and whether it changed.

`inceptool-stages`'s `FlakeLockSummarizationStage` is the sole consumer: it
reads the file from disk, fetches the `HEAD` revision via `gix`, calls
`FlakeLock::diff`, and renders the result as the `reason` for a denied
`Read`.

### `pre_commit`

Parses `.pre-commit-config.yaml` files (as produced by
[pre-commit](https://pre-commit.com)) into `PreCommitConfig` → `Repo` →
`Hook`. String fields use `Cow<'a, str>` with `#[serde(borrow)]` via
`serde-saphyr`, which — unlike the legacy `serde_yaml` crate — properly
borrows plain YAML scalars; quoted scalars that require unescaping fall
back to an owned `Cow`.

`Hook::files_regex` / `Hook::exclude_regex` compile the hook's `files` /
`exclude` patterns with [`fancy_regex`], since pre-commit uses Python `re`
syntax (lookaheads/lookbehinds included), not file globs.

`inceptool-stages`'s `PreCommitRunnerStage` is the sole consumer: it parses
the discovered repo's `.pre-commit-config.yaml`, matches each hook's
patterns against the edited file, and spawns the hook's `entry` binary
directly — bypassing the `pre-commit` CLI.
