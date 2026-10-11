//! File processing logic
//!
//! This module handles processing of different file types,
//! including source code, documentation, and plain text files.
//!
//! # Caching
//!
//! FileProcessor uses an LRU cache to avoid reprocessing the same file content.
//! The cache size is configurable and defaults to 100 entries.

mod builder;
mod code;
mod document;

use cce_config::{ChunkingConfig, LicenseHeaderConfig};
use cce_metrics::{FileProcessingMetrics, ParserMetrics, PipelineStageMetrics};
use cce_parser::ast_to_nl::chunker::{ChunkedResult, GroupChunker};
use cce_parser::ast_to_nl::{AstToNlConverter, ConversionRequest};
use cce_parser::document::PipelineRouter;
use cce_parser::grouper::{PreprocessingPipeline, ProcessingResult};
use cce_parser::parser::ParseCoordinator;
use cce_plugin::PluginRegistry;
use cce_scanner::{
    FileContentPayload, FileEntry, read_verified_payload, read_verified_utf8,
    read_verified_utf8_for_entry,
};
use cce_types::error::ParseError;
use cce_types::{ContentRoute, LanguageInfo, OutputMode, ParsedFile};

use super::super::error::OrchestratorError;

pub(crate) type ChunkCache = Arc<RwLock<LruCache<String, (Vec<ChunkedResult>, usize)>>>;

use lru::LruCache;
use std::hash::{Hash, Hasher};

use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

/// Stable cache-key label for an output mode.
pub(crate) fn output_mode_label(mode: OutputMode) -> &'static str {
    match mode {
        OutputMode::Bm25 => "bm25",
        OutputMode::Embedding => "embedding",
        OutputMode::Both => "both",
    }
}

/// File processing result containing parsed file and chunked results
pub type FileProcessResult = (ParsedFile, Vec<ChunkedResult>);

/// Complete file processing result including pre-processor output
///
/// This struct contains all processing artifacts for a single file,
/// enabling downstream consumers (like summary generation) to access
/// pre-processor results such as entity groups and merged call patterns.
#[derive(Debug, Clone)]
pub struct CompleteFileProcessResult {
    /// Parsed file. Document-route files carry a placeholder with no
    /// entities; code files carry the tree-sitter parse result.
    pub parsed_file: ParsedFile,
    /// Chunked results from AstToNl + Chunker
    pub chunks: Vec<ChunkedResult>,
    /// Pre-processor result with entity groups
    pub processing_result: Option<ProcessingResult>,
    /// Document summary (for document files)
    pub doc_summary: Option<cce_parser::document::DocSummary>,
    /// Blank segments the chunker had to drop while producing `chunks`.
    /// Non-zero means indexed content is incomplete: the file must be
    /// reported as degraded even though processing succeeded.
    pub dropped_blank_segments: usize,
    /// Structured document pipeline degraded to plain text.
    pub document_degraded: bool,
    /// Tree-sitter reported syntax errors while parsing.
    /// Entities inside error regions may be missing, so the file counts
    /// as degraded even when entities and chunks are non-empty.
    pub has_syntax_errors: bool,
}

/// Chunk cache statistics
#[derive(Debug, Clone)]
pub struct ChunkCacheStats {
    /// Current number of entries in cache
    pub entries: usize,
    /// Maximum capacity of cache
    pub capacity: usize,
}

/// File processor for handling different file types
#[derive(Clone)]
pub struct FileProcessor {
    pub(crate) coordinator: Arc<Mutex<ParseCoordinator>>,
    pub(crate) pre_processor: Arc<PreprocessingPipeline>,
    pub(crate) converter: Arc<AstToNlConverter>,
    pub(crate) chunker: Arc<Mutex<GroupChunker>>,
    /// Document processing pipeline for non-code files
    pub(crate) doc_pipeline: Arc<PipelineRouter>,
    /// LRU cache for chunk results to avoid duplicate processing
    ///
    /// Key: project_id + file_path + source_hash
    /// Value: chunked results + blank segments dropped during chunking
    pub(crate) chunk_cache: ChunkCache,
    /// Chunking configuration for document/text processing
    pub(crate) chunking_config: ChunkingConfig,
    /// Document-specific chunking configuration (if None, uses chunking_config)
    pub(crate) document_chunking_config: Option<ChunkingConfig>,
    /// Hash of the AST-to-NL configuration. `pipeline_fingerprint()` folds
    /// this together with the parser pipeline version and the TextGen plugin
    /// set fingerprint to drive storage-module drift detection.
    pub(crate) pipeline_fingerprint: String,
    /// Plugin registry for NL template generation
    pub(crate) plugin_registry: Option<Arc<PluginRegistry>>,
    /// Parser metrics collector
    pub(crate) parser_metrics: Option<Arc<ParserMetrics>>,
    /// Pipeline stage metrics for grouper
    pub(crate) grouper_metrics: Option<Arc<PipelineStageMetrics>>,
    /// Pipeline stage metrics for converter
    pub(crate) converter_metrics: Option<Arc<PipelineStageMetrics>>,
    /// Pipeline stage metrics for chunker
    pub(crate) chunker_metrics: Option<Arc<PipelineStageMetrics>>,
    /// File-level end-to-end processing metrics
    pub(crate) file_processing_metrics: Option<Arc<FileProcessingMetrics>>,
    /// Project ID for cache key isolation
    pub(crate) project_id: i64,
    /// License header filtering configuration applied to the parser
    pub(crate) license_header: LicenseHeaderConfig,
}

impl Default for FileProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl FileProcessor {
    /// Default cache size
    pub(crate) const DEFAULT_CACHE_SIZE: usize = 100;

    /// Get a reference to the AST-to-NL converter
    pub fn converter(&self) -> &AstToNlConverter {
        &self.converter
    }

    /// Deterministic fingerprint of the chunking configuration that produced
    /// this processor's chunks. Hot-update storage modules fold it into their
    /// per-file progress markers so a chunking-config change between a crash
    /// and its resume invalidates previously-completed work.
    pub fn chunking_fingerprint(&self) -> String {
        cce_utils::hash::hash_serializable(&self.chunking_config)
    }

    /// Fingerprint of the full AST-to-NL pipeline configuration.
    ///
    /// Drives storage-module drift detection (`chunking_fingerprint_*` in
    /// `project_meta`); any converter or chunking change produces a new
    /// fingerprint and triggers a regeneration sweep.
    pub fn pipeline_fingerprint(&self) -> String {
        let mut material = String::with_capacity(192);
        material.push_str(&self.pipeline_fingerprint);
        material.push('|');
        material.push_str(cce_parser::ast_to_nl::PIPELINE_VERSION);
        if let Some(registry) = &self.plugin_registry {
            material.push('|');
            material.push_str(&registry.textgen_fingerprint());
        }
        cce_utils::hash::calculate_hash(material.as_bytes())
    }

    /// Convert grouped entities to natural language.
    ///
    /// Converter metrics only record actual conversion work.
    pub(crate) fn convert_groups(
        &self,
        groups: &[cce_types::grouper::EntityGroup],
        file_path: &str,
        processing_result: &ProcessingResult,
        source: &str,
        request: Option<&ConversionRequest>,
    ) -> Vec<cce_types::ast_to_nl::GroupConversions> {
        let converter_start = std::time::Instant::now();
        let group_conversions = self.converter.convert_entity_groups(
            groups,
            file_path,
            request,
            Some(processing_result),
            Some(source),
        );
        let converter_latency = converter_start.elapsed().as_secs_f64() * 1000.0;
        if let Some(ref metrics) = self.converter_metrics {
            metrics.record(
                processing_result.stats.output_groups,
                group_conversions.len(),
                converter_latency,
                false,
            );
        }
        group_conversions
    }

    /// Re-chunk one file from its on-disk content for chunking-drift sweeps.
    ///
    /// Reads the file and verifies its raw bytes still hash to
    /// `expected_hash`; a mismatch (or an unreadable file) means the file
    /// drifted outside change tracking, so `Ok(None)` is returned and the
    /// regular change flow stays responsible for it. On a match the full
    /// local pipeline runs under the CURRENT configuration, so the sweep
    /// output always reflects the latest pipeline behavior: code files go
    /// through parse → group → convert → chunk, while document/config/text
    /// files are re-chunked through the document pipeline under `output_mode`.
    pub async fn rechunk_file_from_disk(
        &mut self,
        read_path: &std::path::Path,
        relative_path: &str,
        expected_hash: &str,
        output_mode: OutputMode,
    ) -> Result<Option<Vec<ChunkedResult>>, OrchestratorError> {
        // Verified read through the scanner: a missing/unreadable file or
        // drifted bytes means the file moved outside change tracking, so the
        // sweep leaves it to the regular change flow. Decoding failures still
        // surface as errors.
        let content = match read_verified_utf8(read_path, Some(expected_hash)).await {
            Ok(content) => content,
            Err(ParseError::ContentChanged(reason)) => {
                tracing::warn!(
                    path = %read_path.display(),
                    %reason,
                    "Chunking-drift sweep found drifted on-disk content; skipping"
                );
                return Ok(None);
            }
            Err(ParseError::Io(error)) => {
                tracing::warn!(
                    path = %read_path.display(),
                    %error,
                    "Chunking-drift sweep cannot read file; skipping"
                );
                return Ok(None);
            }
            Err(error) => return Err(OrchestratorError::Parse(error)),
        };

        // Route like `process_file`: code files run the AST pipeline, while
        // document/config/text files belong to the document pipeline.
        let language_info = LanguageInfo::detect_from_path(relative_path);
        let route = ContentRoute::from_language_info(&language_info);
        if route.is_document() {
            // Use document-specific chunking config if available, otherwise default
            let chunking_config = self
                .document_chunking_config
                .as_ref()
                .unwrap_or(&self.chunking_config);
            let (chunks, _, _) =
                self.process_doc(&content, relative_path, output_mode, chunking_config)?;
            Ok(Some(chunks))
        } else {
            let parsed = {
                let mut coordinator = self.coordinator.lock().map_err(|_| {
                    OrchestratorError::index("parse", "parser coordinator lock poisoned")
                })?;
                coordinator.parse_with_language_info(relative_path, &content, &language_info)
            }?;
            let (chunks, _) = self.process_parsed_file(&parsed).await?;
            Ok(Some(chunks))
        }
    }

    /// Re-chunk one file from a content payload for chunking-drift sweeps.
    ///
    /// Semantics mirror [`Self::rechunk_file_from_disk`]: drifted or
    /// unsupplied content yields `Ok(None)` and stays with the regular change
    /// flow, while decoding failures surface as errors.
    pub async fn rechunk_payload(
        &mut self,
        payload: &FileContentPayload,
        local_path: Option<&std::path::Path>,
        output_mode: OutputMode,
    ) -> Result<Option<Vec<ChunkedResult>>, OrchestratorError> {
        let relative_path = payload.relative_path.to_string_lossy().to_string();
        let content = match read_verified_payload(payload, local_path).await {
            Ok(content) => content,
            Err(ParseError::ContentChanged(reason)) => {
                tracing::warn!(
                    path = %relative_path,
                    %reason,
                    "Chunking-drift sweep found drifted content; skipping"
                );
                return Ok(None);
            }
            Err(ParseError::Io(error)) => {
                tracing::warn!(
                    path = %relative_path,
                    %error,
                    "Chunking-drift sweep cannot load content; skipping"
                );
                return Ok(None);
            }
            Err(error) => return Err(OrchestratorError::Parse(error)),
        };

        let language_info = LanguageInfo::detect_from_path(&relative_path);
        let route = ContentRoute::from_language_info(&language_info);
        if route.is_document() {
            let chunking_config = self
                .document_chunking_config
                .as_ref()
                .unwrap_or(&self.chunking_config);
            let (chunks, _, _) =
                self.process_doc(&content, &relative_path, output_mode, chunking_config)?;
            Ok(Some(chunks))
        } else {
            let parsed = {
                let mut coordinator = self.coordinator.lock().map_err(|_| {
                    OrchestratorError::index("parse", "parser coordinator lock poisoned")
                })?;
                coordinator.parse_with_language_info(&relative_path, &content, &language_info)
            }?;
            let (chunks, _) = self.process_parsed_file(&parsed).await?;
            Ok(Some(chunks))
        }
    }

    /// Process a file based on its type
    pub async fn process_file(
        &mut self,
        file_entry: &FileEntry,
    ) -> Result<FileProcessResult, OrchestratorError> {
        // This entry has no output-mode context, so document files keep the
        // full generation (Both) as a safe fallback.
        let (content, route) = Self::read_and_route(file_entry).await?;
        if route.is_document() {
            self.process_document_file(file_entry, &content, OutputMode::Both)
                .await
        } else {
            self.process_code_file(file_entry, &content).await
        }
    }

    /// Read a file's verified content and resolve its processing route.
    ///
    /// Every entry point shares this single read-and-route step so the
    /// document/code decision is not re-derived at each call site.
    pub(crate) async fn read_and_route(
        file_entry: &FileEntry,
    ) -> Result<(String, ContentRoute), OrchestratorError> {
        // Reuse the scan-phase fingerprint when the file is unchanged
        // (encoding detection happens after the check).
        let content = read_verified_utf8_for_entry(file_entry)
            .await
            .map_err(OrchestratorError::Parse)?;
        let language_info = file_entry.language_info.as_ref().ok_or_else(|| {
            OrchestratorError::Parse(ParseError::unsupported_language(format!(
                "no language info for file: {}",
                file_entry.path.display()
            )))
        })?;
        Ok((content, ContentRoute::from_language_info(language_info)))
    }

    /// Read a content payload and resolve its processing route.
    ///
    /// Ready payloads never touch the filesystem; on-demand payloads load
    /// from `local_path` exactly like the historical path-based read. The
    /// route derives from the canonical relative identity, never from a
    /// local absolute path.
    pub(crate) async fn read_and_route_payload(
        payload: &FileContentPayload,
        local_path: Option<&std::path::Path>,
    ) -> Result<(String, ContentRoute), OrchestratorError> {
        let content = read_verified_payload(payload, local_path)
            .await
            .map_err(OrchestratorError::Parse)?;
        let language_info =
            LanguageInfo::detect_from_path(&payload.relative_path.to_string_lossy());
        Ok((content, ContentRoute::from_language_info(&language_info)))
    }

    /// Build the transient manifest entry backing payload processing.
    ///
    /// Identity, language and hash come from the payload; size and
    /// modification time fall back to deterministic values when the supplier
    /// did not state them, so remote payloads without stat stay stable.
    pub(crate) fn entry_from_payload(
        payload: &FileContentPayload,
        content: &str,
        local_path: Option<&std::path::Path>,
    ) -> FileEntry {
        let relative = payload.relative_path.to_string_lossy().to_string();
        let language_info = LanguageInfo::detect_from_path(&relative);
        let content_hash = payload
            .expected_hash
            .clone()
            .unwrap_or_else(|| cce_utils::hash::calculate_hash(content.as_bytes()));
        let size = payload.size.unwrap_or(content.len() as u64);
        let modified = payload
            .modified
            .unwrap_or_else(|| std::time::UNIX_EPOCH.into());
        let path = local_path.map_or_else(
            || payload.relative_path.clone(),
            |local| local.to_path_buf(),
        );
        FileEntry {
            path,
            relative_path: payload.relative_path.clone(),
            size,
            modified,
            content_hash: Some(content_hash),
            language_info: Some(language_info),
        }
    }

    /// Process a file and return complete result including pre-processor output
    ///
    /// This method provides access to `ProcessingResult` which contains
    /// entity groups, merged call patterns, and utility function markings.
    /// Useful for summary generation and other downstream tasks that need
    /// pre-processor insights.
    pub async fn process_file_complete(
        &mut self,
        file_entry: &FileEntry,
        output_mode: OutputMode,
    ) -> Result<CompleteFileProcessResult, OrchestratorError> {
        let (content, route) = Self::read_and_route(file_entry).await?;
        if route.is_document() {
            self.process_document_file_complete(file_entry, &content, output_mode)
                .await
        } else {
            self.process_code_file_complete(file_entry, &content, output_mode)
                .await
        }
    }

    /// Build the chunk cache key for a file (project, path, mode, source hash).
    ///
    /// `mode_label` is part of the key: entries generated under different
    /// output modes contain different chunk sets and must never be served to
    /// a caller requesting another mode. The AST path passes a fixed label
    /// because its chunk generation has no mode dimension.
    pub(crate) fn chunk_cache_key(&self, path: &str, source: &str, mode_label: &str) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        let source_hash = hasher.finish();
        format!(
            "{}:{}:{}:{}",
            self.project_id, path, mode_label, source_hash
        )
    }

    /// Clear the chunk cache
    ///
    /// This can be called to free memory or force re-computation.
    pub async fn clear_cache(&mut self) {
        {
            let mut cache = self.chunk_cache.write().await;
            cache.clear();
        }
        tracing::debug!("Chunk cache cleared");
    }

    /// Get chunk cache statistics
    pub async fn cache_stats(&self) -> ChunkCacheStats {
        let cache = self.chunk_cache.read().await;
        ChunkCacheStats {
            entries: cache.len(),
            capacity: cache.cap().get(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sweep fallback must only serve files whose on-disk bytes still
    /// hash to the recorded content hash.
    #[tokio::test]
    async fn rechunk_from_disk_verifies_content_hash() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("sample.rs");
        let source = "pub fn alpha() -> i32 { 1 }\n";
        std::fs::write(&path, source).expect("write file");
        let stale_hash = cce_utils::hash::calculate_hash(b"outdated bytes");

        let mut processor = FileProcessor::new();

        let skipped = processor
            .rechunk_file_from_disk(&path, "src/sample.rs", &stale_hash, OutputMode::Both)
            .await
            .expect("rechunk probe");
        assert!(
            skipped.is_none(),
            "a drifted file must be left to the regular change flow"
        );

        let fresh_hash = cce_utils::hash::calculate_hash(source.as_bytes());
        let chunks = processor
            .rechunk_file_from_disk(&path, "src/sample.rs", &fresh_hash, OutputMode::Both)
            .await
            .expect("rechunk probe")
            .expect("matching content must produce chunks");
        assert!(!chunks.is_empty());
    }

    /// Non-code files are swept through the document pipeline.
    #[tokio::test]
    async fn rechunk_from_disk_sweeps_document_files() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("README.md");
        std::fs::write(&path, "# guide\n\nSome explanatory text.\n").expect("write file");
        let hash =
            cce_utils::hash::calculate_hash("# guide\n\nSome explanatory text.\n".as_bytes());

        let mut processor = FileProcessor::new();
        let chunks = processor
            .rechunk_file_from_disk(&path, "README.md", &hash, OutputMode::Both)
            .await
            .expect("rechunk probe")
            .expect("document files must be re-chunked by the sweep");
        assert!(!chunks.is_empty());
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.metadata.content_type.is_document()),
            "swept document chunks must be document-typed"
        );
    }

    /// The shared read-and-route step reads the verified content once and
    /// resolves the document/code pipeline from the entry's language info.
    #[tokio::test]
    async fn read_and_route_reads_content_and_classifies_route() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let cases = [
            ("README.md", "# guide\n", true),
            ("main.rs", "fn main() {}\n", false),
        ];
        for (name, source, is_document) in cases {
            let path = dir.path().join(name);
            std::fs::write(&path, source).expect("write file");
            let metadata = std::fs::metadata(&path).expect("stat file");
            let entry = FileEntry {
                path: path.clone(),
                relative_path: std::path::PathBuf::from(name),
                size: metadata.len(),
                modified: metadata.modified().expect("mtime").into(),
                content_hash: Some(cce_utils::hash::calculate_hash(source.as_bytes())),
                language_info: Some(LanguageInfo::detect_from_path(name)),
            };

            let (content, route) = FileProcessor::read_and_route(&entry)
                .await
                .expect("read-and-route");
            assert_eq!(content, source);
            assert_eq!(route.is_document(), is_document, "route for {name}");
        }
    }

    /// `with_chunk_cache_size` must set the LRU capacity so that the
    /// cache_stats method reports the correct maximum.
    #[tokio::test]
    async fn with_chunk_cache_size_sets_capacity() {
        let processor = FileProcessor::new().with_chunk_cache_size(42);
        let stats = processor.cache_stats().await;
        assert_eq!(stats.capacity, 42);
        assert_eq!(stats.entries, 0);
    }
}
