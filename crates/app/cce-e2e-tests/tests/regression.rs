//! Regression test entry point.
//!
//! Regression tests live under `tests/regression/`.

#[path = "regression/bm25_parity.rs"]
pub mod bm25_parity;
#[path = "regression/e2e_outputs.rs"]
pub mod e2e_outputs;
#[path = "regression/fixture_single_tree.rs"]
pub mod fixture_single_tree;
#[path = "regression/helper.rs"]
pub mod helper;
#[path = "regression/rust_relation.rs"]
pub mod rust_relation;
#[path = "regression/rust_relation_diamond.rs"]
pub mod rust_relation_diamond;
#[path = "regression/rust_sidecar.rs"]
pub mod rust_sidecar;
#[path = "regression/test_marker_language_coverage.rs"]
pub mod test_marker_language_coverage;
#[path = "regression/tree_sitter_queries.rs"]
pub mod tree_sitter_queries;
#[path = "regression/type_inference/mod.rs"]
pub mod type_inference;
