# Inceptool Risk Data Crate (`inceptool-risk-data`)

TOC only — see each file's `//!`/`///` docs for the why.

## `lib.rs` — public API
- `generate_command_table(dir) -> Result<proc_macro2::TokenStream, RiskDataError>`: scan → parse → validate → codegen, one shot. The crate's only public function.
- Re-exports: `CommandEntry`/`FlagEntry`/`ComboRuleEntry`/`OperandRuleEntry`/`ValueRuleEntry`/`PlatformEntry`/`RuleSetEntry` (`entry.rs`); `Auditability`/`BlastRadius`/`Disclosure`/`Effect`/`Exposure`/`FlagGrammar`/`Persistence`/`Platform`/`Privilege`/`ProfilePatch`/`Reversibility`/`TakesValue`/`TrustImpact`/`Verification` (`types/`); `RiskDataError` (`error.rs`). These exist for the *caller's* own code, not the generated tokens — see `codegen/`.
- `types` module is private — `Dataset`/`Command`/`Subcommand`/`Flag`/`ComboRule`/`OperandRule`/`ValueRule`/`CommandKind` are crate-internal only.

## `codegen/` — `.toml` → `phf` table pipeline (crate-private)
- `mod.rs`: `generate_command_table` — walks `dir` (`scanner`), `Dataset::try_from`+`validate`s, renders each `Command` (`command`) into `&[PlatformEntry]` groups keyed by every name/alias, builds a `phf_codegen::Map` (`quote` feature enabled, so its `build()` output implements `quote::ToTokens` and is spliced in directly), wraps it in a `static COMMANDS: ::phf::Map<...> = ...;` item. Also owns `PlatformPair`/`PlatformPairs`, the grouping-pass accumulator, and the integration tests (`syn` dev-dependency parses the output as a `syn::ItemStatic` — catches a malformed-tokens codegen bug substring-`.contains()` checks couldn't).
- `scanner.rs`: `Scanner` (private) — recursive `.toml` discovery (`collect_toml_files`/`walk`), sorted by path for deterministic merge order; `impl TryFrom<Scanner> for Dataset`.
- `render.rs`: the `Renderer` trait every schema type implements (`fn render(self) -> TokenStream`) and `crate_path(name)` — `name` → `::inceptool_risk_data::<name>`, so generated output never depends on a `use` at the `include!` site. Holds the simple variant-path impls: `ProfilePatch`, `Effect`, `FlagGrammar`, `Platform`.
- `rule.rs`: `Renderer` impls for `ValueRule`/`Flag`/`ComboRule`/`OperandRule`; `render_ruleset` — a `Command`'s or `Subcommand`'s baseline/flags/combo/operand rules → `RuleSetEntry` tokens, shared by both scopes.
- `command.rs`: `Renderer` impls for `Subcommand` and `Command`.
- Every render impl builds a `proc_macro2::TokenStream` via `quote::quote!`, never `format!()`-built text (one exception: `phf_codegen::Map::entry`'s value param is text, so a rendered `TokenStream` is `.to_string()`'d once at that boundary).

## `entry.rs` — `'static` codegen-target shapes (pub)
- `PlatformEntry` — one `Platform`-tagged `CommandEntry`; the generated `phf::Map<&str, &[PlatformEntry]>`'s value type.
- `CommandEntry` — `grammar`/`case_sensitive` plus the global-scope `RuleSetEntry` (`rules`) and named `subcommands: &[(&str, RuleSetEntry)]`.
- `RuleSetEntry` — one scope's (global or one subcommand's) baseline/flags/combo-rules/operand-rules; what `Command`'s fields used to be, factored out so a subcommand can have its own independent one.
- `FlagEntry`, `ComboRuleEntry`, `OperandRuleEntry`, `ValueRuleEntry` — unchanged. No lifetime; embedded directly as source.

## `error.rs`
- `RiskDataError`: `MissingRoot` (scanned root missing or not a directory), `Io`, `NoTomlFiles` (root has no `.toml` file recursively), `Toml`, `DuplicateCommand` (carries only `name`; the `(name, Platform)` dedup scoping lives in `Dataset::validate`'s own `BTreeSet<(&str, Platform)>`, not in the error variant), `DuplicateFlagSpelling`, `UnknownComboFlag`, `InvalidPattern` (all three carry `subcommand: Option<Box<str>>`), `DuplicateSubcommand`, `CombinableUnderGoGrammar`. Non-UTF-8 file/directory names are supported (lossily converted), not an error.

## `types/` — TOML schema (crate-private; owned `Box<str>`, parsed once per build)
- `mod.rs`: `Dataset` — `parse` (merge files), `validate` (cross-reference checks schema alone can't enforce; duplicate-command check scoped to `(name_or_alias, platform)`).
- `command.rs`: `Command` (name/aliases/kind/platform/grammar/case_sensitive/baseline/flag/combo_rule/operand_rule/subcommand); `Subcommand` (own independent name/aliases/baseline/flag/combo_rule/operand_rule — not `#[serde(flatten)]`, which conflicts with `deny_unknown_fields`); `CommandKind` (`Builtin`/`External` — provenance only, never read by classification). `validate_ruleset` (private) backs both `Command`'s global scope and each `Subcommand`'s own scope; a subcommand's combo rule may require a flag from either its own scope or the parent's global one.
- `platform.rs`: `Platform` (`GnuLinux`(default)/`Bsd`/`MacOs`/`Busybox`) — which concrete implementation a `Command` declaration models; the same name may be declared more than once, each under a different `Platform`.
- `grammar.rs`: `FlagGrammar` (`Gnu`(default)/`Bsd`/`Go`) — which flag-syntax family a command's tokens follow; tokenization itself lives in `inceptool_parable::risk`, not here.
- `flag.rs`: `Flag`; `TakesValue` (`Combined`=`--name=value` / `Separate`=`-s value`); `ValueRule`.
- `combo_rule.rs`: `ComboRule` — applies only when every `requires` flag spelling is present.
- `operand_rule.rs`: `OperandRule` — matched against literal positional args.
- `rating.rs`: `ProfilePatch` (patch to the 9 axes); `TrustImpact`/`Reversibility`/`BlastRadius`/`Disclosure`/`Persistence`/`Privilege`/`Auditability`/`Exposure`/`Verification` — every axis is 3-level (low/mid/high, e.g. `Disclosure::None`/`DisclosesData`/`DisclosesCredentials`), each with `escalate`=max-fold / `mitigate`=min-fold; `Effect` (`Escalate`/`Mitigate`).
- Every struct is `#[serde(deny_unknown_fields)]` — a typo'd/removed field fails to parse instead of being silently dropped.

## Data & consumer
- Real `.toml` files: `crates/parable/risk-data/*.toml`.
- One real caller: `inceptool-parable`'s `build.rs`, via `OUT_DIR/risk_data.rs` `include!`.
