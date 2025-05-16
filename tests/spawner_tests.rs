use supervisor::{ProgramConfig, OneOrMany, RestartPolicy, SupervisorState, ChildHandle};
use supervisor::test_helpers::{ProcessSpawner, boxed_spawn_children};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Duration;
use supervisor::runtime;

struct FakeSpawner {
    /// How long until this fake child “exits”
    delay: Duration,
    /// Which exit code to report
    code: u32,
}

#[async_trait::async_trait]
impl ProcessSpawner for FakeSpawner {
    async fn spawn(
        &self,
        name: &str,
        cfg: &ProgramConfig,
        state: SupervisorState,
        _idx: usize
    ) -> ChildHandle {
        // simulate the process running for `self.delay`, then “exiting”
        tokio::time::sleep(self.delay).await;

        // actually create a dummy child handle
        let mut children = boxed_spawn_children(name.to_string(), cfg.clone(), state.clone()).await;
        let handle = children.remove(0);

        // **INSERT** a RuntimeJob entry if none exists yet:
        {
            let mut map = state.write().await;
            map.entry(name.to_string())
               .or_insert_with(|| runtime::RuntimeJob {
                   config: cfg.clone(),
                   children: vec![handle.clone()],
                   retries_left: cfg.startretries,
               });
        }

        handle
    }
}


#[test]
fn test_should_restart_unexpected() {
    let expected = vec![0];
    assert!(supervisor::should_restart(1, RestartPolicy::Unexpected, &expected));
    assert!(!supervisor::should_restart(0, RestartPolicy::Unexpected, &expected));
}

#[test]
fn test_should_restart_always() {
    assert!(supervisor::should_restart(123, RestartPolicy::Always, &[]));
}

#[test]
fn test_should_restart_never() {
    assert!(!supervisor::should_restart(0, RestartPolicy::Never, &[]));
    assert!(!supervisor::should_restart(1, RestartPolicy::Never, &[]));
}

#[tokio::test]
async fn early_exit_causes_respawn() {
    // Build a config with a 2s grace period
    let cfg = ProgramConfig {
        cmd:   "./scripts/exit-zero.sh".into(),
        args:  vec![],
        numprocs:    1,
        workingdir:  None,
        autostart:   true,
        autorestart: RestartPolicy::Unexpected,
        exitcodes:   OneOrMany::One(0),
        startretries: 3,
        starttime:    2,   // grace period
        stoptime:     1,
        stopsignal:  "TERM".into(),
        stdout:       None,
        stderr:       None,
        env:          None,
        umask:        None,
    };

    let state: SupervisorState = Arc::new(RwLock::new(Default::default()));
    let spawner = Arc::new(FakeSpawner { delay: Duration::from_secs(1), code: 1 });

    // Kick off an initial fake spawn (idx=0)
    spawner.spawn("p", &cfg, state.clone(), 0).await;

    // Wait long enough for the fake exit + respawn logic
    tokio::time::sleep(Duration::from_secs(4)).await;

    let map = state.read().await;
    // After early exit (before grace) we should still have exactly one live child
    assert_eq!(map["p"].children.len(), 1);
}
