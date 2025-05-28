use crate::parse::{Config, ProgramConfig, OneOrMany, RestartPolicy};
use tokio::process::{Child, Command};
use tokio::sync::{RwLock, Mutex};
use tokio::spawn;
use std::collections::HashMap;
use std::sync::Arc;
use std::process::Stdio;
use std::fs::File;
use tracing::{info, warn};
use nix::libc::{umask, mode_t};
use tokio::time::{sleep, Duration};
use tracing::error;


pub type ChildHandle = Arc<Mutex<Child>>;

// Shared map of Runtime data
// Updated each time the config data changes
pub type SupervisorState = Arc<RwLock<HashMap<String, RuntimeJob>>>;

// Struct for the Parsed Config content with spawned children process
pub struct RuntimeJob {
    pub config: ProgramConfig,
    pub children: Vec<ChildHandle>,
    pub retries_left: usize,
}





/*
    @@@
    @schedule_spawn();
    . Fires off a background task to respawn a child and merge the new handles into the shared state.
*/
fn schedule_spawn(name: String, cfg: ProgramConfig, state: SupervisorState) {
    spawn(async move {
        let mut replacements = spawn_children(&name, &cfg, state.clone()).await;
        let mut map = state.write().await;
        if let Some(job) = map.get_mut(&name) {
            job.children.extend(replacements.drain(..));
        }
    });
}


/// Helper to respawn from anywhere
fn schedule_respawn(name: String, cfg: ProgramConfig, state: SupervisorState) {
    tokio::spawn(async move {
        let mut replacements = spawn_children(&name, &cfg, state.clone()).await;
        let mut map = state.write().await;
        if let Some(job) = map.get_mut(&name) {
            job.children.append(&mut replacements);
        }
    });
}


/*
    @@@
    @monitor_child();
    . Locks and waits on one child process, checks its exit code against the configured policy.
    . The 'wait().await' is the only place where anything actually "sleeps". It suspends that spawned task until the OS process exits.
    . kicks off a local respawn that updates the shared supervisor state --if needed.
*/
async fn monitor_child(name: String, cfg: ProgramConfig, handle: Arc<Mutex<Child>>, state: SupervisorState) {
    let mut child = handle.lock().await;
    match child.wait().await {
        Ok(status) => {
            let code = status.code().unwrap_or(-1) as u32;
            let expected = match &cfg.exitcodes {
                OneOrMany::One(n) => vec![*n],
                OneOrMany::Many(vs) => vs.clone(),
            };
        
            let should_restart = match cfg.autorestart {
                RestartPolicy::Always => true,
                RestartPolicy::Never => false,
                RestartPolicy::Unexpected => !expected.contains(&code),
            };
        
            if should_restart {
                warn!(
                    program = %name,
                    exit_code = code,
                    "Process exited; restarting per policy"
                );
                schedule_spawn(name, cfg, state);
            } else {
                info!(
                    program = %name,
                    exit_code = code,
                    "Process exited; not restarting per policy"
                );
            }
        }
        Err(e) => {
            warn!(
                program = %name,
                error = %e,
                "Failed to await child"
            );
        }
    }
}





/*
    @@@
    @spawn_children();
    . Builds a command —-applying cwd, env, I/O redirection, and umask-— and wraps each child in an array of mutexes.
    . Detaches a tokio::spawn monitor task per process to await exit, update state, and restart if needed.
    . Returns all child handles without blocking.
*/
pub async fn spawn_children(name: &str, cfg: &ProgramConfig, state: SupervisorState) -> Vec<ChildHandle> {
    let mut children = Vec::with_capacity(cfg.numprocs);
    for i in 0..cfg.numprocs {
        let mut cmd = Command::new(&cfg.cmd);
        cmd.args(&cfg.args);

        if let Some(dir) = &cfg.workingdir {
            cmd.current_dir(dir);
        }

        if let Some(envs) = &cfg.env {
            for (k, v) in envs {
                cmd.env(k, v);
            }
        }

        if let Some(path) = &cfg.stdout {
            if path == "null" {
                cmd.stdout(Stdio::null());
            } else {
                let f = File::create(path).unwrap_or_else(|e| panic!("failed to open stdout file `{}`: {}", path, e));
                cmd.stdout(Stdio::from(f));
            }
        }
    
        if let Some(path) = &cfg.stderr {
            if path == "null" {
                cmd.stderr(Stdio::null());
            } else {
                let f = File::create(path).unwrap_or_else(|e| panic!("failed to open stderr file `{}`: {}", path, e));
                cmd.stderr(Stdio::from(f));
            }
        }

        if let Some(umask_str) = &cfg.umask {
            let mask = u32::from_str_radix(umask_str, 8).expect("invalid umask in config") as mode_t;
            unsafe {
                cmd.pre_exec(move || {
                    umask(mask);
                    Ok(())
                });
            }
        }

        let child = cmd.spawn().unwrap_or_else(|e| panic!("failed to spawn child `{}`: {}", cfg.cmd, e));
        info!(
            program = %cfg.cmd,
            index = i,
            pid = child.id().unwrap_or(0),
            "Process started"
        );

        let handle: ChildHandle = Arc::new(Mutex::new(child));
        let name_clone = name.to_string();
        let cfg_clone = cfg.clone();
        let state_clone = state.clone();
        let handle_clone = handle.clone();
        // spawn(async move {
        //     monitor_child(name_clone, cfg_clone, handle_clone, state_clone).await;
        // });
        tokio::spawn(async move {
            let grace = Duration::from_secs(3);
            let mut elapsed = Duration::ZERO;
            let check_interval = Duration::from_millis(100);

            println!("{:?}", grace);
            // println!("{}", elapsed);

            // Poll for early exit during the grace period
            loop {
                if elapsed >= grace {
                    break;  // survived startup
                }
                {
                    let mut child_guard = handle_clone.lock().await;
                    if let Ok(Some(status)) = child_guard.try_wait() {
                        // Exited too early!
                        error!(
                            program = %name_clone,
                            code = %status.code().unwrap_or(-1),
                            "Crashed within {}s startup window", cfg_clone.starttime
                        );
                        // Remove from state
                        let mut map = state_clone.write().await;
                        if let Some(job) = map.get_mut(&name_clone) {
                            job.children.retain(|h| !Arc::ptr_eq(h, &handle_clone));
                        }
                        // If policy == Unexpected, respawn immediately
                        if let RestartPolicy::Unexpected = cfg_clone.autorestart {
                            schedule_respawn(name_clone.clone(), cfg_clone.clone(), state_clone.clone());
                        }
                        return;
                    }
                }
                sleep(check_interval).await;
                elapsed += check_interval;
            }

            // 2) After grace period, hand off to the normal monitor
            monitor_child(name_clone, cfg_clone, handle_clone, state_clone).await;
        });
        children.push(handle);
    }
    children
}






/*
    @@@
    @apply_config();
    . Stops and removes jobs no longer in the Config.
    . Updates running jobs by scaling them up or down to match the new numprocs.
    . Adds new jobs --autostarting them if configured.
*/
pub async fn apply_config(new_cfg: &Config, state: SupervisorState) {
    info!(
        "Applying new configuration with {} programs",
        new_cfg.programs.len()
    );
    let mut map = state.write().await;

    let to_remove: Vec<String> = map.keys()
        .filter(|name| !new_cfg.programs.contains_key(*name))
        .cloned()
        .collect();
    for name in to_remove {
        if let Some(job) = map.remove(&name) {
            for handle in job.children {
                let mut child = handle.lock().await;
                let _ = child.kill().await;
                info!(
                    program = %name,
                    pid = child.id().unwrap_or(0),
                    "Process stopped (job removed)"
                );
            }
        }
    }

    for (name, prog_cfg) in &new_cfg.programs {
        match map.get_mut(name) {
            Some(rt_job) => {
                let current = rt_job.children.len();
                let desired = prog_cfg.numprocs;
                if desired > current {
                    let mut extras = spawn_children(name, &prog_cfg, state.clone()).await;
                    for handle in &extras {
                        info!(
                            program = %name,
                            pid = handle.lock().await.id().unwrap_or(0),
                            "Additional replica started"
                        );
                    }
                    rt_job.children.append(&mut extras);
                } else if desired < current {
                    let surplus = rt_job.children.split_off(desired);
                    for handle in surplus {
                        let mut child = handle.lock().await;
                        let _ = child.kill().await;
                        info!(
                            program = %name,
                            pid = child.id().unwrap_or(0),
                            "Surplus replica stopped (scale down)"
                        );
                    }
                }
                rt_job.config = prog_cfg.clone(); // Updating stored config
            }
            None => {
                info!(
                    program = %name,
                    "Starting new job with {} replicas",
                    prog_cfg.numprocs
                );
        
                let mut children = Vec::new();
                if prog_cfg.autostart {
                    let new_children = spawn_children(name, prog_cfg, state.clone()).await;
                    for handle in &new_children {
                        info!(
                            program = %name,
                            pid = handle.lock().await.id().unwrap_or(0),
                            "Autostarted replica"
                        );
                    }
                    children = new_children;
                } else {
                    info!(
                        program = %name,
                        autorestart = prog_cfg.autostart,
                        "Inserted job without starting (autostart=false)"
                    );
                }
                map.insert(name.clone(), RuntimeJob { config: prog_cfg.clone(), children, retries_left: prog_cfg.startretries });
            }
        }
    }
    info!("Configuration application complete");
}