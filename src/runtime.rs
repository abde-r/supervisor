use crate::parse::{Config, ProgramConfig};
use tokio::process::Child;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// pub use crate::parse::ProgramConfig;


// Struct for the Parsed Config content with spawned children process
pub struct RuntimeJob {
    pub config: ProgramConfig,
    pub children: Vec<Child>,
}

// Shared map of Runtime data
// Updated each time the config data changes
pub type SupervisorState = Arc<RwLock<HashMap<String, RuntimeJob>>>;

pub async fn spawn_children(cfg: &ProgramConfig) -> Vec<Child> {
    let mut vec = Vec::with_capacity(cfg.numprocs);
    for _ in 0..cfg.numprocs {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(&cfg.cmd).current_dir(cfg.workingdir.as_deref().unwrap_or("."));
        if let Some(env) = &cfg.env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
        let child = cmd.spawn().expect("failed to spawn child");
        vec.push(child);
    }
    vec
}

pub async fn apply_config(new_cfg: Config, state: SupervisorState) {
    let mut map = state.write().await;

    // Stoping and Removing jobs no longer in the Config
    let to_remove: Vec<String> = map.keys()
        .filter(|name| !new_cfg.programs.contains_key(*name))
        .cloned()
        .collect();
    for name in to_remove {
        if let Some(job) = map.remove(&name) {
            for mut child in job.children {
                let _ = child.kill().await;
            }
        }
    }

    // Adding and Updating jobs
    for (name, prog_cfg) in new_cfg.programs {
        match map.get_mut(&name) {
            Some(rt_job) => {
                let current = rt_job.children.len();
                let desired = prog_cfg.numprocs;
                if desired > current {
                    let mut extras = spawn_children(&prog_cfg).await;
                    rt_job.children.append(&mut extras);
                } else if desired < current {
                    let surplus = rt_job.children.split_off(desired);
                    for mut child in surplus {
                        let _ = child.kill().await;
                    }
                }
                rt_job.config = prog_cfg; // Updating stored config
            }
            None => {
                let children = spawn_children(&prog_cfg).await;
                map.insert(name.clone(), RuntimeJob { config: prog_cfg, children });
            }
        }
    }
}