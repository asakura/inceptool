#![cfg_attr(
    test,
    expect(
        clippy::panic_in_result_fn,
        reason = "rstest cases return Result for `?`-based setup but use assert_eq!/assert_matches! \
                  for assertions per project convention"
    )
)]

//! The hook execution engine for `inceptool-rs`.
//!
//! This crate is the runtime core of the project: it defines the [`Stage`] trait
//! that individual stages conform to, and the [`Registry`] that runs a sequence
//! of stages against an incoming [`Conn`](inceptool_protocol::Conn), Plug-style.
//!
//! See the [`stage`] module for the trait that pipeline stages implement, and the
//! [`registry`] module for pipeline construction and execution semantics.

pub mod error;
pub mod registry;
pub mod stage;

pub use error::EngineError;
pub use registry::Registry;
pub use stage::Stage;
