//! Configuration modules
//!
//! This module contains all module-specific configuration definitions.
//! Each module's configuration is defined in its own file for clarity.

pub mod ast_to_nl;
pub mod cache;
pub(crate) mod defaults;
pub mod embedder;
pub mod export;
pub mod grouper;
pub mod llm_models;
pub mod orchestrator;
pub mod pattern_detection;
pub mod relation;
pub mod rerank;
pub mod scanner;
pub mod search;
pub mod storage;
pub mod summary;
pub mod symbol_resolution;

// Re-export all configuration types
pub use ast_to_nl::*;
pub use cache::*;
pub use embedder::*;
pub use export::*;
pub use grouper::*;
pub use llm_models::*;
pub use orchestrator::*;
pub use relation::*;
pub use rerank::*;
pub use scanner::*;
pub use search::*;
pub use storage::*;
pub use summary::*;
pub use symbol_resolution::SymbolResolutionConfig;
