//! Application settings
//!
//! This module provides global configuration access through a singleton pattern.
//!
//! # Thread Safety
//!
//! Uses `std::sync::RwLock` for thread-safe read/write access to configuration.
//! Multiple readers can access simultaneously, but writers have exclusive access.
//!
//! # Responsibility boundary
//!
//! The server owns only the global configuration. Project-level effective
//! configuration is resolved per request from the project registry, which
//! combines the global configuration with the project record stored in the
//! metadata database. The server never reads project configuration files.

use std::sync::{
    OnceLock, RwLock,
    atomic::{AtomicU64, Ordering},
};

use super::global::AppConfig;
use super::loader::ConfigLoader;
use cce_types::error::ConfigError;

/// Global configuration singleton
static GLOBAL_CONFIG: OnceLock<RwLock<AppConfig>> = OnceLock::new();

/// Monotonic global configuration version.
///
/// Bumped every time the global configuration is (re)initialized or mutated
/// at runtime. The project registry snapshots this version with each cached
/// entry so a global change invalidates cached effective configurations even
/// when the project profile itself is untouched.
static GLOBAL_CONFIG_VERSION: AtomicU64 = AtomicU64::new(0);

/// Application settings manager
pub struct Settings;

impl Settings {
    /// Initialize global configuration
    ///
    /// This should be called once at application startup.
    /// Returns an error if called multiple times.
    pub fn init(config: AppConfig) -> Result<(), ConfigError> {
        if GLOBAL_CONFIG.get().is_some() {
            return Err(ConfigError::AlreadyInitialized);
        }
        let _ = GLOBAL_CONFIG.set(RwLock::new(config));
        GLOBAL_CONFIG_VERSION.store(1, Ordering::SeqCst);
        Ok(())
    }

    /// Initialize global configuration from file
    ///
    /// Loads configuration from the specified path or default paths.
    pub fn init_from_file(path: Option<&std::path::Path>) -> Result<(), ConfigError> {
        if Self::is_initialized() {
            return Err(ConfigError::AlreadyInitialized);
        }
        let mut loader = ConfigLoader::new();
        if let Some(p) = path {
            loader = loader.with_path(p);
        }
        let config = loader.load()?;
        Self::init(config)
    }

    /// Resolve the configuration file path shared by all entry points.
    ///
    /// An explicit path wins, then the `CCE_CONFIG` environment variable,
    /// then the default `config.toml` beside the working directory.
    pub fn resolve_config_path(explicit: Option<&str>) -> String {
        explicit
            .map(str::to_string)
            .or_else(|| std::env::var("CCE_CONFIG").ok())
            .unwrap_or_else(|| "config.toml".to_string())
    }

    /// Initialize global configuration from the resolved path with fallback.
    ///
    /// Loads the file at the resolved path and falls back to defaults when
    /// the file cannot be read. When global configuration is already present
    /// the existing value is returned so repeated entry calls stay harmless.
    /// Without an explicit path or `CCE_CONFIG`, every default candidate
    /// from the loader is probed in order.
    pub fn init_with_fallback(explicit: Option<&str>) -> AppConfig {
        if let Ok(config) = Self::global() {
            return config;
        }
        if let Some(path) = explicit
            .map(str::to_string)
            .or_else(|| std::env::var("CCE_CONFIG").ok())
        {
            match Self::init_from_file(Some(std::path::Path::new(&path))) {
                Ok(()) => return Self::global().unwrap_or_default(),
                Err(_) => {
                    eprintln!("Failed to load config from {path}; using defaults");
                    let default_config = AppConfig::default();
                    let _ = Self::init(default_config.clone());
                    return Self::global().unwrap_or(default_config);
                }
            }
        }
        for candidate in ConfigLoader::default_config_candidates() {
            if candidate.exists() && Self::init_from_file(Some(candidate.as_path())).is_ok() {
                return Self::global().unwrap_or_default();
            }
        }
        eprintln!("No config file found in default locations; using defaults");
        let default_config = AppConfig::default();
        let _ = Self::init(default_config.clone());
        Self::global().unwrap_or(default_config)
    }

    /// Get global configuration
    ///
    /// Returns a clone of the global configuration if initialized.
    /// Returns an error if configuration has not been initialized via `init()` or `init_from_file()`.
    pub fn global() -> Result<AppConfig, ConfigError> {
        let lock = GLOBAL_CONFIG.get().ok_or_else(|| {
            ConfigError::Other("Configuration not initialized. Call Settings::init() or Settings::init_from_file() first.".to_string())
        })?;
        let config = lock.read().map_err(|e| {
            ConfigError::Other(format!(
                "Failed to acquire read lock for global config: {}",
                e
            ))
        })?;
        Ok(config.clone())
    }

    /// Check if configuration is initialized
    pub fn is_initialized() -> bool {
        GLOBAL_CONFIG.get().is_some()
    }

    /// Get server configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn server() -> Result<super::global::ServerConfig, ConfigError> {
        Ok(Self::global()?.server)
    }

    /// Get database configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn database() -> Result<super::global::DatabaseConfig, ConfigError> {
        Ok(Self::global()?.database)
    }

    /// Get logging configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn logger() -> Result<super::global::LoggingConfig, ConfigError> {
        Ok(Self::global()?.logger)
    }

    /// Current global configuration version.
    ///
    /// Starts at 1 once initialized and advances on every runtime mutation.
    /// Returns 0 when the global configuration is not initialized yet.
    pub fn global_version() -> u64 {
        GLOBAL_CONFIG_VERSION.load(Ordering::SeqCst)
    }

    /// Combine a profile version with a global version.
    ///
    /// The high bits carry the global version and the low 32 bits the
    /// profile version, so a change on either side advances the combined
    /// version while both components stay recoverable.
    pub fn combine_versions(profile_version: u64, global_version: u64) -> u64 {
        (global_version << 32) | (profile_version & 0xFFFF_FFFF)
    }

    /// Update logging configuration at runtime.
    pub fn set_logger(config: super::global::LoggingConfig) -> Result<(), ConfigError> {
        let lock = GLOBAL_CONFIG
            .get()
            .ok_or_else(|| ConfigError::Other("Configuration not initialized.".to_string()))?;
        let mut guard = lock
            .write()
            .map_err(|e| ConfigError::Other(format!("Failed to acquire write lock: {}", e)))?;
        guard.logger = config;
        GLOBAL_CONFIG_VERSION.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// Get scanner configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn scanner() -> Result<super::modules::ScannerConfig, ConfigError> {
        Ok(Self::global()?.scanner)
    }

    /// Get embedder configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn embedder() -> Result<super::modules::EmbedderConfig, ConfigError> {
        Ok(Self::global()?.embedder)
    }

    /// Get grouper configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn grouper() -> Result<super::modules::NestProcessorConfig, ConfigError> {
        Ok(Self::global()?.grouper)
    }

    /// Get orchestrator configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn orchestrator() -> Result<super::modules::OrchestratorConfig, ConfigError> {
        Ok(Self::global()?.orchestrator)
    }

    /// Get indexer configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn indexer() -> Result<super::modules::IndexerConfig, ConfigError> {
        Ok(Self::global()?.orchestrator.indexer)
    }

    /// Get relation configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn relation() -> Result<super::modules::RelationConfig, ConfigError> {
        Ok(Self::global()?.relation)
    }

    /// Get AST to NL configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn ast_to_nl() -> Result<super::modules::AstToNlConfig, ConfigError> {
        Ok(Self::global()?.ast_to_nl)
    }

    /// Get license header filtering configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn license_header() -> Result<super::modules::LicenseHeaderConfig, ConfigError> {
        Ok(Self::global()?.license_header)
    }

    /// Get summary configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn summary() -> Result<super::modules::SummaryConfig, ConfigError> {
        Ok(Self::global()?.summary)
    }

    /// Get export configuration
    ///
    /// Returns an error if configuration has not been initialized.
    pub fn export() -> Result<super::modules::ExportModuleConfig, ConfigError> {
        Ok(Self::global()?.export)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_not_initialized() {
        // Settings::global() should return error when not initialized
        // Note: This test may be affected by other tests that initialize settings.
        // In isolation, it verifies the error path.
        if !Settings::is_initialized() {
            let result = Settings::global();
            assert!(result.is_err());
            assert!(
                result
                    .expect_err("should be error")
                    .reason()
                    .contains("not initialized")
            );
        }
    }

    #[test]
    fn test_initialized_config() {
        // Initialize with default config and verify access
        let _ = Settings::init(AppConfig::default()); // OK if already initialized by another test
        let config = Settings::global().expect("Should be initialized");
        assert_eq!(config.server.port, 9000);
    }

    #[test]
    fn test_server_config() {
        let _ = Settings::init(AppConfig::default()); // OK if already initialized by another test
        let server = Settings::server().expect("Should be initialized");
        assert!(!server.host.is_empty());
    }

    #[test]
    fn test_database_config() {
        let _ = Settings::init(AppConfig::default()); // OK if already initialized by another test
        let db = Settings::database().expect("Should be initialized");
        assert_eq!(db.sqlite.path, "metadata.db");
    }
}
