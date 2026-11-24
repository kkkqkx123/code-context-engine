//! Shared helpers for regression tests.

#![allow(unused_imports)]

pub mod assertion {
    pub use cce_e2e_tests::assertion::*;
}

pub mod cleanup {
    pub use cce_e2e_tests::cleanup::*;
}

pub mod output_manager {
    pub use cce_e2e_tests::output_manager::*;
}

pub mod nl_text_exporter {
    pub use cce_e2e_tests::nl_text_exporter::*;
}

pub mod output_reporter {
    pub use cce_e2e_tests::output_reporter::*;
}

pub mod output_serializer {
    pub use cce_e2e_tests::output_serializer::*;
}

pub mod pipeline_debug {
    pub use cce_e2e_tests::pipeline_debug::*;
}

pub mod type_inference_assert {
    pub use cce_e2e_tests::type_inference_assert::*;
}

pub mod fixture {
    pub use cce_e2e_tests::fixture::{
        EmptyFixture, FixtureAccess, FixtureCategory, FixtureSpec, TestFixture,
    };
}

pub use assertion::{ExpectedIndexResult, assert_index_result};
pub use cleanup::init_minimal_logging;
pub use fixture::{EmptyFixture, FixtureCategory, FixtureSpec, TestFixture};
pub use nl_text_exporter::NlTextDocumentExporter;
pub use output_manager::{OutputBuilder, OutputCategory, OutputConfig, OutputManager};
pub use output_reporter::{OutputReporter, ReportMetadata};
pub use output_serializer::{OutputFormat, OutputSerializer, SerializableIndexResult};
pub use pipeline_debug::PipelineDebugExporter;
