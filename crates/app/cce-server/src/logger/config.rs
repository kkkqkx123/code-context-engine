//! Configuration management for logging

use tracing::Level;

use cce_config::global::{LogFormat, LogLevel, LogOutput, LoggingConfig};
use cce_types::error::ConfigError;

/// Result type alias for logger config operations
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Parse log level enum to tracing::Level
pub fn parse_level(level: LogLevel) -> Level {
    match level {
        LogLevel::Trace => Level::TRACE,
        LogLevel::Debug => Level::DEBUG,
        LogLevel::Info => Level::INFO,
        LogLevel::Warn => Level::WARN,
        LogLevel::Error => Level::ERROR,
    }
}

/// Validate logging configuration
pub fn validate_config(config: &LoggingConfig) -> Result<()> {
    parse_level(config.level);

    if config.output == LogOutput::File && config.file.is_none() {
        return Err(ConfigError::Other(
            "Log file path must be specified when output is 'file'".to_string(),
        ));
    }

    Ok(())
}

/// Get the output format string
pub fn get_format_string(config: &LoggingConfig) -> &'static str {
    match config.format {
        LogFormat::Pretty => "pretty",
        LogFormat::Compact => "compact",
        LogFormat::Json => "json",
    }
}

/// Get the output target string
pub fn get_output_string(config: &LoggingConfig) -> &'static str {
    match config.output {
        LogOutput::Stdout => "stdout",
        LogOutput::Stderr => "stderr",
        LogOutput::File => "file",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_level() {
        assert_eq!(parse_level(LogLevel::Trace), Level::TRACE);
        assert_eq!(parse_level(LogLevel::Debug), Level::DEBUG);
        assert_eq!(parse_level(LogLevel::Info), Level::INFO);
        assert_eq!(parse_level(LogLevel::Warn), Level::WARN);
        assert_eq!(parse_level(LogLevel::Error), Level::ERROR);
    }

    #[test]
    fn test_validate_config_valid() {
        let config = LoggingConfig::default();

        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_config_file_without_path() {
        let config = LoggingConfig {
            output: LogOutput::File,
            ..Default::default()
        };

        assert!(validate_config(&config).is_err());
    }
}
