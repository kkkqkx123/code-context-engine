//! Generator module for summary generation
//!
//! Provides rule-based and model-enhanced summary generators,
//! as well as specialized generators for specific file types.

pub(crate) mod entity_overview;
pub mod model_enhanced;
pub mod rule_based;
pub mod specialized;

// Re-export main types
pub use model_enhanced::{ComponentSummary, ModelEnhancedGenerator};
pub use rule_based::RuleBasedGenerator;
pub use specialized::{
    generate_config_file_summary, generate_documentation_summary, generate_generated_file_summary,
    generate_schema_file_summary, generate_specialized_summary, generate_test_file_summary,
};
