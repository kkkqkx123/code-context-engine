//! Configuration loader
//!
//! This module provides functionality for loading global configuration from:
//! - TOML files
//! - Environment variables
//! - .env files
//! - Default values
//!
//! # Loading Priority (highest to lowest)
//!
//! 1. Configuration file (highest for infrastructure settings: host, port, db path)
//! 2. Environment variables (CCE_*) for API keys, DB URLs, logging, and server bind fallback
//! 3. .env file variables (provides defaults for sensitive values)
//! 4. Global configuration file (config.toml)
//! 5. Default values
//!
//! # Responsibility boundary
//!
//! This loader owns only the global server configuration. Project-level
//! effective configuration is built by the project registry from the global
//! configuration plus the project record in the metadata database. The
//! server never discovers or reads project configuration files.
//!
//! # Infrastructure Settings Policy
//!
//! File-explicit server host/port win; env vars only fill defaults.
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
use super::validation::Validate;
use cce_types::error::ConfigError;

/// Configuration loader
pub struct ConfigLoader {
    /// Configuration file path
    config_path: Option<PathBuf>,
}

impl ConfigLoader {
    /// Default candidate files probed when no explicit path is given.
    ///
    /// Shared by the loader and the global singleton so every entry
    /// point resolves the same sequence.
    pub fn default_config_candidates() -> Vec<PathBuf> {
        ["config.toml", "config/config.toml", ".config/config.toml"]
            .iter()
            .map(PathBuf::from)
            .collect()
    }

    /// Anchor relative filesystem paths to the config file directory.
    ///
    /// Absolute paths, in-memory markers and URLs are left untouched.
    /// Must run right after file parsing and before env overrides so
    /// explicit environment values keep working-directory semantics.
    pub fn anchor_relative_paths(config: &mut AppConfig, base_dir: &Path) {
        fn anchored(value: &str, base_dir: &Path) -> Option<String> {
            let trimmed = value.trim();
            if trimmed.is_empty()
                || trimmed == ":memory:"
                || Path::new(trimmed).is_absolute()
                || trimmed.contains("://")
            {
                return None;
            }
            Some(base_dir.join(trimmed).to_string_lossy().to_string())
        }

        if base_dir.as_os_str().is_empty() {
            return;
        }
        if let Some(next) = anchored(&config.database.sqlite.path, base_dir) {
            config.database.sqlite.path = next;
        }
        for slot in [
            &mut config.database.vector_local.data_dir,
            &mut config.database.qdrant.data_dir,
            &mut config.database.qdrant.binary_path,
            &mut config.database.bm25.index_path,
            &mut config.logger.file,
        ] {
            if let Some(current) = slot.as_ref()
                && let Some(next) = anchored(current, base_dir)
            {
                *slot = Some(next);
            }
        }
        for provider in config.llm.providers.values_mut() {
            if let Some(ref current) = provider.api_key_file
                && let Some(next) = anchored(current, base_dir)
            {
                provider.api_key_file = Some(next);
            }
        }
    }

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
    /// File-explicit server host/port win; env vars only fill defaults.
    pub fn load(self) -> Result<AppConfig, ConfigError> {
        info!("Starting configuration loading process");

        // Start with default configuration
        let mut config = AppConfig::default();

        debug!("Initialized with default configuration");

        // Load .env file first (so env vars are available for placeholder resolution)
        env_loader::load_dotenv();
        debug!("Loaded .env file (if exists)");

        // Load from file if specified
        let mut loaded_path: Option<PathBuf> = None;
        if let Some(path) = &self.config_path {
            if path.exists() {
                info!("Loading configuration from: {:?}", path);
                config = self.load_from_file(path)?;
                loaded_path = Some(path.clone());
                debug!("Successfully loaded configuration from specified path");
            } else {
                warn!("Configuration file not found: {:?}, using defaults", path);
            }
        } else {
            // Try default config paths
            debug!("No config path specified, trying default locations");
            for default_path in Self::default_config_candidates() {
                let path = Path::new(&default_path);
                if path.exists() {
                    info!("Loading configuration from default path: {:?}", path);
                    config = self.load_from_file(path)?;
                    loaded_path = Some(default_path);
                    break;
                }
            }
            if loaded_path.is_none() {
                debug!("No configuration file found in default locations, using defaults");
            }
        }

        if let Some(ref source) = loaded_path
            && let Some(parent) = source.parent()
            && !parent.as_os_str().is_empty()
        {
            Self::anchor_relative_paths(&mut config, parent);
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
        config.validate_for_current_environment()?;
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
    use crate::project::ProjectAppConfig;

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
    fn test_shipped_sample_configs_parse() {
        // The shipped sample configurations must stay loadable: a partial
        // table is only accepted when every field carries a documented
        // default, so a sample that omits a field would break the loader.
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let samples = [
            "config.toml",
            "config.example.toml",
            ".cce/.cce.example.toml",
            ".cce/.cce.minimal.toml",
        ];
        for sample in samples {
            let path = repo_root.join(sample);
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {sample}: {e}"));
            toml::from_str::<AppConfig>(&content)
                .unwrap_or_else(|e| panic!("{sample} must parse: {e}"));
        }
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
        assert_eq!(
            scanner.exclude_patterns,
            Some(vec!["node_modules".to_string(), "dist".to_string()])
        );
    }
}
