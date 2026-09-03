//! Summary generation module
//!
//! Provides file-level summary generation for hierarchical retrieval.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Summary Generation                       │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                             │
//! │  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐   │
//! │ │ Strategy │────→│ Decision │────→│ Generator │ │ │
//! │  │             │     │             │     │             │   │
//! │  │ - Category  │     │ - Importance│     │ - RuleBased │   │
//! │  │ - TestType  │     │ - Strategy  │     │ - Model     │   │
//! │  └─────────────┘     └─────────────┘     └─────────────┘   │
//! │                                                             │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Generation Strategies
//!
//! - **Rule-based**: Fast, deterministic generation using heuristics
//! - **Model-enhanced**: LLM-enhanced summaries for high-importance files
//! - **Specialized**: Custom generators for specific file types (tests, configs, etc.)
//!
//! # Example
//!
//! ```rust,no_run
//! use cce_parser::summary::{RuleBasedGenerator, SummaryConfig};
//! use cce_types::ParsedFile;
//!
//! # async fn example(parsed_file: &ParsedFile) {
//! // Rule-based generation
//! let generator = RuleBasedGenerator::new();
//! let summary = generator.generate(parsed_file).await;
//!
//! println!("File: {}", summary.file_path);
//! println!("Summary: {}", summary.summary_text);
//! println!("Importance: {:?}", summary.importance_level);
//! # }
//! ```

pub(crate) mod dependencies;
pub(crate) mod file_folding;
pub(crate) mod generator;
pub(crate) mod strategy;
pub(crate) mod types;

// Re-export strategy types
pub use strategy::{
    DecisionContext, FileCategory, ImportanceDecision, ImportanceLevel, TestType, categorization,
    decision,
};

// Re-export from categorization for convenience
pub use strategy::categorization::{
    has_any_documentation, is_config_file, is_core_module, is_definition_only_file,
    is_documentation, is_entity_public, is_test_file, is_utility_file,
};

// Re-export generator types
pub use generator::{
    ComponentSummary, ModelEnhancedGenerator, RuleBasedGenerator, generate_config_file_summary,
    generate_documentation_summary, generate_generated_file_summary, generate_schema_file_summary,
    generate_specialized_summary, generate_test_file_summary, model_enhanced, rule_based,
    specialized,
};

// Re-export file folding types
pub use file_folding::{
    FileFolder, FileFoldingConfig, FoldMode, FoldedContent, FoldedSection, SectionType, fold_file,
    fold_file_minimal, is_folded_content_short,
};

// Re-export config types (unified with cce_core)
pub use cce_config::SummaryConfig;
pub use cce_config::modules::summary::SummaryGenerationStrategy as SummaryStrategy;

// Re-export types
pub use types::{
    FileSummary, GenerationDecision, SummaryGenerationStrategy, SummaryGenerator,
    SummaryOrchestrator, SummaryPayload,
};
