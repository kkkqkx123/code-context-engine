//! Semantic compression retrieval implementation
//!
//! Provides single-file AST parsing, grouping, and natural language conversion
//! for large monolithic files. This module is designed for on-demand processing
//! without side effects (no embedding, no caching, no storage).

mod types;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

pub use types::{
    BatchCompressionRequest, BatchCompressionResponse, CompressionError, CompressionRequest,
    CompressionResponse, Result,
};

use crate::index_state_tracker::UpdateStateTracker;
use cce_config::NestProcessorConfig;
use cce_parser::grouper::PreprocessingPipeline;
use cce_parser::grouper::types::EntityGroup;
use cce_parser::parser::coordinator::ParseCoordinator;
use cce_scanner::{FileProcessor, compute_content_hash};
use cce_types::entity::ParsedFile;
use cce_types::language::LanguageInfo;

use crate::export::presentation::PresentationConverter;

/// Maximum file size for compression (10MB)
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Context for building compression response
///
/// Encapsulates all parameters needed to construct a CompressionResponse.
struct ResponseBuildContext {
    /// Original request
    pub request: CompressionRequest,
    /// Programming language
    pub language: String,
    /// File hash (SHA-256)
    pub file_hash: String,
    /// Whether the result came from cache
    pub from_cache: bool,
    /// Parsed file containing entities
    pub parsed_file: ParsedFile,
    /// Entity groups
    pub groups: Vec<EntityGroup>,
    /// Semantic text
    pub semantic_text: String,
}

/// Semantic compression retrieval handler
///
/// Provides on-demand semantic compression for code files without side effects.
pub struct CompressionRetrieval {
    /// State tracker for checking indexed projects (optional)
    state_tracker: Option<Arc<UpdateStateTracker>>,

    /// Preprocessing configuration
    preprocessing_config: NestProcessorConfig,

    /// Presentation text converter
    presentation_converter: PresentationConverter,

    /// File processor for file handling
    file_processor: FileProcessor,

    /// Parse coordinator (reused for batch processing)
    parse_coordinator: Arc<Mutex<ParseCoordinator>>,
}

impl Default for CompressionRetrieval {
    fn default() -> Self {
        Self::new()
    }
}

impl CompressionRetrieval {
    /// Create a new compression retrieval instance
    pub fn new() -> Self {
        Self {
            state_tracker: None,
            preprocessing_config: NestProcessorConfig::default(),
            presentation_converter: PresentationConverter::new(),
            file_processor: FileProcessor::new(),
            parse_coordinator: Arc::new(Mutex::new(ParseCoordinator::new())),
        }
    }

    /// Configure state tracker for checking indexed projects
    pub fn with_state_tracker(self, tracker: Arc<UpdateStateTracker>) -> Self {
        Self {
            state_tracker: Some(tracker),
            ..self
        }
    }

    /// Configure preprocessing
    pub fn with_preprocessing_config(self, config: NestProcessorConfig) -> Self {
        Self {
            preprocessing_config: config,
            ..self
        }
    }

    /// Execute semantic compression on a file
    ///
    /// This method performs the following steps:
    /// 1. Validate file existence, readability, and type
    /// 2. Parse file
    /// 3. Convert to natural language
    /// 4. Return response without side effects
    pub async fn compress(&self, request: CompressionRequest) -> Result<CompressionResponse> {
        // Step 1: Validate file
        let (_canonical_path, source, language_info) = self.validate_file(&request.file_path)?;

        // Step 2: Full parsing
        let parsed_file = self
            .parse_file(&request.file_path, &source, &language_info.language)
            .await?;

        // Step 3: Entity grouping
        let groups = self.group_entities(&parsed_file);

        // Step 4: Natural language conversion
        let semantic_text = self.convert_to_semantic_text(&groups, &request.file_path);

        // Step 5: Build response
        Ok(self.build_response(ResponseBuildContext {
            request,
            language: language_info.language.to_string(),
            file_hash: compute_content_hash(source.as_bytes()),
            from_cache: false,
            parsed_file,
            groups,
            semantic_text,
        }))
    }

    /// Execute batch compression on multiple files
    ///
    /// This method processes multiple files sequentially.
    /// For concurrent processing, consider using multiple CompressionRetrieval instances
    /// or implementing a concurrent version with Arc<Mutex<CompressionRetrieval>>.
    pub async fn compress_batch(
        &self,
        request: BatchCompressionRequest,
    ) -> BatchCompressionResponse {
        let mut successes = Vec::new();
        let mut failures = Vec::new();

        for path in &request.file_paths {
            let req = CompressionRequest {
                file_path: path.clone(),
                include_entities: request.include_entities,
                include_groups: request.include_groups,
            };

            match self.compress(req).await {
                Ok(resp) => successes.push((path.clone(), resp)),
                Err(e) => failures.push((path.clone(), e)),
            }
        }

        BatchCompressionResponse {
            successes,
            failures,
        }
    }

    /// Validate file for compression
    ///
    /// Returns canonical path, file content, and language info if valid.
    fn validate_file(&self, file_path: &str) -> Result<(PathBuf, String, LanguageInfo)> {
        let path = Path::new(file_path);
        let canonical_path = path.canonicalize().map_err(|e| {
            CompressionError::FileNotFound(format!("Cannot canonicalize path: {}", e))
        })?;

        // Check if it's a file
        if !canonical_path.is_file() {
            return Err(CompressionError::UnsupportedFileType(format!(
                "Path is not a file: {}",
                file_path
            )));
        }

        // Check file size
        let metadata = canonical_path.metadata().map_err(|e| {
            CompressionError::FileNotReadable(format!("Failed to read metadata: {}", e))
        })?;

        if metadata.len() > MAX_FILE_SIZE {
            return Err(CompressionError::FileTooLarge(format!(
                "File size {} bytes exceeds maximum {} bytes",
                metadata.len(),
                MAX_FILE_SIZE
            )));
        }

        // Use FileProcessor to process the file
        let file_entry = self
            .file_processor
            .process_file(&canonical_path, &canonical_path)
            .map_err(|e| {
                CompressionError::FileNotReadable(format!("Failed to process file: {}", e))
            })?;

        // Check if file is text and has language info
        if !file_entry.is_text() {
            return Err(CompressionError::UnsupportedFileType(format!(
                "File is not a text file: {}",
                canonical_path.display()
            )));
        }

        let language_info = file_entry
            .language_info
            .as_ref()
            .ok_or_else(|| {
                CompressionError::LanguageDetectionError(format!(
                    "Failed to detect language for: {}",
                    canonical_path.display()
                ))
            })?
            .clone();

        if language_info.language == cce_types::language::Language::Unknown {
            return Err(CompressionError::LanguageDetectionError(format!(
                "Failed to detect language for: {}",
                canonical_path.display()
            )));
        }

        // Read file content
        let source = std::fs::read_to_string(&canonical_path).map_err(|e| {
            CompressionError::FileNotReadable(format!("Failed to read file: {}", e))
        })?;

        Ok((canonical_path, source, language_info))
    }

    /// Check if file is in indexed project
    #[allow(dead_code)]
    async fn check_indexed_project(&self, file_path: &Path) -> bool {
        if let Some(ref tracker) = self.state_tracker {
            if let Some(state) = tracker.get_state(file_path).await {
                return state.all_success();
            }
        }
        false
    }

    /// Parse file using ParseCoordinator
    async fn parse_file(
        &self,
        file_path: &str,
        source: &str,
        _language: &cce_types::language::Language,
    ) -> Result<ParsedFile> {
        let language_info = LanguageInfo::detect_from_path(file_path);
        let mut coordinator = self.parse_coordinator.lock().await;
        coordinator
            .parse_with_language_info(file_path, source, &language_info)
            .map_err(|e| CompressionError::ParseError(format!("Failed to parse file: {}", e)))
    }

    /// Group entities using PreprocessingPipeline
    fn group_entities(&self, parsed_file: &ParsedFile) -> Vec<EntityGroup> {
        let pipeline = PreprocessingPipeline::with_config(self.preprocessing_config.clone());
        let result = pipeline.process(parsed_file);
        result.groups
    }

    /// Convert entity groups to semantic text (pure natural language)
    fn convert_to_semantic_text(&self, groups: &[EntityGroup], file_path: &str) -> String {
        let group_conversions = self
            .presentation_converter
            .convert_entity_groups(groups, file_path);

        // Flatten to get all conversion results
        let results: Vec<_> = group_conversions
            .iter()
            .flat_map(|gc| {
                gc.header_conversion
                    .iter()
                    .chain(gc.member_conversions.iter())
            })
            .collect();

        let mut parts = Vec::new();
        for result in results {
            if let Some(text) = result.embedding_text.clone() {
                parts.push(text);
            }
        }

        parts.join("\n\n")
    }

    /// Build response from fresh parsing
    fn build_response(&self, ctx: ResponseBuildContext) -> CompressionResponse {
        let entities = if ctx.request.include_entities {
            Some(ctx.parsed_file.entities.clone())
        } else {
            None
        };

        let groups = if ctx.request.include_groups {
            Some(ctx.groups)
        } else {
            None
        };

        CompressionResponse {
            file_path: ctx.request.file_path,
            language: ctx.language,
            file_hash: ctx.file_hash,
            from_cache: ctx.from_cache,
            entities,
            groups,
            semantic_text: ctx.semantic_text,
        }
    }
}
