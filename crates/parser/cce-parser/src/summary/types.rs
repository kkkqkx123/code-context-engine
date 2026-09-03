//! Summary types for file-level semantic abstraction
//!
//! Provides types for file summaries used in hierarchical retrieval.

use serde::{Deserialize, Serialize};

use crate::ast_to_nl::clean_comment_content;
use crate::summary::SummaryConfig;
use crate::summary::strategy::categorization::FileCategory;
use cce_types::Language;
use cce_types::test_info::TestInfo;
use cce_utils::normalize_whitespace;

// Re-export ImportanceLevel from strategy module
pub use crate::summary::strategy::ImportanceLevel;

/// File summary for hierarchical retrieval
///
/// Captures high-level semantic information about a code file
/// to enable fast file-level retrieval before detailed code search.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileSummary {
    /// File category for summarization and retrieval
    pub category: Option<FileCategory>,
    /// File path
    pub file_path: String,

    /// Programming language
    pub language: String,

    /// Summary text describing the file's purpose and contents
    pub summary_text: String,

    /// Main entity names (classes, functions, etc.)
    pub main_entities: Vec<String>,

    /// Imported modules/dependencies
    pub imports: Vec<String>,

    /// Exported symbols
    pub exports: Vec<String>,

    /// Total number of entities in the file
    pub entity_count: u32,

    /// Total lines of code
    pub line_count: u32,

    /// Tags for categorization (reliable signals only: path rules and AST metadata)
    pub tags: Vec<String>,

    /// File-level documentation comment (module/crate doc from //! comments)
    pub file_doc_comment: Option<String>,

    /// Importance level for prioritizing model-generated summaries
    pub importance_level: ImportanceLevel,

    /// File-level test marker (path rule plus aggregated group signals)
    pub test_info: TestInfo,
}

impl FileSummary {
    /// Create a new file summary
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
            category: None,
            ..Default::default()
        }
    }

    /// Set the programming language
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    /// Set the summary text
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary_text = summary.into();
        self
    }

    /// Add main entities
    pub fn with_entities(mut self, entities: Vec<String>) -> Self {
        self.main_entities = entities;
        self.entity_count = self.main_entities.len() as u32;
        self
    }

    /// Add imports
    pub fn with_imports(mut self, imports: Vec<String>) -> Self {
        self.imports = imports;
        self
    }

    /// Add exports
    pub fn with_exports(mut self, exports: Vec<String>) -> Self {
        self.exports = exports;
        self
    }

    /// Set line count
    pub fn with_line_count(mut self, lines: u32) -> Self {
        self.line_count = lines;
        self
    }

    /// Set tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set file-level documentation comment
    pub fn with_file_doc_comment(mut self, comment: Option<String>) -> Self {
        self.file_doc_comment = comment;
        self
    }

    /// Set importance level
    pub fn with_importance_level(mut self, level: ImportanceLevel) -> Self {
        self.importance_level = level;
        self
    }

    /// Set file category
    pub fn with_category(mut self, category: FileCategory) -> Self {
        self.category = Some(category);
        self
    }

    /// Compute the file-level test marker: the per-language path rule as the
    /// baseline, enhanced by merging every group-level marker (Test wins,
    /// `Ast` overrides `Path`). A file is a test file when the path rules
    /// match or any group carries a test signal. `groups = None` applies the
    /// path rule only (specialized and document summaries).
    pub fn with_file_level_test_info(
        mut self,
        language: &Language,
        path: &str,
        groups: Option<&[crate::grouper::EntityGroup]>,
    ) -> Self {
        let path_info = TestInfo::from_path(Some(language), path);
        self.test_info = match groups {
            Some(groups) => groups
                .iter()
                .fold(path_info, |acc, g| acc.merge(&g.test_info)),
            None => path_info,
        };
        self
    }

    /// Convert to Qdrant payload format
    pub fn to_payload(&self) -> SummaryPayload {
        SummaryPayload {
            file_path: self.file_path.clone(),
            summary: self.summary_text.clone(),
            main_entities: self.main_entities.clone(),
            imports: self.imports.clone(),
            exports: self.exports.clone(),
            entity_count: self.entity_count,
            line_count: self.line_count,
            language: self.language.clone(),
            tags: self.tags.clone(),
            file_doc_comment: self.file_doc_comment.clone(),
            importance_level: self.importance_level,
            category: self.category.map(|c| c.as_str().to_string()),
        }
    }

    /// Convert to BM25 document text
    pub fn to_bm25_text(&self) -> String {
        let mut parts = vec![
            format!("File: {}", self.file_path),
            format!("Language: {}", self.language),
            format!("Summary: {}", self.summary_text),
            format!("Entities: {}", self.main_entities.join(", ")),
            format!("Imports: {}", self.imports.join(", ")),
            format!("Exports: {}", self.exports.join(", ")),
            format!("Tags: {}", self.tags.join(", ")),
        ];
        if let Some(ref doc) = self.file_doc_comment {
            let cleaned_doc = normalize_whitespace(&clean_comment_content(doc));
            if !cleaned_doc.is_empty() {
                parts.push(format!("FileDoc: {}", cleaned_doc));
            }
        }
        parts.join("\n")
    }

    /// Convert to embedding text for vector storage
    pub fn to_embedding_text(&self) -> String {
        let mut parts = vec![
            format!("File: {}", self.file_path),
            format!("Language: {}", self.language),
            format!("Summary: {}", self.summary_text),
            format!("Entities: {}", self.main_entities.join(", ")),
            format!("Imports: {}", self.imports.join(", ")),
            format!("Exports: {}", self.exports.join(", ")),
            format!("Tags: {}", self.tags.join(", ")),
        ];
        if let Some(ref doc) = self.file_doc_comment {
            let cleaned_doc = normalize_whitespace(&clean_comment_content(doc));
            if !cleaned_doc.is_empty() {
                parts.push(format!("FileDoc: {}", cleaned_doc));
            }
        }
        parts.join("\n")
    }
}

/// Summary payload for vector database storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryPayload {
    /// File path (unique identifier)
    pub file_path: String,

    /// Summary text
    pub summary: String,

    /// Main entity names
    pub main_entities: Vec<String>,

    /// Imported modules
    pub imports: Vec<String>,

    /// Exported symbols
    pub exports: Vec<String>,

    /// Entity count
    pub entity_count: u32,

    /// Line count
    pub line_count: u32,

    /// Programming language
    pub language: String,

    /// Tags for filtering
    pub tags: Vec<String>,

    /// File-level documentation comment
    pub file_doc_comment: Option<String>,

    /// Importance level for prioritizing model-generated summaries
    pub importance_level: ImportanceLevel,
    /// File category for category-aware retrieval
    pub category: Option<String>,
}

/// Concrete generation decision after strategy evaluation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenerationDecision {
    /// Use only rule-based generation
    #[serde(rename = "rule_only")]
    RuleOnly,
    /// Use rule-based + model enhancement
    #[serde(rename = "model_enhanced")]
    ModelEnhanced,
}

/// Trait for summary generation strategies
///
/// This trait defines the interface for different summary generation strategies.
/// Implementations can use different algorithms (rule-based, model-enhanced, etc.)
/// to generate summaries based on file characteristics.
#[async_trait::async_trait]
pub trait SummaryGenerationStrategy: Send + Sync {
    /// Determine the generation decision for a file
    ///
    /// Returns `GenerationDecision` indicating which generation approach to use.
    fn determine_decision(
        &self,
        parsed_file: &cce_types::ParsedFile,
        processing_result: &crate::grouper::ProcessingResult,
        config: &SummaryConfig,
    ) -> GenerationDecision;

    /// Generate summary for a parsed file using this strategy
    async fn generate(&self, parsed_file: &cce_types::ParsedFile) -> FileSummary;

    /// Generate summary using pre-processor results
    async fn generate_with_groups(
        &self,
        parsed_file: &cce_types::ParsedFile,
        processing_result: &crate::grouper::ProcessingResult,
    ) -> FileSummary;

    /// Generate summaries for multiple files
    async fn generate_batch(&self, parsed_files: &[cce_types::ParsedFile]) -> Vec<FileSummary> {
        let mut results = Vec::with_capacity(parsed_files.len());
        for file in parsed_files {
            results.push(self.generate(file).await);
        }
        results
    }

    /// Generate summaries for multiple files with their processing results
    async fn generate_batch_with_groups(
        &self,
        parsed_files: &[cce_types::ParsedFile],
        processing_results: &[crate::grouper::ProcessingResult],
    ) -> Vec<FileSummary> {
        let mut results = Vec::with_capacity(parsed_files.len());
        for (file, result) in parsed_files.iter().zip(processing_results.iter()) {
            results.push(self.generate_with_groups(file, result).await);
        }
        results
    }
}

/// Trait for summary generators
#[async_trait::async_trait]
pub trait SummaryGenerator: Send + Sync {
    /// Generate summary for a parsed file
    async fn generate(&self, parsed_file: &cce_types::ParsedFile) -> FileSummary;

    /// Generate summary using pre-processor results
    ///
    /// This method leverages the `ProcessingResult` from `PreprocessingPipeline`
    /// to generate richer summaries with:
    /// - Class-method associations
    /// - Utility function identification
    /// - Merged call patterns
    /// - Group type information
    async fn generate_with_groups(
        &self,
        parsed_file: &cce_types::ParsedFile,
        processing_result: &crate::grouper::ProcessingResult,
    ) -> FileSummary {
        // Default implementation falls back to basic generation
        // Concrete implementations should override this to utilize group information
        let _ = processing_result;
        self.generate(parsed_file).await
    }

    /// Generate summaries for multiple files
    async fn generate_batch(&self, parsed_files: &[cce_types::ParsedFile]) -> Vec<FileSummary> {
        let mut results = Vec::with_capacity(parsed_files.len());
        for file in parsed_files {
            results.push(self.generate(file).await);
        }
        results
    }

    /// Generate summaries for multiple files with their processing results
    async fn generate_batch_with_groups(
        &self,
        parsed_files: &[cce_types::ParsedFile],
        processing_results: &[crate::grouper::ProcessingResult],
    ) -> Vec<FileSummary> {
        let mut results = Vec::with_capacity(parsed_files.len());
        for (file, result) in parsed_files.iter().zip(processing_results.iter()) {
            results.push(self.generate_with_groups(file, result).await);
        }
        results
    }
}

/// Summary generation orchestrator
///
/// Coordinates different generation strategies based on configuration and file characteristics.
/// This struct decouples the strategy selection from the actual generation implementation.
pub struct SummaryOrchestrator {
    /// Available generation strategies
    pub(crate) strategies: Vec<Box<dyn SummaryGenerationStrategy>>,
    /// Default strategy index
    pub(crate) default_strategy_index: usize,
}

impl SummaryOrchestrator {
    /// Create a new orchestrator with the given strategies
    pub fn new(strategies: Vec<Box<dyn SummaryGenerationStrategy>>, default_index: usize) -> Self {
        Self {
            strategies,
            default_strategy_index: default_index,
        }
    }

    /// Generate summary using the appropriate strategy
    pub async fn generate(
        &self,
        parsed_file: &cce_types::ParsedFile,
        processing_result: &crate::grouper::ProcessingResult,
        config: &SummaryConfig,
    ) -> FileSummary {
        // Find the appropriate strategy
        let strategy_index = self.find_strategy(parsed_file, processing_result, config);
        let strategy = &self.strategies[strategy_index];

        // Generate using the selected strategy
        strategy
            .generate_with_groups(parsed_file, processing_result)
            .await
    }

    /// Generate summaries for multiple files
    pub async fn generate_batch(
        &self,
        parsed_files: &[cce_types::ParsedFile],
        processing_results: &[crate::grouper::ProcessingResult],
        config: &SummaryConfig,
    ) -> Vec<FileSummary> {
        let mut results = Vec::with_capacity(parsed_files.len());

        for (file, result) in parsed_files.iter().zip(processing_results.iter()) {
            let summary = self.generate(file, result, config).await;
            results.push(summary);
        }

        results
    }

    /// Find the appropriate strategy index for a file
    fn find_strategy(
        &self,
        parsed_file: &cce_types::ParsedFile,
        processing_result: &crate::grouper::ProcessingResult,
        config: &SummaryConfig,
    ) -> usize {
        // Try each strategy in order, use the first one that matches
        for (index, strategy) in self.strategies.iter().enumerate() {
            let decision = strategy.determine_decision(parsed_file, processing_result, config);
            // For now, we use the decision to select the strategy
            // This can be extended to support more complex strategy selection logic
            match decision {
                GenerationDecision::RuleOnly => {
                    // Check if this strategy is rule-based
                    if index == 0 {
                        return index; // Assume first strategy is rule-based
                    }
                }
                GenerationDecision::ModelEnhanced => {
                    // Check if this strategy is model-enhanced
                    if index == 1 {
                        return index; // Assume second strategy is model-enhanced
                    }
                }
            }
        }

        // Fallback to default strategy
        self.default_strategy_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_summary_builder() {
        let summary = FileSummary::new("src/main.rs")
            .with_language("rust")
            .with_summary("Main entry point")
            .with_entities(vec!["main".to_string(), "init".to_string()])
            .with_imports(vec!["std::io".to_string()])
            .with_line_count(100)
            .with_importance_level(ImportanceLevel::High);

        assert_eq!(summary.file_path, "src/main.rs");
        assert_eq!(summary.language, "rust");
        assert_eq!(summary.main_entities.len(), 2);
        assert_eq!(summary.importance_level, ImportanceLevel::High);
    }

    #[test]
    fn test_to_bm25_text() {
        let summary = FileSummary::new("test.py")
            .with_language("python")
            .with_summary("Test module")
            .with_entities(vec!["test_func".to_string()]);

        let text = summary.to_bm25_text();
        assert!(text.contains("test.py"));
        assert!(text.contains("python"));
        assert!(text.contains("test_func"));
    }

    #[test]
    fn test_to_bm25_text_cleans_file_doc_comment() {
        let summary = FileSummary::new("test.rs")
            .with_language("rust")
            .with_summary("Test module")
            .with_file_doc_comment(Some(
                "# Overview\n\n```rust\nimpl<T> OnceCell<T> {}\n```\n".to_string(),
            ));

        let text = summary.to_bm25_text();
        assert!(text.contains("# Overview"));
        assert!(!text.contains("```rust"));
        assert!(!text.contains("impl<T>"));
    }

    #[test]
    fn test_to_embedding_text_cleans_file_doc_comment() {
        let summary = FileSummary::new("test.rs")
            .with_language("rust")
            .with_summary("Test module")
            .with_file_doc_comment(Some(
                "# Overview\n\n```rust\nimpl<T> OnceCell<T> {}\n```".to_string(),
            ));

        let text = summary.to_embedding_text();
        assert!(text.contains("FileDoc:"));
        assert!(text.contains("# Overview"));
        assert!(!text.contains("```rust"));
        assert!(!text.contains("impl<T>"));
    }

    #[test]
    fn test_to_embedding_text_includes_imports_exports() {
        let summary = FileSummary::new("src/lib.rs")
            .with_language("rust")
            .with_summary("Library module")
            .with_imports(vec!["std::io".to_string(), "tokio::sync".to_string()])
            .with_exports(vec!["public_func".to_string(), "PublicStruct".to_string()]);

        let text = summary.to_embedding_text();
        assert!(text.contains("Imports: std::io, tokio::sync"));
        assert!(text.contains("Exports: public_func, PublicStruct"));
    }

    #[test]
    fn test_importance_level() {
        let summary = FileSummary::new("test.rs").with_importance_level(ImportanceLevel::High);
        assert_eq!(summary.importance_level, ImportanceLevel::High);

        let summary = FileSummary::new("test.rs").with_importance_level(ImportanceLevel::Low);
        assert_eq!(summary.importance_level, ImportanceLevel::Low);

        let summary = FileSummary::new("test.rs"); // Default
        assert_eq!(summary.importance_level, ImportanceLevel::Medium);
    }
}
