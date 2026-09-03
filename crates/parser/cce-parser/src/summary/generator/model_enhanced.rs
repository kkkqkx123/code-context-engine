//! Model-enhanced summary generator
//!
//! Uses LLM to generate richer, more descriptive summaries
//! for high-importance files.

use crate::grouper::{PreprocessingPipeline, ProcessingResult};
use crate::summary::SummaryConfig;
use crate::summary::generator::{RuleBasedGenerator, specialized};
use crate::summary::strategy::{FileCategory, ImportanceDecision, ImportanceLevel};
use crate::summary::types::{FileSummary, GenerationDecision};
use cce_llm::{ChatConfig, ChatResult, LlmClient, Message};
use cce_metrics::SummaryMetrics;
use cce_types::ParsedFile;
use cce_utils::{
    normalize_whitespace,
    token_estimation::{TokenEstimator, estimate_tokens},
};
use std::sync::Arc;

/// Model-enhanced summary generator
///
/// Combines rule-based generation with LLM enhancement for
/// files that exceed the importance threshold. The LLM client is a generic
/// parameter instead of a trait object: the only production implementation is
/// `HttpLlmClient`, instantiated at the orchestration boundary, while tests
/// inject a stub.
pub struct ModelEnhancedGenerator<C: LlmClient> {
    llm_client: Arc<C>,
    chat_config: ChatConfig,
    rule_generator: RuleBasedGenerator,
    config: SummaryConfig,
    metrics: Option<Arc<SummaryMetrics>>,
}

/// Component-level summary for hierarchical generation
#[derive(Debug, Clone)]
pub struct ComponentSummary {
    pub name: String,
    pub entity_type: String,
    pub summary: String,
    pub start_line: u32,
    pub end_line: u32,
}

impl<C: LlmClient> ModelEnhancedGenerator<C> {
    /// Create a new model-enhanced generator
    pub fn new(llm_client: Arc<C>, chat_config: ChatConfig) -> Self {
        Self {
            llm_client,
            chat_config,
            rule_generator: RuleBasedGenerator::new(),
            config: SummaryConfig::default(),
            metrics: None,
        }
    }

    /// Create with custom configuration
    pub fn with_config(llm_client: Arc<C>, chat_config: ChatConfig, config: SummaryConfig) -> Self {
        Self {
            llm_client,
            chat_config,
            rule_generator: RuleBasedGenerator::with_config(config.clone()),
            config,
            metrics: None,
        }
    }

    /// Set monitoring metrics
    pub fn with_metrics(mut self, metrics: Arc<SummaryMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Generate summary for a parsed file
    ///
    /// This method implements a multi-stage generation pipeline:
    /// 1. File category detection (test, config, doc, etc.)
    /// 2. Special file handling (skip model for certain categories)
    /// 3. Entity grouping for logical unit analysis
    /// 4. Group-aware strategy decision
    /// 5. Model enhancement for high-importance files
    pub async fn generate_impl(&self, parsed_file: &ParsedFile) -> FileSummary {
        self.generate_impl_tracked(parsed_file).await.0
    }

    /// Generate a summary and report whether model enhancement was skipped due
    /// to a rate limit (eligible for a deferred batch-level retry).
    ///
    /// The boolean is `true` only when the model call failed with
    /// [`LlmError::RateLimitExceeded`] and the rule-based fallback was used.
    async fn generate_impl_tracked(&self, parsed_file: &ParsedFile) -> (FileSummary, bool) {
        if FileCategory::should_skip_model_enhancement(parsed_file) {
            return (
                specialized::generate_specialized_summary(parsed_file),
                false,
            );
        }

        // Stage 3: Process entities into groups for logical analysis
        let processor = PreprocessingPipeline::new();
        let processing_result = processor.process(parsed_file);

        self.generate_with_groups_tracked(parsed_file, &processing_result)
            .await
    }

    /// Generate summary for a parsed file with pre-computed processing result
    ///
    /// This method allows callers to provide a pre-computed `ProcessingResult`
    /// to avoid redundant preprocessing when the same file is processed multiple times.
    /// If `processing_result` is `None`, preprocessing will be performed internally.
    pub async fn generate_impl_with_result(
        &self,
        parsed_file: &ParsedFile,
        processing_result: Option<&ProcessingResult>,
    ) -> FileSummary {
        self.generate_impl_tracked_with_result(parsed_file, processing_result)
            .await
            .0
    }

    /// Generate a summary with pre-computed processing result and report rate limit status
    async fn generate_impl_tracked_with_result(
        &self,
        parsed_file: &ParsedFile,
        processing_result: Option<&ProcessingResult>,
    ) -> (FileSummary, bool) {
        if FileCategory::should_skip_model_enhancement(parsed_file) {
            return (
                specialized::generate_specialized_summary(parsed_file),
                false,
            );
        }

        // Run preprocessing if not provided
        let default_result;
        let processing_result = match processing_result {
            Some(result) => result,
            None => {
                let processor = PreprocessingPipeline::new();
                default_result = processor.process(parsed_file);
                &default_result
            }
        };

        self.generate_with_groups_tracked(parsed_file, processing_result)
            .await
    }

    /// Generate a summary using entity groups produced by the caller.
    async fn generate_with_groups_impl(
        &self,
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
    ) -> FileSummary {
        self.generate_with_groups_tracked(parsed_file, processing_result)
            .await
            .0
    }

    /// Group-aware generation with a rate-limit retry flag (see
    /// [`Self::generate_impl_tracked`]).
    async fn generate_with_groups_tracked(
        &self,
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
    ) -> (FileSummary, bool) {
        if FileCategory::should_skip_model_enhancement(parsed_file) {
            return (
                specialized::generate_specialized_summary(parsed_file),
                false,
            );
        }

        let strategy = ImportanceDecision::determine_generation_strategy(
            parsed_file,
            processing_result,
            &self.config,
        );

        match strategy {
            GenerationDecision::RuleOnly => (
                self.rule_generator
                    .generate_with_groups(parsed_file, processing_result)
                    .await,
                false,
            ),
            GenerationDecision::ModelEnhanced => {
                // Rule-based + model enhancement with group context

                let mut summary = self
                    .rule_generator
                    .generate_with_groups(parsed_file, processing_result)
                    .await;

                // Call model for enhancement with group context
                match self
                    .generate_model_summary_with_groups(parsed_file, processing_result)
                    .await
                {
                    Ok(model_summary) => {
                        // Replace the rule-based summary text with the model output
                        summary.summary_text = model_summary.summary_text;
                        // Mark as model-enhanced
                        summary.importance_level = ImportanceLevel::High;
                        (summary, false)
                    }
                    Err(e) => {
                        let rate_limited = matches!(e, cce_llm::LlmError::RateLimitExceeded(_));
                        self.handle_model_error(&e, &parsed_file.path, "ModelEnhanced");
                        // Keep rule-based summary as fallback
                        (summary, rate_limited)
                    }
                }
            }
        }
    }

    /// Extract imports from the parsed-file cache, falling back to AST parsing.
    fn extract_imports_from_file(&self, parsed_file: &ParsedFile) -> Vec<String> {
        crate::summary::dependencies::collect_imports(parsed_file)
    }

    /// Extract all exported symbol names from the parsed entities.
    fn extract_exports_from_file(&self, parsed_file: &ParsedFile) -> Vec<String> {
        crate::summary::dependencies::collect_exports(parsed_file)
    }

    /// Generate model-based summary using processing result groups
    ///
    /// This method implements retry logic and timeout control for LLM requests.
    /// On transient errors (rate limit, timeout, network issues), it will retry
    /// up to `config.max_retries` times with exponential backoff.
    async fn generate_model_summary_with_groups(
        &self,
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
    ) -> Result<FileSummary, cce_llm::LlmError> {
        let start = std::time::Instant::now();
        let imports = self.extract_imports_from_file(parsed_file);
        let exports = self.extract_exports_from_file(parsed_file);
        let prompt = self.build_summary_prompt_from_groups(
            parsed_file,
            processing_result,
            &imports,
            &exports,
        );

        let messages = vec![
            Message::system(
                "You are a code analysis expert. Generate a concise plain-text summary of code files using entity grouping information.",
            ),
            Message::user(prompt),
        ];

        let mut last_error = None;
        let max_retries = self.config.max_retries.max(1);

        for attempt in 0..max_retries {
            let chat_result = if self.config.request_timeout_secs > 0 {
                // Apply timeout if configured
                let timeout_duration =
                    std::time::Duration::from_secs(self.config.request_timeout_secs);
                match tokio::time::timeout(
                    timeout_duration,
                    self.llm_client.chat(&messages, &self.chat_config),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => {
                        let error = cce_llm::LlmError::Timeout(
                            cce_types::error::common::TimeoutError(format!(
                                "LLM request timed out after {} seconds",
                                self.config.request_timeout_secs
                            )),
                        );
                        last_error = Some(error);
                        if attempt < max_retries - 1 {
                            // Exponential backoff: 100ms, 200ms, 400ms, ...
                            let backoff_ms = 100 * 2u64.pow(attempt as u32);
                            tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                            continue;
                        }
                        return Err(last_error.unwrap());
                    }
                }
            } else {
                self.llm_client.chat(&messages, &self.chat_config).await
            };

            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

            match chat_result {
                Ok(result) => {
                    let parsed = self.parse_model_response(&result, parsed_file, imports, exports);
                    if let Some(metrics) = &self.metrics {
                        metrics.record_model_enhancement(elapsed_ms, parsed.is_ok());
                    }

                    return parsed;
                }
                Err(err) => {
                    // Check if we should retry
                    let should_retry = matches!(
                        err,
                        cce_llm::LlmError::RateLimitExceeded(_)
                            | cce_llm::LlmError::Timeout(_)
                            | cce_llm::LlmError::Http(_)
                            | cce_llm::LlmError::HttpStatus { .. }
                    );

                    if should_retry && attempt < max_retries - 1 {
                        // Exponential backoff for retries
                        let backoff_ms = 100 * 2u64.pow(attempt as u32);
                        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                        last_error = Some(err);
                        continue;
                    }

                    if let Some(metrics) = &self.metrics {
                        metrics.record_model_enhancement(elapsed_ms, false);
                    }

                    return Err(err);
                }
            }
        }

        // This should never be reached, but satisfy the compiler
        Err(last_error
            .unwrap_or_else(|| cce_llm::LlmError::Internal("No attempts were made".to_string())))
    }

    /// Build the prompt for summary generation using processing result groups
    fn build_summary_prompt_from_groups(
        &self,
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
        imports: &[String],
        exports: &[String],
    ) -> String {
        let file_path = &parsed_file.path;
        let language = parsed_file.language.to_string();

        // Format groups as folded content
        let folded_content = self.format_groups_as_folded(&processing_result.groups, parsed_file);

        // Get entity overview with call information
        let mut entities_with_info = Vec::new();
        for group in &processing_result.groups {
            if let Some(ref header) = group.header {
                let entity_info = format!("{}: {}", group.group_type, header.name);

                entities_with_info.push(entity_info);
            }
        }

        let limited_imports: Vec<_> = imports
            .iter()
            .take(self.config.max_imports)
            .cloned()
            .collect();

        let limited_exports: Vec<_> = exports
            .iter()
            .take(self.config.max_imports)
            .cloned()
            .collect();

        // Get documented entities
        let documented_entities: Vec<_> = parsed_file
            .entities
            .iter()
            .filter_map(|e| {
                e.doc_comment.as_deref().map(|doc| {
                    format!(
                        "{}: {}\n  Doc: {}",
                        e.kind,
                        e.name,
                        doc.lines().next().unwrap_or(doc).trim()
                    )
                })
            })
            .take(10)
            .collect();

        // Build doc section only if there are documented entities
        let doc_section = if documented_entities.is_empty() {
            String::new()
        } else {
            format!(
                "\nDocumented entities (use these to understand intent):\n{}\n",
                documented_entities.join("\n")
            )
        };

        format!(
            r#"Write one plain-text summary sentence or short paragraph for this code file.
Do not use markdown, headings, bullet points, code fences, JSON, or any explicit field labels.
Keep it concise and focused on what the file does, using the entity grouping information as context.

File Path: {}
Language: {}

Entity Groups:
{}{}

Imports/Dependencies:
{}

Exports:
{}

Folded code representation:
{}
"#,
            file_path,
            language,
            if entities_with_info.is_empty() {
                "(none)".to_string()
            } else {
                entities_with_info.join("\n")
            },
            doc_section,
            if limited_imports.is_empty() {
                "(none)".to_string()
            } else {
                limited_imports.join(", ")
            },
            if limited_exports.is_empty() {
                "(none)".to_string()
            } else {
                limited_exports.join(", ")
            },
            folded_content
        )
    }

    /// Format entity groups as folded content representation
    fn format_groups_as_folded(
        &self,
        groups: &[crate::grouper::EntityGroup],
        parsed_file: &ParsedFile,
    ) -> String {
        use crate::grouper::GroupType;

        let mut lines = Vec::new();
        lines.push(format!("// File: {}", parsed_file.path));
        lines.push(format!("// Language: {}", parsed_file.language));
        lines.push("// Entity Groups:".to_string());

        for group in groups {
            let line = match group.group_type {
                GroupType::ClassWithMethods => {
                    if let Some(ref header) = group.header {
                        format!("{} | class {}", group.span.start_position.row, header.name,)
                    } else {
                        format!("{} | class", group.span.start_position.row,)
                    }
                }
                GroupType::InterfaceWithImpls | GroupType::TraitWithImpls => {
                    if let Some(ref header) = group.header {
                        format!("{} | {}", group.span.start_position.row, header.name,)
                    } else {
                        format!("{} | interface/trait", group.span.start_position.row,)
                    }
                }
                GroupType::RelatedFunctions => {
                    if let Some(ref header) = group.header {
                        format!("{} | {}", group.span.start_position.row, header.name,)
                    } else {
                        format!("{} | related functions", group.span.start_position.row,)
                    }
                }
                GroupType::Standalone => {
                    if let Some(ref header) = group.header {
                        format!("{} | {}", group.span.start_position.row, header.name)
                    } else {
                        format!("{} | standalone entity", group.span.start_position.row)
                    }
                }
                _ => {
                    if let Some(ref header) = group.header {
                        format!(
                            "{} | {} ({})",
                            group.span.start_position.row, header.name, group.group_type
                        )
                    } else {
                        format!(
                            "{} | {} entity",
                            group.span.start_position.row, group.group_type
                        )
                    }
                }
            };
            lines.push(line);
        }

        lines.join("\n")
    }

    /// Parse the model response into a FileSummary
    fn parse_model_response(
        &self,
        result: &ChatResult,
        parsed_file: &ParsedFile,
        imports: Vec<String>,
        exports: Vec<String>,
    ) -> Result<FileSummary, cce_llm::LlmError> {
        let content = &result.content;
        let mut summary = FileSummary::new(&parsed_file.path);

        summary.language = parsed_file.language.to_string();
        summary.line_count = parsed_file.source.lines().count() as u32;
        summary.entity_count = parsed_file.entities.len() as u32;
        summary.importance_level = ImportanceLevel::High; // Model-enhanced summaries are high importance

        // Extract entities from file
        summary.main_entities = parsed_file
            .entities
            .iter()
            .take(self.config.max_entities)
            .map(|e| e.name.clone())
            .collect();

        summary.imports = imports.into_iter().take(self.config.max_imports).collect();
        summary.exports = exports.into_iter().take(self.config.max_imports).collect();

        let summary_text = normalize_whitespace(content);
        if summary_text.is_empty() {
            return Err(cce_llm::LlmError::invalid_response(
                "LLM returned an empty summary".to_string(),
            ));
        }
        summary.summary_text = summary_text;

        // Truncate if too long (using token-based truncation for language-agnostic behavior)
        let estimated_tokens = estimate_tokens(&summary.summary_text);
        if estimated_tokens > self.config.max_summary_length {
            let content_budget = self.config.max_summary_length.saturating_sub(1);
            let split_point =
                TokenEstimator::default().find_split_point(&summary.summary_text, content_budget);
            summary.summary_text.truncate(split_point);
            summary.summary_text.push_str("...");
        }

        Ok(summary)
    }

    /// Generate summaries for multiple files (batch processing)
    ///
    /// Runs file generations concurrently up to the configured limit.
    ///
    /// Files whose model enhancement failed due to a rate limit are retried
    /// once after the whole batch completes, so the retries do not add load
    /// while the batch is still generating.
    pub async fn generate_batch_impl(&self, parsed_files: &[ParsedFile]) -> Vec<FileSummary> {
        let mut results = Vec::with_capacity(parsed_files.len());
        let mut retry_indices: Vec<usize> = Vec::new();

        for batch in parsed_files.chunks(self.config.max_concurrent.max(1)) {
            let base = results.len();
            let chunk: Vec<(FileSummary, bool)> = futures::future::join_all(
                batch.iter().map(|file| self.generate_impl_tracked(file)),
            )
            .await;
            for (offset, (summary, rate_limited)) in chunk.into_iter().enumerate() {
                if rate_limited {
                    retry_indices.push(base + offset);
                }
                results.push(summary);
            }
        }

        if !retry_indices.is_empty() {
            tracing::warn!(
                count = retry_indices.len(),
                "Rate limited during batch generation, retrying deferred files"
            );
            for &index in &retry_indices {
                let (summary, _) = self.generate_impl_tracked(&parsed_files[index]).await;
                results[index] = summary;
            }
        }

        results
    }

    /// Generate summaries for multiple files with pre-computed processing results
    ///
    /// This method allows callers to provide pre-computed `ProcessingResult`s
    /// to avoid redundant preprocessing when the same files are processed multiple times.
    /// If `processing_results` is `None`, preprocessing will be performed internally for each file.
    pub async fn generate_batch_impl_with_results(
        &self,
        parsed_files: &[ParsedFile],
        processing_results: Option<&[ProcessingResult]>,
    ) -> Vec<FileSummary> {
        match processing_results {
            Some(results) => {
                assert_eq!(
                    parsed_files.len(),
                    results.len(),
                    "Number of processing results must match number of parsed files"
                );
                self.generate_batch_with_groups_impl(parsed_files, results)
                    .await
            }
            None => self.generate_batch_impl(parsed_files).await,
        }
    }

    async fn generate_batch_with_groups_impl(
        &self,
        parsed_files: &[ParsedFile],
        processing_results: &[ProcessingResult],
    ) -> Vec<FileSummary> {
        let mut results = Vec::with_capacity(parsed_files.len().min(processing_results.len()));
        let mut retry_indices: Vec<usize> = Vec::new();
        let inputs: Vec<_> = parsed_files.iter().zip(processing_results).collect();
        for batch in inputs.chunks(self.config.max_concurrent.max(1)) {
            let base = results.len();
            let chunk: Vec<(FileSummary, bool)> = futures::future::join_all(
                batch
                    .iter()
                    .map(|&(file, result)| self.generate_with_groups_tracked(file, result)),
            )
            .await;
            for (offset, (summary, rate_limited)) in chunk.into_iter().enumerate() {
                if rate_limited {
                    retry_indices.push(base + offset);
                }
                results.push(summary);
            }
        }

        if !retry_indices.is_empty() {
            tracing::warn!(
                count = retry_indices.len(),
                "Rate limited during batch generation, retrying deferred files"
            );
            for &index in &retry_indices {
                let (file, result) = inputs[index];
                let (summary, _) = self.generate_with_groups_tracked(file, result).await;
                results[index] = summary;
            }
        }

        results
    }

    /// Handle model generation errors with appropriate logging
    fn handle_model_error(&self, error: &cce_llm::LlmError, file_path: &str, strategy: &str) {
        use cce_llm::LlmError;

        match error {
            LlmError::RateLimitExceeded(retry_after) => {
                tracing::warn!(
                    file = %file_path,
                    strategy = %strategy,
                    retry_after_ms = retry_after,
                    "Rate limit exceeded, using rule-based fallback"
                );
            }
            LlmError::Auth(msg) => {
                tracing::error!(
                    file = %file_path,
                    strategy = %strategy,
                    error = %msg,
                    "Authentication failed - please check LLM API key configuration"
                );
            }
            LlmError::Config(msg) => {
                tracing::error!(
                    file = %file_path,
                    strategy = %strategy,
                    error = %msg,
                    "LLM configuration error - please check config"
                );
            }
            LlmError::ModelNotFound(model) => {
                tracing::error!(
                    file = %file_path,
                    strategy = %strategy,
                    model = %model,
                    "Model not found - please check model name"
                );
            }
            LlmError::TokenLimitExceeded(actual, limit) => {
                tracing::warn!(
                    file = %file_path,
                    strategy = %strategy,
                    actual_tokens = actual,
                    limit_tokens = limit,
                    "Token limit exceeded, using rule-based fallback"
                );
            }
            LlmError::Timeout(_) => {
                tracing::warn!(
                    file = %file_path,
                    strategy = %strategy,
                    "LLM request timeout, using rule-based fallback"
                );
            }
            LlmError::Http(msg) => {
                tracing::warn!(
                    file = %file_path,
                    strategy = %strategy,
                    error = %msg,
                    "HTTP error during LLM request, using rule-based fallback"
                );
            }
            LlmError::HttpStatus { status, message } => {
                tracing::warn!(
                    file = %file_path,
                    strategy = %strategy,
                    status = status,
                    error = %message,
                    "HTTP status error during LLM request, using rule-based fallback"
                );
            }
            LlmError::Api(msg) => {
                tracing::warn!(
                    file = %file_path,
                    strategy = %strategy,
                    error = %msg,
                    "LLM API error, using rule-based fallback"
                );
            }
            LlmError::InvalidResponse(msg) => {
                tracing::warn!(
                    file = %file_path,
                    strategy = %strategy,
                    error = %msg,
                    "Invalid LLM response, using rule-based fallback"
                );
            }
            LlmError::InvalidInput(msg) => {
                tracing::warn!(
                    file = %file_path,
                    strategy = %strategy,
                    error = %msg,
                    "Invalid input for LLM, using rule-based fallback"
                );
            }
            LlmError::Internal(msg) => {
                tracing::error!(
                    file = %file_path,
                    strategy = %strategy,
                    error = %msg,
                    "Internal LLM error, using rule-based fallback"
                );
            }
        }
    }

    /// Generate hierarchical summary for large files
    pub async fn generate_hierarchical(
        &self,
        parsed_file: &ParsedFile,
        component_summaries: &[ComponentSummary],
    ) -> Result<FileSummary, cce_llm::LlmError> {
        let prompt = format!(
            r#"Generate a file-level summary based on these component summaries:

File: {}
Language: {}

Components:
{}

Generate a concise file summary that captures the overall purpose and main functionality."#,
            parsed_file.path,
            parsed_file.language,
            component_summaries
                .iter()
                .map(|c| format!("- {} ({}): {}", c.name, c.entity_type, c.summary))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let messages = vec![
            Message::system("You are a code analysis expert."),
            Message::user(prompt),
        ];

        let result = self.llm_client.chat(&messages, &self.chat_config).await?;

        let imports = self.extract_imports_from_file(parsed_file);
        let exports = self.extract_exports_from_file(parsed_file);
        let mut summary = self.parse_model_response(&result, parsed_file, imports, exports)?;

        // Preserve component information
        summary.importance_level = ImportanceLevel::High;

        Ok(summary)
    }
}

#[async_trait::async_trait]
impl<C: LlmClient> crate::summary::types::SummaryGenerator for ModelEnhancedGenerator<C> {
    async fn generate(&self, parsed_file: &ParsedFile) -> FileSummary {
        self.generate_impl(parsed_file).await
    }

    async fn generate_batch(&self, parsed_files: &[ParsedFile]) -> Vec<FileSummary> {
        self.generate_batch_impl(parsed_files).await
    }

    async fn generate_with_groups(
        &self,
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
    ) -> FileSummary {
        self.generate_with_groups_impl(parsed_file, processing_result)
            .await
    }

    async fn generate_batch_with_groups(
        &self,
        parsed_files: &[ParsedFile],
        processing_results: &[ProcessingResult],
    ) -> Vec<FileSummary> {
        self.generate_batch_with_groups_impl(parsed_files, processing_results)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_llm::LlmError;
    use cce_types::{
        Entity, EntityId, EntityKind, ImportKind, ImportTable, Language, StandardizedImport,
    };

    /// Test stub for the `LlmClient` port (no network in unit tests)
    struct TestLlmClient;

    impl LlmClient for TestLlmClient {
        // clippy::manual_async_fn: `async fn` cannot express the `+ Send` bound
        // on the returned future; the trait is RPITIT-style to stay dyn-free.
        #[allow(clippy::manual_async_fn)]
        fn chat(
            &self,
            _messages: &[Message],
            _config: &ChatConfig,
        ) -> impl std::future::Future<Output = Result<ChatResult, LlmError>> + Send {
            async move {
                Ok(ChatResult {
                    content: "mock".to_string(),
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                })
            }
        }
    }

    fn create_test_parsed_file() -> ParsedFile {
        let mut file = ParsedFile::new(
            Language::Rust,
            "src/api.rs".to_string(),
            "pub fn handle_request() {}",
        );

        let entity = Entity {
            id: EntityId(0),
            kind: EntityKind::Function,
            name: "handle_request".to_string(),
            signature: "pub fn handle_request()".to_string(),
            parameters: Vec::new(),
            return_type: None,
            span: cce_types::Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: Some("Handle HTTP requests".to_string()),
            modifiers: vec!["pub".to_string()],
            attributes: std::collections::HashMap::new(),
            metadata: std::collections::HashMap::from([(
                "visibility".to_string(),
                "pub".to_string(),
            )]),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        };
        file.add_entity(entity);

        file
    }

    fn create_test_llm_client() -> Arc<TestLlmClient> {
        Arc::new(TestLlmClient)
    }

    fn create_test_chat_config() -> ChatConfig {
        ChatConfig {
            model: "gpt-4o-mini".to_string(),
            max_tokens: 500,
            temperature: 0.3,
            ..Default::default()
        }
    }

    #[test]
    fn test_parse_model_response() {
        let llm_client = create_test_llm_client();
        let chat_config = create_test_chat_config();
        let generator = ModelEnhancedGenerator::new(llm_client, chat_config);

        let parsed_file = create_test_parsed_file();

        let chat_result = ChatResult {
            content: "Handles API requests and routes by validating input, dispatching business logic, and returning formatted responses.".to_string(),
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };

        let imports = generator.extract_imports_from_file(&parsed_file);
        let exports = generator.extract_exports_from_file(&parsed_file);
        let summary = generator
            .parse_model_response(&chat_result, &parsed_file, imports, exports)
            .expect("Failed to parse model response");

        assert_eq!(
            summary.summary_text,
            "Handles API requests and routes by validating input, dispatching business logic, and returning formatted responses."
        );
        assert!(summary.tags.is_empty());
        assert_eq!(summary.main_entities.len(), 1);
    }

    #[test]
    fn test_extract_imports_uses_cached_table() {
        let llm_client = create_test_llm_client();
        let chat_config = create_test_chat_config();
        let generator = ModelEnhancedGenerator::new(llm_client, chat_config);
        let mut parsed_file = create_test_parsed_file();
        let mut import_table = ImportTable::default();
        import_table
            .add_standardized_import(StandardizedImport::new(ImportKind::ModuleImport, "zeta"));
        import_table
            .add_standardized_import(StandardizedImport::new(ImportKind::ModuleImport, "alpha"));
        import_table
            .add_standardized_import(StandardizedImport::new(ImportKind::ModuleImport, "zeta"));
        parsed_file.import_table = Some(import_table);

        assert_eq!(
            generator.extract_imports_from_file(&parsed_file),
            vec!["alpha".to_string(), "zeta".to_string()]
        );
    }

    #[test]
    fn test_parse_model_response_records_exports() {
        // The model-enhanced summary collects ALL exports of the
        // file into the structured `exports` field, mirroring rule-based.
        let llm_client = create_test_llm_client();
        let chat_config = create_test_chat_config();
        let generator = ModelEnhancedGenerator::new(llm_client, chat_config);
        let parsed_file = create_test_parsed_file();
        let chat_result = ChatResult {
            content: "Handles API requests.".to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        };

        let exports = generator.extract_exports_from_file(&parsed_file);
        let summary = generator
            .parse_model_response(&chat_result, &parsed_file, Vec::new(), exports)
            .expect("Failed to parse model response");

        assert_eq!(
            summary.exports,
            vec!["handle_request".to_string()],
            "the model-enhanced summary must carry all exports of the file"
        );
    }

    #[test]
    fn test_summary_prompt_includes_imports_and_exports() {
        // Imports and exports reach the model prompt so the LLM can
        // describe the file's dependency surface from the file-level summary.
        let llm_client = create_test_llm_client();
        let chat_config = create_test_chat_config();
        let generator = ModelEnhancedGenerator::new(llm_client, chat_config);
        let parsed_file = create_test_parsed_file();
        let processing_result = ProcessingResult {
            groups: Vec::new(),
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: crate::grouper::ProcessingStats::default(),
        };

        let prompt = generator.build_summary_prompt_from_groups(
            &parsed_file,
            &processing_result,
            &["std::fmt".to_string()],
            &["handle_request".to_string()],
        );

        assert!(
            prompt.contains("Imports/Dependencies:\nstd::fmt"),
            "the model prompt must list all imports"
        );
        assert!(
            prompt.contains("Exports:\nhandle_request"),
            "the model prompt must list all exports"
        );
    }

    #[test]
    fn test_parse_model_response_truncates_unicode_safely() {
        let llm_client = create_test_llm_client();
        let chat_config = create_test_chat_config();
        let mut config = SummaryConfig::model_enhanced();
        config.max_summary_length = 4;
        let generator = ModelEnhancedGenerator::with_config(llm_client, chat_config, config);
        let parsed_file = create_test_parsed_file();
        let chat_result = ChatResult {
            content: "功能😀摘要内容很长".to_string(),
            prompt_tokens: 1,
            completion_tokens: 10,
            total_tokens: 11,
        };

        let summary = generator
            .parse_model_response(&chat_result, &parsed_file, Vec::new(), Vec::new())
            .expect("Unicode summary should truncate without panicking");

        assert!(summary.summary_text.ends_with("..."));
        assert!(
            summary
                .summary_text
                .is_char_boundary(summary.summary_text.len())
        );
    }

    #[tokio::test]
    async fn test_generate_with_groups_reuses_processing_result() {
        let llm_client = create_test_llm_client();
        let chat_config = create_test_chat_config();
        let generator = ModelEnhancedGenerator::with_config(
            llm_client,
            chat_config,
            SummaryConfig::rule_based(),
        );
        let parsed_file = create_test_parsed_file();
        let processing_result = ProcessingResult {
            groups: Vec::new(),
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: crate::grouper::ProcessingStats {
                input_entities: 42,
                ..Default::default()
            },
        };

        let summary = generator
            .generate_with_groups_impl(&parsed_file, &processing_result)
            .await;

        assert_eq!(summary.entity_count, 42);
    }

    #[tokio::test]
    async fn test_generate_impl_with_result_uses_provided_result() {
        let llm_client = create_test_llm_client();
        let chat_config = create_test_chat_config();
        let generator = ModelEnhancedGenerator::with_config(
            llm_client,
            chat_config,
            SummaryConfig::rule_based(),
        );
        let parsed_file = create_test_parsed_file();
        let processing_result = ProcessingResult {
            groups: Vec::new(),
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: crate::grouper::ProcessingStats {
                input_entities: 10,
                ..Default::default()
            },
        };

        // Test with provided result
        let summary = generator
            .generate_impl_with_result(&parsed_file, Some(&processing_result))
            .await;
        assert_eq!(summary.entity_count, 10);

        // Test with None result (should create processing result internally)
        let summary = generator
            .generate_impl_with_result(&parsed_file, None)
            .await;
        assert!(summary.entity_count >= 1);
    }

    #[tokio::test]
    async fn test_generate_batch_impl_with_results() {
        let llm_client = create_test_llm_client();
        let chat_config = create_test_chat_config();
        let generator = ModelEnhancedGenerator::with_config(
            llm_client,
            chat_config,
            SummaryConfig::rule_based(),
        );

        let mut parsed_file1 = ParsedFile::new(
            Language::Rust,
            "src/test1.rs".to_string(),
            "pub fn test1() {}",
        );
        parsed_file1.add_entity(Entity::new(
            EntityId(0),
            EntityKind::Function,
            "test1".to_string(),
            cce_types::Span::default(),
        ));

        let mut parsed_file2 = ParsedFile::new(
            Language::Rust,
            "src/test2.rs".to_string(),
            "pub fn test2() {}",
        );
        parsed_file2.add_entity(Entity::new(
            EntityId(0),
            EntityKind::Function,
            "test2".to_string(),
            cce_types::Span::default(),
        ));

        let processing_result1 = ProcessingResult {
            groups: Vec::new(),
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: crate::grouper::ProcessingStats {
                input_entities: 5,
                ..Default::default()
            },
        };

        let processing_result2 = ProcessingResult {
            groups: Vec::new(),
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: crate::grouper::ProcessingStats {
                input_entities: 10,
                ..Default::default()
            },
        };

        // Test with provided results
        let summaries = generator
            .generate_batch_impl_with_results(
                &[parsed_file1.clone(), parsed_file2.clone()],
                Some(&[processing_result1.clone(), processing_result2.clone()]),
            )
            .await;
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].entity_count, 5);
        assert_eq!(summaries[1].entity_count, 10);

        // Test with None results
        let summaries = generator
            .generate_batch_impl_with_results(&[parsed_file1, parsed_file2], None)
            .await;
        assert_eq!(summaries.len(), 2);
    }

    #[test]
    fn test_summary_config_retry_and_timeout_settings() {
        let config = SummaryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.request_timeout_secs, 30);
        assert!(config.enable_graceful_degradation);

        let config = SummaryConfig {
            max_retries: 5,
            request_timeout_secs: 60,
            enable_graceful_degradation: false,
            ..Default::default()
        };
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.request_timeout_secs, 60);
        assert!(!config.enable_graceful_degradation);
    }

    #[test]
    fn test_summary_storage_separation() {
        // Test that SummaryStorage can be created
        // This is a basic test to verify the separation of concerns
        use crate::summary::types::SummaryOrchestrator;

        let strategies: Vec<Box<dyn crate::summary::types::SummaryGenerationStrategy>> = vec![];
        let orchestrator = SummaryOrchestrator::new(strategies, 0);
        assert_eq!(orchestrator.strategies.len(), 0);
        assert_eq!(orchestrator.default_strategy_index, 0);
    }
}
