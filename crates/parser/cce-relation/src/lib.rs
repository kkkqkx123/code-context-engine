//! Relation index, symbol tables, and call-chain queries.
//!
//! This crate provides:
//! - **Index construction** from `ParsedFile` data
//! - **Call-chain queries** based on `EntityId`
//! - **File state management** (incremental parsing, dependency tracking)
//! - **Four-level symbol table hierarchy** (project / package / module / local)
//! - **Build-system configuration parsing** (Cargo, pom.xml, go.mod, …)

pub mod config_parser;
pub mod dependency_graph;
pub mod error;
pub mod external;
pub mod helpers;
pub mod index;
pub mod policy;
pub mod query;
pub mod stdlib_classifier;
pub mod symbol;
pub mod symbol_table;
pub mod type_inference;
pub mod types;

pub use config_parser::{
    BuildConfigParser, DependencyCollection, Dev, DevDependency, External, ExternalDependency,
    Local, LocalDependency, PackageKind, UntypedDependency,
};
pub use dependency_graph::{
    DependencyGraphError, EntityDependencyGraph, EntityImpactAnalysis, FileDependencyGraph,
};
pub use error::{IndexError, PersistenceError, RelationError, RelationQueryError, ResolutionError};
pub use index::{
    LayeredSnapshotIndex, LocalCallResolver, ThreadSafeIndex,
    builder::{IndexBuilder, SymbolTableBuilder},
    compact::CompactRelationIndex,
    core::{CallChainNode, CallChainPath, ExportInfo, ExportType, RelationIndex},
    unified_snapshot::{SnapshotManager, UnifiedSnapshotIndex},
};
pub use query::{CallChainQuery, ThreadSafeQuery, UnifiedCallChainQuery};
pub use symbol_table::{
    LocalSymbolTable, ModuleSymbolTable, PackageSymbolTable, ProjectSymbolTable, ResolutionContext,
};
pub use types::CallChainGraph;
