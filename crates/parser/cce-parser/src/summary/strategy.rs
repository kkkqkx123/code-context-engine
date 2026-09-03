//! Strategy module for summary generation
//!
//! Provides file categorization and importance decision logic.

pub mod categorization;
pub mod decision;
pub mod doc_quality;

// Re-export main types
pub use categorization::{FileCategory, TestType};
pub use decision::{DecisionContext, ImportanceDecision, ImportanceLevel};
