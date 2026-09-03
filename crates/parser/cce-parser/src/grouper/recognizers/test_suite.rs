//! Test suite recognition module
//!
//! Detects test entities across multiple languages and produces the
//! end-to-end [`TestInfo`](cce_types::TestInfo) marker consumed by the
//! chunker and evaluation.
//!
//! # Architecture
//!
//! - [`detector`]: Main entry point for test detection (AST adjacency + conventions)
//! - [`languages`]: Language-specific detection implementations
//!
//! # Detection flow (per entity)
//!
//! 1. AST attribute adjacency (highest priority, confidence `High`):
//!    inspect `attribute_item`/`marker_annotation`/`decorator` nodes that
//!    directly precede the entity span. Never falls back to name matching.
//! 2. Constrained naming conventions (confidence `High`): per-language
//!    conventions that require additional file context (`*_test.go` +
//!    `TestXxx`, `@pytest` test directory + `test_` prefix, etc.).

pub mod detector;
pub mod languages;

pub use detector::TestSuiteDetector;
