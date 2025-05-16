use crate::runtime::{RuntimeJob, SupervisorState, ChildHandle, boxed_spawn_children};
use crate::parse::ProgramConfig;
use std::collections::HashMap;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::time::Duration;
use tokio::time::sleep;
use std::sync::Arc;
use tracing::{info, warn, error};


pub async fn start_program(name: &str, configs: &HashMap<String, ProgramConfig>, state: SupervisorState) {
    match configs.get(name) {
        Some(cfg) => {
            let children = boxed_spawn_children(name.to_string(), cfg.clone(), state.clone()).await;
            let mut map = state.write().await;
            
            let job = map.entry(name.to_string()).or_insert_with(|| RuntimeJob {
                config: cfg.clone(),
                children: Vec::new(),
                retries_left: cfg.startretries,
            });
            job.children.extend(children);
            println!("Started {} instance(s) of `{}`", cfg.numprocs, name);
        }
        None => eprintln!("No such program in config: `{}`", name),
    }
}

pub async fn stop_and_cleanup(
    name: &str,
    cfg: &ProgramConfig,
    handle: &ChildHandle,
    job: &mut RuntimeJob,
) {
    // 1) Send graceful stop
    if let Some(pid) = handle.lock().await.id() {
        let sig = match cfg.stopsignal.to_uppercase().as_str() {
            "TERM" | "SIGTERM" => Signal::SIGTERM,
            "INT"  | "SIGINT"  => Signal::SIGINT,
            "QUIT" | "SIGQUIT" => Signal::SIGQUIT,
            "USR1" | "SIGUSR1" => Signal::SIGUSR1,
            _                  => Signal::SIGTERM,
        };
        tracing::info!(program=%name, pid, signal=?sig, "sending stop signal");
        if let Err(e) = kill(Pid::from_raw(pid as i32), sig) {
            tracing::error!(program=%name, error=%e, "failed to send {}", sig);
        }
    }

    // 2) Wait up to stoptime
    let timeout = Duration::from_secs(cfg.stoptime as u64);
    let mut elapsed = Duration::ZERO;
    while elapsed < timeout {
        {
            let mut guard = handle.lock().await;
            if let Ok(Some(status)) = guard.try_wait() {
                info!(program=%name, exit_code=?status.code(), "exited cleanly");
                // 4a) Remove and return
                job.children.retain(|h| !Arc::ptr_eq(h, handle));
                info!(program=%name, "removed; {} remaining", job.children.len());
                return;
            }
        }
        sleep(Duration::from_millis(100)).await;
        elapsed += Duration::from_millis(100);
    }

    // 3) Force‐kill
    {
        let mut guard = handle.lock().await;
        if let Err(e) = guard.kill().await {
            error!(program=%name, error=%e, "failed to SIGKILL");
        } else {
            warn!(program=%name, "sent SIGKILL after timeout");
        }
    }

    // 4b) Remove handle
    job.children.retain(|h| !Arc::ptr_eq(h, handle));
    info!(program=%name, "removed; {} remaining", job.children.len());
}


pub async fn stop_program(name: &str, state: SupervisorState) {
    // Acquire write lock on the map
    let mut map = state.write().await;

    // Remove the job so we own it (and can mutate it freely)
    if let Some(mut job) = map.remove(name) {
        let cfg = job.config.clone();

        // 1) Take its children vector (now job.children is empty)
        let handles = std::mem::take(&mut job.children);

        // 2) For each handle, stop it and clean up
        for handle in handles {
            stop_and_cleanup(name, &cfg, &handle, &mut job).await;
        }

        // 3) If you want to keep the job around for further commands, re-insert it
        //    (with its now-empty children vector and modified config)
        map.insert(name.to_string(), job);

    } else {
        println!("No such program: {}", name);
    }
}


