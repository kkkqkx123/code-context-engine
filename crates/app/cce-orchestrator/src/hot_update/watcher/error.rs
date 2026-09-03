//! Error types for file watching module
//!
//! This module provides error handling for file system watching operations.

use std::path::PathBuf;
use thiserror::Error;

/// Error type for file watching operations
#[derive(Error, Debug)]
pub enum WatchError {
    /// Error from the notify library
    #[error("Notify error: {0}")]
    Notify(#[from] notify::Error),

    /// Error when watching a path
    #[error("Failed to watch path '{path}': {message}")]
    WatchPath {
        /// The path that failed to watch
        path: PathBuf,
        /// Error message
        message: String,
    },

    /// Error when sending event
    #[error("Failed to send watch event: {0}")]
    SendEvent(String),

    /// Error when receiving event
    #[error("Failed to receive watch event: {0}")]
    ReceiveEvent(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid path
    #[error("Invalid path: {0}")]
    InvalidPath(PathBuf),

    /// Watcher not initialized
    #[error("Watcher not initialized")]
    NotInitialized,

    /// Watcher already running
    #[error("Watcher already running")]
    AlreadyRunning,

    /// Event storm detected
    #[error("Event storm detected: {events_per_sec} events/second exceeds threshold {threshold}")]
    EventStorm {
        /// Events per second
        events_per_sec: usize,
        /// Threshold
        threshold: usize,
    },

    /// Failed to reload configuration
    #[error("Failed to reload configuration from '{path}': {message}")]
    ConfigReload {
        /// Configuration file path
        path: PathBuf,
        /// Error message
        message: String,
    },
}

impl WatchError {
    /// Create a new watch path error
    pub fn watch_path(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::WatchPath {
            path: path.into(),
            message: message.into(),
        }
    }

    /// Create a new send event error
    pub fn send_event(message: impl Into<String>) -> Self {
        Self::SendEvent(message.into())
    }

    /// Create a new receive event error
    pub fn receive_event(message: impl Into<String>) -> Self {
        Self::ReceiveEvent(message.into())
    }

    /// Create a new config error
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
    }

    /// Create a new invalid path error
    pub fn invalid_path(path: impl Into<PathBuf>) -> Self {
        Self::InvalidPath(path.into())
    }

    /// Create a new config reload error
    pub fn config_reload(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::ConfigReload {
            path: path.into(),
            message: message.into(),
        }
    }
}

/// Result type alias for watch operations
pub type Result<T> = std::result::Result<T, WatchError>;
