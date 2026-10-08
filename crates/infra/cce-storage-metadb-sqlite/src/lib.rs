//! SQLite storage module for code metadata persistence.
//!
//! This crate provides a complete SQLite-based storage layer including
//! client management, schema creation, migration, and repository implementations
//! for files, entities, chunks, checkpoints, relation snapshots, and more.

pub mod cache;
pub mod client;
pub mod config;
pub mod contract;
pub mod helpers;
pub mod metrics;
pub mod migration;
pub mod project_registry;
pub mod repo;
pub mod schema;
pub mod snapshot_store;
pub mod source_reader;
pub mod types;
pub mod utils;

pub use cce_storage_common::metadb::{
    AdmissionAuditRecord, CheckpointRecord, CheckpointStatus, ChunkRecord, EntityDetailMapping,
    EntityRecord, FileCheckpointRecord, FileRecord, GenerationOverride, OverrideDisposition,
    ProjectIndexManifest, ProjectIndexManifestState, ProjectRecord, RelationChunk, RelationEntity,
    RelationFile, RelationStorage, WorkUnitCheckpointRecord, WorkUnitStatus,
    assert_relation_storage,
};
pub use client::SqliteClient;
pub use config::SqliteConfig;
pub use contract::FileHashCachePort;
pub use metrics::SqliteMetrics;
pub use repo::{
    AdmissionAuditRepository, CheckpointRepository, ChunkRepository, EntityDetailMappingRepository,
    EntityRepository, FileRepository, FileSummaryRepository, GenerationOverrideRepository,
    ProjectIndexManifestRepository, ProjectRepository, RelationSnapshotManifest,
    RelationSnapshotRepository, RelationSnapshotState, generate_project_name,
};
pub use snapshot_store::SqliteSnapshotStore;
pub use types::{
    BatchCheckpointRecord, DbId, ModuleStatus, NewProjectRecord, OverallStatus,
    ProjectUpdateRecord, RetryStatus, ScanCheckpointRecord, ScanStatus, SummaryGenerationStats,
};
