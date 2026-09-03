//! Application settings
//!
//! This module provides global configuration access through a singleton pattern.
//! It also supports project-specific configuration that can override global settings.
//!
//! # Thread Safety
//!
//! Uses `std::sync::RwLock` for thread-safe read/write access to configuration.
//! Multiple readers can access simultaneously, but writers have exclusive access.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use super::global::AppConfig;
use super::loader::ConfigLoader;
use cce_types::error::ConfigError;

/// Global configuration singleton
static GLOBAL_CONFIG: OnceLock<RwLock<AppConfig>> = OnceLock::new();

/// Project-specific configurations
static PROJECT_CONFIGS: OnceLock<RwLock<HashMap<String, AppConfig>>> = OnceLock::new();

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

    /// Initialize configuration for a specific project
    ///
    /// This loads global configuration and merges it with project-specific
    /// configuration. The result is cached for subsequent access.
    ///
    /// # Arguments
    ///
    /// * `project_id` - Unique identifier for the project
    /// * `project_root` - Path to the project root directory
    pub fn init_for_project(
        project_id: &str,
        project_root: &std::path::Path,
    ) -> Result<(), ConfigError> {
        let config = ConfigLoader::new().load_for_project(project_root)?;

        // Initialize global config if not already done
        if !Self::is_initialized() {
            // Use the merged config as global for now
            // (in production, global should be loaded separately)
            Self::init(config.clone())?;
        }

        // Store project-specific config
        let configs = PROJECT_CONFIGS.get_or_init(|| RwLock::new(HashMap::new()));
        let mut configs_write = configs.write().map_err(|e| {
            ConfigError::Other(format!(
                "Failed to acquire write lock for project configs: {}",
                e
            ))
        })?;
        configs_write.insert(project_id.to_string(), config);

        Ok(())
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

    /// Get configuration for a specific project
    ///
    /// Returns project-specific configuration if available, otherwise global config.
    /// Returns an error if neither project nor global configuration is initialized.
    pub fn for_project(project_id: &str) -> Result<AppConfig, ConfigError> {
        // Try to get project-specific config first
        if let Some(configs_lock) = PROJECT_CONFIGS.get() {
            let configs = configs_lock.read().map_err(|e| {
                ConfigError::Other(format!(
                    "Failed to acquire read lock for project configs: {}",
                    e
                ))
            })?;
            if let Some(config) = configs.get(project_id) {
                return Ok(config.clone());
            }
        }

        // Fall back to global config
        Self::global()
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
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9000);
    }

    #[test]
    fn test_server_config() {
        let _ = Settings::init(AppConfig::default()); // OK if already initialized by another test
        let server = Settings::server().expect("Should be initialized");
        assert_eq!(server.host, "0.0.0.0");
    }

    #[test]
    fn test_database_config() {
        let _ = Settings::init(AppConfig::default()); // OK if already initialized by another test
        let db = Settings::database().expect("Should be initialized");
        assert_eq!(db.sqlite.path, "metadata.db");
    }
}
