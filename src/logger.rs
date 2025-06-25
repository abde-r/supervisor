use tracing_subscriber::fmt::{SubscriberBuilder};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_appender::non_blocking::WorkerGuard;



/*
    @@@
    @logs_tracing();
    . Creates a daily-rotating log file (logs/supervisor.log) and wraps it in a non-blocking writer.
    . Configures a tracing subscriber to log INFO-level events (with timestamps, thread IDs, and targets) to that writer.
    . keeps the appender alive by returning the guard.
*/
pub fn logs_tracing() -> WorkerGuard {
    let file_appender = RollingFileAppender::new(Rotation::DAILY, "logs", "supervisor.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let subscriber = SubscriberBuilder::default()
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .with_writer(non_blocking)
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set global subscriber");
    guard
}