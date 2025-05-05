mod parse;
mod runtime;
mod logger;

use parse::{Config, parser};
use runtime::{apply_config, SupervisorState};
use logger::{logs_tracing};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use rustyline::{Editor, error::ReadlineError};

struct CmdCompleter {
    commands: Vec<String>,
}

impl Completer for CmdCompleter {
    type Candidate = Pair;
    fn complete(&self, line: &str, line: &str, _pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let start = 0;
        let mut match = Vec::new();
        for cmd in &self.commands {
            if cmd.starts_with(line) {
                matches.push(Pair {
                    display: cmd.clone(),
                    replacement: cmd.clone(),
                });
            }
        }
        Ok((start, matches))
    }
}

let helper = CmdCompleter {
    Commands: vec![
        "status".into(),
        "reload".into(),
        "start".into(),
        "stop".into(),
        "exit".into(),
    ],
};

let mut rl = Editor::with_config(
    Config::builder().completion_type(CompletionType::List).build()
)?;
rl.set_helper(Some(helper));


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg: Config = parser("config/config.yml")?;
    let state: SupervisorState = Arc::new(RwLock::new(HashMap::new()));
    
    let _guard = logs_tracing();
    tracing::info!("Process started!");
    apply_config(cfg, state.clone()).await;

    let helper = CmdCompleter {};
    let mut rl = Editor::with_config(
        Config::builder().complete_type(CompletionType::List).build()
    )?;
    rl.set_helper(Some(helper));
    rl.load_history("history.txt").ok();

    loop {
        match rl.readline("supervisor> ") {
            Ok(line) => {
                let input = line.trim();
                rl.add_history_entry(input)?;
                match input {
                    "status" => {}
                    "reload" => {}
                    cmd if cmd.starts_with("start ") => {}
                    cmd if cmd.starts_with("stop ") => {}
                    "exit" => break,
                    _ => println!("Unknown command"),
                }
            }
            Err(ReadlineError::Interruped) | Err(ReadlineError::Eof) => break,
            Err(err) => { eprintln!("Error: {:?}", err); break; },
        }
    }

    rl.save_history("history.txt")?;
    Ok(())
}