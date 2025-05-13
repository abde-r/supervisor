use std::fs::File;
use crate::parse::{Config, ProgramConfig, OneOrMany, RestartPolicy};
use tokio::process::Child;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::Mutex;
use tokio::task::spawn_local;
use tokio::process::Command;
use tracing::{info, warn, error};
use tokio::time::{sleep, Duration};
use std::process::Stdio;
use std::future::Future;
use std::pin::Pin;


pub type ChildHandle = Arc<Mutex<Child>>;

// Struct for the Parsed Config content with spawned children process
pub struct RuntimeJob {
    pub config: ProgramConfig,
    pub children: Vec<ChildHandle>,
    pub retries_left: usize,
}

pub fn boxed_spawn_children(
    name: String,
    cfg: ProgramConfig,
    state: SupervisorState,
) -> Pin<Box<dyn Future<Output = Vec<ChildHandle>> + Send>> {
    Box::pin(async move {
        spawn_children(&name, &cfg, state).await
    })
}

async fn monitor_child(
    name: String,
    cfg: ProgramConfig,
    handle: Arc<Mutex<Child>>,
    state: SupervisorState,
) {
    // 1) Wait for the process to exit, then drop the lock immediately.
    let status = {
        let mut child = handle.lock().await;
        child.wait().await.unwrap_or_else(|e| {
            warn!(program=%name, error=%e, "Failed to await child");
            // We bail out if we can't wait
            std::process::exit(1);
        })
    };

    let code = status.code().unwrap_or(-1) as u32;
    let expected = match &cfg.exitcodes {
        OneOrMany::One(n)   => vec![*n],
        OneOrMany::Many(vs) => vs.clone(),
    };

    // 2) Log expected vs. unexpected
    if expected.contains(&code) {
        info!(program=%name, code, "Exited with expected code");
    } else {
        error!(program=%name, code, "Unexpected exit");
    }

    // 3) Remove the dead handle from supervision immediately
    {
        let mut map = state.write().await;
        if let Some(job) = map.get_mut(&name) {
            job.children.retain(|h| !Arc::ptr_eq(h, &handle));
        }
    }

    // 4) Decide whether to restart
    let should_restart = match cfg.autorestart {
        RestartPolicy::Always     => true,
        RestartPolicy::Never      => false,
        RestartPolicy::Unexpected => !expected.contains(&code),
    };

    if should_restart {
        warn!(program=%name, exit_code=code, "Restarting per policy");
        let name_clone  = name.clone();
        let cfg_clone   = cfg.clone();
        let state_clone = state.clone();

        // Use tokio::spawn so this monitor can be Send + 'static
        tokio::spawn(async move {
            info!(program=%name_clone, "Spawning replacement for unexpected exit");
            let mut replacements = boxed_spawn_children(name.clone(), cfg_clone, state_clone.clone()).await;
            let mut map = state_clone.write().await;
            if let Some(job) = map.get_mut(&name_clone) {
                job.children.append(&mut replacements);
            }
        });
    } else {
        info!(program=%name, exit_code=code, "Not restarting per policy");
    }
}



// Shared map of Runtime data
// Updated each time the config data changes
pub type SupervisorState = Arc<RwLock<HashMap<String, RuntimeJob>>>;

pub async fn spawn_children(name: &str, cfg: &ProgramConfig, state: SupervisorState) -> Vec<ChildHandle> {
    let mut children = Vec::with_capacity(cfg.numprocs);
    info!("About to spawn: {:?} {:?} {:?}", &cfg.cmd, &cfg.args, &cfg.numprocs);
    for i in 0..cfg.numprocs {
        let mut cmd = Command::new(&cfg.cmd);
        cmd.args(&cfg.args);

        // Working directory
        if let Some(dir) = &cfg.workingdir {
            cmd.current_dir(dir);
        }
        
        // Env vars
        if let Some(envs) = &cfg.env {
            for (k, v) in envs {
                cmd.env(k, v);
            }
        }

        // STDOUT redirection or discard
        if let Some(path) = &cfg.stdout {
            if path == "null" {
                cmd.stdout(Stdio::null());
            } else {
                let f = File::create(path).unwrap_or_else(|e| panic!("failed to open stdout file `{}`: {}", path, e));
                cmd.stdout(Stdio::from(f));
            }
        }
    
        // STDERR redirection or discard
        if let Some(path) = &cfg.stderr {
            if path == "null" {
                cmd.stderr(Stdio::null());
            } else {
                let f = File::create(path).unwrap_or_else(|e| panic!("failed to open stderr file `{}`: {}", path, e));
                cmd.stderr(Stdio::from(f));
            }
        }


        // Spawn Child
        let child = cmd.spawn().unwrap_or_else(|e| panic!("failed to spawn child `{}`: {}", cfg.cmd, e));
        info!(
            program = %cfg.cmd,
            index = i,
            pid = child.id().unwrap_or(0),
            "Process started"
        );

        let handle: ChildHandle = Arc::new(Mutex::new(child));

        // Clone everything needed
        let name_clone1 = name.clone().to_string();
        let handle_clone1 = handle.clone();
        let grace_secs = cfg.starttime as u64;
        let handle_for_monitor = handle.clone();
        let name_for_monitor = name.to_string();
        let cfg_for_monitor = cfg.clone();
        let state_for_monitor = state.clone();

        tokio::spawn(async move {
            let interval = Duration::from_millis(100);
            let mut elapsed = Duration::ZERO;
            let grace = Duration::from_secs(grace_secs);
        
            while elapsed < grace {
                let exited_early = {
                    let mut guard = handle_clone1.lock().await;
                    match guard.try_wait() {
                        Ok(Some(status)) => {
                            error!(
                                program = %name_clone1,
                                code = %status.code().unwrap_or(-1),
                                "Exited before reaching starttime ({}s)", grace_secs
                            );
                            true
                        }
                        Ok(None) => false,
                        Err(e) => {
                            error!(
                                program = %name_clone1,
                                error = %e,
                                "Error checking child status"
                            );
                            true
                        }
                    }
                };
        
                if exited_early {
                    // Remove the handle from state
                    {
                        let mut map = state_for_monitor.write().await;
                        if let Some(job) = map.get_mut(&name_clone1) {
                            job.children.retain(|h| !Arc::ptr_eq(h, &handle_clone1));
                        }
                    }
        
                    // Respawn immediately if policy is Unexpected
                    if let RestartPolicy::Unexpected = cfg_for_monitor.autorestart {
                        info!(program = %name_clone1, "Early exit — respawning (Unexpected policy)");
                        let new_children = boxed_spawn_children(
                            name_clone1.clone(),
                            cfg_for_monitor.clone(),
                            state_for_monitor.clone(),
                        )
                        .await;
                        let mut map = state_for_monitor.write().await;
                        if let Some(job) = map.get_mut(&name_clone1) {
                            job.children.extend(new_children);
                        }
                    }
                    return;
                }
        
                sleep(interval).await;
                elapsed += interval;
            }
        
            // Passed grace period — monitor normally
            info!(
                program = %name_clone1,
                starttime = grace_secs,
                "Marked healthy after grace period"
            );
            tokio::spawn(async move {
                monitor_child(
                    name_for_monitor,
                    cfg_for_monitor,
                    handle_for_monitor,
                    state_for_monitor,
                )
                .await;
            });
        });

        children.push(handle);
    }
    children
}

pub async fn apply_config(new_cfg: &Config, state: SupervisorState) {
    info!(
        "Applying new configuration with {} programs",
        new_cfg.programs.len()
    );
    let mut map = state.write().await;

    // Stoping and Removing jobs no longer in the Config
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

    // Adding and Updating jobs
    for (name, prog_cfg) in &new_cfg.programs {
        match map.get_mut(name) {
            Some(rt_job) => {
                let current = rt_job.children.len();
                rt_job.retries_left = prog_cfg.startretries;
                let desired = prog_cfg.numprocs;
                if desired > current {
                    let mut extras = boxed_spawn_children(name.clone(), prog_cfg.clone(), state.clone()).await;
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
                    let new_children = boxed_spawn_children(name.clone(), prog_cfg.clone(), state.clone()).await;
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