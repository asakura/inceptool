//! Access policy for a guarded file — see [`Access`].

use std::borrow::Cow;

/// Access policy for a guarded file, driving how [`super::ReadWriteGuardStage`]
/// responds to reads and writes of it.
///
/// Deliberately not `serde::Deserialize`: this is the in-memory policy
/// representation [`super::ReadWriteGuardStage`] runs against, not the TOML
/// schema. The `inceptool.toml` shape (and its own `deny_unknown_fields`, to
/// catch user typos) lives in the consuming binary's private
/// `config::raw_config::RawAccess`, which converts into this type via
/// `From`. Keeping them separate means changes to this enum (made for
/// stage-policy reasons) can't silently change — or break — the config file
/// schema, and vice versa.
///
/// Variant names spell out exactly what's denied — never what's allowed —
/// so e.g. `DenyWrite` can't be misread as "write access granted" (it
/// means the opposite: writes are blocked, reads pass through).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    /// Deny both reads and writes. Reads get a flat, no-frills reason
    /// pointing at `git diff`; writes get `hint` and `note`. The policy used
    /// by all built-in rules.
    DenyAll {
        /// The terminal command suggested when a write is denied.
        hint: Cow<'static, str>,
        /// Additional caveats about the scope of `hint` (e.g. "updates ALL
        /// packages").
        note: Cow<'static, str>,
    },
    /// Like [`Access::DenyAll`], but reads are intended to get a richer
    /// diff/summary instead of the flat reason, once diff-content generation
    /// exists. Currently degrades to [`Access::DenyAll`]'s read behavior.
    /// Reserved for a future upgrade of any rule, built-in or
    /// user-supplied.
    DenyAllWithDiff {
        /// The terminal command suggested when a write is denied.
        hint: Cow<'static, str>,
        /// Additional caveats about the scope of `hint`.
        note: Cow<'static, str>,
    },
    /// Deny writes only (with `hint` and `note`); reads pass through
    /// untouched.
    DenyWrite {
        /// The terminal command suggested when a write is denied.
        hint: Cow<'static, str>,
        /// Additional caveats about the scope of `hint`.
        note: Cow<'static, str>,
    },
    /// Deny reads only (flat reason); writes pass through untouched.
    DenyRead,
}
