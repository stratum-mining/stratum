use std::{fs::OpenOptions, io, path::Path, str::FromStr};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Registry};

/// Initialize logging to stdout and optionally to a file.
///
/// If `log_file` is Some, logs will be written to both stdout and the file.
/// If `log_level` is not provided or is invalid, it defaults to "info".
pub fn init_logging(log_file: Option<&Path>) {
    let rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let log_level_filter = LevelFilter::from_str(&rust_log).unwrap_or(LevelFilter::INFO);
    let env_filter = EnvFilter::new(log_level_filter.to_string());

    let subscriber: Box<dyn tracing::Subscriber + Send + Sync> = match log_file {
        Some(path) => {
            // Log to both file and stdout
            let path = path.to_owned();
            let file_layer = fmt::layer().with_writer(move || {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .expect("Failed to open log file")
            });
            let stdout_layer = fmt::layer().with_writer(io::stdout);
            Box::new(
                Registry::default()
                    .with(env_filter)
                    .with(stdout_layer)
                    .with(file_layer),
            )
        }
        None => {
            // Log only to stdout
            let stdout_layer = fmt::layer().with_writer(io::stdout);
            Box::new(Registry::default().with(env_filter).with(stdout_layer))
        }
    };

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set global subscriber");
}
