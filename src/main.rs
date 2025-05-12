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
    let cfg = Arc::new(parser("config/config-sigtest.yml")?);
    let state: SupervisorState = Arc::new(RwLock::new(HashMap::new()));
    
    let _guard = logs_tracing();
    tracing::info!("Supervisor started!");
    apply_config(&cfg, state.clone()).await;

    tokio::spawn({
        let cfg = cfg.clone();
        let state = state.clone();
        async move {
            run_shell(
                || {
                    let map = futures::executor::block_on(state.read());
                    for (name, job) in map.iter() {
                        println!("{} : {} instance(s)", name, job.children.len());
                    }
                },
                || {
                    if let Ok(new_cfg) = parser("config/config-sigtest.yml") {
                        futures::executor::block_on(apply_config(&new_cfg, state.clone()));
                        println!("Configuration reloaded");
                    }
                    else {
                        println!("Failed to reload config");
                    }
                },
                {
                    let cfg = cfg.clone();
                    let state = state.clone();
                    move |name: &str| {
                        let program_name = name.to_string();
                        let programs = cfg.programs.clone();
                        let state = state.clone();
                        tokio::spawn(async move {
                            start_program(&program_name, &programs, state).await;
                        });
                    }
                },
                {
                    let state = state.clone();
                    move |name: &str| {
                        let program_name = name.to_string();
                        let state = state.clone();
                        tokio::spawn(async move {
                            stop_program(&program_name, state).await;
                        });
                    }
                },
            ).await.unwrap_or_else(|e| eprintln!("shell error: {}", e));
        }
    });

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