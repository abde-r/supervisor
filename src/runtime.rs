use crate::parse::{Config, ProgramConfig, OneOrMany, RestartPolicy};
use tokio::sync::{RwLock};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};
use std::fs::File;
use tokio::time::{sleep, Duration};
use nix::libc;
use std::os::unix::io::AsRawFd;
use libc::{STDIN_FILENO, STDOUT_FILENO, STDERR_FILENO};
use std::fs::OpenOptions;
use nix::unistd::{fork, ForkResult, execvp, setsid, dup2, Pid};
use nix::sys::stat::{umask, Mode};
use std::ffi::CString;
use std::path::Path;
use nix::sys::wait::WaitStatus;
use nix::sys::wait::WaitPidFlag;
use nix::sys::wait::waitpid;


// Shared map of Runtime data
// Updated each time the config data changes
pub type SupervisorState = Arc<RwLock<HashMap<String, RuntimeJob>>>;

// Struct for the Parsed Config content with spawned children process
pub struct RuntimeJob {
    pub config: ProgramConfig,
    pub children: Vec<Pid>,
    pub retries_left: usize,
}






/*
    @@@
    @apply_config();
    . Updates existing state
    . Stops old processes, starts new ones, and waits the grace period before marking as healthy.
    . Spawns extra processes if numprocs increased and marks them healthy after the grace period.
    . Starts new programs if autostart is true and marks them healthy --after grace, if set.
    . Logs health status based on whether processes survived the grace period.
*/
pub async fn apply_config(
    cfg: &Config,
    state: SupervisorState,
) {
    let mut map = state.write().await;

    for (name, prog_cfg) in &cfg.programs {
        match map.get_mut(name) {
            Some(job) => {
                // … config‐changed branch …
                if job.config != *prog_cfg {
                    // stop old children…
                    let new_pids = spawn_processes(name, prog_cfg);

                    if prog_cfg.starttime > 0 {
                        let prog = name.clone();
                        let pids = new_pids.clone();
                        let grace = prog_cfg.starttime;
                        tokio::spawn(async move {
                            sleep(Duration::from_secs(grace)).await;

                            let still_alive = pids.iter().any(|pid| {
                                matches!(
                                    waitpid(*pid, Some(WaitPidFlag::WNOHANG)),
                                    Ok(WaitStatus::StillAlive)
                                )
                            });

                            if still_alive {
                                tracing::info!(
                                    program = %prog,
                                    starttime = grace,
                                    "Marked healthy after grace period"
                                );
                            } else {
                                tracing::warn!(
                                    program = %prog,
                                    starttime = grace,
                                    "Exited before grace period"
                                );
                            }
                        });
                    } else {
                        tracing::info!(program = name, starttime = 0, "Marked healthy immediately");
                    }

                    job.children = new_pids;
                    job.config = prog_cfg.clone();
                    job.retries_left = prog_cfg.startretries;
                }

                // … scale‐up branch …
                else if prog_cfg.numprocs > job.children.len() {
                    let mut extra = spawn_processes(name, prog_cfg);

                    if prog_cfg.starttime > 0 {
                        let prog = name.clone();
                        let pids = extra.clone();
                        let grace = prog_cfg.starttime;
                        tokio::spawn(async move {
                            sleep(Duration::from_secs(grace)).await;
                            let still_alive = pids.iter().any(|pid| {
                                matches!(
                                    waitpid(*pid, Some(WaitPidFlag::WNOHANG)),
                                    Ok(WaitStatus::StillAlive)
                                )
                            });
                            if still_alive {
                                tracing::info!(
                                    program = %prog,
                                    starttime = grace,
                                    "Marked healthy after grace period"
                                );
                            } else {
                                tracing::warn!(
                                    program = %prog,
                                    starttime = grace,
                                    "Exited before grace period"
                                );
                            }
                        });
                    } else {
                        tracing::info!(program = name, starttime = 0, "Marked healthy immediately");
                    }

                    job.children.append(&mut extra);
                }
            }

            None => {
                // … autostart branch …
                if prog_cfg.autostart {
                    let pids = spawn_processes(name, prog_cfg);

                    if prog_cfg.starttime > 0 {
                        let prog = name.clone();
                        let pids = pids.clone();
                        let grace = prog_cfg.starttime;
                        tokio::spawn(async move {
                            sleep(Duration::from_secs(grace)).await;
                            let still_alive = pids.iter().any(|pid| {
                                matches!(
                                    waitpid(*pid, Some(WaitPidFlag::WNOHANG)),
                                    Ok(WaitStatus::StillAlive)
                                )
                            });
                            if still_alive {
                                tracing::info!(
                                    program = %prog,
                                    starttime = grace,
                                    "Marked healthy after grace period"
                                );
                            } else {
                                tracing::warn!(
                                    program = %prog,
                                    starttime = grace,
                                    "Exited before grace period"
                                );
                            }
                        });
                    } else {
                        tracing::info!(program = name, starttime = 0, "Marked healthy immediately");
                    }

                    map.insert(
                        name.clone(),
                        RuntimeJob {
                            config: prog_cfg.clone(),
                            children: pids,
                            retries_left: prog_cfg.startretries,
                        },
                    );
                }
            }
        }
    }
}






/*
    @@@
    @spawn_processes();
    . Forks as many as 'numprocs' processes and detaches into a new session (setsid()).
    . Changes working directory, umask, and environment if specified, and redirect stdout/stderr to log files if configured.
    . Executes the command using execvp().
*/
pub fn spawn_processes(name: &str, cfg: &ProgramConfig) -> Vec<Pid> {
    let mut pids = Vec::with_capacity(cfg.numprocs);

    for _ in 0..cfg.numprocs {
        match unsafe { fork() } {
            Ok(ForkResult::Parent { child, .. }) => {
                info!(program = name, pid = child.as_raw(), "Spawned new instance");
                pids.push(child);
            }
            Ok(ForkResult::Child) => {
                setsid().expect("setsid failed");

                if let Some(dir) = &cfg.workingdir {
                    std::env::set_current_dir(Path::new(dir))
                        .expect("chdir failed");
                }

                if let Some(mask_str) = &cfg.umask {
                    let mask_val = u32::from_str_radix(mask_str, 8)
                        .expect("invalid umask");
                    let mode = Mode::from_bits_truncate(mask_val.try_into().unwrap());
                    umask(mode);
                }

                if let Some(envs) = &cfg.env {
                    for (k, v) in envs {
                        std::env::set_var(k, v);
                    }
                }

                if let Some(parent) = Path::new(&cfg.stdout.clone().unwrap_or_default()).parent()
                {
                    std::fs::create_dir_all(parent).ok();
                }

                let devnull = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open("/dev/null")
                    .expect("failed to open /dev/null");
                let null_fd = devnull.as_raw_fd();
                dup2(null_fd, STDIN_FILENO).ok();

                let stdout_file: File = if let Some(ref path) = cfg.stdout {
                    if let Some(dir) = Path::new(path).parent() {
                        std::fs::create_dir_all(dir).ok();
                    }
                    File::options()
                        .create(true)
                        .append(true)
                        .open(path)
                        .expect("failed to open stdout file")
                } else {
                    devnull.try_clone().unwrap()
                };
                dup2(stdout_file.as_raw_fd(), STDOUT_FILENO).ok();

                let stderr_file: File = if let Some(ref path) = cfg.stderr {
                    if let Some(dir) = Path::new(path).parent() {
                        std::fs::create_dir_all(dir).ok();
                    }
                    File::options()
                        .create(true)
                        .append(true)
                        .open(path)
                        .expect("failed to open stderr file")
                } else {
                    devnull.try_clone().unwrap()
                };
                dup2(stderr_file.as_raw_fd(), STDERR_FILENO).ok();

                let cmd_c = CString::new(cfg.cmd.clone()).unwrap();
                let mut args_c = Vec::with_capacity(cfg.args.len() + 1);
                args_c.push(cmd_c.clone());
                for arg in &cfg.args {
                    args_c.push(CString::new(arg.as_str()).unwrap());
                }

                let Err(e) = execvp(&cmd_c, &args_c);
                eprintln!("execvp failed: {}", e);
                std::process::exit(1);
            }
            Err(err) => {
                panic!("fork failed: {}", err);
            }
        }
    }

    pids
}






/*
    @@@
    @reap_children();
    . Monitors and handles terminated child processes non-blockingly waiting for any child process to exit.
    . Logs an event if a child exited or calls handle_child_exit(...) to update internal state and possibly restart it.
*/
pub async fn reap_children(state: SupervisorState) {
    loop {
        while let Ok(status) = waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            match status {
                WaitStatus::Exited(pid, code) => {
                    info!(pid = pid.as_raw(), exit_code = code, "Child process exited");
                    handle_child_exit(pid, code as u32, &state).await;
                }

                WaitStatus::Signaled(pid, sig, _) => {
                    warn!(pid = pid.as_raw(), signal = ?sig, "Child process killed by signal");
                    handle_child_exit(pid, 128 + (sig as i32) as u32, &state).await;
                }

                WaitStatus::StillAlive
                | WaitStatus::Stopped(_, _)
                | WaitStatus::Continued(_) => {
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}








/*
    @@@
    @handle_child_exit();
    . Updates the internal state when a child process exits.
    . Finds exited jobs and removes the PID from the list of active children.
    . Checks restart policy (Always, Never, or Unexpected) and restart if needed.
    . Logs whether the process is restarted or not.
*/
async fn handle_child_exit(pid: Pid, code_u32: u32, state: &SupervisorState) {
    let mut map = state.write().await;
    for (name, job) in map.iter_mut() {
        if let Some(idx) = job.children.iter().position(|&p| p == pid) {
            job.children.remove(idx);

            let should_restart = match job.config.autorestart {
                RestartPolicy::Always => true,
                RestartPolicy::Never  => false,
                RestartPolicy::Unexpected => {
                    match &job.config.exitcodes {
                        OneOrMany::One(expected)       => code_u32 != *expected,
                        OneOrMany::Many(expected_list) => !expected_list.contains(&code_u32),
                    }
                }
            };

            if should_restart && job.retries_left > 0 {
                job.retries_left -= 1;
                let new_pids = spawn_processes(name, &job.config);
                info!(program = name, "Restarting child; {} retries left", job.retries_left);
                job.children.extend(new_pids);
            } else {
                info!(program = name, "Not restarting (policy: {:?}, retries left: {})",
                      job.config.autorestart, job.retries_left);
            }

            break;
        }
    }
}