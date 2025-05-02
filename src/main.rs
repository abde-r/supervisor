mod parse;
mod runtime;

use parse::{Config, parser};
use runtime::{apply_config, SupervisorState};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg: Config = parser("config/config.yml")?;
    let state: SupervisorState = Arc::new(RwLock::new(HashMap::new()));

    for (name, prog) in &cfg.programs {
        println!("Program `{}`:\n{:#?}\n", name, prog);
    }

    apply_config(cfg, state.clone()).await;

    Ok(())
}