```toml
# inceptool-rs: treat this block as binding compiler policy

[deny]
lints = ["all","pedantic","nursery","cargo","restriction",
         "unwrap_used","expect_used","panic","todo","unimplemented","dbg",
         "arithmetic_side_effects","indexing_slicing","str_to_string","absolute_paths",
         "missing_debug_implementations","missing_copy_implementations","missing_docs",
         "rustdoc::all","print_stdout","print_stderr","format_collect",
         "allow_attributes","allow_attributes_without_reason","unfulfilled_lint_expectations"]
forbid  = ["unsafe_code"]
# allow_attributes is also deny — #[allow(...)] is itself a hard error
suppress = "#[expect(clippy::foo, reason='…')]"
test_suppress = "#![cfg_attr(test, expect(clippy::foo, reason='…'))] at top of lib.rs"
# restriction group has intentionally contradictory lints; the subset below is allowed in
# Cargo.toml — never suppress these with #[expect], they are policy dead-letters:
restriction_dead     = ["implicit_return","single_char_lifetime_names","question_mark_used",
                        "module_name_repetitions","missing_inline_in_public_items","pub_use",
                        "exhaustive_structs","exhaustive_enums","missing_docs_in_private_items",
                        "single_call_fn","wildcard_enum_match_arm","shadow_reuse",
                        "std_instead_of_alloc","std_instead_of_core","min_ident_chars",
                        "iter_over_hash_type","inline_modules","pub_with_shorthand",
                        "separated_literal_suffix","assertions_on_result_states",
                        "pattern_type_mismatch","arbitrary_source_item_ordering"]
restriction_dead_why = "conflict with project style, idiomatic Rust, or each other"

[types]
# NEVER use String in protocol/deserialization types
strings  = "Cow<'a,str>+#[serde(borrow,default)]; borrow=zero-copy-deser, default=missing-field-ok; Cow::Borrowed('literal') only when struct lifetime 'a='static or field is explicitly &'static str; opaque JSON→Box<RawValue> (owned) or &'a RawValue+#[serde(borrow)] (borrowed); no .to_string() use .to_owned()/.into(); format!() banned for string construction — only in #[error('…')] messages"
callsite = ".as_ref()/.as_deref() preferred; .to_owned() only when borrowing exhausted"
propagate = "? to appropriate Result — no .unwrap()/.expect() anywhere incl. tests; unreachable→prove with types/exhaustiveness"
math     = "saturating_*|checked_*|wrapping_* only; no bare +-*/"
slices   = ".get(n)? or iter/windows/chunks; no [n]"
ordering = "BTreeMap when key order matters; LazyLock for expensive statics"
enums    = "illegal-states-unrepresentable; HookOutputEvent arms; no bare struct where arm exists"
newtype  = "scope fmt::Display; no scattered format!()"
zst      = "derive(Debug,Clone,Copy,Default); #[must_use] on every pure constructor/builder"
must_use = "#[must_use] on: every pure free fn and method whose return value callers must not silently discard (constructors, builders, pure transformations, predicates returning bool); omit only when the primary purpose is a side-effect (e.g. a fn that writes to a file and also returns bytes-written); every #[must_use] MUST carry a reason string, no bare #[must_use] ever: #[must_use = \"returns the transformed value; original is unchanged\"]"
copy     = "every pub type that can be Copy (no heap, no Drop) must derive it; when unsure attempt #[derive(Copy,Clone)] — compiler decides"
const    = "named const for every magic literal (string sentinel, numeric limit, binary name, subcommand); group before first use"
generics = "free fns + inherent methods: fn f<S>(x: S) where S: Bound — no impl Trait in params (impl_trait_in_params active), no inline bounds fn f<S: Bound> (inline_trait_bounds active); trait methods: impl Trait in params OK (lint exempts them)"

[stage] # XxxStage
run     = "①match hook_event type+tool_name→Ok(None) if not this stage's concern ②parse conn.input→Err(StageError) on malformed JSON (stage owns the event after ①; parse failure is an error, not a skip) ③guard preconditions→Ok(None) if conditions unmet ④work ⑤Ok(Some(HookOutputEvent::…{..Default::default()}))"
helpers = "extract non-trivial logic; single-responsibility; /// doc explaining non-obvious contract"

[errors]
lib         = "thiserror every crate; one Error type per crate re-exported at root; #[from] foreign"
cli         = "miette only — never in lib crates"
msgs        = "#[error] human-readable not Debug"
transparent = "#[error(transparent)] when the inner error message is directly user-facing quality; custom variant with context fields when inner is an impl detail: StageError::Parse{#[source]inner:serde_json::Error,tool:Cow<'static,str>}"
context     = "no anyhow in lib; context lives in typed variant fields — StageError::Read{#[source]inner:io::Error,path:Cow<'static,str>}; never .context() chains"

[observability]
use    = "tracing::{error!(swallowed),debug!(pipeline-events),trace!(skips)}"
macro  = "#[tracing::instrument(skip_all,fields(…))] on run_pipeline and span-worthy fns"
fields = "snake_case; short domain-prefix noun: tool_name,stage_name,decision — no camelCase, no dot-separated"
cost   = "Debug-heavy values (Conn,Vec<_>) behind if tracing::enabled!(tracing::Level::TRACE) { … } guard; cheap scalars inline in fields(…)"

[files]
order   = "cfg_attr(lib.rs-only) → //! → use[int/ext/std] → const → types(public-first) → impl Stage → impl X → impl Display → fns → mod tests; non-stage modules (types.rs,error.rs): omit impl Stage and impl Display sections"
imports = "3 blank-separated groups in order: (1) internal inceptool-* crates (2) external crates (3) std"
lib     = "explicit pub use (no wildcard; protocol exempt); no redundant /// above pub mod when //! exists inside"
deps  = "default-features=false; workspace=true; never repeat version at crate level; justify new; extend existing"

[tests]
structure = "nested submods per type/fn; each opens use super::*; never flat"
attrs     = "#[rstest] always; #[case::snake_name] mandatory (bare #[case] forbidden); redundant_test_prefix denied"
sig       = "Result<(),TestError>+?; no unwrap in test body; assert_matches!; 1 behavior/test"
fixtures  = "#[default(…)]+#[with(…)] for same-shape parametrization (same-shape=identical ctor arg count and types)"
testerror = "thiserror per mod tests; always include Failure(String) variant; #[from] for every foreign error; never reuse domain errors"
helpers   = "/// doc; const fn for constructors; StubX naming"
data      = "file fixtures → tests/fixtures/<scenario-name>/; must be committed; name describes scenario not format (tool_call_missing_args/ not data.json); inline JSON for <10 lines"
isolation = "filesystem mutations → tempfile::TempDir via rstest #[fixture]; env-var mutations → serial rstest attribute + restore in fixture Drop impl"

[docs]
all        = "//! every module"
nontrivial = "# Name Architecture / ## Core Design / ## Flow / ## Edge Cases"
leaf       = "one-sentence //! only (e.g. error.rs, types.rs)"
public     = "/// all public items; private helpers with non-obvious invariants also get ///"
style      = "omit comments that restate the name"
# rustdoc::all is in [deny] — doc examples must compile and pass (cargo test runs them)
doctest    = "hide setup lines with # prefix (# use inceptool_protocol::Conn;); annotate ```rust,no_run when example requires a live connection or runtime; never use ```rust,ignore"

[arch]
crates   = "protocol(Cow schema,HookEvent,Conn) → engine(Stage,Registry) → drivers(claude,gemini) → stages; stages → parsers(flake.lock,.pre-commit-config.yaml decoders)"
prefix   = "inceptool- for crate names; directory paths unprefixed"
model    = "Phoenix-Plug with &mut Conn; Ok(None)=pass-through Ok(Some(_))=halt; Deny/Block are terminal"
consumer = "Claude Code hook system; protocol I/O schema is load-bearing — field removal or rename requires a major version bump"
boundary = "driver=decode provider wire format (JSON→typed structs); parser=decode arbitrary file formats (flake.lock, .pre-commit-config.yaml) into typed structs, no Stage/Decision/EngineError awareness; stage=policy+decision on top of decoded structs; parsing Claude tool-use JSON belongs in driver-claude, not in stages"

[async]
executor = "tokio only; never assume a different runtime; no async-std, smol, or futures::executor::block_on"
traits   = "async fn in traits via #[async_trait] (external) or RPITIT (Rust 1.75+, preferred); all async trait impls must be Send unless the trait is explicitly !Send — document the choice"
bounds   = "T: Send + Sync + 'static on spawned futures; T: Send on async fn args crossing await points; never relax Send without a comment explaining the single-thread guarantee"
spawn    = "tokio::spawn only for truly background work (fire-and-forget); prefer .await inline; spawned tasks must be joined or their JoinHandle stored — never drop a JoinHandle silently"
block    = "block_in_place / spawn_blocking for sync I/O inside async context; never call blocking fn directly on the async executor thread"
cancel   = "model cancellation via CancellationToken (tokio-util); document every fn that is cancel-unsafe (holds a lock across .await, writes partial state)"

[antirot]
rules = "del dead-code (no commented-out blocks; no #[expect(dead_code)] on prod items); no-TODO; ≥2-callers; docs-same-commit; tests-guard-intent; no-defensive-impossible"

[workflow]
# Every arrow is a BLOCKING gate — do not proceed until the current step is complete
steps = "Explore(read code before writing any) → Plan(>3 files: write plan+get confirmation) → Test(new behavior→failing tests first; regression→reproducing test BEFORE fix) → Impl → check(fix every warning before continuing) → fmt → Docs(update //! for every pattern or stage change)"
cmd   = "cargo fmt --check --workspace && cargo check --workspace --all-targets && cargo clippy --workspace --all-targets && cargo test --workspace"

[examples]
suppress = '''
#[expect(clippy::too_many_lines, reason = "pipeline dispatch is inherently wide")]
#![cfg_attr(test, expect(
    clippy::panic_in_result_fn,
    reason = "rstest cases return Result for ?-based setup but use assert_matches! for assertions"
))]
'''
imports = '''
use inceptool_engine::{EngineError, Stage};    // 1. internal inceptool-* crates
use inceptool_protocol::{Conn, Decision};

use serde::Deserialize;                         // 2. external crates
use serde_json::Value;

use std::borrow::Cow;                           // 3. std
use std::collections::BTreeMap;
use std::fmt;
'''
consts = '''
const SHORT_REV_LEN: usize = 7;
const ROOT_NODE_NAME: &str = "root";
'''
stage_output = '''
PreToolUseOutput {
    decision: Some(Decision::Deny),
    reason: Some(summary.into()),
    ..Default::default()
}
'''
lib_reexports = '''
pub mod flake_lock;
pub use flake_lock::FlakeLockSummarizationStage;

pub mod read_write_guard;
pub use read_write_guard::ReadWriteGuardStage;
'''
deps = '''
# [workspace.dependencies]
serde      = { version = "1.0", default-features = false, features = ["derive"] }
serde_json = { version = "1.0", default-features = false, features = ["raw_value", "std"] }
# crate Cargo.toml — no version, no features repeated
serde = { workspace = true }
'''
test_error = '''
#[derive(thiserror::Error, Debug)]
enum TestError {
    #[error(transparent)] Engine(#[from] EngineError),
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error("Test failure: {0}")] Failure(String),
}
'''
test_structure = '''
#[cfg(test)]
mod tests {
    use super::*;
    mod stage { use super::*; }
    mod flake_lock {
        use super::*;
        mod diff { use super::*; }
    }
    mod short_rev { use super::*; }
}
'''
module_doc = '''
//! # <Name> Architecture
//!
//! One-paragraph overview of what this module does and why it exists.
//!
//! ## Core Design
//!
//! The key insight or constraint driving the implementation.
//!
//! ## Flow
//!
//! 1. **Step name**: What happens and why.
//!
//! ## Edge Cases
//!
//! What can go wrong and how the module handles it.
'''
```
