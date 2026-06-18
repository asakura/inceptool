//! TOML-facing mirror of [`Rule`] — see the parent module's doc for why
//! this exists instead of deriving `Deserialize` on [`Rule`] itself.

use super::access::RawAccess;

use inceptool_stages::read_write_guard::Rule;

use serde::{Deserialize, Serialize};

use std::borrow::Cow;

/// TOML-facing mirror of [`Rule`] — see the module doc for why this exists
/// instead of deriving `Deserialize` on [`Rule`] itself.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawRule {
    /// See [`Rule`]'s `filename` field for the three accepted pattern
    /// forms.
    pub filename: Cow<'static, str>,
    /// Which operations are intercepted, and how.
    pub access: RawAccess,
}

impl From<RawRule> for Rule {
    fn from(raw: RawRule) -> Self {
        Self::new(raw.filename, raw.access.into())
    }
}

/// Inverse of the conversion above: renders a resolved [`Rule`] back into
/// its TOML-facing [`RawRule`] shape, for `inceptool config`'s
/// `Config`-to-TOML round trip.
impl From<Rule> for RawRule {
    fn from(rule: Rule) -> Self {
        let (filename, access) = rule.into_parts();

        Self {
            filename,
            access: access.into(),
        }
    }
}
