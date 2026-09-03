//! SQLite storage module for code metadata persistence.
//!
//! This crate provides a complete SQLite-based storage layer including
//! client management, schema creation, migration, and repository implementations
//! for files, entities, chunks, checkpoints, relation snapshots, and more.

pub mod client;
pub mod config;
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

pub use client::SqliteClient;
pub use config::SqliteConfig;
pub use metrics::SqliteMetrics;
pub use repo::{
    CheckpointRepository, ChunkRepository, EntityDetailMappingRepository, EntityRepository,
    FileRepository, FileSummaryRepository, GenerationOverride, GenerationOverrideRepository,
    OverrideDisposition, ProjectIndexManifest, ProjectIndexManifestRepository,
    ProjectIndexManifestState, ProjectRepository, RelationSnapshotManifest,
    RelationSnapshotRepository, RelationSnapshotState, generate_project_name,
};
pub use snapshot_store::SqliteSnapshotStore;
pub use types::{
    BatchCheckpointRecord, CheckpointRecord, CheckpointStatus, ChunkRecord, DbId,
    EntityDetailMapping, EntityRecord, FileCheckpointRecord, FileRecord, ModuleStatus,
    NewProjectRecord, OverallStatus, ProjectRecord, ProjectUpdateRecord, RetryStatus,
    ScanCheckpointRecord, ScanStatus, SummaryGenerationStats, WorkUnitCheckpointRecord,
    WorkUnitStatus,
};
