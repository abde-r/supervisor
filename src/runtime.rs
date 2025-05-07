use crate::parse::{Config, ProgramConfig, OneOrMany, RestartPolicy};
use tokio::process::Child;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::Mutex;
use tokio::task::spawn_local;
use tokio::process::Command;
use tracing::{info, warn};


pub type ChildHandle = Arc<Mutex<Child>>;

// Struct for the Parsed Config content with spawned children process
pub struct RuntimeJob {
    pub config: ProgramConfig,
    pub children: Vec<ChildHandle>,
}


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
            
                spawn_local(async move {
                    let mut replacements = spawn_children(&name, &cfg, state.clone()).await;
                    let mut map = state.write().await;
                    if let Some(job) = map.get_mut(&name) {
                        job.children.extend(replacements.drain(..));
                    }
                });
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

// Shared map of Runtime data
// Updated each time the config data changes
pub type SupervisorState = Arc<RwLock<HashMap<String, RuntimeJob>>>;

pub async fn spawn_children(name: &str, cfg: &ProgramConfig, state: SupervisorState) -> Vec<ChildHandle> {
    let mut children = Vec::with_capacity(cfg.numprocs);
    info!("About to spawn: {:?} {:?} {:?}", &cfg.cmd, &cfg.args, &cfg.numprocs);
    for i in 0..cfg.numprocs {
        let mut cmd = Command::new(&cfg.cmd);
        cmd.args(&cfg.args);
        if let Some(dir) = &cfg.workingdir {
            cmd.current_dir(dir);
        }
        // cmd.arg("-c").arg(&cfg.cmd).current_dir(cfg.workingdir.as_deref().unwrap_or("."));
        if let Some(envs) = &cfg.env {
            for (k, v) in envs {
                cmd.env(k, v);
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
        let name_clone = name.clone().to_string();
        let cfg_clone = cfg.clone();
        let state_clone = state.clone();
        let handle_clone = handle.clone();
        
        tokio::spawn(async move {
            monitor_child(name_clone, cfg_clone, handle_clone, state_clone).await;
        });
        // tokio::spawn(async move {  });
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
                map.insert(name.clone(), RuntimeJob { config: prog_cfg.clone(), children });
            }
        }
    }
    info!("Configuration application complete");
}