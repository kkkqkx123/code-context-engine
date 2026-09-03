//! Configuration loader
//!
//! This module provides functionality for loading configuration from:
//! - TOML files
//! - Environment variables
//! - .env files
//! - Default values
//!
//! # Loading Priority (highest to lowest)
//!
//! 1. Configuration file (highest for infrastructure settings: host, port, db path)
//! 2. Environment variables (CCE_*) for API keys, DB URLs, and logging
//! 3. .env file variables (provides defaults for sensitive values)
//! 4. Project local config (.cce/config.local.toml)
//! 5. Project config (.cce/config.toml)
//! 6. Global configuration file (config.toml)
//! 7. Default values
//!
//! # Infrastructure Settings Policy
//!
//! Server host/port are ONLY settable via config.toml, never via environment variables.
//! API keys and database URLs can be overridden via env vars for flexibility.
//!
//! # Environment Variable Placeholders
//!
//! Configuration values can reference environment variables using `${VAR_NAME}` syntax.
//! This is useful for keeping sensitive values out of configuration files.
//!
//! ```toml
//! [embedder]
//! api_keys = ["${EMB_API_KEY}"]
//!
//! [[llm.providers]]
//! api_keys = ["${LLM_API_KEY}"]
//! ```

use std::path::{Path, PathBuf};

use tracing::{debug, info, warn};

use super::env_loader;
use super::global::AppConfig;
use super::project::{ProjectAppConfig, ProjectConfigPaths};
use super::validation::Validate;
use cce_types::error::ConfigError;

/// Configuration loader
pub struct ConfigLoader {
    /// Configuration file path
    config_path: Option<PathBuf>,
}

impl ConfigLoader {
    /// Create a new configuration loader
    pub fn new() -> Self {
        Self { config_path: None }
    }

    /// Set configuration file path
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config_path = Some(path.into());
        self
    }

    /// Load configuration from all sources
    ///
    /// Priority (highest to lowest):
    /// 1. Configuration file (highest for infrastructure settings: host, port, db path)
    /// 2. Environment variables (CCE_*) for API keys, DB URLs, and logging
    /// 3. .env file (provides defaults for sensitive values)
    /// 4. Default values
    ///
    /// Infrastructure settings (server host/port) are ONLY settable via config.toml,
    /// never via environment variables. This ensures consistent deployment topology.
    pub fn load(self) -> Result<AppConfig, ConfigError> {
        info!("Starting configuration loading process");

        // Start with default configuration
        let mut config = AppConfig::default();

        debug!("Initialized with default configuration");

        // Load .env file first (so env vars are available for placeholder resolution)
        env_loader::load_dotenv();
        debug!("Loaded .env file (if exists)");

        // Load from file if specified
        if let Some(path) = &self.config_path {
            if path.exists() {
                info!("Loading configuration from: {:?}", path);
                config = self.load_from_file(path)?;
                debug!("Successfully loaded configuration from specified path");
            } else {
                warn!("Configuration file not found: {:?}, using defaults", path);
            }
        } else {
            // Try default config paths
            debug!("No config path specified, trying default locations");
            let mut loaded = false;
            for default_path in &["config.toml", "config/config.toml", ".config/config.toml"] {
                let path = Path::new(default_path);
                if path.exists() {
                    info!("Loading configuration from default path: {:?}", path);
                    config = self.load_from_file(path)?;
                    loaded = true;
                    break;
                }
            }
            if !loaded {
                debug!("No configuration file found in default locations, using defaults");
            }
        }

        // Resolve environment variable placeholders in config values
        debug!("Resolving environment variable placeholders");
        env_loader::resolve_config_placeholders(&mut config);

        // Set provider IDs from hashmap keys (required because #[serde(skip)] on id field)
        debug!("Setting provider IDs from configuration keys");
        for (provider_id, provider) in config.llm.providers.iter_mut() {
            provider.id = provider_id.clone();
        }

        // Load API keys from file if api_key_file is specified
        debug!("Checking for API key files");
        env_loader::resolve_llm_api_key_file(&mut config)?;

        // Override with environment variables
        debug!("Applying environment variable overrides");
        env_loader::apply_env_vars(&mut config)?;

        // Validate configuration
        debug!("Validating configuration");
        self.validate(&config)?;

        // Validate and resolve feature dependencies
        let dependency_messages = config.validate_and_resolve_dependencies();
        if !dependency_messages.is_empty() {
            debug!(
                message_count = dependency_messages.len(),
                "Configuration dependency resolution completed"
            );
        }

        info!("Configuration loading completed successfully");
        Ok(config)
    }

    /// Load configuration for a specific project
    ///
    /// This method loads global configuration and merges it with project-specific
    /// configuration found in the project root directory.
    ///
    /// # Arguments
    ///
    /// * `project_root` - Path to the project root directory
    ///
    /// # Returns
    ///
    /// Merged configuration with project overrides applied to global defaults.
    pub fn load_for_project(self, project_root: &Path) -> Result<AppConfig, ConfigError> {
        info!(
            project_root = %project_root.display(),
            "Starting project configuration loading"
        );

        // First, load global configuration
        let global_config = self.load()?;
        debug!("Loaded global configuration");

        // Find project configuration file
        let project_config_path = ProjectConfigPaths::find_project_config(project_root);

        if let Some(config_path) = project_config_path {
            info!("Loading project configuration from: {:?}", config_path);

            // Load project config
            let project_config = Self::load_project_config_static(&config_path)?;
            debug!("Successfully loaded project configuration");

            // Merge with global config
            let mut merged_config = global_config.merge_with_project(&project_config);
            debug!("Merged project configuration with global defaults");

            // Check for local override config
            if let Some(local_config_path) = ProjectConfigPaths::find_local_config(&config_path) {
                info!(
                    "Loading local project configuration from: {:?}",
                    local_config_path
                );
                let local_config = Self::load_project_config_static(&local_config_path)?;
                merged_config = merged_config.merge_with_project(&local_config);
                debug!("Applied local configuration overrides");
            }

            // Re-apply environment variables (API keys, DB URLs, logging only)
            // Infrastructure settings (host, port) are not overridable via env vars
            debug!("Re-applying environment variable overrides");
            env_loader::apply_env_vars(&mut merged_config)?;

            // Validate merged configuration
            debug!("Validating merged configuration");
            Self::validate_static(&merged_config)?;

            info!("Project configuration loading completed successfully");
            Ok(merged_config)
        } else {
            info!("No project configuration found, using global config only");
            Ok(global_config)
        }
    }

    /// Load project-level configuration from a TOML file (static version)
    fn load_project_config_static(path: &Path) -> Result<ProjectAppConfig, ConfigError> {
        debug!("Reading project config file: {:?}", path);
        let content = std::fs::read_to_string(path).map_err(|e| {
            ConfigError::Other(format!("Failed to read project config file: {}", e))
        })?;

        debug!(
            content_length = content.len(),
            "Parsing project config TOML"
        );
        let result = toml::from_str(&content)
            .map_err(|e| ConfigError::Other(format!("Failed to parse project config file: {}", e)));

        if result.is_ok() {
            debug!("Successfully parsed project config file");
        }

        result
    }

    /// Load project-level configuration from a TOML file
    ///
    /// This method loads only the project configuration without merging with global config.
    /// Useful for testing or when you need to inspect project config separately.
    pub fn load_project_config(self, path: &Path) -> Result<ProjectAppConfig, ConfigError> {
        Self::load_project_config_static(path)
    }

    /// Validate configuration (static version)
    fn validate_static(config: &AppConfig) -> Result<(), ConfigError> {
        config.validate_structured()?;
        Ok(())
    }

    /// Load configuration from a TOML file
    fn load_from_file(&self, path: &Path) -> Result<AppConfig, ConfigError> {
        debug!("Reading configuration file: {:?}", path);
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Other(format!("Failed to read config file: {}", e)))?;

        debug!(content_length = content.len(), "Parsing configuration TOML");
        let result = toml::from_str(&content)
            .map_err(|e| ConfigError::Other(format!("Failed to parse config file: {}", e)));

        if result.is_ok() {
            debug!("Successfully parsed configuration file");
        }

        result
    }

    /// Validate configuration
    fn validate(&self, config: &AppConfig) -> Result<(), ConfigError> {
        config.validate_structured()?;
        Ok(())
    }

    /// Generate a default configuration file
    pub fn generate_default_config() -> Result<String, ConfigError> {
        let config = AppConfig::default();
        toml::to_string_pretty(&config)
            .map_err(|e| ConfigError::Other(format!("Failed to serialize config: {}", e)))
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        // Clear any existing env vars that might affect the test
        unsafe {
            std::env::remove_var("SERVER_HOST");
            std::env::remove_var("SERVER_PORT");
            std::env::remove_var("LLM_API_KEY");
            std::env::remove_var("LLM_BASE_URL");
            std::env::remove_var("LLM_PROVIDER");
            std::env::remove_var("LLM_MODEL");
        }

        // Load config.minimal.toml which has a valid embedder configuration
        let loader = ConfigLoader::new();
        let config_path = Path::new("config.minimal.toml");

        if config_path.exists() {
            let config = loader
                .with_path(config_path)
                .load()
                .expect("Failed to load config.minimal.toml");
            assert!(!config.server.host.is_empty());
            assert!(config.server.port > 0);
            // Verify embedder is configured
            assert!(!config.llm.providers.is_empty());
            assert!(!config.embedder.default_model.is_empty());
        } else {
            // If config.minimal.toml doesn't exist, skip this test
            eprintln!("Skipping test: config.minimal.toml not found");
        }
    }

    #[test]
    fn test_generate_default_config() {
        let toml_str = ConfigLoader::generate_default_config().expect("Failed to generate");
        assert!(toml_str.contains("[server]"));
        // Database section is nested, so it appears as [database.qdrant], [database.sqlite], etc.
        assert!(toml_str.contains("[database.qdrant]") || toml_str.contains("[database.sqlite]"));
    }

    #[test]
    fn test_load_project_config() {
        let project_config_toml = r#"
name = "test-project"

[scanner]
follow_symlinks = false
respect_gitignore = true
exclude_patterns = ["node_modules", "dist"]
include_patterns = []
gitignore_patterns = []
binary_check_size = 8192
max_hash_file_size = 10485760
default_max_content_size = 1048576
max_file_size = 512000

[orchestrator.indexer]
extensions = ["ts", "tsx", "js", "jsx"]
exclude_dirs = ["node_modules", "dist"]
batch_size = 100
max_concurrency = 10
store_vectors = true
store_bm25 = true
store_summaries = true
build_relations = true

[relation.index]
filter_stdlib_calls = true
"#;

        let config: ProjectAppConfig =
            toml::from_str(project_config_toml).expect("Failed to parse project config");

        assert_eq!(config.name, Some("test-project".to_string()));
        assert!(config.scanner.is_some());
        assert!(config.orchestrator.is_some());

        let scanner = config.scanner.as_ref().expect("Scanner should be present");
        assert_eq!(scanner.exclude_patterns, vec!["node_modules", "dist"]);
    }

    #[test]
    fn test_load_for_project_no_config() {
        // Test loading for a project without project config
        // Should return global config from config.minimal.toml
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

        let config_path = Path::new("config.minimal.toml");
        if !config_path.exists() {
            eprintln!("Skipping test: config.minimal.toml not found");
            return;
        }

        // Load with config.minimal.toml as the global config
        let config = ConfigLoader::new()
            .with_path(config_path)
            .load_for_project(temp_dir.path())
            .expect("Failed to load config");

        // Should have valid config
        assert!(!config.server.host.is_empty());
        assert!(config.server.port > 0);
        // Verify embedder is configured
        assert!(!config.llm.providers.is_empty());
        assert!(!config.embedder.default_model.is_empty());
    }
}
