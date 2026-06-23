# inceptool-risk-data

Recursively scans, validates, and code-generates the `phf` command-risk lookup table
`inceptool-parable`'s build script reads.

## Overview

A `.toml` file under a data directory declares zero or more `[[command]]` tables,
each with a baseline risk rating plus the flag/combo/operand rules that can
escalate or mitigate it, and optionally one or more `[[command.subcommands]]`
blocks (`git push`, `docker run`) with their own independent ruleset. The crate's
one public function, `generate_command_table`, walks that directory, parses and
merges every file, validates every cross-reference the schema alone can't enforce
(duplicate names, a combo rule requiring an undeclared flag, an invalid regex
pattern), and renders the result into Rust source for
a `static COMMANDS: phf::Map<&'static str, &'static [PlatformEntry]> = ...;`
declaration — perfect-hashed once, at build time, not read back as TOML at runtime.

```toml
[[command]]
name = "kill"
kind = "builtin"
baseline_reason = "Sends a signal to another process — routine process management by itself."

  [[command.flag]]
  spellings = ["-9", "-KILL", "-SIGKILL"]
  effect = "escalate"
  profile = { reversibility = "irreversible" }
  reason = "SIGKILL can't be caught or cleaned up after."

  [[command.operand_rule]]
  pattern = "^-1$"
  effect = "escalate"
  profile = { blast_radius = "broad" }
  reason = "PID -1 broadcasts to every process the caller may signal."
```

Every schema struct uses `#[serde(deny_unknown_fields)]`, so a typo'd or stale
field name fails to parse instead of being silently ignored.

## The `[[command]]` table

| field                    | type                                                    | default       | meaning                                                                                                                                                           |
| ------------------------ | ------------------------------------------------------- | ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `name`                   | string                                                  | —             | Canonical name, as it appears as a `Statement::Command`'s name.                                                                                                   |
| `aliases`                | array of string                                         | `[]`          | Alternate names resolving to this same entry (e.g. `readarray` for `mapfile`).                                                                                    |
| `kind`                   | `"builtin"` \| `"external"`                             | —             | Provenance/documentation only — never read by classification logic.                                                                                               |
| `platform`               | `"gnu_linux"` \| `"bsd"` \| `"mac_os"` \| `"busybox"`   | `"gnu_linux"` | Which concrete implementation this declaration models — see [Platform](#platform-and-multiple-implementations) below.                                             |
| `grammar`                | `"gnu"` \| `"bsd"` \| `"go"`                            | `"gnu"`       | Which flag-syntax family this command's tokens follow — see [Flag grammar](#flag-grammar) below.                                                                  |
| `case_sensitive`         | bool                                                    | `true`        | Whether flag spellings match case-sensitively.                                                                                                                    |
| `baseline`               | [profile patch](#the-risk-model)                        | all-unset     | This command's rating with no flags considered. Any axis left unset defaults to its lowest value.                                                                 |
| `baseline_reason`        | string                                                  | —             | Why the baseline is what it is. Required.                                                                                                                         |
| `short_flags_combinable` | bool                                                    | `false`       | Whether multi-letter single-dash flags combine (`-ex` is `-e` plus `-x`) rather than being one atomic flag. Rejected when `grammar = "go"`, which never clusters. |
| `flag`                   | array of [flag](#the-commandflag-table)                 | `[]`          | This command's individually rated flags, in its global scope.                                                                                                     |
| `combo_rule`             | array of [combo rule](#the-commandcombo_rule-table)     | `[]`          | Rules keyed on more than one global-scope flag being present at once.                                                                                             |
| `operand_rule`           | array of [operand rule](#the-commandoperand_rule-table) | `[]`          | Rules matched against every literal positional argument, flag-shaped or not.                                                                                      |
| `subcommands`            | array of [subcommand](#the-commandsubcommands-table)    | `[]`          | One level deep only. Each has its own independent ruleset, layered on top of the fields above.                                                                    |

## The `[[command.flag]]` table

One semantically distinct flag — every spelling/alias of it rated identically,
rather than one row per spelling.

| field         | type                                                    | default   | meaning                                                                                                                                                                                     |
| ------------- | ------------------------------------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `spellings`   | array of string                                         | —         | Every spelling this one semantic flag is known by, e.g. `["-9", "-KILL", "-SIGKILL"]`. Must be unique within the command's scope (checked at validation).                                   |
| `effect`      | `"escalate"` \| `"mitigate"`                            | —         | Whether this rates the command worse or better.                                                                                                                                             |
| `profile`     | [profile patch](#the-risk-model)                        | all-unset | The axis change this flag applies.                                                                                                                                                          |
| `reason`      | string                                                  | —         | Why this flag has this effect. Required.                                                                                                                                                    |
| `takes_value` | `"combined"` \| `"separate"`                            | unset     | How this flag's value is spelled, when present: `combined` is `--name=value`; `separate` is `-s value` (the value is the _next_ argument). Leave unset for a flag that never takes a value. |
| `value_rule`  | array of [value rule](#the-commandflagvalue_rule-table) | `[]`      | Value-conditioned overrides, tried in order.                                                                                                                                                |

### The `[[command.flag.value_rule]]` table

A flag-value-conditioned rating override, tried in declaration order; the flag's
own `effect`/`profile` apply when none match (or the flag carries no value at all).

| field     | type                             | default   | meaning                                                                 |
| --------- | -------------------------------- | --------- | ----------------------------------------------------------------------- |
| `pattern` | string (regex)                   | —         | Matched against the flag's value. Must compile (checked at validation). |
| `effect`  | `"escalate"` \| `"mitigate"`     | —         | Whether a match rates the command worse or better.                      |
| `profile` | [profile patch](#the-risk-model) | all-unset | The axis change a match applies.                                        |
| `reason`  | string                           | —         | Why a match has this effect. Required.                                  |

## The `[[command.combo_rule]]` table

A rule that escalates or mitigates only when _every_ listed flag spelling is
present on the same invocation.

| field      | type                             | default   | meaning                                                                                                                                                                                          |
| ---------- | -------------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `requires` | array of string                  | —         | Every flag spelling that must be present. Each must be declared by some `flag` in this scope, or — for a subcommand's combo rule — in the parent command's global scope (checked at validation). |
| `effect`   | `"escalate"` \| `"mitigate"`     | —         | Whether satisfying this combo rates the command worse or better.                                                                                                                                 |
| `profile`  | [profile patch](#the-risk-model) | all-unset | The axis change this combo applies.                                                                                                                                                              |
| `reason`   | string                           | —         | Why this combination has this effect. Required.                                                                                                                                                  |

## The `[[command.operand_rule]]` table

A rule matched against every literal positional argument, independent of whether it's
flag-shaped (e.g. `kill`'s `-1` pid operand).

| field     | type                             | default   | meaning                                                                                   |
| --------- | -------------------------------- | --------- | ----------------------------------------------------------------------------------------- |
| `pattern` | string (regex)                   | —         | Matched against each literal argument's exact text. Must compile (checked at validation). |
| `effect`  | `"escalate"` \| `"mitigate"`     | —         | Whether a match rates the command worse or better.                                        |
| `profile` | [profile patch](#the-risk-model) | all-unset | The axis change a match applies.                                                          |
| `reason`  | string                           | —         | Why a match has this effect. Required.                                                    |

## The `[[command.subcommands]]` table

One subcommand's own baseline rating and flag/combo/operand rules (`push` on
`git`, `run` on `docker`) — one level deep only. Its flag/combo/operand fields
are an independent namespace from the parent command's global scope: the same
literal spelling can mean something different globally vs. inside one subcommand.

| field                    | type                                                    | default   | meaning                                                                                                                   |
| ------------------------ | ------------------------------------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------- |
| `name`                   | string                                                  | —         | Canonical name, as it appears as the first literal argument after the parent command's own flags.                         |
| `aliases`                | array of string                                         | `[]`      | Alternate names resolving to this same subcommand. Must be unique among the parent's subcommands (checked at validation). |
| `baseline`               | [profile patch](#the-risk-model)                        | all-unset | This subcommand's rating with no flags considered, independent of the parent's own baseline.                              |
| `baseline_reason`        | string                                                  | —         | Why the baseline is what it is. Required.                                                                                 |
| `short_flags_combinable` | bool                                                    | `false`   | Independent of the parent command's own setting.                                                                          |
| `flag`                   | array of [flag](#the-commandflag-table)                 | `[]`      | This subcommand's own flags.                                                                                              |
| `combo_rule`             | array of [combo rule](#the-commandcombo_rule-table)     | `[]`      | May `requires` a flag from this subcommand's own scope _or_ the parent's global scope.                                    |
| `operand_rule`           | array of [operand rule](#the-commandoperand_rule-table) | `[]`      | Matched against every literal positional argument once this subcommand has been identified.                               |

```toml
[[command]]
name = "git"
kind = "external"
baseline_reason = "Version control - most subcommands are routine and additive by themselves."

  [[command.flag]]
  spellings = ["-C"]
  effect = "escalate"
  takes_value = "separate"
  reason = "Runs as if started in the given directory - benign by itself."

  [[command.subcommands]]
  name = "push"
  baseline_reason = "Uploads local commits to the remote - routine and additive by itself."

    [[command.subcommands.flag]]
    spellings = ["-f", "--force"]
    effect = "escalate"
    profile = { reversibility = "irreversible" }
    reason = "Overwrites the remote branch's history."

    [[command.subcommands.combo_rule]]
    requires = ["-C", "-f"]
    effect = "escalate"
    reason = "-C is a parent-scope global flag, -f is this subcommand's own — either scope satisfies requires."
```

## The risk model

Every rule (a command's `baseline`, a `flag`, a `value_rule`, a `combo_rule`, or
an `operand_rule`) ratings the same way: as a _profile patch_ — `{ trust, reversibility,
blast_radius, disclosure, persistence, privilege, auditability, exposure, verification }` —
touching only the axes it has an opinion about. Each axis is independently optional; an unset
axis means "this rule has nothing to say about that axis", not "narrow" or "none".

| axis            | values (low → high)                                                  | question                                                                         |
| --------------- | --------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `trust`         | `none` → `delegates_execution` → `arbitrary_execution`                | Does this extend what the script trusts (arbitrary/uncontrolled code execution)? |
| `reversibility` | `reversible` → `recoverable` → `irreversible`                         | Can this be undone afterward?                                                    |
| `blast_radius`  | `narrow` → `moderate` → `broad`                                       | How much of the system can one invocation affect?                                |
| `disclosure`    | `none` → `discloses_data` → `discloses_credentials`                   | Does this reveal information the script shouldn't?                              |
| `persistence`   | `ephemeral` → `session_scoped` → `persistent`                         | Does this outlive the current invocation/session?                               |
| `privilege`     | `none` → `delegated` → `elevated`                                     | Does this change what identity/privilege level the script operates as?           |
| `auditability`  | `intact` → `reduced` → `tampered`                                     | Does this destroy or disable evidence of what happened?                          |
| `exposure`      | `contained` → `peer_reachable` → `network_reachable`                  | Does this open a new path for something outside to reach in?                     |
| `verification`  | `checked` → `weakened` → `bypassed`                                   | Does this skip a check whose entire job is catching a problem before it causes harm? |

Every axis is now 3-level. `escalate` folds in the worse value via per-axis maximum (high beats mid beats
low); `mitigate` caps via per-axis minimum, the same as it always has — only the variant count grew, the
fold semantics didn't change.

A command's final rating is its `baseline`, folded with every applicable flag/combo/operand
rule's patch, per axis, independently:

- Rules with `effect = "escalate"` fold in via a per-axis **maximum** with every
  other escalating rule and the baseline — the worst of all of them wins.
- Rules with `effect = "mitigate"` fold in via a per-axis **minimum**, applied
  _after_ every escalation — a mitigating flag always wins regardless of
  declaration order (e.g. `git push --force-with-lease` caps `reversibility`
  back to `reversible` even though `--force-with-lease` is declared after
  `-f`/`--force` in the same flag list).

A patch with every axis unset (`profile` omitted entirely) carries no risk by
itself — useful for a flag/combo rule declared only so something else can
reference it (e.g. a combo rule's `requires`), or a rule whose only purpose
is documentation.

## What the corpus catches

The nine axes above are abstract; the `.toml` files under
`crates/parable/risk-data/` are where they're given concrete meaning, one
real misuse pattern per rule rather than a hypothetical worst case. Grouped
by the axis each mostly drives:

- **`trust` (arbitrary execution).** Anything that hands an attacker-influenced
  string to an interpreter: shell builtins (`eval`, `exec`, `source`/`.`,
  `trap`), commands that *are* interpreters (`awk`, `cargo` via build scripts,
  `ansible-playbook`, `duckdb` extensions), and exec-shaped flags on otherwise
  routine tools (`find -exec`, `fd -x`, `docker run --privileged`/
  `--network=host`/`--pid=host`/`--userns=host`, `git config
  core.sshCommand`/`alias.*=!cmd`, `fzf --listen-unsafe`).
- **`reversibility` (irreversible).** Operations with no undo: `rm -rf`, `dd`,
  `shred`, `wipefs`, `mkfs`, `git push --force`/`reset --hard`/
  `filter-branch`, `curl -o` (overwrites a local file with no confirmation),
  `chmod a+rwx`/`0777` (not because the permission bits can't be reverted, but
  because whatever an attacker did while they were wide open can't be undone
  after the fact by reverting them).
- **`blast_radius` (broad).** Operations not scoped to the invocation:
  `sudo` (root for whatever follows, though it doesn't recursively re-rate
  that command — see `privilege.toml`), `chmod -R`/`chown -R`, `dd
  of=/dev/sdX` (a whole device, not a partition or file), `mkfs`/`wipefs`,
  `docker run --privileged`.
- **`disclosure` (data/credentials).** Operations that reveal secrets or
  local data to a party that shouldn't see them: cloud-CLI auth flows
  (`aws ... get-secret-value`, `gcloud ... login`, `az ... get-credentials`,
  `gh auth`/`secret`, `auth0 login`), `curl -d`/`-F`/`-T` (sends local data to
  a remote URL), `set -x` (execution tracing prints expanded command lines —
  any interpolated token or password included — to the terminal or a log).
- **`persistence` (outlives the invocation).** Operations that change
  remote or durable state: cloud resource mutations (`aws create-*`/`put-*`,
  `az create`/`deploy`, `gcloud create`/`deploy`), a non-`GET` `curl -X`,
  `defaults`/`dscl` (persistent system/user configuration).
- **`privilege` (elevated).** Operations that switch the identity the rest of
  the invocation runs as: `sudo`, `su` (both rate the privilege switch
  itself, not what follows it — see `privilege.toml`), `aws ... assume-role`
  (retrieves credentials for another IAM identity, not just a secret value).
- **`auditability` (tampered).** Operations that destroy or disable a record
  of what happened, rather than changing or disclosing anything themselves:
  `history -c`/`-d` (clears the shell's own audit trail), `aws ...
  stop-logging` (disables CloudTrail).
- **`exposure` (network-reachable).** Operations that open a path reachable
  from outside an existing boundary: `docker run -p 0.0.0.0:...` (binds to
  every host interface, not just loopback or the container network), `aws
  ... authorize-security-group-ingress` (opens a new inbound firewall rule).
- **`verification` (bypassed).** Operations that skip a check whose entire
  job is catching a problem before it causes harm: `curl -k`/`--insecure`
  (skips TLS certificate validation), `git commit --no-verify` (skips
  pre-commit/commit-msg hooks). Never independently raises severity — see
  `RiskProfile::severity`'s doc comment in `crates/parable/src/risk.rs`.

A few mechanisms cut across more than one axis, or don't fit any single one
cleanly enough to list above:

- **Destructive by default, not by flag.** `black`, `gzip`/`gunzip`, and
  `binhex` carry their irreversibility in the *baseline* — overwriting or
  deleting the original is what they do absent any flag at all; a flag
  (`-k`/`--keep`, `--check`) is what makes them safe, the inverse of `rm`'s
  shape (benign baseline, `-f` escalates).
- **Delegation to a trailing command.** `caffeinate`, `env`, and `arch` don't
  interpret a string the way `eval`/`awk` do — they just `exec` whatever
  command is given as a trailing argument, however innocuous the wrapper
  itself looks.
- **Deferred, triggered execution.** A registered action that doesn't run
  immediately, only later, on some event: `alias` (a destructive command
  embedded in the definition, run on next invocation), `bind -x` (on
  keypress), `complete -C` (on tab-completion), `mapfile -C` (per line
  read), `trap` (on a signal, or EXIT/ERR/DEBUG), `fc -s` (re-executes a
  history entry without ever displaying it), `hash -p` (remaps where a
  command name resolves to).
- **Supply-chain execution.** Installing or extending something runs
  third-party code as a side effect of the install itself: `brew
  install`/`upgrade`/`tap` (downloads and runs Ruby), `gh extension`
  (installs and runs a third-party binary), `cargo`/`cdk` (build scripts /
  app synthesis, which run local project code as a side effect of most
  subcommands).
- **Mitigation, not just escalation.** The corpus models the inverse just as
  carefully: flags/operands that prove an operation is safe despite looking
  risky — `rm -i`, `git restore --staged`, `curl -X GET`, `cdk metadata`,
  `gzip -k`, `black --check`, `trap -p`/`-l`. A mitigating rule always wins
  over an escalating one regardless of declaration order — see
  [the risk model](#the-risk-model).

This taxonomy describes what the dataset is *for* — the real-world danger each
rule is standing in for. It says nothing about the crate's own code-security
posture (trusted input, no sandboxing, heuristic coverage); that's the
[Threat model](#threat-model) section below.

## Platform and multiple implementations

The same command `name` may be declared more than once, each under a different `platform`,
to model multiple real implementations of the same command (GNU vs. BSD vs.
macOS vs. busybox coreutils) without unioning every implementation's flags into
one conservative ruleset. Two declarations of the same name _under the same_
`platform` are a validation error (`DuplicateCommand`); two declarations of
the same name under _different_ platforms are not — both are kept, and
a caller selects which to look up.

## Flag grammar

`grammar` selects which flag-tokenization convention a command's arguments follow.
The schema only _names_ the convention; the tokenization logic itself lives
in `inceptool_parable::risk`, not in this crate.

| `grammar`       | convention                                                                                                                                                        |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gnu` (default) | `--long[=value]`, `-short`; a run of single-dash lowercase letters clusters into one flag per letter iff `short_flags_combinable` is set.                         |
| `bsd`           | `-short` only — a `--`-prefixed token (other than the bare `--` end-of-options marker) is never parsed as a flag.                                                 |
| `go`            | `-name` and `--name` are the _same_ flag regardless of length, and are never clustered. `short_flags_combinable = true` is a validation error under this grammar. |

## Validation

Beyond what the schema's types enforce structurally, `generate_command_table`
rejects a dataset for any of the following, each surfaced as
a distinct `RiskDataError` variant naming the offending command
(and subcommand, when applicable):

- The scanned root missing, not a directory, or unreadable — likewise any
  directory entry or `.toml` file under it that fails to read.
- No `.toml` file found anywhere under the scanned root.
- A file whose content isn't valid TOML, or doesn't match this schema (including
  an unknown field, since every struct denies them).
- Two commands — or a command and another command's alias — sharing
  a name under the _same_ `platform`.
- A command (or one of its subcommands) declaring the same flag spelling twice
  within its own scope.
- A combo rule's `requires` naming a flag spelling that scope doesn't declare
  (a subcommand's combo rule may also draw on the parent's global flags).
- A `value_rule` or `operand_rule` pattern that isn't a valid regex.
- Two subcommands of the same command — by name or alias — sharing a name.
- `grammar = "go"` together with `short_flags_combinable = true`.

## Threat model

This crate is a **build-time data compiler**, not a runtime security boundary. Its entire
attack surface is the `.toml` corpus and the crate's own code; there is no user input at
runtime, because everything it produces is baked into `'static` Rust source and compiled in.

- **Trusted input.** The `.toml` files under the scanned root are first-party, reviewed source
  — the same trust level as any other file in this repository. `generate_command_table`
  validates *structure* (duplicate names, dangling `combo_rule.requires`, malformed regex), not
  *intent*: nothing stops a reviewed-but-wrong entry from under- or over-rating a real command.
  That risk is mitigated by code review of the data, not by this crate's validation pass.
- **Not a sandbox.** Generating (or consuming) this table never executes, blocks, or sandboxes
  anything. Enforcement, if any, lives entirely in the caller — `inceptool-parable`'s
  `UnsafeCommand` rule turns a non-benign `RiskProfile` into a `Finding`; this crate has no idea
  that rule exists.
- **Heuristic, not exhaustive.** Coverage is whatever the `.toml` corpus happens to declare. A
  command, subcommand, or flag spelling that isn't catalogued is indistinguishable from one
  explicitly rated benign — both fold to the all-lowest `ProfilePatch`. Treat a clean
  classification as "nothing *known* to be risky," not as a guarantee of safety.
- **Regex cost is a build-time, not runtime, concern.** `value_rule`/`operand_rule` patterns are
  compiled once at runtime (by the consumer, from the generated table) and matched per-argument;
  since the corpus is first-party and reviewed, a catastrophically backtracking pattern is a
  data-quality bug to fix, not an externally triggerable denial of service — no untrusted party
  controls which patterns exist.
- **Out of scope.** Anything downstream of classification (how a caller acts on a `RiskProfile`,
  whether it's bypassed, how taint-tracking composes with it) is `inceptool-parable`'s concern,
  not this crate's.

## Usage

```rust,no_run
use inceptool_risk_data::generate_command_table;

let source = generate_command_table("risk-data")?;
std::fs::write(format!("{}/risk_data.rs", std::env::var("OUT_DIR")?), source)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or
[MIT License](../../LICENSE-MIT) at your option.
