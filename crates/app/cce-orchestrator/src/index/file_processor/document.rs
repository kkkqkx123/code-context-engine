use super::*;
use crate::error::OrchestratorError;
use cce_config::ChunkingConfig;
use cce_parser::ast_to_nl::chunker::ChunkedResult;
use cce_parser::summary::FileCategory;
use cce_scanner::{FileContentPayload, FileEntry};
use cce_types::{ContentRoute, LanguageInfo, OutputMode, ParsedFile};

impl FileProcessor {
    /// Process document/content via the router, consulting `FormatParse`
    /// plugins when a registry is configured.
    pub(crate) fn process_doc(
        &self,
        content: &str,
        file_path: &str,
        output_mode: OutputMode,
        chunking_config: &ChunkingConfig,
    ) -> Result<
        (
            Vec<ChunkedResult>,
            Option<cce_parser::document::DocSummary>,
            bool,
        ),
        OrchestratorError,
    > {
        let result = match &self.plugin_registry {
            Some(registry) => self.doc_pipeline.process_with_plugins_degraded_flag(
                content,
                file_path,
                chunking_config,
                output_mode,
                registry,
            ),
            None => self.doc_pipeline.process_with_degraded_flag(
                content,
                file_path,
                chunking_config,
                output_mode,
            ),
        };
        // The document pipeline already reports typed ParseError variants
        // (Json/Yaml/Xml/... parsing); propagate instead of stringifying.
        result.map_err(OrchestratorError::Parse)
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
        let (chunks, _summary, _) =
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
        self.process_document_file_complete_sync(file_entry, content, output_mode)
    }

    /// Synchronous variant of [`Self::process_document_file_complete`].
    ///
    /// The caller must run this off the async workers (for example under
    /// `tokio::task::spawn_blocking`): the document pipeline is blocking
    /// CPU work.
    pub fn process_document_file_complete_sync(
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
        let (chunks, doc_summary, document_degraded) =
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
            dropped_blank_segments: 0,
            document_degraded,
            has_syntax_errors: false,
            syntax_diagnostics: Vec::new(),
            syntax_diagnostic_tag: None,
        })
    }

    /// Process a content payload through the document/code pipeline.
    ///
    /// This is the supply-agnostic entry point: the payload carries the
    /// identity and the expected hash, `local_path` serves the on-demand
    /// form only. A transient manifest entry is built from the payload so
    /// the document and code branches keep a single consumption shape.
    pub async fn process_payload(
        &mut self,
        payload: &FileContentPayload,
        local_path: Option<&std::path::Path>,
        output_mode: OutputMode,
    ) -> Result<FileProcessResult, OrchestratorError> {
        let (content, route) = Self::read_and_route_payload(payload, local_path).await?;
        let entry = Self::entry_from_payload(payload, &content, local_path);
        if route.is_document() {
            self.process_document_file(&entry, &content, output_mode)
                .await
        } else {
            self.process_code_file(&entry, &content).await
        }
    }

    /// Process a content payload with an explicit output mode.
    pub async fn process_payload_complete(
        &mut self,
        payload: &FileContentPayload,
        local_path: Option<&std::path::Path>,
        output_mode: OutputMode,
    ) -> Result<CompleteFileProcessResult, OrchestratorError> {
        let (content, route) = Self::read_and_route_payload(payload, local_path).await?;
        let entry = Self::entry_from_payload(payload, &content, local_path);
        if route.is_document() {
            self.process_document_file_complete(&entry, &content, output_mode)
                .await
        } else {
            self.process_code_file_complete(&entry, &content, output_mode)
                .await
        }
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

        // Probe under a read lock so concurrent files sharing the cache do
        // not serialize on an exclusive lock for what is usually a hit check.
        // `peek` leaves recency untouched; the backfill below records the
        // entry as most-recent.
        {
            let cache = self.chunk_cache.read().await;
            if let Some(cached) = cache.peek(&cache_key) {
                return Ok(cached.0.clone());
            }
        }

        // Use document-specific chunking config if available, otherwise default
        let chunking_config = self
            .document_chunking_config
            .as_ref()
            .unwrap_or(&self.chunking_config);

        let (chunks, _, degraded) = self.process_doc(source, path, output_mode, chunking_config)?;
        if degraded {
            tracing::warn!(
                path,
                "Document pipeline degraded to plain text; content is incomplete"
            );
        }
        let mut cache = self.chunk_cache.write().await;
        cache.put(cache_key, (chunks.clone(), 0));
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
    ) -> Result<(Vec<ChunkedResult>, usize), OrchestratorError> {
        let cache_key = self.chunk_cache_key(&parsed.path, &parsed.source, "ast");

        // Probe under a read lock so concurrent files sharing the cache do
        // not serialize on an exclusive lock for what is usually a hit check.
        // A lost race here only repeats deterministic work: the backfill
        // below overwrites the same key with an identical value.
        {
            let cache = self.chunk_cache.read().await;
            if let Some(cached) = cache.peek(&cache_key) {
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
        let chunk_result = self
            .chunker
            .chunk_groups(&group_conversions, &parsed.path)
            .map_err(OrchestratorError::Parse);
        let chunker_latency = chunker_start.elapsed().as_secs_f64() * 1000.0;
        let chunks = match chunk_result {
            Ok(c) => {
                if let Some(ref metrics) = self.chunker_metrics {
                    metrics.record(
                        group_conversions.len(),
                        c.chunks.len(),
                        chunker_latency,
                        false,
                    );
                    for chunk in &c.chunks {
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
        let dropped_blank_segments = chunks.dropped_blank_segments;
        let enhanced_chunks: Vec<ChunkedResult> = chunks
            .chunks
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
            cache.put(cache_key, (enhanced_chunks.clone(), dropped_blank_segments));
        }

        Ok((enhanced_chunks, dropped_blank_segments))
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
        let (chunks, dropped_blank_segments) = if content_route.is_document() {
            (
                self.process_document_chunks(&parsed.path, &parsed.source, output_mode)
                    .await?,
                0,
            )
        } else {
            self.process_parsed_file(parsed).await?
        };

        Ok(CompleteFileProcessResult {
            parsed_file: parsed.clone(),
            chunks,
            processing_result: Some(processing_result),
            doc_summary: None,
            dropped_blank_segments,
            document_degraded: false,
            has_syntax_errors: false,
            syntax_diagnostics: Vec::new(),
            syntax_diagnostic_tag: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let (chunks, _dropped) = processor
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
}
