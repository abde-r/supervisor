use crate::runtime::{spawn_children, RuntimeJob, SupervisorState};
use crate::parse::ProgramConfig;
use std::collections::HashMap;

pub async fn start_program(name: &str, configs: &HashMap<String, ProgramConfig>, state: SupervisorState) {
    match configs.get(name) {
        Some(cfg) => {
            let children = spawn_children(name, cfg, state.clone()).await;
            let mut map = state.write().await;
            
            let job = map.entry(name.to_string()).or_insert_with(|| RuntimeJob {
                config: cfg.clone(),
                children: Vec::new(),
            });
            job.children.extend(children);
            println!("Started {} instance(s) of `{}`", cfg.numprocs, name);
        }
        None => eprintln!("No such program in config: `{}`", name),
    }
}

pub async fn stop_program(name: &str, state: SupervisorState) {
    let mut map = state.write().await;

    if let Some(mut job) = map.remove(name) {
        for handle in job.children.drain(..) {
            let mut child = handle.lock().await;
            let pid = child.id().unwrap_or(0);
            let _ = child.kill();
            println!("Stopped process {} of `{}`", pid, name);
        }
        println!("All instances of `{}` stopped", name);
    } else {
        println!("Program `{}` is not running", name);
    }
}
