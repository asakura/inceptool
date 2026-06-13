//! Fundamental data types used across the protocol.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

/// A borrowed JSON blob that is not parsed immediately.
///
/// This is used to hold arbitrary JSON data from the underlying protocol
/// driver, delaying parsing until a specific hook actually needs to inspect it.
/// This significantly improves performance when hooks don't need to modify
/// or read a given payload.
///
/// # Examples
///
/// ```
/// # use inceptool_protocol::error::ProtocolError;
/// # fn main() -> Result<(), ProtocolError> {
/// use inceptool_protocol::RawJson;
/// use serde_json::value::RawValue;
///
/// let raw = RawValue::from_string(r#"{"hello":"world"}"#.to_string())?;
/// let json_blob = RawJson(&raw);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Deserialize)]
pub struct RawJson<'a>(#[serde(borrow)] pub &'a RawValue);

impl Serialize for RawJson<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

/// Represents a decision made by a hook regarding a proposed action.
///
/// # Examples
///
/// ```
/// # use inceptool_protocol::error::ProtocolError;
/// # fn main() -> Result<(), ProtocolError> {
/// use inceptool_protocol::Decision;
///
/// let decision: Decision = serde_json::from_str(r#""allow""#)?;
/// assert_eq!(decision, Decision::Allow);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// The action is explicitly permitted to proceed immediately without user intervention.
    Allow,
    /// The action is rejected. The caller receives an error (e.g., "Permission Denied")
    /// and may attempt an alternative approach.
    Deny,
    /// Execution is paused and the user is prompted for manual approval.
    Ask,
    /// The action is forcefully blocked. Depending on the backend driver, this may
    /// completely terminate the session rather than allowing a retry.
    Block,
}

/// The permission mode active for the current session.
///
/// # Examples
///
/// ```
/// # use inceptool_protocol::error::ProtocolError;
/// # fn main() -> Result<(), ProtocolError> {
/// use inceptool_protocol::PermissionMode;
///
/// let mode: PermissionMode = serde_json::from_str(r#""bypassPermissions""#)?;
/// assert_eq!(mode, PermissionMode::BypassPermissions);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Standard interactive permission prompts.
    Default,
    /// Read-only planning mode; no mutating tools may run.
    Plan,
    /// Edits are automatically accepted without prompting.
    AcceptEdits,
    /// The session runs autonomously, evaluating permissions itself.
    Auto,
    /// Permission prompts are skipped without asking the user.
    DontAsk,
    /// All permission checks are bypassed entirely.
    BypassPermissions,
    /// A permission mode not recognized by this version of the protocol.
    #[serde(other)]
    Unknown,
}

/// The effort level configured for the current session.
///
/// # Examples
///
/// ```
/// # use inceptool_protocol::error::ProtocolError;
/// # fn main() -> Result<(), ProtocolError> {
/// use inceptool_protocol::EffortLevel;
///
/// let level: EffortLevel = serde_json::from_str(r#""high""#)?;
/// assert_eq!(level, EffortLevel::High);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EffortLevel {
    /// Minimal reasoning effort.
    Low,
    /// Default/balanced reasoning effort.
    Medium,
    /// Increased reasoning effort.
    High,
    /// Higher-than-high reasoning effort.
    Xhigh,
    /// Maximum reasoning effort.
    Max,
    /// An effort level not recognized by this version of the protocol.
    #[serde(other)]
    Unknown,
}

/// The effort configuration for the current session.
///
/// # Examples
///
/// ```
/// # use inceptool_protocol::error::ProtocolError;
/// # fn main() -> Result<(), ProtocolError> {
/// use inceptool_protocol::{Effort, EffortLevel};
///
/// let effort: Effort = serde_json::from_str(r#"{"level": "max"}"#)?;
/// assert_eq!(effort.level, EffortLevel::Max);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub struct Effort {
    /// The configured reasoning effort level.
    pub level: EffortLevel,
}

/// The behavior decided for a `PermissionRequest` hook.
///
/// # Examples
///
/// ```
/// # use inceptool_protocol::error::ProtocolError;
/// # fn main() -> Result<(), ProtocolError> {
/// use inceptool_protocol::PermissionBehavior;
///
/// let behavior = PermissionBehavior::Allow;
/// assert_eq!(serde_json::to_string(&behavior)?, r#""allow""#);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionBehavior {
    /// Allow the tool to execute.
    Allow,
    /// Deny the tool from executing.
    Deny,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ProtocolError;
    use rstest::{fixture, rstest};

    #[derive(thiserror::Error, Debug)]
    pub enum TestError {
        #[error(transparent)]
        Protocol(#[from] ProtocolError),
        #[error(transparent)]
        Json(#[from] serde_json::Error),
    }

    #[fixture]
    fn raw_json_str() -> String {
        r#"{"key": "value", "nested": [1, 2, 3]}"#.to_string()
    }

    #[rstest]
    #[case(r#""allow""#, Decision::Allow)]
    #[case(r#""deny""#, Decision::Deny)]
    #[case(r#""ask""#, Decision::Ask)]
    #[case(r#""block""#, Decision::Block)]
    fn test_decision_deserialization(
        #[case] input: &str,
        #[case] expected: Decision,
    ) -> Result<(), TestError> {
        let parsed: Decision = serde_json::from_str(input)?;
        assert_eq!(parsed, expected);
        Ok(())
    }

    #[rstest]
    #[case(Decision::Allow, r#""allow""#)]
    #[case(Decision::Deny, r#""deny""#)]
    #[case(Decision::Ask, r#""ask""#)]
    #[case(Decision::Block, r#""block""#)]
    fn test_decision_serialization(
        #[case] input: Decision,
        #[case] expected: &str,
    ) -> Result<(), TestError> {
        let serialized = serde_json::to_string(&input)?;
        assert_eq!(serialized, expected);
        Ok(())
    }

    #[rstest]
    #[case(r#""default""#, PermissionMode::Default)]
    #[case(r#""plan""#, PermissionMode::Plan)]
    #[case(r#""acceptEdits""#, PermissionMode::AcceptEdits)]
    #[case(r#""auto""#, PermissionMode::Auto)]
    #[case(r#""dontAsk""#, PermissionMode::DontAsk)]
    #[case(r#""bypassPermissions""#, PermissionMode::BypassPermissions)]
    #[case(r#""somethingNew""#, PermissionMode::Unknown)]
    fn test_permission_mode_deserialization(
        #[case] input: &str,
        #[case] expected: PermissionMode,
    ) -> Result<(), TestError> {
        let parsed: PermissionMode = serde_json::from_str(input)?;
        assert_eq!(parsed, expected);
        Ok(())
    }

    #[rstest]
    #[case(r#""low""#, EffortLevel::Low)]
    #[case(r#""medium""#, EffortLevel::Medium)]
    #[case(r#""high""#, EffortLevel::High)]
    #[case(r#""xhigh""#, EffortLevel::Xhigh)]
    #[case(r#""max""#, EffortLevel::Max)]
    #[case(r#""futuristic""#, EffortLevel::Unknown)]
    fn test_effort_level_deserialization(
        #[case] input: &str,
        #[case] expected: EffortLevel,
    ) -> Result<(), TestError> {
        let parsed: EffortLevel = serde_json::from_str(input)?;
        assert_eq!(parsed, expected);
        Ok(())
    }

    #[rstest]
    fn test_effort_deserialization() -> Result<(), TestError> {
        let parsed: Effort = serde_json::from_str(r#"{"level": "high"}"#)?;
        assert_eq!(parsed.level, EffortLevel::High);
        Ok(())
    }

    #[rstest]
    #[case(PermissionBehavior::Allow, r#""allow""#)]
    #[case(PermissionBehavior::Deny, r#""deny""#)]
    fn test_permission_behavior_serialization(
        #[case] input: PermissionBehavior,
        #[case] expected: &str,
    ) -> Result<(), TestError> {
        let serialized = serde_json::to_string(&input)?;
        assert_eq!(serialized, expected);
        Ok(())
    }

    #[rstest]
    fn test_raw_json_serialization(raw_json_str: String) -> Result<(), TestError> {
        #[derive(serde::Deserialize, serde::Serialize)]
        struct Payload<'a> {
            #[serde(borrow)]
            data: RawJson<'a>,
        }

        let full_json = format!(r#"{{"data": {}}}"#, raw_json_str);
        let parsed: Payload<'_> = serde_json::from_str(&full_json)?;

        let serialized = serde_json::to_string(&parsed)?;
        let val1: serde_json::Value = serde_json::from_str(&full_json)?;
        let val2: serde_json::Value = serde_json::from_str(&serialized)?;

        assert_eq!(val1, val2);

        Ok(())
    }
}
