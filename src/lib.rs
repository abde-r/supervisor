pub mod parse;
pub mod runtime;
pub mod control;
pub mod shell;
pub mod logger;

pub use parse::{Config, ProgramConfig, OneOrMany, RestartPolicy};
pub use runtime::{spawn_children, boxed_spawn_children, should_restart, ChildHandle, SupervisorState};
pub use control::{start_program, stop_program};
pub use shell::run_shell;
pub use logger::logs_tracing;


/// Test‐helper module: trait and boxed helper for spawn_children
pub mod test_helpers {
    use super::*;
    use async_trait::async_trait;
    use std::future::Future;
    use std::pin::Pin;

    /// A trait for injecting custom spawners in tests.
    #[async_trait]
    pub trait ProcessSpawner {
        /// Spawn one child instance for `name` with `cfg`, updating `state`, for index `idx`.
        async fn spawn(
            &self,
            name: &str,
            cfg: &ProgramConfig,
            state: SupervisorState,
            idx: usize
        ) -> ChildHandle;
    }

    /// Boxed wrapper around the real `spawn_children` to make it `Send + 'static`.
    pub fn boxed_spawn_children(
        name: String,
        cfg: ProgramConfig,
        state: SupervisorState,
    ) -> Pin<Box<dyn Future<Output = Vec<ChildHandle>> + Send + 'static>> {
        Box::pin(async move { runtime::spawn_children(&name, &cfg, state).await })
    }
}
