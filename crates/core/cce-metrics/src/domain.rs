//! Domain-level metrics wrappers
//!
//! This module provides high-level metric structures for domain modules,
//! encapsulating the low-level metric primitives (Counter, Gauge, Histogram)
//! with domain-specific semantics.

// Pipeline-level metrics (individual processing stages)
pub mod pipeline;

// Plugin execution metrics
pub mod plugin;

// LLM retry and circuit breaker metrics
pub mod llm;

// Scanner metrics
pub mod scanner;

// HTTP request metrics
pub mod http;

// Orchestrator-level metrics (system coordination)
pub mod orchestrator;

// Queue backpressure metrics
pub mod queue;

// Tokio runtime metrics
pub mod runtime;

// Search engine metrics
pub mod search;

// Storage backend metrics
pub mod storage;

// System resource metrics
pub mod system;

pub use http::HttpMetrics;
pub use llm::LlmRetryMetrics;
pub use orchestrator::{HotUpdateMetrics, HotUpdateStorageMetrics, QueryMetrics, WatchMetrics};
pub use pipeline::{
    EmbeddingMetrics, FileProcessingMetrics, ParserMetrics, PipelineStageMetrics, RelationMetrics,
    SummaryMetrics,
};
pub use plugin::{BackgroundTaskMetrics, PluginMetrics, RerankMetrics};
pub use queue::QueueMetrics;
pub use runtime::RuntimeMetrics;
pub use scanner::ScannerMetrics;
pub use search::SearchMetrics;
pub use storage::{Bm25Metrics, QdrantMetrics, SqliteMetrics};
pub use system::SystemMetrics;
