//! TOML-facing mirror of [`Access`] — see the parent module's doc for why
//! this exists instead of deriving `Deserialize` on [`Access`] itself.

use inceptool_stages::read_write_guard::Access;

use serde::{Deserialize, Serialize};

use std::borrow::Cow;

/// TOML-facing mirror of [`Access`] — see the module doc for why this
/// exists instead of deriving `Deserialize` on [`Access`] itself.
///
/// Variant names match [`Access`]'s exactly (see that type's doc for why
/// they spell out what's denied rather than what's allowed), so e.g. an
/// `inceptool.toml` rule writes `access.deny_write` and unambiguously means
/// "writes are blocked here, reads pass through" — not "write access is
/// granted."
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[expect(
    clippy::enum_variant_names,
    reason = "the shared `Deny` prefix is intentional, not accidental \
              repetition: every variant must spell out what's denied (see \
              `Access`'s doc) — clippy only skips this check for `pub` \
              enums, and `RawAccess` is module-private by design"
)]
pub(super) enum RawAccess {
    /// See [`Access::DenyAll`].
    DenyAll {
        /// The terminal command suggested when a write is denied.
        hint: Cow<'static, str>,
        /// Additional caveats about the scope of `hint`.
        note: Cow<'static, str>,
    },
    /// See [`Access::DenyAllWithDiff`].
    DenyAllWithDiff {
        /// The terminal command suggested when a write is denied.
        hint: Cow<'static, str>,
        /// Additional caveats about the scope of `hint`.
        note: Cow<'static, str>,
    },
    /// See [`Access::DenyWrite`].
    DenyWrite {
        /// The terminal command suggested when a write is denied.
        hint: Cow<'static, str>,
        /// Additional caveats about the scope of `hint`.
        note: Cow<'static, str>,
    },
    /// See [`Access::DenyRead`].
    DenyRead,
}

impl From<RawAccess> for Access {
    fn from(raw: RawAccess) -> Self {
        match raw {
            RawAccess::DenyAll { hint, note } => Self::DenyAll { hint, note },
            RawAccess::DenyAllWithDiff { hint, note } => Self::DenyAllWithDiff { hint, note },
            RawAccess::DenyWrite { hint, note } => Self::DenyWrite { hint, note },
            RawAccess::DenyRead => Self::DenyRead,
        }
    }
}

/// Inverse of the conversion above: renders a resolved [`Access`] back into
/// its TOML-facing [`RawAccess`] shape, for `inceptool config`'s
/// `Config`-to-TOML round trip.
impl From<Access> for RawAccess {
    fn from(access: Access) -> Self {
        match access {
            Access::DenyAll { hint, note } => Self::DenyAll { hint, note },
            Access::DenyAllWithDiff { hint, note } => Self::DenyAllWithDiff { hint, note },
            Access::DenyWrite { hint, note } => Self::DenyWrite { hint, note },
            Access::DenyRead => Self::DenyRead,
        }
    }
}
