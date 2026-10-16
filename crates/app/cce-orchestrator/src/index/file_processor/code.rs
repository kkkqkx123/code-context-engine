use super::*;
use crate::error::OrchestratorError;
use cce_parser::ast_to_nl::ConversionRequest;
use cce_parser::ast_to_nl::chunker::ChunkedResult;
use cce_parser::summary::FileCategory;
use cce_scanner::FileEntry;
use cce_types::OutputMode;

impl FileProcessor {
    /// Process code file (programming languages)
    ///
    /// This method includes PreProcessor for entity optimization:
    /// Parse → PreProcess → Convert → Chunk → Store
    pub(crate) async fn process_code_file(
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
            let mut coordinator = self.coordinator.lock().map_err(|_| {
                OrchestratorError::index("parse", "parser coordinator lock poisoned")
            })?;
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
        let chunk_result = self
            .chunker
            .chunk_groups(&group_conversions, &parsed.path)
            .map(|output| output.chunks)
            .map_err(OrchestratorError::Parse);
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
    pub(crate) async fn process_code_file_complete(
        &mut self,
        file_entry: &FileEntry,
        content: &str,
        output_mode: OutputMode,
    ) -> Result<CompleteFileProcessResult, OrchestratorError> {
        self.process_code_file_complete_sync(file_entry, content, output_mode)
    }

    /// Synchronous code pipeline shared by the async wrapper above and the
    /// blocking-pool entry point below. The whole body is CPU-bound with no
    /// awaits, so batch processing runs it under `spawn_blocking` instead of
    /// occupying an async worker thread.
    pub(crate) fn process_code_file_complete_sync(
        &mut self,
        file_entry: &FileEntry,
        content: &str,
        output_mode: OutputMode,
    ) -> Result<CompleteFileProcessResult, OrchestratorError> {
        let file_start = std::time::Instant::now();
        let result = self.process_code_file_complete_inner(file_entry, content, output_mode);
        let file_latency = file_start.elapsed().as_secs_f64() * 1000.0;
        if let Some(ref metrics) = self.file_processing_metrics {
            metrics.record_file(file_latency, result.is_ok());
        }
        result
    }

    fn process_code_file_complete_inner(
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
        let (parsed, has_syntax_errors, syntax_diagnostics, syntax_diagnostic_tag) = {
            let mut coordinator = self.coordinator.lock().map_err(|_| {
                OrchestratorError::index("parse", "parser coordinator lock poisoned")
            })?;
            let (parsed, diagnostics) = coordinator
                .parse_with_language_info_detailed_with_diagnostics(
                    &file_entry.relative_path.to_string_lossy(),
                    content,
                    language_info,
                )?;
            (
                parsed,
                diagnostics.has_syntax_errors,
                diagnostics.syntax_error_spans,
                diagnostics.coarse_tag.map(str::to_string),
            )
        };

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
        let chunk_result = self
            .chunker
            .chunk_groups(&group_conversions, &parsed.path)
            .map_err(OrchestratorError::Parse);
        let chunker_latency = chunker_start.elapsed().as_secs_f64() * 1000.0;
        let chunk_output = match chunk_result {
            Ok(output) => {
                if let Some(ref metrics) = self.chunker_metrics {
                    metrics.record(
                        group_conversions.len(),
                        output.chunks.len(),
                        chunker_latency,
                        false,
                    );
                    for chunk in &output.chunks {
                        metrics.record_chunk_size(chunk.text.len());
                    }
                }
                output
            }
            Err(e) => {
                if let Some(ref metrics) = self.chunker_metrics {
                    metrics.record(group_conversions.len(), 0, chunker_latency, true);
                }
                return Err(e);
            }
        };

        // Step 5: Enhance chunks with entity association info
        let dropped_blank_segments = chunk_output.dropped_blank_segments;
        let enhanced_chunks: Vec<ChunkedResult> = chunk_output
            .chunks
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
            dropped_blank_segments,
            document_degraded: false,
            has_syntax_errors,
            syntax_diagnostics,
            syntax_diagnostic_tag,
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
        self.process_code_file_with_mode_sync(file_entry, content, output_mode)
    }

    /// Synchronous variant of [`Self::process_code_file_with_mode`].
    ///
    /// The caller must run this off the async workers (for example under
    /// `tokio::task::spawn_blocking`): parse, grouping, conversion and
    /// chunking are all blocking CPU work.
    pub fn process_code_file_with_mode_sync(
        &mut self,
        file_entry: &FileEntry,
        content: &str,
        output_mode: OutputMode,
    ) -> Result<CompleteFileProcessResult, OrchestratorError> {
        self.process_code_file_complete_sync(file_entry, content, output_mode)
    }
}
