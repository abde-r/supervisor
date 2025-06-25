use rustyline::{Editor, Helper, Config, error::ReadlineError, Context};
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use std::future::Future;
use chrono::Local;
use std::fs::File;
use std::io::{BufRead, BufReader};




/*
    @@@
    @CmdCompleter;
    . Drops CmdCompleter into 'rl.set_helper(Some(...))' and get instant, prefix-based command completion.
    . Plugs into rustyline to provide simple tab-completion based on a fixed list of command names.
*/
struct CmdCompleter {
    commands: Vec<String>,
}
impl Helper for CmdCompleter {}
impl Hinter for CmdCompleter {
    type Hint = String;
}
impl Highlighter for CmdCompleter {}
impl Validator for CmdCompleter {}
impl Completer for CmdCompleter {
    type Candidate = Pair;
    fn complete(&self, line: &str, _pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let mut matches = Vec::new();
        for cmd in &self.commands {
            if cmd.starts_with(line) {
                matches.push(Pair {
                    display: cmd.clone(),
                    replacement: cmd.clone(),
                });
            }
        }
        Ok((0, matches))
    }
}




/*
    @@@
    @run_shell();
    . Parses the config file and initializes a shared, thread‐safe map guarded by an RwLock.
    . Sets up tracing/logging and applies the initial config (spawning all autostart processes).
    . Returns an async move based on the closures --status, reload, start, stop and exit-- which performs the requested operation.
*/
pub async fn run_shell<SFut, RFut, StFut, SpFut, OnStatus, OnReload, OnStart, OnStop>(
    mut on_status: OnStatus,
    mut on_reload: OnReload,
    mut on_start: OnStart,
    mut on_stop: OnStop,
) -> rustyline::Result<()>
where
    OnStatus: FnMut() -> SFut + 'static,
    SFut: Future<Output = ()> + 'static,
    OnReload: FnMut() -> RFut + 'static,
    RFut: Future<Output = ()> + 'static,
    OnStart: FnMut(&str) -> StFut + 'static,
    StFut: Future<Output = ()> + 'static,
    OnStop: FnMut(&str) -> SpFut + 'static,
    SpFut: Future<Output = ()> + 'static,
{
    let config = Config::builder().build();
    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(CmdCompleter {
        commands: vec!["status", "reload", "start", "stop", "exit"].into_iter().map(String::from).collect(),
    }));
    let _ = rl.load_history("logs/history.txt");

    loop {
        let line = rl.readline("task-slave> ");
        match line {
            Ok(line) => {
                let input = line.trim();
                rl.add_history_entry(input)?;
                match input {
                    "status" => on_status().await,
                    "reload" => on_reload().await,
                    cmd if cmd.starts_with("start ") => {
                        let name = cmd["start ".len()..].trim();
                        on_start(name).await;
                    }
                    cmd if cmd.starts_with("stop ") => {
                        let name = cmd["stop ".len()..].trim();
                        on_stop(name).await;
                    }
                    "exit" => {
                        tracing::info!("Supervisor exited!");
                        break
                    },
                    "tail" => {
                        let today = Local::now().format("%Y-%m-%d");
                        let path = format!("logs/supervisor.log.{}", today);

                        let file = match File::open(&path) {
                            Ok(f) => f,
                            Err(e) => {
                                eprintln!("Couldn't open {}: {}", path, e);
                                break;
                            }
                        };

                        let reader = BufReader::new(file);
                        let lines: Vec<String> = reader
                            .lines()
                            .filter_map(Result::ok)
                            .collect();

                        let start = lines.len().saturating_sub(10);
                        for line in &lines[start..] {
                            println!("{}", line);
                        }
                    }
                    "help" => println!("start -instance_name --start a program\nstop -instance_name --stop a program\nreload --reload all programs\nstatus --status of all programs\nexit --exit supervisor"),
                    other => println!("Unknown command: {}", other),
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            },
        }
    }

    rl.save_history("logs/history.txt")?;
    Ok(())
}
