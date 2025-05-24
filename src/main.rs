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





/*
    @@@
    @async_main();
    . Parses the config file and initializes a shared, thread‐safe map guarded by an RwLock.
    . Sets up tracing/logging and applies the initial config (spawning all autostart processes).
    . Returns an async move based on the closures --status, reload, start, stop and exit-- which performs the requested operation.
*/
async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Arc::new(parser("config/config.yml")?);
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
            let state = status_state.clone();
            async move {
                let map = state.read().await;
                for (name, job) in map.iter() {
                    println!("{} : {} instance(s)", name, job.children.len());
                }
            }
        },
        move || {
            let state = reload_state.clone();
            async move {
                if let Ok(new_cfg) = parser("config/config.yml") {
                    apply_config(&new_cfg, state).await;
                    println!("Configuration reloaded");
                } else {
                    println!("Failed to reload config");
                }
            }
        },
        move |prog: &str| {
            let state = start_state.clone();
            let cfg = cfg_for_start.clone();
            let prog = prog.to_string();
            async move {
                start_program(&prog, &cfg.programs, state).await;
            }
        },
        move |prog: &str| {
            let state = stop_state.clone();
            let prog = prog.to_string();
            async move {
                stop_program(&prog, state).await;
            }
        },
    )
    .await
    .unwrap();

    Ok(())
}




/*
    @@@
    @main();
    . Builds a multi-threaded runtime with 4 workers and wraps it in a LocalSet to allow non-Send tasks.
    . Uses 4 OS threads for driving async tasks --interactive shell, child monitoring, spawning/killing processes, tracing and other tasks-- each for one.
    . Run async_main as the root future on that runtime, catching any top-level errors before exiting.
*/
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