use crate::runtime::{spawn_children, RuntimeJob, SupervisorState, ChildHandle};
use crate::parse::ProgramConfig;
use std::collections::HashMap;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn, error};
use anyhow::anyhow;

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

async fn stop_child(
    name: &str,
    cfg: &ProgramConfig,
    handle: &ChildHandle
) {
    // 1) Extract PID
    let pid = {
        let guard = handle.lock().await;
        guard.id().unwrap_or_else(|| {
            warn!(program=%name, "No PID—nothing to stop");
            return 42;
        }) as i32
    };

    // 2) Map your stopsignal string to a nix Signal
    let sig = match cfg.stopsignal.to_uppercase().as_str() {
        "TERM" | "SIGTERM" => Signal::SIGTERM,
        "INT"  | "SIGINT"  => Signal::SIGINT,
        "QUIT" | "SIGQUIT" => Signal::SIGQUIT,
        "USR1" | "SIGUSR1" => Signal::SIGUSR1,
        _                  => Signal::SIGTERM,
    };

    // 3) Send that graceful signal
    if let Err(e) = kill(Pid::from_raw(pid), sig) {
        error!(program=%name, error=%e, "Failed to send {}", sig);
    }

    // 4) Wait up to stoptime
    let timeout = Duration::from_secs(cfg.stoptime as u64);
    let mut elapsed = Duration::ZERO;
    let interval = Duration::from_millis(100);

    loop {
        // check exit
        {
            let mut guard = handle.lock().await;
            if let Ok(Some(_)) = guard.try_wait() {
                info!(program=%name, "Exited cleanly after {}", cfg.stoptime);
                return;
            }
        }
        if elapsed >= timeout {
            break;
        }
        sleep(interval).await;
        elapsed += interval;
    }

    // 5) Force-kill if still alive
    {
        let mut guard = handle.lock().await;
        if let Err(e) = guard.kill().await {
            error!(program=%name, error=%e, "Failed to SIGKILL");
        } else {
            warn!(program=%name, "SIGKILL sent after {}s timeout", cfg.stoptime);
        }
    }
}

pub async fn stop_program(name: &str, state: SupervisorState) {
    let mut map = state.write().await;

    if let Some(mut job) = map.remove(name) {
        for handle in job.children.drain(..) {
            let child = handle.lock().await;
            let pid = child.id().unwrap_or(0);
            // let _ = child.kill();
            stop_child(&name, &job.config, &handle).await;
            //     error!(program=%name, error=%e, "Error shutting down child");
            // }
            println!("Stopped process {} of `{}`", pid, name);
        }
        println!("All instances of `{}` stopped", name);
    } else {
        println!("Program `{}` is not running", name);
    }
}

