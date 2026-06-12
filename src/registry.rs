//! Builds the stage [`Registry`] from user configuration.

use crate::config::Config;

use inceptool_engine::Registry;
use inceptool_stages::RtkStage;
use inceptool_stages::{
};

/// Registers the standard stages enabled by `config`.
pub fn build_registry(config: &Config) -> Registry {
    let mut registry = Registry::new();

    macro_rules! register_stages {
        ($($name:expr => $stage:expr),* $(,)?) => {
            $(
                if config.is_hook_enabled($name) {
                    registry.register($stage);
                }
            )*
        };
    }

    register_stages!(
        "rtk" => RtkStage,
    );

    #[cfg(debug_assertions)]
    register_mock_stages(&mut registry);

    registry
}

/// In debug builds, registers test-only stages gated behind environment
/// variables, used by the integration test suite to exercise behavior that's
/// otherwise impractical to trigger (e.g. `WorktreeCreate`).
#[cfg(debug_assertions)]
fn register_mock_stages(registry: &mut Registry) {
    use inceptool_engine::{EngineError, Stage};
    use inceptool_protocol::{Conn, HookKind, HookOutputEvent, WorktreeCreateOutput};

    if std::env::var("INCEPTOOL_TEST_MOCK_WORKTREE").is_ok() {
        struct MockWorktreeStage;

        impl Stage for MockWorktreeStage {
            fn name(&self) -> &'static str {
                "mock-worktree"
            }

            fn hook(&self) -> HookKind {
                HookKind::WorktreeCreate
            }

            fn tool_names(&self) -> &'static [&'static str] {
                &["*"]
            }

            fn run(&self, _conn: &mut Conn) -> Result<Option<HookOutputEvent>, EngineError> {
                Ok(Some(HookOutputEvent::WorktreeCreate(
                    WorktreeCreateOutput {
                        worktree_path: Some("/mock/worktree/path".to_string()),
                    },
                )))
            }
        }

        registry.register(MockWorktreeStage);
    }
}
