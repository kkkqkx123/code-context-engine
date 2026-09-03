//! Configuration version control for hot update
//!
//! This module provides version tracking for configuration files to prevent
//! old configurations from overwriting newer ones.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration version identifier
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigVersion {
    /// Configuration file path
    pub path: PathBuf,
    /// Last modification timestamp
    pub timestamp: DateTime<Utc>,
    /// Content hash (for detecting actual changes)
    pub content_hash: String,
}

impl ConfigVersion {
    /// Create a new config version
    pub fn new(path: PathBuf, content: &str) -> Self {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        Self {
            path,
            timestamp: Utc::now(),
            content_hash: hash,
        }
    }

    /// Check if this version is newer than another
    ///
    /// A change only counts when the content actually differs: a rewrite of
    /// identical content (e.g. a `touch` or editor save without edits) is not
    /// a real configuration change and must not re-trigger a reload.
    pub fn is_newer_than(&self, other: &ConfigVersion) -> bool {
        if self.content_hash == other.content_hash {
            return false;
        }
        self.timestamp >= other.timestamp
    }
}

/// Configuration version registry
#[derive(Debug, Default)]
pub struct ConfigVersionRegistry {
    versions: HashMap<PathBuf, ConfigVersion>,
}

impl ConfigVersionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update config version, returns true if this is a new version
    pub fn update(&mut self, version: ConfigVersion) -> bool {
        let path = version.path.clone();

        if let Some(existing) = self.versions.get(&path) {
            if version.is_newer_than(existing) {
                self.versions.insert(path, version);
                true
            } else {
                false
            }
        } else {
            self.versions.insert(path, version);
            true
        }
    }

    /// Get current version for a config file
    pub fn get(&self, path: &PathBuf) -> Option<&ConfigVersion> {
        self.versions.get(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_version_creation() {
        let path = PathBuf::from("Cargo.toml");
        let content = "[package]\nname = \"test\"";
        let version = ConfigVersion::new(path.clone(), content);

        assert_eq!(version.path, path);
        assert!(!version.content_hash.is_empty());
    }

    #[test]
    fn test_config_version_comparison() {
        let path = PathBuf::from("Cargo.toml");
        let old_content = "[package]\nname = \"test\"";
        let new_content = "[package]\nname = \"other\"";

        let v1 = ConfigVersion::new(path.clone(), old_content);
        // Sleep to ensure timestamp difference
        std::thread::sleep(std::time::Duration::from_millis(10));
        let v2 = ConfigVersion::new(path, new_content);

        assert!(v2.is_newer_than(&v1));
        assert!(!v1.is_newer_than(&v2));

        // Identical content is never newer, regardless of the timestamp.
        let v3 = ConfigVersion::new(PathBuf::from("Cargo.toml"), old_content);
        std::thread::sleep(std::time::Duration::from_millis(10));
        let v4 = ConfigVersion::new(PathBuf::from("Cargo.toml"), old_content);
        assert!(!v4.is_newer_than(&v3), "identical content must be rejected");
    }

    #[test]
    fn test_config_version_registry() {
        let mut registry = ConfigVersionRegistry::new();
        let path = PathBuf::from("Cargo.toml");
        let content = "[package]\nname = \"test\"";

        let version = ConfigVersion::new(path.clone(), content);

        // First update should succeed
        assert!(registry.update(version.clone()));

        // Second update with same version should fail
        assert!(!registry.update(version));

        // Should be able to get the version
        assert!(registry.get(&path).is_some());
    }
}
