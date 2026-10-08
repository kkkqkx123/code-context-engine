//! Data types for SQLite records.
//!
//! This module defines the record types that map to SQLite tables.

pub mod checkpoint;
pub mod records;
pub mod status;

pub use checkpoint::{
    BatchCheckpointRecord, CheckpointRecord, FileCheckpointRecord, ScanCheckpointRecord,
    WorkUnitCheckpointRecord,
};
pub use records::{
    ChunkRecord, DbId, EntityDetailMapping, EntityRecord, FileRecord, NewProjectRecord,
    ProjectRecord, ProjectUpdateRecord, SummaryGenerationStats,
};
pub use status::{
    CheckpointStatus, ModuleStatus, OverallStatus, RetryStatus, ScanStatus, WorkUnitStatus,
};
