//! Compatibility shim for workflow tests.
//!
//! The workflow suite keeps shared behavior here, while fixtures live in the
//! single crate-root `fixtures/` tree loaded through `cce_e2e_tests::fixture`.

#![allow(unused_imports)]

pub mod assertion {
    pub use cce_e2e_tests::assertion::*;
}

pub mod cleanup {
    pub use cce_e2e_tests::cleanup::*;
}

pub mod embedding {
    pub use cce_e2e_tests::embedding::*;
}

/// Helper: construct a mock EmbeddingConfig for self-contained tests.
pub fn mock_embedding() -> EmbeddingConfig {
    EmbeddingConfig {
        provider_type: EmbeddingProviderType::Mock,
        endpoint: String::new(),
        model: "mock".to_string(),
        dimension: 384,
        api_key: String::new(),
    }
}

#[path = "helper/fixture.rs"]
pub mod fixture;

pub mod index_test {
    pub use cce_e2e_tests::index_test::*;
}

pub mod nl_text_exporter {
    pub use cce_e2e_tests::nl_text_exporter::*;
}

pub mod output_manager {
    pub use cce_e2e_tests::output_manager::*;
}

pub mod output_reporter {
    pub use cce_e2e_tests::output_reporter::*;
}

pub mod output_serializer {
    pub use cce_e2e_tests::output_serializer::*;
}

pub mod query_test {
    pub use cce_e2e_tests::query_test::*;
}

pub mod storage {
    pub use cce_e2e_tests::storage::*;
}

pub use assertion::{ExpectedIndexResult, assert_index_result};
pub use cleanup::init_minimal_logging;
pub use embedding::{EmbeddingConfig, EmbeddingProviderType};
pub use fixture::{EmptyFixture, FixtureCategory, FixtureSpec, TestFixture};
pub use index_test::IndexWorkflowTest;
pub use nl_text_exporter::NlTextDocumentExporter;
pub use output_manager::{OutputBuilder, OutputCategory, OutputConfig, OutputManager};
pub use output_reporter::{OutputReporter, ReportMetadata};
pub use output_serializer::{OutputFormat, OutputSerializer, SerializableIndexResult};
pub use query_test::QueryWorkflowTest;
pub use storage::StorageConfig;
