//! Embedding Service Module

pub mod handler;
pub mod provider;
pub mod response_parser;
pub mod types;

pub(crate) mod preprocessor;

pub use handler::EmbeddingRequestHandler;
pub use provider::OpenAICompatibleProvider;
pub use types::EmbeddingResult;
