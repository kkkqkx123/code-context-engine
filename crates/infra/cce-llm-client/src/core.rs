//! LLM Core Module
//!
//! Shared configuration and error building blocks for the llm-suite-backed
//! providers.

pub mod config;
pub mod error;

pub use config::{ChatConfig, EmbeddingConfig, ResponseFormat};
pub use error::{LlmConfigError, LlmError};
