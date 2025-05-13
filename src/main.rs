mod parse;
mod runtime;
mod logger;
mod shell;
mod control;

use parse::{parser};
use runtime::{apply_config, SupervisorState};
use logger::{logs_tracing};
use shell::run_shell;
use control::{start_program, stop_program};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::LocalSet;
use tokio::runtime::Builder;


async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Arc::new(parser("config/config-scripts.yml")?);
    let state: SupervisorState = Arc::new(RwLock::new(HashMap::new()));
    
    let _guard = logs_tracing();
    tracing::info!("Supervisor started!");
    apply_config(&cfg, state.clone()).await;

    let status_state = state.clone();
    let reload_state = state.clone();
    let start_state  = state.clone();
    let stop_state   = state.clone();
    let cfg_for_start = cfg.clone();
    
    run_shell(
        move || {
            let map = futures::executor::block_on(status_state.read());
            for (name, job) in map.iter() {
                println!("{} : {} instance(s)", name, job.children.len());
            }
        },
        
        move || {
            if let Ok(new_cfg) = parser("config/config-scripts.yml") {
                futures::executor::block_on(apply_config(&new_cfg, reload_state.clone()));
                println!("Configuration reloaded");
            } else {
                println!("Failed to reload config");
            }
        },
        
        move |name: &str| {
            let cfg_clone = cfg_for_start.clone();
            let program_name = name.to_string();
            let programs = cfg_clone.programs.clone();
            let st       = start_state.clone();
            futures::executor::block_on(async { start_program(&program_name, &programs, st).await; });
        },
        
        move |name: &str| {
            let name = name.to_string();
            let state = stop_state.clone();
            futures::executor::block_on(async { stop_program(&name, state).await; });
        },
    )
    .await
    .unwrap();

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let multi_thread_runtime =  Builder::new_multi_thread().worker_threads(4).enable_all().build()?;
    let local = LocalSet::new();
    local.block_on(&multi_thread_runtime, async {
        if let Err(e) = async_main().await {
            eprintln!("Supervisor error: {}", e);
        }
    });

    // tracing::info!("Supervisor exiting");
    Ok(())
}