//! Storage backend configuration for E2E tests

/// Storage backend configuration
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Use in-memory storage
    pub in_memory: bool,
    /// Local vector data dir. `None` (default) means a per-run temp directory,
    /// so no stale vectors leak across runs. Set explicitly only for heavy
    /// scenarios that need cross-run reuse.
    pub local_data_dir: Option<String>,
    /// Qdrant endpoint for remote-branch verification only. `None` (default)
    /// means the embedded local engine.
    pub qdrant_endpoint: Option<String>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self::in_memory()
    }
}

impl StorageConfig {
    /// Create in-memory storage configuration with the default local backend
    pub fn in_memory() -> Self {
        Self {
            in_memory: true,
            local_data_dir: None,
            qdrant_endpoint: None,
        }
    }

    /// Pin the local vector engine to an explicit data dir (cross-run reuse).
    pub fn with_local_data_dir(mut self, dir: impl Into<String>) -> Self {
        self.local_data_dir = Some(dir.into());
        self
    }

    /// Add a Qdrant endpoint for remote-branch verification
    pub fn with_qdrant(mut self, endpoint: impl Into<String>) -> Self {
        self.qdrant_endpoint = Some(endpoint.into());
        self
    }

    /// Check if the remote Qdrant backend is configured
    pub fn has_qdrant(&self) -> bool {
        self.qdrant_endpoint.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_config() {
        let config = StorageConfig::in_memory();
        assert!(config.in_memory);
        assert!(!config.has_qdrant());
        assert!(config.local_data_dir.is_none());
    }

    #[test]
    fn test_with_qdrant() {
        let config = StorageConfig::in_memory().with_qdrant("http://qdrant-remote:6333");
        assert!(config.has_qdrant());
    }

    #[test]
    fn test_with_local_data_dir() {
        let config = StorageConfig::in_memory().with_local_data_dir("/tmp/vectors");
        assert!(!config.has_qdrant());
        assert_eq!(config.local_data_dir.as_deref(), Some("/tmp/vectors"));
    }
}
