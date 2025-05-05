use rustyline::error::ReadlineError;
use rustyline::{Editor, Helper, CompletionType, Config};
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::Context;


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

pub async fn run_shell<F1, F2, F3, F4>(
    mut on_status: F1,
    mut on_reload: F2,
    mut on_start: F3,
    mut on_stop: F4
) -> rustyline::Result<()>
where
    F1: FnMut(),
    F2: FnMut(),
    F3: FnMut(&str),
    F4: FnMut(&str),
{
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let mut rl = Editor::with_config(config)?;
    let helper = CmdCompleter {
        commands: vec![
            "status".into(),
            "reload".into(),
            "start".into(),
            "stop".into(),
            "exit".into(),
        ],
    };
    rl.set_helper(Some(helper));
    let _ = rl.load_history("logs/history.txt");

    loop {
        let line = rl.readline("supervisor> ");
        match line {
            Ok(line) => {
                let input = line.trim();
                rl.add_history_entry(input)?;
                match input {
                    "status" => on_status(),
                    "reload" => on_reload(),
                    cmd if cmd.starts_with("start ") => {
                        let name = cmd["start ".len()..].trim();
                        on_start(name);
                    }
                    cmd if cmd.starts_with("stop ") => {
                        let name = cmd["stop ".len()..].trim();
                        on_stop(name);
                    }
                    "exit" => break,
                    _ => println!("Unknown command: {}", input),
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
