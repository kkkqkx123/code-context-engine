//! Structured logging with tracing
//!
//! This module provides unified logging infrastructure with support for:
//! - Main application logs (stdout/stderr/file)
//! - Separate metrics logging to dedicated file
//!
//! The architecture uses concrete types for zero dynamic dispatch overhead.

use std::path::Path;
use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use cce_config::global::LoggingConfig;
use cce_types::error::ConfigError;

/// Result type alias for logger operations
pub type Result<T> = std::result::Result<T, ConfigError>;

mod config;

pub use config::{get_format_string, get_output_string, parse_level, validate_config};

/// Global guard to keep the log appenders alive
pub static LOG_GUARD: OnceLock<Vec<WorkerGuard>> = OnceLock::new();

/// Initialize the logging system
pub fn init(config: &LoggingConfig) -> Result<()> {
    validate_config(config)?;

    let filter = EnvFilter::new(config.level.to_string());

    let format = get_format_string(config);
    let output = get_output_string(config);

    match (output, format) {
        ("file", fmt) => {
            init_file(config, filter, fmt)?;
        }
        ("stderr", fmt) => {
            init_stderr(filter, fmt)?;
        }
        (_, fmt) => {
            init_stdout(filter, fmt)?;
        }
    }

    Ok(())
}

fn init_file(config: &LoggingConfig, filter: EnvFilter, format: &str) -> Result<()> {
    let file_path = config
        .file
        .as_ref()
        .map(Path::new)
        .ok_or_else(|| ConfigError::Other("Log file path not specified".to_string()))?;

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ConfigError::Other(format!("Failed to create log directory: {}", e)))?;
    }

    let file_appender = tracing_appender::rolling::daily(
        file_path.parent().unwrap_or(Path::new(".")),
        file_path.file_name().unwrap_or_default(),
    );

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let _ = LOG_GUARD.set(vec![guard]);

    let main_layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = match format {
        "json" => Box::new(
            fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_target(true)
                .with_level(true),
        ),
        "compact" => Box::new(
            fmt::layer()
                .compact()
                .with_writer(non_blocking)
                .with_target(true)
                .with_level(true),
        ),
        _ => Box::new(
            fmt::layer()
                .with_writer(non_blocking)
                .with_target(true)
                .with_level(true),
        ),
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(main_layer)
        .try_init()
        .ok();

    Ok(())
}

fn init_stderr(filter: EnvFilter, format: &str) -> Result<()> {
    let main_layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = match format {
        "json" => Box::new(
            fmt::layer()
                .json()
                .with_writer(std::io::stderr)
                .with_target(false)
                .with_level(true),
        ),
        "compact" => Box::new(
            fmt::layer()
                .compact()
                .with_writer(std::io::stderr)
                .with_target(false)
                .with_level(true),
        ),
        _ => Box::new(
            fmt::layer()
                .pretty()
                .with_writer(std::io::stderr)
                .with_target(false)
                .with_level(true),
        ),
    };

    let dummy_guard = create_dummy_guard();
    let _ = LOG_GUARD.set(vec![dummy_guard]);

    tracing_subscriber::registry()
        .with(filter)
        .with(main_layer)
        .try_init()
        .ok();

    Ok(())
}

fn init_stdout(filter: EnvFilter, format: &str) -> Result<()> {
    let main_layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = match format {
        "json" => Box::new(fmt::layer().json().with_target(false).with_level(true)),
        "compact" => Box::new(fmt::layer().compact().with_target(false).with_level(true)),
        _ => Box::new(fmt::layer().pretty().with_target(false).with_level(true)),
    };

    let dummy_guard = create_dummy_guard();
    let _ = LOG_GUARD.set(vec![dummy_guard]);

    tracing_subscriber::registry()
        .with(filter)
        .with(main_layer)
        .try_init()
        .ok();

    Ok(())
}

fn create_dummy_guard() -> WorkerGuard {
    use tracing_appender::rolling;
    let appender = rolling::never(".", ".dummy");
    let (_, guard) = tracing_appender::non_blocking(appender);
    guard
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_config::global::{LogFormat, LogLevel, LogOutput};

    fn create_test_config(
        level: LogLevel,
        output: LogOutput,
        format: LogFormat,
        file: Option<String>,
    ) -> LoggingConfig {
        LoggingConfig {
            level,
            output,
            format,
            file,
        }
    }

    #[test]
    fn test_parse_level_valid() {
        assert_eq!(parse_level(LogLevel::Trace), tracing::Level::TRACE);
        assert_eq!(parse_level(LogLevel::Debug), tracing::Level::DEBUG);
        assert_eq!(parse_level(LogLevel::Info), tracing::Level::INFO);
        assert_eq!(parse_level(LogLevel::Warn), tracing::Level::WARN);
        assert_eq!(parse_level(LogLevel::Error), tracing::Level::ERROR);
    }

    #[test]
    fn test_validate_config_stdout() {
        let config = create_test_config(LogLevel::Info, LogOutput::Stdout, LogFormat::Pretty, None);
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_config_file_without_path() {
        let config = create_test_config(LogLevel::Info, LogOutput::File, LogFormat::Pretty, None);
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_config_file_with_path() {
        let config = create_test_config(
            LogLevel::Info,
            LogOutput::File,
            LogFormat::Compact,
            Some("test.log".to_string()),
        );
        assert!(validate_config(&config).is_ok());
    }
}
