//! Session and connection metadata structures.

use crate::input::HookInputEvent;
use crate::types::{Effort, PermissionMode, RawJson};

use serde::Deserialize;
use std::borrow::Cow;

/// The driver-agnostic connection record.
///
/// A `Conn` ties together information about the current session and the specific
/// `HookInputEvent` that is currently being evaluated.
#[derive(Debug)]
pub struct Conn<'a> {
    /// Metadata about the ongoing session.
    pub session: SessionMeta<'a>,
    /// The event that triggered this connection.
    pub event: HookInputEvent<'a>,
}

/// Metadata about the ongoing session.
#[derive(Debug, Deserialize)]
pub struct SessionMeta<'a> {
    /// A unique identifier for the current session.
    pub session_id: Cow<'a, str>,
    /// Optional path to the transcript of the session.
    pub transcript_path: Option<Cow<'a, str>>,
    /// The current working directory of the session.
    pub cwd: Option<Cow<'a, str>>,
    /// An optional timestamp for the current event.
    pub timestamp: Option<Cow<'a, str>>,
    /// A string identifying the driver (e.g., "Claude" or "Gemini").
    pub driver: Cow<'a, str>,
    /// Driver-specific metadata attached to the session.
    #[serde(borrow)]
    pub driver_meta: Option<RawJson<'a>>,
    /// The permission mode active for the session, if known.
    #[serde(default)]
    pub permission_mode: Option<PermissionMode>,
    /// The reasoning effort configured for the session, if known.
    #[serde(default)]
    pub effort: Option<Effort>,
    /// The identifier of the agent handling this session, if applicable.
    #[serde(default)]
    pub agent_id: Option<Cow<'a, str>>,
    /// The type of agent handling this session, if applicable.
    #[serde(default)]
    pub agent_type: Option<Cow<'a, str>>,
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
        #[error("Test failure: {0}")]
        Failure(String),
    }

    #[fixture]
    fn session_meta_json() -> String {
        r#"{
            "session_id": "123",
            "cwd": "/tmp",
            "driver": "Gemini",
            "driver_meta": {"some": "data"}
        }"#
        .to_string()
    }

    #[fixture]
    fn session_meta_with_common_fields_json() -> String {
        r#"{
            "session_id": "123",
            "driver": "Claude",
            "permission_mode": "bypassPermissions",
            "effort": {"level": "high"},
            "agent_id": "agent-1",
            "agent_type": "explore"
        }"#
        .to_string()
    }

    #[rstest]
    fn test_session_meta_deserialization_id(session_meta_json: String) -> Result<(), TestError> {
        let session: SessionMeta = serde_json::from_str(&session_meta_json)?;
        assert_eq!(session.session_id, "123");
        Ok(())
    }

    #[rstest]
    fn test_session_meta_deserialization_cwd(session_meta_json: String) -> Result<(), TestError> {
        let session: SessionMeta = serde_json::from_str(&session_meta_json)?;
        assert_eq!(session.cwd.as_deref(), Some("/tmp"));
        Ok(())
    }

    #[rstest]
    fn test_session_meta_deserialization_driver(
        session_meta_json: String,
    ) -> Result<(), TestError> {
        let session: SessionMeta = serde_json::from_str(&session_meta_json)?;
        assert_eq!(session.driver, "Gemini");
        Ok(())
    }

    #[rstest]
    fn test_session_meta_deserialization_driver_meta(
        session_meta_json: String,
    ) -> Result<(), TestError> {
        let session: SessionMeta = serde_json::from_str(&session_meta_json)?;
        assert_eq!(
            session
                .driver_meta
                .ok_or(TestError::Failure("missing driver_meta".into()))?
                .0
                .get(),
            r#"{"some": "data"}"#
        );
        Ok(())
    }

    #[rstest]
    fn test_session_meta_deserialization_missing_common_fields_default_to_none(
        session_meta_json: String,
    ) -> Result<(), TestError> {
        let session: SessionMeta = serde_json::from_str(&session_meta_json)?;
        assert!(session.permission_mode.is_none());
        assert!(session.effort.is_none());
        assert!(session.agent_id.is_none());
        assert!(session.agent_type.is_none());
        Ok(())
    }

    #[rstest]
    fn test_session_meta_deserialization_permission_mode(
        session_meta_with_common_fields_json: String,
    ) -> Result<(), TestError> {
        let session: SessionMeta = serde_json::from_str(&session_meta_with_common_fields_json)?;
        assert_eq!(
            session.permission_mode,
            Some(crate::types::PermissionMode::BypassPermissions)
        );
        Ok(())
    }

    #[rstest]
    fn test_session_meta_deserialization_effort(
        session_meta_with_common_fields_json: String,
    ) -> Result<(), TestError> {
        let session: SessionMeta = serde_json::from_str(&session_meta_with_common_fields_json)?;
        assert_eq!(
            session.effort.map(|e| e.level),
            Some(crate::types::EffortLevel::High)
        );
        Ok(())
    }

    #[rstest]
    fn test_session_meta_deserialization_agent_id(
        session_meta_with_common_fields_json: String,
    ) -> Result<(), TestError> {
        let session: SessionMeta = serde_json::from_str(&session_meta_with_common_fields_json)?;
        assert_eq!(session.agent_id.as_deref(), Some("agent-1"));
        Ok(())
    }

    #[rstest]
    fn test_session_meta_deserialization_agent_type(
        session_meta_with_common_fields_json: String,
    ) -> Result<(), TestError> {
        let session: SessionMeta = serde_json::from_str(&session_meta_with_common_fields_json)?;
        assert_eq!(session.agent_type.as_deref(), Some("explore"));
        Ok(())
    }
}
