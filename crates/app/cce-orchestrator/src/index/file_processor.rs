//! File processing logic
//!
//! This module handles processing of different file types,
//! including source code, documentation, and plain text files.
//!
//! # Caching
//!
//! FileProcessor uses an LRU cache to avoid reprocessing the same file content.
//! The cache size is configurable and defaults to 100 entries.

use cce_config::NestProcessorConfig;
use cce_config::{AstToNlConfig, ChunkingConfig, Settings};
use cce_metrics::{FileProcessingMetrics, ParserMetrics, PipelineStageMetrics};
use cce_parser::ast_to_nl::chunker::{ChunkedResult, GroupChunker};
use cce_parser::ast_to_nl::{AstToNlConverter, ConversionRequest};
use cce_parser::document::PipelineRouter;
use cce_parser::grouper::{PreprocessingPipeline, ProcessingResult};
use cce_parser::parser::ParseCoordinator;
use cce_parser::summary::FileCategory;
use cce_plugin::PluginRegistry;
use cce_scanner::FileEntry;
use cce_types::error::ParseError;
use cce_types::{ContentRoute, LanguageInfo, OutputMode, ParsedFile};

use super::super::error::OrchestratorError;

use lru::LruCache;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::path::Path;

/// Partial-hash window the scanner applies to large files.
///
/// Must stay in sync with `FileProcessorConfig::partial_hash_size` in
/// `cce_infrastructure/src/scanner/file_processor.rs`.
const SCAN_PARTIAL_HASH_SIZE: usize = 1024 * 1024;

/// Check raw file bytes against the scan-phase content hash.
///
/// The scanner hashes raw bytes — full content up to its large-file
/// threshold, only the first [`SCAN_PARTIAL_HASH_SIZE`] bytes above it. The
/// processor cannot see the scanner's threshold configuration, so both
/// domains are tried; either match proves the bytes still correspond to the
/// scanned snapshot. The partial check degrades gracefully: for files at or
/// below the window it equals the full-content hash.
fn raw_bytes_match_scan_hash(bytes: &[u8], expected: &str) -> bool {
    cce_utils::hash::calculate_hash(bytes) == expected
        || cce_utils::hash::calculate_hash_with_limit(bytes, Some(SCAN_PARTIAL_HASH_SIZE))
            == expected
}

/// Read a file, verify its raw bytes against the scan-phase content hash,
/// then decode to UTF-8 with automatic encoding detection.
///
/// This closes the scan→process race: without the verification, a file
/// modified between the scanning and processing phases would be parsed into
/// data that no longer matches the recorded hashes, silently poisoning
/// checkpoints and the hot-update change baseline. Pass `None` when no scan
/// baseline exists (event-driven reads); verification is skipped in that
/// case.
pub(crate) async fn read_verified_utf8(
    path: &Path,
    expected_hash: Option<&str>,
) -> Result<String, String> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;
    if let Some(expected) = expected_hash {
        if !raw_bytes_match_scan_hash(&bytes, expected) {
            return Err(format!(
                "content of '{}' changed between scan and processing (scan-time hash {expected} no longer matches); a re-scan is required",
                path.display()
            ));
        }
    }
    cce_utils::file::decode_bytes_to_utf8(&bytes, path)
}

/// Stable cache-key label for an output mode.
fn output_mode_label(mode: OutputMode) -> &'static str {
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
}

/// Chunk cache statistics
#[derive(Debug, Clone)]
pub struct ChunkCacheStats {
    /// Current number of entries in cache
    pub entries: usize,
    /// Maximum capacity of cache
    pub capacity: usize,
}

use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

/// File processor for handling different file types
#[derive(Clone)]
pub struct FileProcessor {
    coordinator: Arc<Mutex<ParseCoordinator>>,
    pre_processor: Arc<PreprocessingPipeline>,
    converter: Arc<AstToNlConverter>,
    chunker: Arc<Mutex<GroupChunker>>,
    /// Document processing pipeline for non-code files
    doc_pipeline: Arc<PipelineRouter>,
    /// LRU cache for chunk results to avoid duplicate processing
    ///
    /// Key: project_id + file_path + source_hash
    /// Value: chunked results
    chunk_cache: Arc<RwLock<LruCache<String, Vec<ChunkedResult>>>>,
    /// Chunking configuration for document/text processing
    chunking_config: ChunkingConfig,
    /// Document-specific chunking configuration (if None, uses chunking_config)
    document_chunking_config: Option<ChunkingConfig>,
    /// Hash of the AST-to-NL configuration. `pipeline_fingerprint()` folds
    /// this together with the parser pipeline version and the TextGen plugin
    /// set fingerprint to drive storage-module drift detection.
    pipeline_fingerprint: String,
    /// Plugin registry for NL template generation
    plugin_registry: Option<Arc<PluginRegistry>>,
    /// Parser metrics collector
    parser_metrics: Option<Arc<ParserMetrics>>,
    /// Pipeline stage metrics for grouper
    grouper_metrics: Option<Arc<PipelineStageMetrics>>,
    /// Pipeline stage metrics for converter
    converter_metrics: Option<Arc<PipelineStageMetrics>>,
    /// Pipeline stage metrics for chunker
    chunker_metrics: Option<Arc<PipelineStageMetrics>>,
    /// File-level end-to-end processing metrics
    file_processing_metrics: Option<Arc<FileProcessingMetrics>>,
    /// Project ID for cache key isolation
    project_id: i64,
}

impl Default for FileProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl FileProcessor {
    /// Default cache size
    const DEFAULT_CACHE_SIZE: usize = 100;

    /// Create a new file processor with default configuration
    pub fn new() -> Self {
        Self::with_config(&AstToNlConfig::default())
    }

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
    fn convert_groups(
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
        let bytes = match tokio::fs::read(read_path).await {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(
                    path = %read_path.display(),
                    %error,
                    "Chunking-drift sweep cannot read file; skipping"
                );
                return Ok(None);
            }
        };
        if cce_utils::hash::calculate_hash(&bytes) != expected_hash {
            tracing::warn!(
                path = %read_path.display(),
                "Chunking-drift sweep found drifted on-disk content; skipping"
            );
            return Ok(None);
        }
        let content = cce_utils::file::decode_bytes_to_utf8(&bytes, read_path)
            .map_err(|e| OrchestratorError::index("read file", e))?;

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
            let (chunks, _) =
                self.process_doc(&content, relative_path, output_mode, chunking_config)?;
            Ok(Some(chunks))
        } else {
            let parsed = {
                let mut coordinator = self.coordinator.lock().unwrap();
                coordinator.parse_with_language_info(relative_path, &content, &language_info)
            }?;
            let chunks = self.process_parsed_file(&parsed).await?;
            Ok(Some(chunks))
        }
    }

    /// Create a file processor from initialized Settings
    ///
    /// Returns an error if Settings has not been initialized.
    pub fn from_settings() -> Result<Self, cce_types::error::ConfigError> {
        let config = Settings::ast_to_nl()?;
        Ok(Self::with_config(&config))
    }

    /// Create with custom configuration
    pub fn with_config(config: &AstToNlConfig) -> Self {
        let cache_size = NonZeroUsize::new(Self::DEFAULT_CACHE_SIZE)
            .expect("DEFAULT_CACHE_SIZE must be non-zero");
        Self {
            coordinator: Arc::new(Mutex::new(ParseCoordinator::new())),
            pre_processor: Arc::new(PreprocessingPipeline::new()),
            converter: Arc::new(AstToNlConverter::with_config(config)),
            chunker: Arc::new(Mutex::new(GroupChunker::new(config.chunking.clone()))),
            doc_pipeline: Arc::new(PipelineRouter::global().clone()),
            chunk_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            chunking_config: config.chunking.clone(),
            document_chunking_config: config.document_chunking.clone(),
            pipeline_fingerprint: cce_utils::hash::hash_serializable(&config),
            plugin_registry: None,
            parser_metrics: None,
            grouper_metrics: None,
            converter_metrics: None,
            chunker_metrics: None,
            file_processing_metrics: None,
            project_id: 0,
        }
    }

    /// Create with custom pre-processor configuration
    pub fn with_pre_processor_config(pre_config: NestProcessorConfig) -> Self {
        let config = AstToNlConfig::default();
        let cache_size = NonZeroUsize::new(Self::DEFAULT_CACHE_SIZE)
            .expect("DEFAULT_CACHE_SIZE must be non-zero");
        Self {
            coordinator: Arc::new(Mutex::new(ParseCoordinator::new())),
            pre_processor: Arc::new(PreprocessingPipeline::with_config(pre_config)),
            converter: Arc::new(AstToNlConverter::with_config(&config)),
            chunker: Arc::new(Mutex::new(GroupChunker::new(config.chunking.clone()))),
            doc_pipeline: Arc::new(PipelineRouter::global().clone()),
            chunk_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            pipeline_fingerprint: cce_utils::hash::hash_serializable(&config),
            chunking_config: config.chunking,
            document_chunking_config: config.document_chunking.clone(),
            plugin_registry: None,
            parser_metrics: None,
            grouper_metrics: None,
            converter_metrics: None,
            chunker_metrics: None,
            file_processing_metrics: None,
            project_id: 0,
        }
    }

    /// Create with custom pre-processor and AST to NL configuration
    pub fn with_configs(pre_config: NestProcessorConfig, ast_to_nl_config: &AstToNlConfig) -> Self {
        let cache_size = NonZeroUsize::new(Self::DEFAULT_CACHE_SIZE).unwrap();
        Self {
            coordinator: Arc::new(Mutex::new(ParseCoordinator::new())),
            pre_processor: Arc::new(PreprocessingPipeline::with_config(pre_config)),
            converter: Arc::new(AstToNlConverter::with_config(ast_to_nl_config)),
            chunker: Arc::new(Mutex::new(GroupChunker::new(
                ast_to_nl_config.chunking.clone(),
            ))),
            doc_pipeline: Arc::new(PipelineRouter::global().clone()),
            chunk_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            chunking_config: ast_to_nl_config.chunking.clone(),
            document_chunking_config: ast_to_nl_config.document_chunking.clone(),
            pipeline_fingerprint: cce_utils::hash::hash_serializable(ast_to_nl_config),
            plugin_registry: None,
            parser_metrics: None,
            grouper_metrics: None,
            converter_metrics: None,
            chunker_metrics: None,
            file_processing_metrics: None,
            project_id: 0,
        }
    }

    /// Create with custom cache size
    pub fn with_cache_size(cache_size: usize) -> Self {
        let config = AstToNlConfig::default();
        let cache_size = NonZeroUsize::new(cache_size.max(1)).unwrap();
        Self {
            coordinator: Arc::new(Mutex::new(ParseCoordinator::new())),
            pre_processor: Arc::new(PreprocessingPipeline::new()),
            converter: Arc::new(AstToNlConverter::with_config(&config)),
            chunker: Arc::new(Mutex::new(GroupChunker::new(config.chunking.clone()))),
            doc_pipeline: Arc::new(PipelineRouter::global().clone()),
            chunk_cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
            pipeline_fingerprint: cce_utils::hash::hash_serializable(&config),
            chunking_config: config.chunking,
            document_chunking_config: config.document_chunking.clone(),
            plugin_registry: None,
            parser_metrics: None,
            grouper_metrics: None,
            converter_metrics: None,
            chunker_metrics: None,
            file_processing_metrics: None,
            project_id: 0,
        }
    }

    /// Set the project ID for cache key isolation
    pub fn with_project_id(mut self, project_id: i64) -> Self {
        self.project_id = project_id;
        self
    }

    /// Inject a custom document pipeline router.
    ///
    /// This replaces the default global singleton and allows tests or
    /// per-project configurations to use a tailored pipeline.  The `Arc`
    /// wrapping is shared naturally by `FileProcessor: Clone`.
    pub fn with_doc_pipeline(mut self, doc_pipeline: Arc<PipelineRouter>) -> Self {
        self.doc_pipeline = doc_pipeline;
        self
    }

    /// Set the chunk cache capacity.
    ///
    /// Overrides the default 100-entry LRU limit.  A value of 0 is treated
    /// as 1 (minimum viable cache).
    pub fn with_chunk_cache_size(mut self, cache_size: usize) -> Self {
        let nz =
            NonZeroUsize::new(cache_size.max(1)).expect("cache_size must be non-zero after max(1)");
        self.chunk_cache = Arc::new(RwLock::new(LruCache::new(nz)));
        self
    }

    /// Set plugin registry for NL template generation
    pub fn with_plugin_registry(mut self, plugin_registry: Arc<PluginRegistry>) -> Self {
        // Create new components with plugin registry
        let new_coordinator = ParseCoordinator::with_plugin_registry(plugin_registry.clone());
        self.coordinator = Arc::new(Mutex::new(new_coordinator));

        self.pre_processor =
            Arc::new(PreprocessingPipeline::new().with_plugin_registry(plugin_registry.clone()));

        // Get current config from existing converter
        let config = AstToNlConfig::default();
        self.converter = Arc::new(
            AstToNlConverter::with_config(&config).with_plugin_registry(plugin_registry.clone()),
        );

        self.chunker = Arc::new(Mutex::new(
            GroupChunker::new(config.chunking.clone())
                .with_plugin_registry(plugin_registry.clone()),
        ));

        self.plugin_registry = Some(plugin_registry);

        // Re-inject parser metrics if previously set, since we replaced the coordinator
        if let Some(ref metrics) = self.parser_metrics {
            if let Ok(mut coord) = self.coordinator.lock() {
                coord.set_metrics(metrics.clone());
            }
        }

        self
    }

    /// Inject parser metrics into the coordinator
    pub fn with_parser_metrics(mut self, metrics: Arc<ParserMetrics>) -> Self {
        if let Ok(mut coord) = self.coordinator.lock() {
            coord.set_metrics(metrics.clone());
        }
        self.parser_metrics = Some(metrics);
        self
    }

    /// Inject pipeline stage metrics for grouper
    pub fn with_grouper_metrics(mut self, metrics: Arc<PipelineStageMetrics>) -> Self {
        self.grouper_metrics = Some(metrics);
        self
    }

    /// Inject pipeline stage metrics for converter
    pub fn with_converter_metrics(mut self, metrics: Arc<PipelineStageMetrics>) -> Self {
        self.converter_metrics = Some(metrics);
        self
    }

    /// Inject pipeline stage metrics for chunker
    pub fn with_chunker_metrics(mut self, metrics: Arc<PipelineStageMetrics>) -> Self {
        self.chunker_metrics = Some(metrics);
        self
    }

    /// Inject file-level end-to-end processing metrics
    pub fn with_file_processing_metrics(mut self, metrics: Arc<FileProcessingMetrics>) -> Self {
        self.file_processing_metrics = Some(metrics);
        self
    }

    /// Process a file based on its type
    pub async fn process_file(
        &mut self,
        file_entry: &FileEntry,
    ) -> Result<FileProcessResult, OrchestratorError> {
        // Read file content, verifying the raw bytes still match the hash
        // recorded during scanning (encoding detection happens after the check)
        let content = read_verified_utf8(&file_entry.path, file_entry.content_hash.as_deref())
            .await
            .map_err(|e| OrchestratorError::index("read file", e.to_string()))?;

        // Use language info from FileEntry (already detected during scanning)
        let language_info = file_entry.language_info.as_ref().ok_or_else(|| {
            OrchestratorError::index(
                "get language info",
                format!("No language info for file: {}", file_entry.path.display()),
            )
        })?;

        // Route to different processing paths based on the shared routing
        // predicate: document-like files (documentation/config/text) go
        // through the document pipeline, everything else through AST parsing.
        // This entry has no output-mode context, so document files keep the
        // full generation (Both) as a safe fallback.
        if language_info.is_document_like() {
            self.process_document_file(file_entry, &content, OutputMode::Both)
                .await
        } else {
            self.process_code_file(file_entry, &content).await
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
        // Read file content, verifying the raw bytes still match the hash
        // recorded during scanning (encoding detection happens after the check)
        let content = read_verified_utf8(&file_entry.path, file_entry.content_hash.as_deref())
            .await
            .map_err(|e| OrchestratorError::index("read file", e.to_string()))?;

        // Use language info from FileEntry (already detected during scanning)
        let language_info = file_entry.language_info.as_ref().ok_or_else(|| {
            OrchestratorError::index(
                "get language info",
                format!("No language info for file: {}", file_entry.path.display()),
            )
        })?;

        // Route to different processing paths based on the shared routing
        // predicate (see `process_file`).
        if language_info.is_document_like() {
            self.process_document_file_complete(file_entry, &content, output_mode)
                .await
        } else {
            self.process_code_file_complete(file_entry, &content, output_mode)
                .await
        }
    }

    /// Process document/content via the router, consulting `FormatParse`
    /// plugins when a registry is configured.
    fn process_doc(
        &self,
        content: &str,
        file_path: &str,
        output_mode: OutputMode,
        chunking_config: &ChunkingConfig,
    ) -> Result<(Vec<ChunkedResult>, Option<cce_parser::document::DocSummary>), OrchestratorError>
    {
        let result = match &self.plugin_registry {
            Some(registry) => self.doc_pipeline.process_with_plugins(
                content,
                file_path,
                chunking_config,
                output_mode,
                registry,
            ),
            None => self
                .doc_pipeline
                .process(content, file_path, chunking_config, output_mode),
        };
        result.map_err(|e| OrchestratorError::Parse(ParseError::ast_parsing(e.to_string())))
    }

    /// Process a document-like file (documentation, config, plain text)
    /// through the document pipeline.
    pub async fn process_document_file(
        &mut self,
        file_entry: &FileEntry,
        content: &str,
        output_mode: OutputMode,
    ) -> Result<FileProcessResult, OrchestratorError> {
        let file_path = file_entry.relative_path.to_string_lossy();

        // Use document-specific chunking config if available, otherwise default
        let chunking_config = self
            .document_chunking_config
            .as_ref()
            .unwrap_or(&self.chunking_config);

        // Use document pipeline for processing
        let (chunks, _summary) =
            self.process_doc(content, &file_path, output_mode, chunking_config)?;

        // Generate a placeholder ParsedFile so downstream storage can handle
        // code and document files uniformly. Document files carry no entities
        // or relations, but the path, language and hash are preserved.
        let language_info = file_entry
            .language_info
            .as_ref()
            .cloned()
            .unwrap_or_else(|| LanguageInfo::detect_from_path(&file_path));
        let mut parsed = ParsedFile::new(language_info.language, file_path.to_string(), content);
        if let Some(hash) = &file_entry.content_hash {
            parsed.file_hash = Some(hash.clone());
        } else {
            parsed.file_hash = Some(cce_utils::hash::calculate_hash(content.as_bytes()));
        }
        Ok((parsed, chunks))
    }

    /// Process document-like file and return complete result with summary
    pub async fn process_document_file_complete(
        &mut self,
        file_entry: &FileEntry,
        content: &str,
        output_mode: OutputMode,
    ) -> Result<CompleteFileProcessResult, OrchestratorError> {
        let file_path = file_entry.relative_path.to_string_lossy();

        // Use document-specific chunking config if available, otherwise default
        let chunking_config = self
            .document_chunking_config
            .as_ref()
            .unwrap_or(&self.chunking_config);

        // Use document pipeline for processing
        let (chunks, doc_summary) =
            self.process_doc(content, &file_path, output_mode, chunking_config)?;

        let language_info = file_entry
            .language_info
            .as_ref()
            .cloned()
            .unwrap_or_else(|| LanguageInfo::detect_from_path(&file_path));
        let mut parsed = ParsedFile::new(language_info.language, file_path.to_string(), content);
        if let Some(hash) = &file_entry.content_hash {
            parsed.file_hash = Some(hash.clone());
        } else {
            parsed.file_hash = Some(cce_utils::hash::calculate_hash(content.as_bytes()));
        }

        Ok(CompleteFileProcessResult {
            parsed_file: parsed,
            chunks,
            processing_result: None,
            doc_summary,
        })
    }

    /// Process code file (programming languages)
    ///
    /// This method includes PreProcessor for entity optimization:
    /// Parse → PreProcess → Convert → Chunk → Store
    async fn process_code_file(
        &mut self,
        file_entry: &FileEntry,
        content: &str,
    ) -> Result<FileProcessResult, OrchestratorError> {
        let file_start = std::time::Instant::now();
        let result = self.process_code_file_inner(file_entry, content).await;
        let file_latency = file_start.elapsed().as_secs_f64() * 1000.0;
        if let Some(ref metrics) = self.file_processing_metrics {
            metrics.record_file(file_latency, result.is_ok());
        }
        result
    }

    async fn process_code_file_inner(
        &mut self,
        file_entry: &FileEntry,
        content: &str,
    ) -> Result<FileProcessResult, OrchestratorError> {
        // Use language info from FileEntry to avoid redundant detection
        let language_info = file_entry.language_info.as_ref().ok_or_else(|| {
            OrchestratorError::index(
                "get language info",
                format!("No language info for file: {}", file_entry.path.display()),
            )
        })?;

        // Step 1: Parse file with pre-detected language info
        let parsed = {
            let mut coordinator = self.coordinator.lock().unwrap();
            coordinator.parse_with_language_info(
                &file_entry.relative_path.to_string_lossy(),
                content,
                language_info,
            )
        }?;

        // Step 2: Pre-process entities
        let grouper_start = std::time::Instant::now();
        let processing_result = self.pre_processor.process(&parsed);
        let grouper_latency = grouper_start.elapsed().as_secs_f64() * 1000.0;
        if let Some(ref metrics) = self.grouper_metrics {
            metrics.record(
                processing_result.stats.input_entities,
                processing_result.stats.output_groups,
                grouper_latency,
                false,
            );
        }

        // Step 3: Convert to natural language using entity groups
        let group_conversions = self.convert_groups(
            &processing_result.groups,
            &parsed.path,
            &processing_result,
            &parsed.source,
            None,
        );

        // Step 4: Chunk the conversion results
        let chunker_start = std::time::Instant::now();
        let chunk_result = (|| -> Result<Vec<ChunkedResult>, OrchestratorError> {
            let mut chunker = self.chunker.lock().map_err(|e| {
                OrchestratorError::index(
                    "chunk_groups",
                    format!("Failed to acquire chunker lock: {}", e),
                )
            })?;
            Ok(chunker.chunk_groups(&group_conversions, &parsed.path))
        })();
        let chunker_latency = chunker_start.elapsed().as_secs_f64() * 1000.0;
        let chunks = match chunk_result {
            Ok(c) => {
                if let Some(ref metrics) = self.chunker_metrics {
                    metrics.record(group_conversions.len(), c.len(), chunker_latency, false);
                    for chunk in &c {
                        metrics.record_chunk_size(chunk.text.len());
                    }
                }
                c
            }
            Err(e) => {
                if let Some(ref metrics) = self.chunker_metrics {
                    metrics.record(group_conversions.len(), 0, chunker_latency, true);
                }
                return Err(e);
            }
        };

        // Step 5: Enhance chunks with entity association info
        let enhanced_chunks: Vec<ChunkedResult> = chunks
            .into_iter()
            .map(|mut chunk| {
                // File-level content category from parse-time classification
                chunk.metadata.file_category = FileCategory::determine(&parsed);
                // Find the corresponding group
                if let Some(group) = processing_result
                    .groups
                    .iter()
                    .find(|g| g.group_id == chunk.source_group_id)
                {
                    // Set entity metadata if not already set
                    let mut entity_names = Vec::new();
                    let mut entity_kinds = Vec::new();

                    if let Some(ref header) = group.header {
                        entity_names.push(header.name.clone());
                        entity_kinds.push(header.kind.to_string());
                    }
                    for member in &group.members {
                        entity_names.push(member.name.clone());
                        entity_kinds.push(member.kind.to_string());
                    }

                    if let Some(code_meta) = chunk.metadata.as_code_mut() {
                        code_meta.entity_kind = group.kind;
                    }
                }
                chunk
            })
            .collect();

        Ok((parsed, enhanced_chunks))
    }

    /// Process code file and return complete result with processing metadata
    ///
    /// Similar to `process_code_file` but returns the `ProcessingResult`
    /// for downstream consumers like summary generation.
    async fn process_code_file_complete(
        &mut self,
        file_entry: &FileEntry,
        content: &str,
        output_mode: OutputMode,
    ) -> Result<CompleteFileProcessResult, OrchestratorError> {
        let file_start = std::time::Instant::now();
        let result = self
            .process_code_file_complete_inner(file_entry, content, output_mode)
            .await;
        let file_latency = file_start.elapsed().as_secs_f64() * 1000.0;
        if let Some(ref metrics) = self.file_processing_metrics {
            metrics.record_file(file_latency, result.is_ok());
        }
        result
    }

    async fn process_code_file_complete_inner(
        &mut self,
        file_entry: &FileEntry,
        content: &str,
        output_mode: OutputMode,
    ) -> Result<CompleteFileProcessResult, OrchestratorError> {
        // Use language info from FileEntry to avoid redundant detection
        let language_info = file_entry.language_info.as_ref().ok_or_else(|| {
            OrchestratorError::index(
                "get language info",
                format!("No language info for file: {}", file_entry.path.display()),
            )
        })?;

        // Step 1: Parse file with pre-detected language info
        let parsed = {
            let mut coordinator = self.coordinator.lock().unwrap();
            coordinator.parse_with_language_info(
                &file_entry.relative_path.to_string_lossy(),
                content,
                language_info,
            )
        }?;

        // Step 2: Pre-process entities
        let grouper_start = std::time::Instant::now();
        let processing_result = self.pre_processor.process(&parsed);
        let grouper_latency = grouper_start.elapsed().as_secs_f64() * 1000.0;
        if let Some(ref metrics) = self.grouper_metrics {
            metrics.record(
                processing_result.stats.input_entities,
                processing_result.stats.output_groups,
                grouper_latency,
                false,
            );
        }

        // Step 3: Convert to natural language using entity groups with the
        // requested mode
        let request = ConversionRequest {
            force_mode: Some(output_mode),
        };
        let group_conversions = self.convert_groups(
            &processing_result.groups,
            &parsed.path,
            &processing_result,
            &parsed.source,
            Some(&request),
        );

        // Step 4: Chunk the conversion results
        let chunker_start = std::time::Instant::now();
        let chunker_locked = self.chunker.lock().map_err(|e| {
            OrchestratorError::index(
                "chunk_groups",
                format!("Failed to acquire chunker lock: {}", e),
            )
        });
        let chunker_latency = chunker_start.elapsed().as_secs_f64() * 1000.0;
        let chunks = match chunker_locked {
            Ok(mut chunker) => {
                let c = chunker.chunk_groups(&group_conversions, &parsed.path);
                if let Some(ref metrics) = self.chunker_metrics {
                    metrics.record(group_conversions.len(), c.len(), chunker_latency, false);
                    for chunk in &c {
                        metrics.record_chunk_size(chunk.text.len());
                    }
                }
                c
            }
            Err(e) => {
                if let Some(ref metrics) = self.chunker_metrics {
                    metrics.record(group_conversions.len(), 0, chunker_latency, true);
                }
                return Err(e);
            }
        };

        // Step 5: Enhance chunks with entity association info
        let enhanced_chunks: Vec<ChunkedResult> = chunks
            .into_iter()
            .map(|mut chunk| {
                chunk.metadata.file_category = FileCategory::determine(&parsed);
                if let Some(group) = processing_result
                    .groups
                    .iter()
                    .find(|g| g.group_id == chunk.source_group_id)
                {
                    if let Some(code_meta) = chunk.metadata.as_code_mut() {
                        code_meta.entity_kind = group.kind;
                    }
                }
                chunk
            })
            .collect();

        Ok(CompleteFileProcessResult {
            parsed_file: parsed,
            chunks: enhanced_chunks,
            processing_result: Some(processing_result),
            doc_summary: None,
        })
    }

    /// Build the chunk cache key for a file (project, path, mode, source hash).
    ///
    /// `mode_label` is part of the key: entries generated under different
    /// output modes contain different chunk sets and must never be served to
    /// a caller requesting another mode. The AST path passes a fixed label
    /// because its chunk generation has no mode dimension.
    fn chunk_cache_key(&self, path: &str, source: &str, mode_label: &str) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        let source_hash = hasher.finish();
        format!(
            "{}:{}:{}:{}",
            self.project_id, path, mode_label, source_hash
        )
    }

    /// Chunk a document-route file through the document pipeline.
    ///
    /// Documentation/config/text files arriving from hot updates carry no AST
    /// entities; their chunks are produced here under `output_mode` and shared
    /// between the BM25 and embedding processors via the LRU cache. Callers
    /// branch on `ParseResultWithChanges::content_route` to reach this method.
    pub async fn process_document_chunks(
        &mut self,
        path: &str,
        source: &str,
        output_mode: OutputMode,
    ) -> Result<Vec<ChunkedResult>, OrchestratorError> {
        let cache_key = self.chunk_cache_key(path, source, output_mode_label(output_mode));

        // Check cache
        {
            let mut cache = self.chunk_cache.write().await;
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        // Use document-specific chunking config if available, otherwise default
        let chunking_config = self
            .document_chunking_config
            .as_ref()
            .unwrap_or(&self.chunking_config);

        let (chunks, _) = self.process_doc(source, path, output_mode, chunking_config)?;
        let mut cache = self.chunk_cache.write().await;
        cache.put(cache_key, chunks.clone());
        Ok(chunks)
    }

    /// Chunk an already-parsed code file for hot updates
    ///
    /// This method skips parsing and directly processes the ParsedFile
    /// to generate ChunkedResults for embedding.
    ///
    /// Uses LRU caching to avoid duplicate processing of the same file.
    /// Document-route files must go through [`FileProcessor::process_document_chunks`]
    /// instead; callers branch on `ParseResultWithChanges::content_route`.
    pub async fn process_parsed_file(
        &mut self,
        parsed: &ParsedFile,
    ) -> Result<Vec<ChunkedResult>, OrchestratorError> {
        let cache_key = self.chunk_cache_key(&parsed.path, &parsed.source, "ast");

        // Check cache
        {
            let mut cache = self.chunk_cache.write().await;
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        // Step 1: Pre-process entities
        let grouper_start = std::time::Instant::now();
        let processing_result = self.pre_processor.process(parsed);
        let grouper_latency = grouper_start.elapsed().as_secs_f64() * 1000.0;
        if let Some(ref metrics) = self.grouper_metrics {
            metrics.record(
                processing_result.stats.input_entities,
                processing_result.stats.output_groups,
                grouper_latency,
                false,
            );
        }

        // Step 2: Convert to natural language using entity groups
        let group_conversions = self.convert_groups(
            &processing_result.groups,
            &parsed.path,
            &processing_result,
            &parsed.source,
            None,
        );

        // Step 3: Chunk the conversion results
        let chunker_start = std::time::Instant::now();
        let chunk_result = (|| -> Result<Vec<ChunkedResult>, OrchestratorError> {
            let mut chunker = self.chunker.lock().map_err(|e| {
                OrchestratorError::index(
                    "chunk_groups",
                    format!("Failed to acquire chunker lock: {}", e),
                )
            })?;
            Ok(chunker.chunk_groups(&group_conversions, &parsed.path))
        })();
        let chunker_latency = chunker_start.elapsed().as_secs_f64() * 1000.0;
        let chunks = match chunk_result {
            Ok(c) => {
                if let Some(ref metrics) = self.chunker_metrics {
                    metrics.record(group_conversions.len(), c.len(), chunker_latency, false);
                    for chunk in &c {
                        metrics.record_chunk_size(chunk.text.len());
                    }
                }
                c
            }
            Err(e) => {
                if let Some(ref metrics) = self.chunker_metrics {
                    metrics.record(group_conversions.len(), 0, chunker_latency, true);
                }
                return Err(e);
            }
        };

        // Step 4: Enhance chunks with entity association info
        let enhanced_chunks: Vec<ChunkedResult> = chunks
            .into_iter()
            .map(|mut chunk| {
                chunk.metadata.file_category = FileCategory::determine(parsed);
                // Find the corresponding group
                if let Some(group) = processing_result
                    .groups
                    .iter()
                    .find(|g| g.group_id == chunk.source_group_id)
                {
                    if let Some(code_meta) = chunk.metadata.as_code_mut() {
                        code_meta.entity_kind = group.kind;
                    }
                }
                chunk
            })
            .collect();

        // Update cache
        {
            let mut cache = self.chunk_cache.write().await;
            cache.put(cache_key, enhanced_chunks.clone());
        }

        Ok(enhanced_chunks)
    }

    /// Rebuild downstream artifacts from a persisted parser result.
    ///
    /// This path deliberately skips `ParseCoordinator`, so checkpoint recovery
    /// can regenerate chunks and summaries without invoking tree-sitter again.
    /// `content_route` decides the chunking pipeline; recovery boundaries
    /// derive it via [`ContentRoute::detect_from_path`]. `output_mode` is
    /// forwarded to the document chunker so recovered files honor the same
    /// backend selection as freshly processed batches.
    pub async fn process_parsed_file_complete(
        &mut self,
        parsed: &ParsedFile,
        content_route: ContentRoute,
        output_mode: OutputMode,
    ) -> Result<CompleteFileProcessResult, OrchestratorError> {
        let processing_result = self.pre_processor.process(parsed);
        let chunks = if content_route.is_document() {
            self.process_document_chunks(&parsed.path, &parsed.source, output_mode)
                .await?
        } else {
            self.process_parsed_file(parsed).await?
        };

        Ok(CompleteFileProcessResult {
            parsed_file: parsed.clone(),
            chunks,
            processing_result: Some(processing_result),
            doc_summary: None,
        })
    }

    /// Process code file with specific output mode
    ///
    /// Similar to `process_code_file_complete` but allows specifying the output mode
    /// to control whether BM25, Embedding, or Both texts are generated.
    pub async fn process_code_file_with_mode(
        &mut self,
        file_entry: &FileEntry,
        content: &str,
        output_mode: OutputMode,
    ) -> Result<CompleteFileProcessResult, OrchestratorError> {
        self.process_code_file_complete(file_entry, content, output_mode)
            .await
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

    /// Document-route parse results (placeholders arriving from hot updates)
    /// must be chunked through the document pipeline based on their explicit
    /// `content_route` marker, not the AST pipeline.
    #[tokio::test]
    async fn process_parsed_file_complete_routes_documents_by_marker() {
        let parsed = ParsedFile::new(
            cce_types::Language::Unknown,
            "docs/README.md".to_string(),
            "# guide\n\nSome explanatory text.\n",
        );
        assert_eq!(
            ContentRoute::detect_from_path(&parsed.path),
            ContentRoute::Documentation,
            "the recovery boundary must derive the document marker from the path"
        );

        let mut processor = FileProcessor::new();
        let complete = processor
            .process_parsed_file_complete(&parsed, ContentRoute::Documentation, OutputMode::Both)
            .await
            .expect("document chunks");
        let chunks = complete.chunks;
        assert!(
            !chunks.is_empty(),
            "markdown content must produce document chunks"
        );
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.metadata.content_type.is_document()),
            "all chunks must be document-typed, got {:?}",
            chunks
                .iter()
                .map(|c| c.metadata.content_type.clone())
                .collect::<Vec<_>>()
        );

        // The dedicated document chunking entry point shares the same output.
        let mut processor = FileProcessor::new();
        let chunks = processor
            .process_document_chunks(
                "docs/README.md",
                "# guide\n\nSome explanatory text.\n",
                OutputMode::Both,
            )
            .await
            .expect("document chunks");
        assert!(!chunks.is_empty());
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.metadata.content_type.is_document())
        );
    }

    /// Code paths keep the AST pipeline even when entities are present.
    #[tokio::test]
    async fn process_parsed_file_keeps_ast_pipeline_for_code_paths() {
        let mut coordinator = cce_parser::parser::ParseCoordinator::new();
        let parsed = coordinator
            .parse(
                "src/lib.rs",
                "fn sample_main() {\n    println!(\"hi\");\n}\n",
            )
            .expect("rust should parse");
        assert!(!parsed.entities.is_empty());

        let mut processor = FileProcessor::new();
        let chunks = processor
            .process_parsed_file(&parsed)
            .await
            .expect("code chunks");
        assert!(!chunks.is_empty(), "rust code must produce code chunks");
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.metadata.content_type.is_code()),
            "all chunks must be code-typed"
        );
    }

    /// The full-content hash domain (small files) must verify.
    #[test]
    fn raw_bytes_match_scan_hash_full_domain() {
        let bytes = b"small file body".to_vec();
        let full = cce_utils::hash::calculate_hash(&bytes);
        assert!(raw_bytes_match_scan_hash(&bytes, &full));
    }

    /// Large files are scanned under the partial-hash domain; verification
    /// must accept a prefix-window match even though the full hash differs.
    #[test]
    fn raw_bytes_match_scan_hash_partial_domain() {
        let big = vec![b'a'; SCAN_PARTIAL_HASH_SIZE + 128];
        let prefix = cce_utils::hash::calculate_hash_with_limit(&big, Some(SCAN_PARTIAL_HASH_SIZE));
        assert!(raw_bytes_match_scan_hash(&big, &prefix));

        let unrelated = vec![b'b'; SCAN_PARTIAL_HASH_SIZE + 128];
        let other_full = cce_utils::hash::calculate_hash(&unrelated);
        assert!(!raw_bytes_match_scan_hash(&big, &other_full));
    }

    /// Verified read accepts content whose raw bytes still hash to the
    /// scan-time value and decodes it with encoding detection.
    #[tokio::test]
    async fn read_verified_utf8_accepts_matching_hash() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("sample.txt");
        let source = "hello verified world";
        std::fs::write(&path, source).expect("write file");
        let hash = cce_utils::hash::calculate_hash(source.as_bytes());

        let content = read_verified_utf8(&path, Some(&hash))
            .await
            .expect("matching hash must decode");
        assert_eq!(content, source);
    }

    /// Content drifted inside the scan→process window must fail explicitly
    /// instead of silently producing data inconsistent with recorded hashes.
    #[tokio::test]
    async fn read_verified_utf8_rejects_drifted_content() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("drifted.txt");
        std::fs::write(&path, "current on-disk content").expect("write file");
        let stale = cce_utils::hash::calculate_hash(b"content at scan time");

        let error = read_verified_utf8(&path, Some(&stale))
            .await
            .expect_err("drifted content must fail verification");
        assert!(error.contains("changed between scan and processing"));
    }

    /// Without a scan baseline the check is skipped (event-driven reads).
    #[tokio::test]
    async fn read_verified_utf8_skips_check_without_baseline() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("no_baseline.txt");
        std::fs::write(&path, "any content").expect("write file");

        let content = read_verified_utf8(&path, None)
            .await
            .expect("missing baseline must skip verification");
        assert_eq!(content, "any content");
    }

    /// Non-UTF-8 files verify in the raw-byte domain before decoding; a GBK
    /// document must pass the hash check and decode through encoding detection.
    #[tokio::test]
    async fn read_verified_utf8_decodes_non_utf8_after_raw_check() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("gbk.txt");
        // GBK bytes of a Chinese greeting (decodes to the assertion below).
        let gbk: Vec<u8> = vec![0xC4, 0xE3, 0xBA, 0xC3, 0xCA, 0xC0, 0xBD, 0xE7];
        std::fs::write(&path, &gbk).expect("write file");
        let hash = cce_utils::hash::calculate_hash(&gbk);

        let content = read_verified_utf8(&path, Some(&hash))
            .await
            .expect("raw-byte hash must match for non-UTF-8 files");
        assert_eq!(content, "你好世界");
    }

    /// The document pipeline must honor the caller's output mode so a
    /// single-backend deployment never generates the other path's text.
    #[tokio::test]
    async fn document_file_complete_honors_output_mode() {
        use cce_parser::ast_to_nl::chunker::ChunkPath;

        let mut processor = FileProcessor::new();
        let entry = FileEntry {
            path: std::path::PathBuf::from("unused-on-disk.md"),
            relative_path: std::path::PathBuf::from("docs/guide.md"),
            size: 0,
            modified: chrono::Utc::now(),
            content_hash: None,
            language_info: None,
        };
        let content = "# guide\n\nSome explanatory text.\n";

        for (mode, expected) in [
            (OutputMode::Bm25, ChunkPath::Bm25),
            (OutputMode::Embedding, ChunkPath::Embedding),
        ] {
            let complete = processor
                .process_document_file_complete(&entry, content, mode)
                .await
                .expect("document processing");
            assert!(
                !complete.chunks.is_empty()
                    && complete.chunks.iter().all(|chunk| chunk.path == expected),
                "{mode:?} request must only produce {expected:?} chunks"
            );
            assert!(complete.doc_summary.is_some());
        }
    }

    /// Cache entries must be isolated per output mode: after generating under
    /// Bm25, an Embedding request for the same file must not be served the
    /// single-path chunks from the wrong-mode cache entry.
    #[tokio::test]
    async fn document_chunk_cache_isolates_output_modes() {
        use cce_parser::ast_to_nl::chunker::ChunkPath;

        let mut processor = FileProcessor::new();
        let source = "# guide\n\nSome explanatory text.\n";

        let bm25 = processor
            .process_document_chunks("docs/guide.md", source, OutputMode::Bm25)
            .await
            .expect("bm25 chunks");
        assert!(!bm25.is_empty());
        assert!(bm25.iter().all(|chunk| chunk.path == ChunkPath::Bm25));

        let embedding = processor
            .process_document_chunks("docs/guide.md", source, OutputMode::Embedding)
            .await
            .expect("embedding chunks");
        assert!(!embedding.is_empty());
        assert!(
            embedding
                .iter()
                .all(|chunk| chunk.path == ChunkPath::Embedding),
            "an embedding-mode request must not receive bm25-mode cache entries"
        );
    }

    /// A custom pipeline injected via `with_doc_pipeline` must be used
    /// instead of the global singleton.
    #[tokio::test]
    async fn with_doc_pipeline_injects_custom_router() {
        let custom_router = Arc::new(PipelineRouter::new());
        let processor = FileProcessor::new().with_doc_pipeline(custom_router.clone());
        assert!(Arc::ptr_eq(&processor.doc_pipeline, &custom_router));
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
