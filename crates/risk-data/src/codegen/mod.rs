//! Recursively scans a directory for command-risk `.toml` files and code-generates the `phf`
//! lookup table a caller's runtime classification logic reads — see [`generate_command_table`].
//!
//! ## Core Design
//!
//! [`generate_command_table`] is the crate's one public entry point: filesystem discovery
//! (`scanner`), `crate::types`' TOML schema (parse + merge + validate), and rendering into
//! [`crate::entry::CommandEntry`]-shaped tokens (`render`/`rule`/`command`) all happen
//! behind it, so a caller's build script needs nothing more than a directory path and a place to
//! write the result. Every `Renderer` impl builds a [`proc_macro2::TokenStream`] via
//! `quote::quote!` rather than formatting text by hand, and every type it names resolves through
//! an absolute `render::crate_path` helper (e.g. `::inceptool_risk_data::CommandEntry`) — the
//! generated tokens never depend on a `use` statement at the call site, unlike a hand-formatted
//! `format!()` string would.
//!
//! ## Flow
//!
//! 1. `scanner::Scanner::new` walks the directory tree, depth-first, collecting every `.toml`
//!    file's `(path, content)` pair and sorting the flat result by path for a deterministic
//!    merge order regardless of the filesystem's own directory-listing order.
//! 2. `Dataset::try_from(scanner)` parses and merges every file's `command` list via the
//!    crate-internal `types::Dataset::parse`, which also validates the merged result —
//!    `types::Dataset::validate` catches every cross-reference the schema alone can't.
//! 3. Each `types::Command` is rendered into `CommandEntry`-shaped tokens via its `Renderer`
//!    impl, grouped by every name/alias it resolves to across all parsed files into a
//!    `BTreeMap<Box<str>, PlatformPairs>` (one `PlatformPair` per declared `Platform` variant),
//!    then each group's variants are rendered into one `&[PlatformEntry]` slice and handed to a
//!    `phf_codegen::Map` (stringified once, since its `entry` method takes text) —
//!    perfect-hash construction happens once, here, not at runtime.
//! 4. The built `phf_codegen::Map` implements `quote::ToTokens` (the crate's `quote` feature),
//!    so it's spliced directly into the final `static COMMANDS: ::phf::Map<...> = ...;` tokens
//!    rather than being formatted back into text by this crate.
//!
//! ## Edge Cases
//!
//! - A missing, or non-directory, root is reported via [`crate::RiskDataError::MissingRoot`]
//!   rather than silently treated as having no risk data to declare.
//! - A root that exists and is a directory, but contains no `.toml` file recursively, is
//!   reported via [`crate::RiskDataError::NoTomlFiles`] rather than silently producing an empty
//!   table.
//! - A non-UTF-8 file/directory name under the root is supported: it's lossily converted (`�`
//!   for invalid sequences) rather than rejected, since the name is only ever used as a sort key
//!   and a diagnostic label, never round-tripped back into a real filesystem path.

mod command;
mod platform_pairs;
mod render;
mod rule;
mod scanner;
mod table;

pub use self::table::generate_command_table;
