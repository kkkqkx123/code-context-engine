//! Metadata backend surface used by the orchestrator.
//!
//! The backend enum, construction, and dispatch live in the storage facade
//! (`cce-storage-metadb`); this module only re-exports the handle so the
//! assembly and query paths keep one import site.

pub use cce_storage_metadb::{RelationDiagnostics, RelationStore};
