use serde::{Deserialize, Serialize};

use crate::serde_helpers::empty_string_as_none;

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Trace level (most verbose)
    Trace,
    /// Debug level
    Debug,
    /// Info level
    #[default]
    Info,
    /// Warning level
    Warn,
    /// Error level (least verbose)
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "trace"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}

/// Log output target
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogOutput {
    /// Standard output
    #[default]
    Stdout,
    /// Standard error
    Stderr,
    /// File output
    File,
}

impl std::fmt::Display for LogOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogOutput::Stdout => write!(f, "stdout"),
            LogOutput::Stderr => write!(f, "stderr"),
            LogOutput::File => write!(f, "file"),
        }
    }
}

/// Log output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Pretty format (human-readable)
    #[default]
    Pretty,
    /// Compact format
    Compact,
    /// JSON format
    Json,
}

impl std::fmt::Display for LogFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogFormat::Pretty => write!(f, "pretty"),
            LogFormat::Compact => write!(f, "compact"),
            LogFormat::Json => write!(f, "json"),
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    #[serde(default)]
    pub level: LogLevel,
    /// Output target
    #[serde(default)]
    pub output: LogOutput,
    /// Output format
    #[serde(default)]
    pub format: LogFormat,
    /// Log file path (required if output is "file")
    /// Empty string is treated as None
    #[serde(deserialize_with = "empty_string_as_none", default)]
    pub file: Option<String>,
}
