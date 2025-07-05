use std::{fs::OpenOptions, io, path::PathBuf, str::FromStr};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Registry};

/// Initialize logging to stdout and optionally to a file.
///
/// If `log_file` is Some, logs will be written to both stdout and the file.
/// If `log_level` is not provided or is invalid, it defaults to "info".
pub fn init_logging(log_file: Option<&PathBuf>, log_level: &str, verbose_stdout: bool) {
    let log_level_filter = LevelFilter::from_str(log_level).unwrap_or(LevelFilter::INFO);
    let env_filter = EnvFilter::new(log_level_filter.to_string());

    let subscriber: Box<dyn tracing::Subscriber + Send + Sync> = match (log_file, verbose_stdout) {
        (Some(path), true) => {
            // Log to both file and stdout
            let path = path.clone();
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
        (Some(path), false) => {
            // Log only to file
            let path = path.clone();
            let file_layer = fmt::layer().with_writer(move || {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .expect("Failed to open log file")
            });
            Box::new(Registry::default().with(env_filter).with(file_layer))
        }
        (None, _) => {
            // Log only to stdout
            let stdout_layer = fmt::layer().with_writer(io::stdout);
            Box::new(Registry::default().with(env_filter).with(stdout_layer))
        }
    };

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set global subscriber");
}
