//! Index manager for BM25 search

use std::path::Path;

use crate::Bm25Error;
use crate::config::{Bm25AlgorithmConfig, IndexManagerConfig};
use cce_text::MixedTokenizer;
use tantivy::index::Bm25Params;
use tantivy::{Index, IndexBuilder, IndexReader, IndexSettings, IndexWriter, ReloadPolicy};

use crate::schema::IndexSchema;

/// BM25 index on-disk format version.
pub const BM25_FORMAT_VERSION: u32 = 1;

const FORMAT_VERSION_FILE: &str = "format_version.json";

/// Index manager for Tantivy-based BM25 search
#[derive(Clone)]
pub struct IndexManager {
    index: Index,
    schema: IndexSchema,
    config: IndexManagerConfig,
    reader: Option<IndexReader>,
}

impl IndexManager {
    /// Create a new index at the specified path with default config
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self, Bm25Error> {
        Self::create_with_config(
            path,
            IndexManagerConfig::default(),
            Bm25AlgorithmConfig::default(),
        )
    }

    /// Create a new index at the specified path with custom config
    pub fn create_with_config<P: AsRef<Path>>(
        path: P,
        config: IndexManagerConfig,
        algorithm: Bm25AlgorithmConfig,
    ) -> Result<Self, Bm25Error> {
        let path = path.as_ref();
        std::fs::create_dir_all(path)?;
        let schema = IndexSchema::new();
        let tantivy_schema = schema.schema().clone();

        let bm25_params = Bm25Params {
            k1: algorithm.k1,
            b: algorithm.b,
        };

        let index_settings = IndexSettings {
            bm25_params: Some(bm25_params),
            ..Default::default()
        };

        let index = IndexBuilder::new()
            .schema(tantivy_schema)
            .settings(index_settings)
            .create_in_dir(path)?;

        index
            .tokenizers()
            .register("mixed", MixedTokenizer::default());
        let reader = Self::create_reader(&index, &config)?;

        Self::write_format_version(path)?;

        Ok(Self {
            index,
            schema,
            config,
            reader,
        })
    }

    /// Open an existing index at the specified path with default config
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Bm25Error> {
        Self::open_with_config(path, IndexManagerConfig::default())
    }

    /// Open an existing index at the specified path with custom config
    pub fn open_with_config<P: AsRef<Path>>(
        path: P,
        config: IndexManagerConfig,
    ) -> Result<Self, Bm25Error> {
        let path = path.as_ref();
        Self::check_format_version(path)?;

        let index = Index::open_in_dir(path)?;
        let schema = IndexSchema::from_tantivy_schema(&index.schema())?;

        index
            .tokenizers()
            .register("mixed", MixedTokenizer::default());
        let reader = Self::create_reader(&index, &config)?;

        Ok(Self {
            index,
            schema,
            config,
            reader,
        })
    }

    /// Read the stored format version of an index directory.
    pub fn read_format_version(path: &Path) -> Result<Option<u32>, Bm25Error> {
        let file = path.join(FORMAT_VERSION_FILE);
        if !file.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&file)?;
        let value: u32 = content
            .trim()
            .parse()
            .map_err(|e| Bm25Error::Schema(format!("invalid format version: {e}")))?;
        Ok(Some(value))
    }

    fn check_format_version(path: &Path) -> Result<(), Bm25Error> {
        match Self::read_format_version(path)? {
            Some(v) if v == BM25_FORMAT_VERSION => Ok(()),
            Some(v) => Err(Bm25Error::Schema(format!(
                "BM25 index format version mismatch: expected {BM25_FORMAT_VERSION}, found {v}"
            ))),
            None => Err(Bm25Error::Schema(
                "BM25 index has no format version marker (legacy index)".to_string(),
            )),
        }
    }

    fn write_format_version(path: &Path) -> Result<(), Bm25Error> {
        let file = path.join(FORMAT_VERSION_FILE);
        std::fs::write(&file, BM25_FORMAT_VERSION.to_string())?;
        Ok(())
    }

    /// Check whether the index directory's format version is compatible.
    pub fn is_compatible(path: &Path) -> bool {
        Self::read_format_version(path)
            .map(|v| v == Some(BM25_FORMAT_VERSION))
            .unwrap_or(false)
    }

    /// Versioned index directory under an index root path.
    pub fn versioned_path(root: &Path) -> std::path::PathBuf {
        root.join(format!("i{}", cce_types::INDEX_FORMAT_VERSION))
    }

    /// Get an index writer with configured memory budget
    pub fn writer(&self) -> Result<IndexWriter, Bm25Error> {
        Ok(self.index.writer(self.config.writer_memory_budget)?)
    }

    /// Get an index reader with configured reload policy
    pub fn reader(&self) -> Result<IndexReader, Bm25Error> {
        if let Some(reader) = &self.reader {
            return Ok(reader.clone());
        }

        Self::build_reader(&self.index, &self.config)
    }

    /// Refresh the cached reader after a successful index commit.
    pub fn reload_reader(&self) -> Result<(), Bm25Error> {
        if let Some(reader) = &self.reader {
            reader.reload()?;
        }
        Ok(())
    }

    fn create_reader(
        index: &Index,
        config: &IndexManagerConfig,
    ) -> Result<Option<IndexReader>, Bm25Error> {
        if config.reader_cache_enabled {
            Ok(Some(Self::build_reader(index, config)?))
        } else {
            Ok(None)
        }
    }

    fn build_reader(index: &Index, config: &IndexManagerConfig) -> Result<IndexReader, Bm25Error> {
        let reload_policy = match config.reload_policy.as_str() {
            "on_commit" | "on_commit_with_delay" => ReloadPolicy::OnCommitWithDelay,
            "manual" => ReloadPolicy::Manual,
            _ => ReloadPolicy::OnCommitWithDelay,
        };

        let builder = index.reader_builder().reload_policy(reload_policy);

        Ok(builder.try_into()?)
    }

    /// Get the index schema
    pub fn schema(&self) -> &IndexSchema {
        &self.schema
    }

    /// Get the underlying Tantivy index
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// Get the index manager configuration
    pub fn config(&self) -> &IndexManagerConfig {
        &self.config
    }
}
