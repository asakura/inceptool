//! The [`Stage`] trait that pipeline stages implement.

use crate::EngineError;

use inceptool_protocol::{Conn, HookKind, HookOutputEvent};

/// A stage that can be executed during the agent lifecycle.
pub trait Stage: Send + Sync {
    /// The unique name of the stage.
    fn name(&self) -> &'static str;

    /// The [`HookKind`] this stage runs for.
    ///
    /// [`crate::Registry::register`] places this stage into the pipeline bucket for
    /// this kind; [`Stage::run`] is never invoked for events of any other kind.
    fn hook(&self) -> HookKind;

    /// Tool names this stage runs for.
    ///
    /// Defaults to `&["*"]`, which matches any tool name — including events
    /// whose [`HookInputEvent::tool_name`](inceptool_protocol::HookInputEvent::tool_name)
    /// is `None`.
    fn tool_names(&self) -> &'static [&'static str] {
        &["*"]
    }

    /// Process the given connection.
    ///
    /// If the stage decides to return output (e.g. to override data, provide context,
    /// or halt the pipeline), it returns `Some(HookOutputEvent)`.
    /// Otherwise, it returns `None` to allow the next stage to run.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the stage fails to process the connection.
    fn run(&self, conn: &mut Conn<'_>) -> Result<Option<HookOutputEvent>, EngineError>;
}
