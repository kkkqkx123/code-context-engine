//! Structured output framework for symbol tables, relations, and type inference.
//!
//! Provides markdown renderers that are used by the `export_rs` example and by
//! standalone type-inference diagnostics. The design reuses tree-sitter parse
//! products (`ParsedFile`) and the `RelationIndex` so that export and relation
//! pipelines share the same underlying data.
//!
//! Output layout (per project, hierarchical):
//!   SUMMARY.md                        — project-level summary with counts
//!   <rel_path>.txt                    — per-file report: symbols + relations + type inference
//!                                       (preserves original directory structure,
//!                                        e.g. `crates/cli/src/decompress.rs.txt`)
//!   <dir>.dir.txt                     — per-directory overview (sibling to the
//!                                       directory, e.g. `crates/cli/src.dir.txt`,
//!                                       `crates/cli.dir.txt`) to avoid clashing with
//!                                       normal files.
//!
//! Inside each per-file report, symbols are grouped by kind domain (struct/enum,
//! impl blocks, functions/methods, variables, etc.) following the same table
//! hierarchy as the previous aggregated `SYMBOL_TABLE.md`. Relations are
//! organized per file — outgoing calls, type relationships, containment,
//! dependencies, and module metadata are colocated with their owning file so the
//! reader does not need to cross-reference a global `RELATIONS.md`. Type
//! inference for the file is appended in the same file.

pub mod relations;
pub mod reports;
pub mod summary;
pub mod symbol_table;
pub mod type_inference;
pub mod types;
pub mod writer;

pub use relations::{collect_relations, render_relations};
pub use reports::{render_directory_report, render_file_report};
pub use summary::{collect_summary, render_summary};
pub use symbol_table::{collect_symbols, render_symbol_table};
pub use type_inference::{render_type_inference, render_type_inference_with_index};
pub use types::{ProjectSummary, RelationEntry, SymbolEntry};
pub use writer::StructuredOutputWriter;
