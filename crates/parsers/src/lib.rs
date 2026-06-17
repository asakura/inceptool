#![cfg_attr(
    test,
    expect(
        clippy::panic_in_result_fn,
        reason = "rstest cases return Result for `?`-based setup but use assert_eq!/assert_matches! \
                  for assertions per project convention"
    )
)]

//! # Inceptool Parsers
//!
//! Zero-copy parsers that decode external file formats into typed Rust
//! structs, for `inceptool-stages` to build policy on top of. A parser has
//! no awareness of `Stage`, `Decision`, or any other engine/protocol
//! concept — it only knows how to turn raw file content into structured
//! data.

pub mod flake_lock;
pub use flake_lock::{DiffEntry, FlakeLock};

pub mod pre_commit;
pub use pre_commit::{Hook, PreCommitConfig, Repo};
