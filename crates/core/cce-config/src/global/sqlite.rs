use serde::{Deserialize, Serialize};

use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

/// SQLite synchronous mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum SqliteSyncMode {
    /// No synchronous calls
    Off,
    /// Normal synchronous mode
    #[default]
    Normal,
    /// Full synchronous mode
    Full,
    /// Extra synchronous mode
    Extra,
}

impl std::fmt::Display for SqliteSyncMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SqliteSyncMode::Off => write!(f, "OFF"),
            SqliteSyncMode::Normal => write!(f, "NORMAL"),
            SqliteSyncMode::Full => write!(f, "FULL"),
            SqliteSyncMode::Extra => write!(f, "EXTRA"),
        }
    }
}

/// SQLite configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SqliteConfig {
    /// Database file path
    pub path: String,
    /// Enable WAL mode for better concurrency
    pub enable_wal: bool,
    /// Enable foreign key constraints
    pub enable_fk: bool,
    /// Synchronous mode
    /// In WAL mode, NORMAL is recommended for balance of safety and performance
    #[serde(default)]
    pub synchronous: SqliteSyncMode,
    /// Cache size in KB (negative value means KB, positive means pages)
    /// Default: -64000 (64MB)
    pub cache_size: i32,
    /// Busy timeout in milliseconds for lock waiting
    /// Default: 5000 (5 seconds)
    pub busy_timeout_ms: u32,
    /// Memory-mapped I/O size in bytes (0 to disable)
    /// Default: 268435456 (256MB)
    pub mmap_size: u64,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            path: "metadata.db".to_string(),
            enable_wal: true,
            enable_fk: true,
            synchronous: SqliteSyncMode::default(),
            cache_size: -64000,
            busy_timeout_ms: 5000,
            mmap_size: 268435456,
        }
    }
}

impl Validate for SqliteConfig {
    fn validate_structured(&self) -> ValidationResult {
        if self.cache_size < -1048576 || self.cache_size > 1048576 {
            return Err(ConfigValidationError::out_of_range(
                "cache_size",
                self.cache_size.to_string(),
                "-1048576",
                "1048576",
            ));
        }
        Ok(())
    }
}

impl SqliteConfig {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            ..Default::default()
        }
    }

    pub fn with_path(path: impl Into<String>) -> Self {
        Self::new(path)
    }

    pub fn enable_wal(mut self) -> Self {
        self.enable_wal = true;
        self
    }

    pub fn disable_wal(mut self) -> Self {
        self.enable_wal = false;
        self
    }

    pub fn enable_fk(mut self) -> Self {
        self.enable_fk = true;
        self
    }

    pub fn disable_fk(mut self) -> Self {
        self.enable_fk = false;
        self
    }

    pub fn synchronous(mut self, mode: SqliteSyncMode) -> Self {
        self.synchronous = mode;
        self
    }

    pub fn cache_size(mut self, size: i32) -> Self {
        self.cache_size = size;
        self
    }

    pub fn busy_timeout_ms(mut self, timeout: u32) -> Self {
        self.busy_timeout_ms = timeout;
        self
    }

    pub fn mmap_size(mut self, size: u64) -> Self {
        self.mmap_size = size;
        self
    }
}
