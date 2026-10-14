use super::{Access, Rule};

use inceptool_protocol::{Decision, PreToolUseOutput};

use std::fmt;

/// Denial reason for a guarded read of `display_name`. A [`fmt::Display`]
/// newtype rather than an inline `format!()`, per project convention.
struct ReadDenialReason<'a>(&'a str);

/// Denial reason for a guarded write of `display_name`, using the matched
/// [`Access`] variant's `hint` and `note`. A [`fmt::Display`] newtype rather
/// than an inline `format!()`, per project convention.
struct WriteDenialReason<'a> {
    display_name: &'a str,
    hint: &'a str,
    note: &'a str,
}

impl fmt::Display for ReadDenialReason<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(display_name) = self;
        write!(
            f,
            "Reading {display_name} directly wastes context \u{2014} it's \
             machine-generated or otherwise low-signal. Run `git diff \
             {display_name}` to see what changed."
        )
    }
}

impl fmt::Display for WriteDenialReason<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            display_name,
            hint,
            note,
        } = self;
        write!(f, "Writing to {display_name} is not allowed. {hint} {note}")
    }
}

/// Dispatches a guarded-file access: returns the [`Decision::Deny`] response
/// for `rule`'s policy and the current tool kind, or `None` if `rule.access`
/// allows this operation to pass through untouched. `display_name` is what
/// [`RuleSet::get`] returned alongside the rule — the basename for
/// exact/basename-glob matches, the full path for path-glob matches.
#[expect(
    clippy::rest_pattern_accessible_field,
    reason = "a read of a write-only-denied file passes whatever its hint/note"
)]
pub(super) fn dispatch(
    display_name: &str,
    rule: &Rule,
    is_read_tool: bool,
) -> Option<PreToolUseOutput> {
    match (rule.access(), is_read_tool) {
        (Access::DenyWrite { .. }, true) | (Access::DenyRead, false) => None,
        (_, true) => Some(deny_read(display_name)),
        (
            Access::DenyAll { hint, note }
            | Access::DenyAllWithDiff { hint, note }
            | Access::DenyWrite { hint, note },
            false,
        ) => Some(deny_write(display_name, hint, note)),
    }
}

/// Builds the [`Decision::Deny`] response for a denied read of
/// `display_name`.
fn deny_read(display_name: &str) -> PreToolUseOutput {
    PreToolUseOutput {
        decision: Some(Decision::Deny),
        reason: Some(ReadDenialReason(display_name).to_string().into()),
        ..Default::default()
    }
}

/// Builds the [`Decision::Deny`] response for a denied write of
/// `display_name`, using the matched [`Access`] variant's `hint` and `note`.
fn deny_write(display_name: &str, hint: &str, note: &str) -> PreToolUseOutput {
    PreToolUseOutput {
        decision: Some(Decision::Deny),
        reason: Some(
            WriteDenialReason {
                display_name,
                hint,
                note,
            }
            .to_string()
            .into(),
        ),
        ..Default::default()
    }
}
