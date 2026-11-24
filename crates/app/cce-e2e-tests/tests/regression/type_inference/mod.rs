//! Type inference integration tests.
//!
//! Tests the full pipeline: source code parsing -> type inference -> symbol table construction.
//! Each language fixture contains realistic code snippets exercising type inference patterns.
//!
//! Indexing smoke tests use the orchestrator; precise type assertions parse the
//! fixture with [`ParseCoordinator`](cce_parser::parser::ParseCoordinator) and
//! check the canonical snapshot from
//! [`collect_type_bindings`](cce_e2e_tests::type_inference_assert::collect_type_bindings).
//! Run `cargo run -p cce-e2e-tests --example export_type_inference` to regenerate
//! the human-readable `TYPE_INFERENCE.md` reports for visual inspection.

pub mod bash;
pub mod c_cpp;
pub mod common;
pub mod csharp;
pub mod dart;
pub mod go;
pub mod index_smoke;
pub mod java;
pub mod javascript;
pub mod kotlin;
pub mod lua;
pub mod php;
pub mod python;
pub mod ruby;
pub mod rust;
pub mod scala;
