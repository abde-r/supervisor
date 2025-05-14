use tokio::time::{sleep, Duration};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
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


// 1) A small helper trait so we can inject our fake spawner
#[async_trait::async_trait]
pub trait ProcessSpawner: Send + Sync + 'static {
    async fn spawn(&self, cfg: &ProgramConfig, idx: usize) -> ChildHandle;
}

// 2) A real‐world implementation (in production you’d use CommandExt.pre_exec etc.)
pub struct RealSpawner;
#[async_trait::async_trait]
impl ProcessSpawner for RealSpawner {
    async fn spawn(&self, cfg: &ProgramConfig, idx: usize) -> ChildHandle {
        // the real code inside spawn_children…
        // but for tests we’ll never use this
        unimplemented!()
    }
}

// 3) Fake child handle that “completes” after a delay
struct FakeChild {
    delay: Duration,
    code: u32,
}

#[async_trait::async_trait]
impl ProcessSpawner for FakeSpawner {
    async fn spawn(&self, _cfg: &ProgramConfig, _idx: usize) -> ChildHandle {
        let (tx, rx) = tokio::sync::oneshot::channel::<u32>();
        let delay = self.delay;
        let code = self.code;
        // simulate child exiting after `delay`
        tokio::spawn(async move {
            sleep(delay).await;
            let _ = tx.send(code);
        });

        // wrap the oneshot receiver in a “child‐like” Future
        // here we cheat by using a small wrapper type that
        // implements the same wait/try_wait interface
        ChildHandle::fake(rx)
    }
}

// Utility to turn a oneshot receiver into a ChildHandle
impl ChildHandle {
    fn fake(rx: tokio::sync::oneshot::Receiver<u32>) -> Self {
        // Internally you can store the receiver and implement an async `wait()`
        // that awaits `rx` and returns a fake ExitStatus.
        // For brevity assume this exists in your codebase.
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Shortcut to build a ProgramConfig
    fn mk_cfg(autorestart: RestartPolicy, exitcodes: OneOrMany, starttime: u64) -> ProgramConfig {
        ProgramConfig {
            cmd: String::new(),
            args: vec![],
            numprocs: 1,
            autostart: true,
            autorestart,
            exitcodes,
            startretries: 0,
            starttime: starttime as usize,
            stoptime: 1,
            stopsignal: "TERM".into(),
            workingdir: None,
            umask: None,
            stdout: None,
            stderr: None,
            env: None,
        }
    }

    #[test]
    fn test_should_restart_unexpected() {
        let expected = &[0u32][..];
        assert!(should_restart(1, RestartPolicy::Unexpected, expected));
        assert!(!should_restart(0, RestartPolicy::Unexpected, expected));
    }

    #[test]
    fn test_should_restart_always() {
        let expected = &[0u32][..];
        assert!(should_restart(123, RestartPolicy::Always, expected));
    }

    #[test]
    fn test_should_restart_never() {
        let expected = &[0u32][..];
        assert!(!should_restart(0, RestartPolicy::Never, expected));
        assert!(!should_restart(1, RestartPolicy::Never, expected));
    }

    #[tokio::test]
    async fn test_early_exit_respawn_unexpected() {
        // child exits in 0ms, before starttime=1s
        let cfg = mk_cfg(RestartPolicy::Unexpected, OneOrMany::One(0), 1);
        let state = Arc::new(RwLock::new(Default::default()));
        let spawner = Arc::new(FakeSpawner { delay: Duration::from_millis(0), code: 1 });

        // spawn and let the grace‐period logic run
        spawn_children_with_spawner("p", &cfg, state.clone(), spawner.clone()).await;
        sleep(Duration::from_secs(2)).await; // > starttime
        let map = state.read().await;
        assert_eq!(map["p"].children.len(), 1);
    }

    #[tokio::test]
    async fn test_early_exit_no_respawn_never() {
        let cfg = mk_cfg(RestartPolicy::Never, OneOrMany::One(0), 1);
        let state = Arc::new(RwLock::new(Default::default()));
        let spawner = Arc::new(FakeSpawner { delay: Duration::from_millis(0), code: 42 });

        spawn_children_with_spawner("p", &cfg, state.clone(), spawner.clone()).await;
        sleep(Duration::from_secs(2)).await;
        let map = state.read().await;
        assert_eq!(map["p"].children.len(), 0);
    }

    #[tokio::test]
    async fn test_late_exit_expected_no_restart() {
        // child exits at 2s (> starttime), code=0 expected
        let cfg = mk_cfg(RestartPolicy::Unexpected, OneOrMany::One(0), 1);
        let state = Arc::new(RwLock::new(Default::default()));
        let spawner = Arc::new(FakeSpawner { delay: Duration::from_secs(2), code: 0 });

        spawn_children_with_spawner("q", &cfg, state.clone(), spawner.clone()).await;
        sleep(Duration::from_secs(3)).await;
        let map = state.read().await;
        assert_eq!(map["q"].children.len(), 0);
    }

    #[tokio::test]
    async fn test_multiple_exitcodes() {
        // child exits at 0ms (< starttime) but 2 is in exitcodes
        let cfg = mk_cfg(RestartPolicy::Unexpected, OneOrMany::Many(vec![0,2]), 1);
        let state = Arc::new(RwLock::new(Default::default()));
        let spawner = Arc::new(FakeSpawner { delay: Duration::from_millis(0), code: 2 });

        spawn_children_with_spawner("r", &cfg, state.clone(), spawner.clone()).await;
        sleep(Duration::from_secs(2)).await;
        let map = state.read().await;
        // still respawns because early‐exit always triggers Unexpected
        assert_eq!(map["r"].children.len(), 1);
    }
}
