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
use nix::sys::signal::killpg;




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
    // 1) Acquire lock just to read PID, then drop immediately:
    let pid_opt = {
        let child = handle.lock().await;
        child.id().map(|u| u as i32)
    };
    println!("i'm here"); // No longer blocks

    if let Some(pid) = pid_opt {
        let sig = match cfg.stopsignal.to_uppercase().as_str() {
            "TERM" | "SIGTERM" => Signal::SIGTERM,
            "INT" | "SIGINT"   => Signal::SIGINT,
            "QUIT" | "SIGQUIT" => Signal::SIGQUIT,
            "USR1" | "SIGUSR1" => Signal::SIGUSR1,
            _                  => Signal::SIGTERM,
        };
        warn!(program=%name, pid, signal=?sig, "sending stop signal");
        // Send SIG to the whole process group, not just one PID:
        let pgid = Pid::from_raw(-pid);
        if let Err(e) = killpg(pgid, sig) {
            error!(program=%name, error=?e, "failed to send {}", sig);
        }
    }

    // 2) Now wait up to stoptime for the child to exit, but lock only briefly:
    let timeout = Duration::from_secs(cfg.stoptime as u64);
    let mut elapsed = Duration::ZERO;
    let check_interval = Duration::from_millis(100);

    while elapsed < timeout {
        let has_exited = {
            let mut guard = handle.lock().await;
            guard.try_wait().unwrap_or(Some(std::process::ExitStatus::from_raw(-1))).is_some()
        }; // guard dropped here

        if has_exited {
            // child is gone; remove from job.children and return
            job.children.retain(|h| !Arc::ptr_eq(h, handle));
            return;
        }

        sleep(check_interval).await;
        elapsed += check_interval;
    }

    // 3) If it still hasn’t exited, force‐kill the pgid:
    if let Some(pid) = pid_opt {
        let pgid = Pid::from_raw(-pid);
        let _ = killpg(pgid, Signal::SIGKILL);
        job.children.retain(|h| !Arc::ptr_eq(h, handle));
    }
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