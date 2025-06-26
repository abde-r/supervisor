use crate::runtime::{RuntimeJob, SupervisorState, spawn_processes};
use crate::parse::ProgramConfig;
use std::collections::HashMap;
use nix::sys::signal::{Signal};
use std::time::Duration;
use nix::sys::wait::{WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use nix::sys::signal::killpg;
use nix::sys::wait::waitpid;


/*
    @@@
    @start_program();
    . Forks & execs program's numprocs child processes and collect their PIDs by name.
    . Acquires a write-lock on the shared supervisor state and Appends the new child handles to that job’s children list.
    . Prints a confirmation of how many instances were started --or an error if the name wasn’t found.
*/
pub async fn start_program(
    name: &str,
    configs: &HashMap<String, ProgramConfig>,
    state: SupervisorState,
) {
    if let Some(cfg) = configs.get(name) {
        let pids = spawn_processes(name, cfg);
        {
            let mut map = state.write().await;
            let job = map
                .entry(name.to_string())
                .or_insert_with(|| RuntimeJob {
                    config: cfg.clone(),
                    children: Vec::new(),
                    retries_left: cfg.startretries,
                });

            job.children = pids;
            job.retries_left = cfg.startretries;
        }

        println!("Started {} instance(s) of `{}`", cfg.numprocs, name);
    } else {
        eprintln!("No such program in config: `{}`", name);
    }
}





/*
    @@@
    @stop_and_cleanup_pid();
    . Sends a configurable stop signal (e.g., SIGTERM, SIGINT) to the process group of pid.
    . Waits up to stoptime seconds, if the process exits in that window, it returns immediately.
    . Force-kills the entire group with SIGKILL if the timeout expires and the process is still alive.
*/
pub fn stop_and_cleanup_pid(pid: Pid, cfg: &ProgramConfig) {
    let sig = match cfg.stopsignal.to_uppercase().as_str() {
        "TERM" | "SIGTERM" => Signal::SIGTERM,
        "INT"  | "SIGINT"  => Signal::SIGINT,
        "QUIT" | "SIGQUIT" => Signal::SIGQUIT,
        "USR1" | "SIGUSR1" => Signal::SIGUSR1,
        _                  => Signal::SIGTERM,
    };

    let pgid = Pid::from_raw(pid.as_raw());
    tracing::info!("Sending {:?} to process group {}", sig, pgid);
    let _ = killpg(pgid, sig);

    let timeout = Duration::from_secs(cfg.stoptime as u64);
    let mut elapsed = Duration::ZERO;
    let interval = Duration::from_millis(100);

    while elapsed < timeout {
        match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => {
                std::thread::sleep(interval);
                elapsed += interval;
                continue;
            }
            Ok(status) => {
                tracing::info!("Process {} exited with status {:?}", pid, status);
                return;
            }
            Err(e) => {
                tracing::warn!("waitpid error for {}: {:?}", pid, e);
                return;
            }
        }
    }

    tracing::warn!("Timeout expired. Sending SIGKILL to process group {}", pgid);
    let _ = killpg(pgid, Signal::SIGKILL);
}






/*
    @@@
    @stop_program();
    . Stops a running program by name with looking up the program in the shared state.
    . Sends stop signals to all its child processes and clears the list of child PIDs.
    . Puts the program back into the state map to keep track of the its config.
*/
pub async fn stop_program(name: &str, state: SupervisorState) {
    let mut map = state.write().await;

    if let Some(mut job) = map.remove(name) {
        for pid in &job.children {
            stop_and_cleanup_pid(*pid, &job.config);
        }
        job.children.clear();

        map.insert(name.to_string(), job);
    } else {
        println!("No such program: {}", name);
    }
}