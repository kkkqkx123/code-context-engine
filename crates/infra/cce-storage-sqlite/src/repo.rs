//! Repository implementations for SQLite.
//!
//! This module contains all repository implementations for different data types.

pub mod checkpoint_repo;
pub mod chunk_repo;
pub mod entity_detail_mapping_repo;
pub mod entity_repo;
pub mod file_repo;
pub mod file_summaries_repo;
pub mod generation_override_repo;
pub mod project_index_manifest_repo;
pub mod project_repo;
pub mod relation_snapshot_repo;

pub use checkpoint_repo::CheckpointRepository;
pub use chunk_repo::ChunkRepository;
pub use entity_detail_mapping_repo::EntityDetailMappingRepository;
pub use entity_repo::EntityRepository;
pub use file_repo::FileRepository;
pub use file_summaries_repo::FileSummaryRepository;
pub use generation_override_repo::{
    GenerationOverride, GenerationOverrideRepository, OverrideDisposition,
};
pub use project_index_manifest_repo::{
    GenerationGcPlan, ProjectIndexManifest, ProjectIndexManifestRepository,
    ProjectIndexManifestState,
};
pub use project_repo::{ProjectRepository, generate_project_name};
pub use relation_snapshot_repo::{
    RelationSnapshotManifest, RelationSnapshotRepository, RelationSnapshotState,
};
