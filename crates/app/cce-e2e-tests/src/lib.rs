//! Shared E2E test support for CCE (Code Context Engine).
//!
//! This crate exposes reusable fixtures, output helpers, and workflow test
//! utilities for integration tests under `tests/`.

pub mod aggregation_enhance;
pub mod annotation_review;
pub mod assertion;
pub mod baselines;
pub mod bench_data;
pub mod bench_gen;
pub mod benchmark_projects;
pub mod benchmark_runner;
pub mod bm25_parameter_benchmark;
pub mod cleanup;
pub mod cross_language;
pub mod direct_chunker;
pub mod embedding;
pub mod fixture;
pub mod index_test;
pub mod infra;
pub mod judgments;
pub mod mock_chat_server;
pub mod mock_embedding_server;
pub mod mock_qdrant;
pub mod nl_text_exporter;
pub mod output_manager;
pub mod output_reporter;
pub mod output_serializer;
pub mod pipeline_debug;
pub mod query_aggregation;
pub mod query_review;
pub mod query_test;
pub mod range_evaluator;
pub mod relation_review;
pub mod relation_snapshot;
pub mod rerank_benchmark;
pub mod retrieval_method;
pub mod review_export;
pub mod review_filter;
pub mod storage;
pub mod structured_output;
pub mod stub_embedder;
pub mod type_inference_assert;
pub mod type_inference_cases;

pub use assertion::{ExpectedIndexResult, assert_index_result};
pub use benchmark_projects::BenchmarkProject;
pub use cleanup::init_minimal_logging;
pub use embedding::{EmbeddingConfig, EmbeddingProviderType};
pub use fixture::{EmptyFixture, FixtureCategory, FixtureSpec, TestFixture};
pub use index_test::IndexWorkflowTest;
pub use nl_text_exporter::NlTextDocumentExporter;
pub use output_manager::{OutputBuilder, OutputCategory, OutputConfig, OutputManager};
pub use output_reporter::{OutputReporter, ReportMetadata};
pub use output_serializer::{OutputFormat, OutputSerializer, SerializableIndexResult};
pub use pipeline_debug::PipelineDebugExporter;
pub use query_test::QueryWorkflowTest;
pub use storage::StorageConfig;
