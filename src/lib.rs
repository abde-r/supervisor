/* src/lib.rs */

// Re-export modules for both binary and tests
pub mod parse;
pub mod runtime;
pub mod control;
pub mod shell;
pub mod logger;

// Helpers exposed only during testing
#[cfg(test)]
pub mod test_helpers {
    pub use crate::runtime::{
        boxed_spawn_children,
        spawn_children_with_spawner,
        should_restart,
        ChildHandle,
        SupervisorState,
    };
    pub use crate::parse::{ProgramConfig, OneOrMany, RestartPolicy};

    /// A fake spawner that immediately returns a “dummy” child handle;
    /// you’ll have to implement this so your tests compile.
    pub struct FakeSpawner {
        pub delay: std::time::Duration,
        pub code: u32,
    }
    #[async_trait::async_trait]
    pub trait ProcessSpawner {
        async fn spawn(&self, cfg: &ProgramConfig, idx: usize) -> ChildHandle;
    }
    impl ProcessSpawner for FakeSpawner {
        async fn spawn(&self, _cfg: &ProgramConfig, _idx: usize) -> ChildHandle {
            // return a ChildHandle that immediately “exits” with `self.code`
            // ...
            unimplemented!()
        }
    }
    /// Wraps `runtime::spawn_children` but lets you inject your own spawner
    pub async fn spawn_children_with_spawner<S: ProcessSpawner + Send + Sync + 'static>(
        name: &str,
        cfg: &ProgramConfig,
        state: SupervisorState,
        spawner: std::sync::Arc<S>,
    ) -> Vec<ChildHandle> {
        crate::runtime::spawn_children_with_spawner(name, cfg, state, spawner).await
    }
}

/* tests/spawner_tests.rs */

use supervisor::test_helpers::{
    should_restart,
    spawn_children_with_spawner,
    FakeSpawner,
    ProgramConfig,
    OneOrMany,
    RestartPolicy,
    ChildHandle,
    SupervisorState,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Duration;

#[test]
fn test_should_restart_unexpected() {
    let expected = &[0];
    assert!(should_restart(1, RestartPolicy::Unexpected, expected));
    assert!(!should_restart(0, RestartPolicy::Unexpected, expected));
}

#[test]
fn test_should_restart_always() {
    assert!(should_restart(123, RestartPolicy::Always, &[]));
}

#[test]
fn test_should_restart_never() {
    assert!(!should_restart(0, RestartPolicy::Never, &[]));
    assert!(!should_restart(1, RestartPolicy::Never, &[]));
}

#[tokio::test]
async fn test_early_exit_respawn() {
    // configure a program that exits quickly with code 1
    let cfg = ProgramConfig {
        // fill mandatory fields, e.g. cmd, numprocs, etc.
        // ...
        numprocs: 1,
        startretries: 3,
        starttime: 2,
        autorestart: RestartPolicy::Unexpected,
        exitcodes: OneOrMany::One(0),
        cmd: String::from("/bin/true"),
        args: Vec::new(),
        umask: None,
        workingdir: None,
        autostart: true,
        stoptime: 1,
        stopsignal: String::from("TERM"),
        stdout: None,
        stderr: None,
        env: None,
    };
    let state: SupervisorState = Arc::new(RwLock::new(Default::default()));
    let spawner = Arc::new(FakeSpawner { delay: Duration::from_millis(0), code: 1 });

    let handles = spawn_children_with_spawner("p", &cfg, state.clone(), spawner.clone()).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let map = state.read().await;
    // after one early failure, a new child should have been spawned
    assert_eq!(map["p"].children.len(), 1);
}
