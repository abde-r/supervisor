use crate::runtime::{RuntimeJob, SupervisorState, ChildHandle, spawn_children};
use crate::parse::ProgramConfig;
use std::collections::HashMap;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::time::Duration;
use tokio::time::sleep;
use std::sync::Arc;
use tracing::{info, warn, error};
use std::os::unix::process::ExitStatusExt;





/*
    @@@
    @start_program();
    . Spawns asynchronously the configured number of child processes.
    . Acquires a write-lock on the shared supervisor state and Appends the new child handles to that job’s children list.
    . Prints a confirmation of how many instances were started --or an error if the name wasn’t found.
*/
pub async fn start_program(name: &str, configs: &HashMap<String, ProgramConfig>, state: SupervisorState) {
    match configs.get(name) {
        Some(cfg) => {
            let children = spawn_children(name, cfg, state.clone()).await;
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





/*
    @@@
    @stop_and_cleanup();
    . Sends the configured stop signal (like SIGTERM) to a child process.
    . Waits for the process to exit cleanly within a timeout --kills it forcefully (SIGKILL) if it doesn't exit in time.
    . Removes the process handle from the job’s list of children and logs relevant actions and errors during the process.
*/
pub async fn stop_and_cleanup(
    name: &str,
    cfg: &ProgramConfig,
    handle: &ChildHandle,
    job: &mut RuntimeJob,
) {
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

    let timeout = Duration::from_secs(3 as u64);
    let mut elapsed = Duration::ZERO;
    while elapsed < timeout {
        {
            let mut guard = handle.lock().await;
            if let Ok(Some(status)) = guard.try_wait() {
                if let Some(code) = status.code() {
                    info!(program = %name, exit_code = code, "exited cleanly");
                }
                if let Some(sig) = status.signal() {
                    info!(program = %name, signal = sig, "process exited due to signal");
                }

                job.children.retain(|h| !Arc::ptr_eq(h, handle));
                info!(program=%name, "removed; {} remaining", job.children.len());
                return;
            }
        }
        sleep(Duration::from_millis(100)).await;
        elapsed += Duration::from_millis(100);
    }

    {
        let mut guard = handle.lock().await;
        if let Err(e) = guard.kill().await {
            error!(program=%name, error=%e, "failed to SIGKILL");
        } else {
            warn!(program=%name, "sent SIGKILL after timeout");
        }
    }

    job.children.retain(|h| !Arc::ptr_eq(h, handle));
    info!(program=%name, "removed; {} remaining", job.children.len());
}





/*
    @@@
    @stop_program();
    . Stops asynchronously and cleans up each child via stop_and_cleanup.
    . Acquires a write-lock on the shared state and removes the RuntimeJob for name --if exists.
    . Reinserts the (now-updated) RuntimeJob back into the map --or an error if the name wasn’t found.
*/
pub async fn stop_program(name: &str, state: SupervisorState) {
    let mut map = state.write().await;

    if let Some(mut job) = map.remove(name) {
        let cfg = job.config.clone();

        let handles = std::mem::take(&mut job.children);
        for handle in handles {
            stop_and_cleanup(name, &cfg, &handle, &mut job).await;
        }
        map.insert(name.to_string(), job);
    } else {
        println!("No such program: {}", name);
    }
}