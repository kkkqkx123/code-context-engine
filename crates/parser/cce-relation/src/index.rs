//! Index module for relation data structures
//!
//! This module provides:
//! - RelationIndex: Core index structure for storing function call relationships
//! - IndexBuilder: Builder pattern for constructing relation indexes
//! - ProjectSymbolTable: Project-level symbol table for cross-file reference resolution
//! - RelationResolver: Resolves raw relations to resolved relations
//! - LocalCallResolver: Resolves calls within a single file (pre-processing stage)
//! - ThreadSafeIndex: Type alias for thread-safe relation index
//! - DependencyIndex: Efficient lookup structure for dependency matching
//!
//! # Module Organization
//!
//! The index functionality is organized into several modules:
//! - `core`: Core `RelationIndex` structure definition
//! - `delta`: Delta computation/application over the index
//! - `snapshot_index`: Immutable snapshot views (base snapshot + layered deltas)
//! - `entity_index`: Entity-related operations (add/get functions)
//! - `relation_query`: Query operations (forward/reverse lookups, hierarchy queries)
//! - `file_index`: File-related operations (imports, exports, file metadata)
//! - `resolver`: Cross-file relation resolution
//! - `local_call_resolver`: Single-file call resolution (pre-processing stage)
//! - `dependency_index`: Efficient dependency lookup structure
//!
//! # Configuration
//!
//! Configuration types are defined in the parent module `crate::config`:
//! - `RelationConfig`: Top-level configuration
//! - `IndexConfig`: Index-specific configuration
//!
//! # Resolution Pipeline
//!
//! The resolution process follows a two-stage pipeline:
//! 1. `LocalCallResolver`: Resolves calls within a single file
//! 2. `RelationResolver`: Resolves cross-file calls and builds the full index

pub mod builder;
pub mod compact;
pub mod core;
pub mod cow_snapshot;
pub mod delta;
pub mod dependency_index;
pub mod entity_index;
pub mod file_index;
pub mod relation_query;
pub mod resolver;
pub mod snapshot_generation;
pub mod snapshot_index;
pub mod snapshot_loader;
pub mod snapshot_query;
pub mod snapshot_view;
pub mod stores;
pub mod unified_snapshot;
pub mod view;

#[cfg(test)]
mod test_support;

// Re-export main types
pub use builder::IndexBuilder;
pub use compact::CompactRelationIndex;
pub use core::{CallChainNode, CallChainPath, ExportInfo, ExportType, RelationIndex, SymbolKey};
pub use cow_snapshot::{CoWRelationSnapshot, CowLayeredSnapshot};
pub use delta::RelationDeltaOps;
pub use dependency_index::{DependencyIndex, IndexStats};
pub use entity_index::EntityIndexOps;
pub use file_index::{ExportIndexOps, FileIndexOps, FileLevelOps, ImportIndexOps};
pub use unified_snapshot::{SnapshotManager, UnifiedSnapshotIndex};
// LocalCallResolver is defined in cce-parser-core (the canonical copy)
// and re-exported here for backward compatibility.
pub use cce_parser_core::{LocalCall, LocalCallResolver, LocalCallResolverConfig};
pub use relation_query::{FrontendQueryOps, HierarchyQueryOps, RelationQueryOps};
pub use resolver::RelationResolver;
pub use snapshot_generation::{CoWSnapshotGuard, SnapshotGeneration};
pub use snapshot_index::{FileScopedSnapshot, LayeredSnapshotIndex, RelationSnapshotIndex};
pub use snapshot_query::{
    SnapshotEntityQueryOps, SnapshotFileQueryOps, SnapshotFrontendQueryOps,
    SnapshotHierarchyQueryOps, SnapshotQueryIndex, SnapshotRelationQueryOps,
    SnapshotSymbolQueryOps,
};
pub use view::RelationIndexView;

// Re-export configuration types from config module
pub use cce_config::IndexConfig;

/// Thread-safe relation index type alias
/// RelationIndex uses DashMap internally which is thread-safe
pub type ThreadSafeIndex = RelationIndex;

/// Shared type for file-ordered entity start index mapping file paths to sorted
/// (start_row, EntityId) entries.
pub(crate) type FileEntitiesByStart = std::sync::Arc<
    parking_lot::RwLock<
        std::collections::HashMap<
            String,
            smallvec::SmallVec<[(u32, cce_types::entity::EntityId); 8]>,
        >,
    >,
>;
