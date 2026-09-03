//! Indexing options and configuration
//!
//! This module provides configuration options for the indexing process,
//! including file filtering, storage options, and performance tuning.

use cce_config::Settings;
use cce_types::error::ConfigError;
use std::path::PathBuf;

/// Indexing options
#[derive(Debug, Clone)]
pub struct IndexOptions {
    /// Root directory to index
    pub root_dir: PathBuf,
    /// File extensions to include
    pub extensions: Vec<String>,
    /// Directories to exclude
    pub exclude_dirs: Vec<String>,
    /// Maximum concurrent file processing
    pub max_concurrency: usize,
    /// Store in vector database
    pub store_vectors: bool,
    /// Store in BM25 index
    pub store_bm25: bool,
    /// Store file summaries
    pub store_summaries: bool,
    /// Build relation index
    pub build_relations: bool,
    /// Whether to respect .gitignore files
    pub respect_gitignore: bool,
    /// Additional ignore patterns (gitignore-style)
    pub additional_ignore_patterns: Vec<String>,
    /// Path to custom gitignore file
    pub custom_gitignore_path: Option<PathBuf>,
}

impl Default for IndexOptions {
    fn default() -> Self {
        // Delegate to IndexerConfig for canonical default values,
        // avoiding duplication between two sources of truth.
        let config = cce_config::modules::IndexerConfig::default();
        Self {
            root_dir: PathBuf::from("."),
            extensions: config.extensions,
            exclude_dirs: config.exclude_dirs,
            max_concurrency: 10,
            store_vectors: config.store_vectors,
            store_bm25: config.store_bm25,
            store_summaries: config.store_summaries,
            build_relations: config.build_relations,
            respect_gitignore: true,
            additional_ignore_patterns: Vec::new(),
            custom_gitignore_path: None,
        }
    }
}

impl IndexOptions {
    /// Create options populated from initialized Settings
    ///
    /// Returns an error if Settings has not been initialized.
    pub fn from_settings(root_dir: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        let indexer_config = Settings::indexer()?;
        let batch_config = &Settings::orchestrator()?.batch;
        let scanner_config = &Settings::scanner()?;
        Ok(Self {
            root_dir: root_dir.into(),
            extensions: indexer_config.extensions.clone(),
            exclude_dirs: indexer_config.exclude_dirs.clone(),
            max_concurrency: batch_config.process_concurrency,
            store_vectors: indexer_config.store_vectors,
            store_bm25: indexer_config.store_bm25,
            store_summaries: indexer_config.store_summaries,
            build_relations: indexer_config.build_relations,
            respect_gitignore: scanner_config.respect_gitignore,
            additional_ignore_patterns: scanner_config.gitignore_patterns.clone(),
            custom_gitignore_path: None,
        })
    }

    /// Create options for a root directory
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
            ..Default::default()
        }
    }

    /// Set file extensions
    pub fn with_extensions(mut self, extensions: Vec<String>) -> Self {
        self.extensions = extensions;
        self
    }

    /// Set exclude directories
    pub fn with_exclude_dirs(mut self, dirs: Vec<String>) -> Self {
        self.exclude_dirs = dirs;
        self
    }

    /// Set gitignore support
    pub fn with_gitignore(mut self, respect: bool) -> Self {
        self.respect_gitignore = respect;
        self
    }

    /// Set additional ignore patterns
    pub fn with_ignore_patterns(mut self, patterns: Vec<String>) -> Self {
        self.additional_ignore_patterns = patterns;
        self
    }

    /// Set custom gitignore file path
    pub fn with_custom_gitignore(mut self, path: impl Into<PathBuf>) -> Self {
        self.custom_gitignore_path = Some(path.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_options_builder() {
        let options = IndexOptions::new("/test/dir")
            .with_extensions(vec!["rs".to_string(), "py".to_string()])
            .with_exclude_dirs(vec!["target".to_string()]);

        assert_eq!(options.root_dir, PathBuf::from("/test/dir"));
        assert_eq!(options.extensions, vec!["rs", "py"]);
    }

    #[test]
    fn test_default_options() {
        let options = IndexOptions::default();
        assert!(!options.store_vectors);
        assert!(!options.store_bm25);
        assert!(options.build_relations);
    }
}
