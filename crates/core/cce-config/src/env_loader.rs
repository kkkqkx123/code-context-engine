//! Environment variable loader for sensitive configuration
//!
//! Provides functionality to load API keys and other sensitive configuration
//! from environment variables and .env files.
//!
//! # Configuration Priority
//!
//! 1. Direct environment variables
//! 2. `.env` file in project root
//! 3. `.env` file in current working directory
//!
//! # Environment Variables
//!
//! ## Infrastructure (set in config.toml, NOT overrideable via env vars)
//! - `CCE_SERVER_HOST` - Server host address (use config.toml)
//! - `CCE_SERVER_PORT` - Server port number (use config.toml)
//!
//! ## Database
//! - `CCE_DB_QDRANT_URL` - Qdrant server URL
//! - `CCE_DB_QDRANT_API_KEY` - Qdrant API key (optional, can use placeholder in config.toml)
//! - `CCE_DB_SQLITE_PATH` - SQLite database path
//! - `CCE_DB_SQLITE_SYNC` - SQLite sync mode (OFF/NORMAL/FULL/EXTRA)
//! - `CCE_DB_SQLITE_CACHE_SIZE` - SQLite cache size in KB
//! - `CCE_DB_SQLITE_BUSY_TIMEOUT` - SQLite busy timeout in ms
//! - `CCE_DB_SQLITE_MMAP_SIZE` - SQLite mmap size in bytes
//!
//! ## Embedder (Multi-Provider Support)
//!
//! Use placeholder syntax in config.toml:
//! ```toml
//! [llm.providers.siliconflow]
//! api_keys = ["${CCE_EMB_API_KEY_SILICONFLOW}"]
//! ```
//!
//! Environment variables:
//! - `CCE_EMB_API_KEY_{PROVIDER_ID}` - API key for specific provider (e.g., CCE_EMB_API_KEY_SILICONFLOW, CCE_EMB_API_KEY_OPENAI)
//!
//! ## LLM (Multi-Provider Support)
//!
//! Use placeholder syntax in config.toml:
//! ```toml
//! [llm.providers.siliconflow]
//! api_keys = ["${CCE_LLM_API_KEY_SILICONFLOW}"]
//! ```
//!
//! Environment variables:
//! - `CCE_LLM_API_KEY_{PROVIDER_ID}` - API key for specific provider (e.g., CCE_LLM_API_KEY_SILICONFLOW, CCE_LLM_API_KEY_OPENAI)
//!
//! ## Logger
//! - `CCE_LOG_LEVEL` - Log level (trace/debug/info/warn/error)
//! - `CCE_LOG_OUTPUT` - Log output (stdout/stderr/file)
//! - `CCE_LOG_FORMAT` - Log format (pretty/compact/json)
//! - `CCE_LOG_FILE` - Log file path
//!
//! # Migration Guide
//!
//! See `docs/config/env-refactor.md` for detailed migration instructions for
//! the unified provider registry configuration.

use tracing::{debug, info, warn};

use crate::global::AppConfig;
use cce_types::error::ConfigError;

/// Load .env file if it exists
///
/// Tries to load from the following locations in order:
/// 1. Project root (where Cargo.toml or config.toml exists)
/// 2. Current working directory
pub fn load_dotenv() {
    let candidates = find_dotenv_candidates();

    for path in &candidates {
        if path.exists() {
            match dotenvy::from_path(path) {
                Ok(()) => {
                    info!("Loaded environment variables from: {}", path.display());
                }
                Err(e) => {
                    warn!("Failed to load .env from {}: {}", path.display(), e);
                }
            }
        }
    }

    if candidates.iter().all(|p| !p.exists()) {
        debug!("No .env file found in any candidate location");
    }
}

/// Find candidate paths for .env files
fn find_dotenv_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();

    // Try project root (look for config.toml or Cargo.toml)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            // Check executable directory
            candidates.push(parent.join(".env"));

            // Check parent directories for config.toml or Cargo.toml
            let mut current = Some(parent);
            while let Some(dir) = current {
                if dir.join("config.toml").exists() || dir.join("Cargo.toml").exists() {
                    candidates.push(dir.join(".env"));
                    break;
                }
                current = dir.parent();
            }
        }
    }

    // Try current working directory
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(".env"));
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

/// Apply environment variables to configuration
///
/// This function applies all environment variables to the configuration.
/// It should be called after loading the .env file.
///
/// # Override Policy
///
/// Only the following settings are overridable via environment variables:
/// - API keys (sensitive, never stored in config files)
/// - Database URLs (for environment-specific targets)
/// - Logging settings (for runtime debugging)
///
/// Infrastructure settings (server host/port) are NOT overridable via env vars.
/// They must be configured in config.toml, which serves as the single source
/// of truth for deployment topology.
pub fn apply_env_vars(config: &mut AppConfig) -> Result<(), ConfigError> {
    apply_database_env_vars(config)?;
    apply_logger_env_vars(config)?;

    Ok(())
}

/// Apply database-related environment variables
fn apply_database_env_vars(config: &mut AppConfig) -> Result<(), ConfigError> {
    use crate::global::SqliteSyncMode;

    if let Ok(val) = std::env::var("CCE_DB_QDRANT_URL") {
        config.database.qdrant.url = val;
    }
    if let Ok(val) = std::env::var("CCE_DB_QDRANT_API_KEY") {
        config.database.qdrant.api_key = Some(val);
    }

    // SQLite configuration
    if let Ok(val) = std::env::var("CCE_DB_SQLITE_PATH") {
        config.database.sqlite.path = val;
    }
    if let Ok(val) = std::env::var("CCE_DB_SQLITE_SYNC") {
        config.database.sqlite.synchronous = match val.to_uppercase().as_str() {
            "OFF" => SqliteSyncMode::Off,
            "NORMAL" => SqliteSyncMode::Normal,
            "FULL" => SqliteSyncMode::Full,
            "EXTRA" => SqliteSyncMode::Extra,
            _ => {
                return Err(ConfigError::invalid_env_var(
                    "CCE_DB_SQLITE_SYNC_MODE",
                    format!("invalid sync mode: {}", val),
                ));
            }
        };
    }
    if let Ok(val) = std::env::var("CCE_DB_SQLITE_CACHE_SIZE") {
        config.database.sqlite.cache_size = val.parse().map_err(|_| {
            ConfigError::invalid_env_var("CCE_DB_SQLITE_CACHE_SIZE", "invalid value")
        })?;
    }
    if let Ok(val) = std::env::var("CCE_DB_SQLITE_BUSY_TIMEOUT") {
        config.database.sqlite.busy_timeout_ms = val.parse().map_err(|_| {
            ConfigError::invalid_env_var("CCE_DB_SQLITE_BUSY_TIMEOUT", "invalid value")
        })?;
    }
    if let Ok(val) = std::env::var("CCE_DB_SQLITE_MMAP_SIZE") {
        config.database.sqlite.mmap_size = val.parse().map_err(|_| {
            ConfigError::invalid_env_var("CCE_DB_SQLITE_MMAP_SIZE", "invalid value")
        })?;
    }

    Ok(())
}

/// Apply logger-related environment variables
fn apply_logger_env_vars(config: &mut AppConfig) -> Result<(), ConfigError> {
    use crate::global::{LogFormat, LogLevel, LogOutput};

    if let Ok(val) = std::env::var("CCE_LOG_LEVEL") {
        config.logger.level = match val.to_lowercase().as_str() {
            "trace" => LogLevel::Trace,
            "debug" => LogLevel::Debug,
            "info" => LogLevel::Info,
            "warn" => LogLevel::Warn,
            "error" => LogLevel::Error,
            _ => {
                return Err(ConfigError::invalid_env_var(
                    "CCE_LOG_LEVEL",
                    format!("invalid log level: {}", val),
                ));
            }
        };
    }
    if let Ok(val) = std::env::var("CCE_LOG_OUTPUT") {
        config.logger.output = match val.to_lowercase().as_str() {
            "stdout" => LogOutput::Stdout,
            "stderr" => LogOutput::Stderr,
            "file" => LogOutput::File,
            _ => {
                return Err(ConfigError::invalid_env_var(
                    "CCE_LOG_OUTPUT",
                    format!("invalid log output: {}", val),
                ));
            }
        };
    }
    if let Ok(val) = std::env::var("CCE_LOG_FORMAT") {
        config.logger.format = match val.to_lowercase().as_str() {
            "pretty" => LogFormat::Pretty,
            "compact" => LogFormat::Compact,
            "json" => LogFormat::Json,
            _ => {
                return Err(ConfigError::invalid_env_var(
                    "CCE_LOG_FORMAT",
                    format!("invalid log format: {}", val),
                ));
            }
        };
    }
    if let Ok(val) = std::env::var("CCE_LOG_FILE") {
        config.logger.file = Some(val);
    }
    Ok(())
}

/// Resolve environment variable placeholders in a string
///
/// Supports `${VAR_NAME}` syntax. If the variable is not set,
/// the placeholder is left as-is.
///
/// # Examples
///
/// ```
/// unsafe { std::env::set_var("TEST_API_KEY", "sk-test") };
/// let result = cce_core::config::env_loader::resolve_env_placeholders("${TEST_API_KEY}");
/// assert_eq!(result, "sk-test");
/// unsafe { std::env::remove_var("TEST_API_KEY") };
/// ```
pub fn resolve_env_placeholders(input: &str) -> String {
    let mut result = input.to_string();
    let mut start = 0;

    while let Some(pos) = result[start..].find("${") {
        let var_start = start + pos;
        if let Some(end) = result[var_start..].find('}') {
            let var_end = var_start + end;
            let var_name = &result[var_start + 2..var_end];

            if let Ok(val) = std::env::var(var_name) {
                result.replace_range(var_start..=var_end, &val);
                // Continue from the same position since the string changed
                continue;
            }
        }
        start = var_start + 2;
    }

    result
}

/// Resolve environment variable placeholders in API keys
pub fn resolve_api_keys(keys: &[String]) -> Vec<String> {
    keys.iter().map(|k| resolve_env_placeholders(k)).collect()
}

/// Load API key from file if api_key_file is specified
///
/// If the file exists and can be read, the content (trimmed) is used as the API key.
/// If the file doesn't exist or can't be read, an error is returned.
pub fn load_api_key_from_file(file_path: &str) -> Result<String, ConfigError> {
    let content = std::fs::read_to_string(file_path).map_err(|e| {
        ConfigError::Other(format!(
            "Failed to read API key file '{}': {}",
            file_path, e
        ))
    })?;
    Ok(content.trim().to_string())
}

/// Resolve API keys from file for LLM providers
///
/// If api_key_file is specified in any provider, loads the key from file.
pub fn resolve_llm_api_key_file(config: &mut AppConfig) -> Result<(), ConfigError> {
    for (_provider_id, provider) in config.llm.providers.iter_mut() {
        if let Some(ref file_path) = provider.api_key_file {
            let key = load_api_key_from_file(file_path)?;
            if !key.is_empty() {
                provider.api_keys.push(key);
                debug!(
                    "Loaded LLM API key for provider '{}' from file: {}",
                    _provider_id, file_path
                );
            }
        }
    }
    Ok(())
}

/// Validate that all required environment variables are set
///
/// Checks configuration for environment variable placeholders and ensures
/// they are properly set in the environment.
///
/// # Arguments
/// * `config` - The application configuration to validate
///
/// # Returns
/// * `Ok(())` if all required environment variables are set
/// * `Err(ConfigError)` if any required variable is missing
pub fn validate_required_env_vars(config: &AppConfig) -> Result<(), ConfigError> {
    let mut missing_vars: Vec<String> = Vec::new();

    // Note: Embedder providers are now managed through llm.embedding_models and llm.providers
    // API key validation for embedding is handled through the LLM provider configuration

    // Check LLM providers
    for (_provider_id, provider) in &config.llm.providers {
        for key_ref in &provider.api_keys {
            if key_ref.starts_with("${") && key_ref.ends_with('}') {
                let var_name = &key_ref[2..key_ref.len() - 1];
                if std::env::var(var_name).is_err() {
                    missing_vars.push(format!("llm.provider.{}.{}", _provider_id, var_name));
                }
            }
        }
    }

    // Check database Qdrant API key
    if let Some(ref key_ref) = config.database.qdrant.api_key {
        if key_ref.starts_with("${") && key_ref.ends_with('}') {
            let var_name = &key_ref[2..key_ref.len() - 1];
            if std::env::var(var_name).is_err() {
                missing_vars.push(format!("database.qdrant.{}", var_name));
            }
        }
    }

    if !missing_vars.is_empty() {
        return Err(ConfigError::Other(format!(
            "Required environment variables not set: {}",
            missing_vars.join(", ")
        )));
    }

    Ok(())
}

/// Resolve environment variable placeholders in configuration strings
///
/// This function processes all string fields in the configuration that might
/// contain environment variable placeholders.
pub fn resolve_config_placeholders(config: &mut AppConfig) {
    // Resolve embedder configuration placeholders
    // Note: EmbedderConfig no longer has providers field - providers are in llm.providers
    // API keys and base_url are resolved when creating embedder from model config

    // Resolve LLM provider configurations
    for provider in config.llm.providers.values_mut() {
        provider.api_keys = resolve_api_keys(&provider.api_keys);
        provider.base_url = resolve_env_placeholders(&provider.base_url);
        if let Some(ref mut proxy) = provider.proxy_url {
            *proxy = resolve_env_placeholders(proxy);
        }
    }

    // Resolve database URL and API key
    config.database.qdrant.url = resolve_env_placeholders(&config.database.qdrant.url);
    if let Some(ref mut key) = config.database.qdrant.api_key {
        *key = resolve_env_placeholders(key);
    }

    // Resolve logger file path
    if let Some(ref mut file) = config.logger.file {
        *file = resolve_env_placeholders(file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_env_placeholders() {
        unsafe { std::env::set_var("TEST_KEY", "secret_value") };
        let result = resolve_env_placeholders("prefix-${TEST_KEY}-suffix");
        assert_eq!(result, "prefix-secret_value-suffix");
        unsafe { std::env::remove_var("TEST_KEY") };
    }

    #[test]
    fn test_resolve_env_placeholders_missing() {
        let result = resolve_env_placeholders("prefix-${MISSING_VAR}-suffix");
        assert_eq!(result, "prefix-${MISSING_VAR}-suffix");
    }

    #[test]
    fn test_resolve_api_keys() {
        unsafe {
            std::env::set_var("API_KEY_1", "key1");
            std::env::set_var("API_KEY_2", "key2");
        }
        let keys = vec![
            "${API_KEY_1}".to_string(),
            "${API_KEY_2}".to_string(),
            "static_key".to_string(),
        ];
        let resolved = resolve_api_keys(&keys);
        assert_eq!(resolved, vec!["key1", "key2", "static_key"]);
        unsafe {
            std::env::remove_var("API_KEY_1");
            std::env::remove_var("API_KEY_2");
        }
    }

    /// Test multiple provider API key resolution
    #[test]
    fn test_multiple_provider_api_keys() {
        use crate::modules::ProviderConfig;
        use std::collections::HashMap;

        let mut config = AppConfig::default();

        // Setup multiple providers with environment variable placeholders
        let mut providers = HashMap::new();
        providers.insert(
            "provider1".to_string(),
            ProviderConfig {
                id: "provider1".to_string(),
                name: "Provider 1".to_string(),
                api_keys: vec!["${PROVIDER1_API_KEY}".to_string()],
                base_url: "https://api.provider1.com".to_string(),
                ..Default::default()
            },
        );
        providers.insert(
            "provider2".to_string(),
            ProviderConfig {
                id: "provider2".to_string(),
                name: "Provider 2".to_string(),
                api_keys: vec!["${PROVIDER2_API_KEY}".to_string()],
                base_url: "https://api.provider2.com".to_string(),
                ..Default::default()
            },
        );

        config.llm.providers = providers;

        // Set environment variables
        unsafe {
            std::env::set_var("PROVIDER1_API_KEY", "key-from-env-1");
            std::env::set_var("PROVIDER2_API_KEY", "key-from-env-2");
        }

        resolve_config_placeholders(&mut config);

        // Verify both providers' API keys were resolved
        assert_eq!(
            config.llm.providers.get("provider1").unwrap().api_keys[0],
            "key-from-env-1"
        );
        assert_eq!(
            config.llm.providers.get("provider2").unwrap().api_keys[0],
            "key-from-env-2"
        );

        // Cleanup
        unsafe {
            std::env::remove_var("PROVIDER1_API_KEY");
            std::env::remove_var("PROVIDER2_API_KEY");
        }
    }

    /// Test missing environment variable validation
    #[test]
    fn test_missing_env_var_validation() {
        use crate::modules::ProviderConfig;
        use std::collections::HashMap;

        let mut config = AppConfig::default();

        // Setup provider with missing environment variable
        let mut providers = HashMap::new();
        providers.insert(
            "test-provider".to_string(),
            ProviderConfig {
                id: "test-provider".to_string(),
                name: "Test Provider".to_string(),
                api_keys: vec!["${MISSING_VAR_12345}".to_string()],
                base_url: "https://api.test.com".to_string(),
                ..Default::default()
            },
        );

        config.llm.providers = providers;

        // Ensure the env var doesn't exist
        unsafe {
            std::env::remove_var("MISSING_VAR_12345");
        }

        // Should return an error for missing required env var
        let result = validate_required_env_vars(&config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("MISSING_VAR_12345"));
    }

    /// Test non-sensitive config in TOML (not overridden by env vars)
    #[test]
    fn test_non_sensitive_config_not_overridden() {
        let mut config = AppConfig::default();

        // Set some non-sensitive values
        config.server.host = "localhost".to_string();
        config.server.port = 8080;
        config.embedder.default_model = "test-model".to_string();

        // Set environment variables that should NOT override these
        unsafe {
            std::env::set_var("CCE_SERVER_HOST", "should-not-override");
            std::env::set_var("CCE_SERVER_PORT", "9999");
        }

        // Non-sensitive config should not be overridden by env vars
        // (only sensitive data like API keys should be resolved from env vars)
        resolve_config_placeholders(&mut config);

        // Values should remain unchanged
        assert_eq!(config.server.host, "localhost");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.embedder.default_model, "test-model");

        // Cleanup
        unsafe {
            std::env::remove_var("CCE_SERVER_HOST");
            std::env::remove_var("CCE_SERVER_PORT");
        }
    }

    /// Test API key file loading for LLM providers (embedder uses same mechanism)
    #[test]
    fn test_llm_api_key_file_loading_for_embedder() {
        use crate::modules::ProviderConfig;
        use std::collections::HashMap;
        use std::io::Write;

        // Create a temporary file with API key
        let temp_dir = std::env::temp_dir();
        let key_file = temp_dir.join("test_embedder_api_key.txt");
        let mut file = std::fs::File::create(&key_file).expect("Should create temp file");
        writeln!(file, "embedder-secret-key-789").expect("Should write to temp file");
        drop(file);

        let mut config = AppConfig::default();

        // Setup provider with api_key_file field
        let mut providers = HashMap::new();
        providers.insert(
            "test-provider".to_string(),
            ProviderConfig {
                id: "test-provider".to_string(),
                name: "Test Provider".to_string(),
                api_keys: vec![], // Empty, will be loaded from file
                api_key_file: Some(key_file.to_str().unwrap().to_string()),
                base_url: "https://api.test.com".to_string(),
                ..Default::default()
            },
        );

        config.llm.providers = providers;

        // Load API keys from files
        resolve_llm_api_key_file(&mut config).expect("Should load API key from file");

        // Verify the API key was loaded from file
        assert_eq!(
            config.llm.providers.get("test-provider").unwrap().api_keys[0],
            "embedder-secret-key-789"
        );

        // Cleanup
        std::fs::remove_file(&key_file).ok();
    }

    /// Test API key file loading for LLM providers
    #[test]
    fn test_llm_api_key_file_loading() {
        use crate::modules::ProviderConfig;
        use std::io::Write;

        // Create a temporary file with API key
        let temp_dir = std::env::temp_dir();
        let key_file = temp_dir.join("test_llm_api_key.txt");
        let mut file = std::fs::File::create(&key_file).expect("Should create temp file");
        writeln!(file, "llm-secret-key-456").expect("Should write to temp file");
        drop(file);

        // Create config with api_key_file
        let mut config = AppConfig::default();
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "openai-gpt4".to_string(),
            ProviderConfig {
                id: "openai-gpt4".to_string(),
                name: "OpenAI GPT-4".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                api_keys: vec![],
                api_key_file: Some(key_file.to_str().unwrap().to_string()),
                ..Default::default()
            },
        );
        config.llm.providers = providers;

        // Resolve API keys from file
        resolve_llm_api_key_file(&mut config).expect("Should load API key from file");

        // Verify the key was loaded
        let provider = config.llm.providers.get("openai-gpt4").unwrap();
        assert_eq!(provider.api_keys.len(), 1);
        assert_eq!(provider.api_keys[0], "llm-secret-key-456");

        // Cleanup
        std::fs::remove_file(&key_file).ok();
    }
}
