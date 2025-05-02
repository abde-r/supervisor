use serde::Deserialize;
use std::{collections::HashMap, fs, process::Stdio, sync::Arc};
use tokio::{process::Command, signal::unix::{signal, SignalKind}, sync::RwLock};
use tracing::{info, warn};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::FmtSubscriber;
use rustyline::Editor;

#[derive(Deserialize, Clone)]
enum RestartPolicy { Always, OnFailure, Never }

#[derive(Deserialize, Clone)]
struct JobConfig {
    name: String,
    cmd: String,
    args: Vec<String>,
    replicas: usize,
    restart: RestartPolicy,
}

#[derive(Deserialize)]
struct Config { jobs: Vec<JobConfig> }

struct ChildHandle { mut child: Command }

struct RuntimeJob {
    config: JobConfig,
    children: Vec<tokio::process::Child>,
}

type SharedState = Arc<RwLock<HashMap<String, RuntimeJob>>>;

async fn spawn_job(state: SharedState, jc: JobConfig) {
    let mut rt = state.write().await;
    let mut job = RuntimeJob { config: jc.clone(), children: Vec::new() };
    for _ in 0..jc.replicas {
        let mut cmd = Command::new(&jc.cmd);
        cmd.args(&jc.args).stdout(Stdio::null()).stderr(Stdio::null());
        let child = cmd.spawn().expect("failed to spawn");
        info!(job=%jc.name, pid=child.id().unwrap(), "spawned");
        let s = state.clone();
        let name = jc.name.clone();
        tokio::spawn(async move {
            let status = child.await.expect("child error");
            warn!(job=%name, code=?status, "exited");
            // TODO: restart per policy
        });
        job.children.push(child);
    }
    rt.insert(jc.name.clone(), job);
}

async fn load_config(state: SharedState) {
    let data = fs::read_to_string("supervisor.toml").expect("read failed");
    let cfg: Config = toml::from_str(&data).expect("parse failed");
    for jc in cfg.jobs {
        spawn_job(state.clone(), jc).await;
    }
    info!("config loaded");
}

#[tokio::main]
async fn main() {
    // Logging
    let file_appender: RollingFileAppender =
        tracing_appender::rolling::daily("./logs", "supervisor.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let subscriber = FmtSubscriber::builder()
        .with_writer(non_blocking)
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let state: SharedState = Arc::new(RwLock::new(HashMap::new()));

    // Initial load
    load_config(state.clone()).await;

    // Handle SIGHUP
    let mut hup = signal(SignalKind::hangup()).unwrap();
    let s2 = state.clone();
    tokio::spawn(async move {
        while hup.recv().await.is_some() {
            info!("SIGHUP received, reloading");
            load_config(s2.clone()).await;
        }
    });

    // REPL
    let mut rl = Editor::<()>::new().unwrap();
    println!("Supervisor control shell. Type 'help'.");
    loop {
        let line = rl.readline("> ").unwrap_or_else(|_| String::new());
        match line.trim() {
            "status" => {
                let map = state.read().await;
                for (name, job) in map.iter() {
                    println!("{}: {} replicas", name, job.children.len());
                }
            }
            "reload" => {
                load_config(state.clone()).await;
                println!("reloaded");
            }
            "exit" => break,
            cmd if cmd.starts_with("start ") => { /* TODO */ }
            cmd if cmd.starts_with("stop ") => { /* TODO */ }
            _ => println!("Unknown command"),
        }
    }
}
