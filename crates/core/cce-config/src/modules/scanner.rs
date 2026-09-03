//! Scanner configuration
//!
//! This module provides configuration for the file system scanner.

use serde::{Deserialize, Serialize};

/// Scanner configuration
///
/// Contains behavioral settings for file scanning (patterns, limits, options).
/// The scan target path should be specified via API parameters or project registration,
/// not in this configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScannerConfig {
    /// Whether to follow symbolic links
    pub follow_symlinks: bool,
    /// Whether to respect .gitignore files
    pub respect_gitignore: bool,
    /// Default exclude patterns
    pub exclude_patterns: Vec<String>,
    /// Include patterns (glob patterns)
    pub include_patterns: Vec<String>,
    /// Additional gitignore-style patterns (e.g., ["target/", "*.log"])
    pub gitignore_patterns: Vec<String>,
    /// Maximum bytes to check for binary detection (default: 8KB)
    pub binary_check_size: usize,
    /// Maximum file size to read entirely for hash computation in bytes (default: 10MB)
    pub max_hash_file_size: u64,
    /// Default maximum file size to read content in bytes (default: 1MB)
    pub default_max_content_size: u64,
    /// Maximum file size to process in bytes (default: 500KB), files larger than this will be skipped
    pub max_file_size: Option<u64>,
    /// Whether `FileFilter` plugins can make inclusion/exclusion decisions
    /// during scanning. Default off for performance and trust.
    pub plugin_filter_enabled: bool,
}

impl ScannerConfig {
    /// Default exclude patterns, derived from the canonical manifest scan
    /// exclusions plus IDE-specific directories.
    pub fn default_exclude_patterns() -> Vec<String> {
        let mut patterns: Vec<String> = cce_types::build_system::MANIFEST_SCAN_EXCLUDED_DIRS
            .iter()
            .map(|s| s.to_string())
            .collect();
        // IDE / editor directories not in manifest scan list
        for extra in [".idea", ".vs", ".vscode", ".env"] {
            if !patterns.contains(&extra.to_string()) {
                patterns.push(extra.to_string());
            }
        }
        patterns
    }
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            follow_symlinks: false,
            respect_gitignore: true,
            exclude_patterns: Self::default_exclude_patterns(),
            include_patterns: vec![],
            gitignore_patterns: vec![],
            binary_check_size: 8192,               // 8KB
            max_hash_file_size: 10 * 1024 * 1024,  // 10MB
            default_max_content_size: 1024 * 1024, // 1MB
            max_file_size: Some(500 * 1024),       // 500KB
            plugin_filter_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ScannerConfig::default();
        assert!(!config.follow_symlinks);
        assert!(config.respect_gitignore);
        assert_eq!(config.binary_check_size, 8192);
        assert!(config.max_file_size.is_some());
    }
}
