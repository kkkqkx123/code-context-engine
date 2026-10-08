//! Stores grouping for `RelationIndex`.
//!
//! The monolithic `RelationIndex` (24 fields) is decomposed into
//! intention-revealing sub-stores:
//!
//! - `diagnostics` — counters + metrics sink (extracted, shared)
//! - `entity_store` — function_index / name_index / entity_file_index / file_entities_by_start / entity_id_counter / entity_id_remaps
//! - `relation_store` — resolved_relation_index / file_relation_index / file_callers_by_callee
//! - `symbol_registry` — symbol_key_to_entity / entity_to_symbol_key / stable_id_to_entity / file_symbol_keys
//! - `file_store` — file_records (unified info + imports + exports)
//!
//! Each store provides `deep_clone()` for creating independent mutable copies
//! (used by `detached_clone` and snapshot creation).

pub mod diagnostics;
pub mod entity_store;
pub mod relation_store;

use std::sync::Arc;

pub use diagnostics::RelationDiagnostics;
pub use entity_store::EntityStore;
pub use relation_store::{FileRecord, FileStore, RelationStore, SymbolRegistry};

/// Re-export for `RelationIndex` to hold a single shared diagnostics handle.
pub type SharedDiagnostics = Arc<RelationDiagnostics>;
