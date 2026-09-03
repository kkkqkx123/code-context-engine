//! Qdrant vector database client
//!
//! This crate provides a client for interacting with Qdrant vector database
//! via HTTP REST API.
//!
//! # Configuration Presets
//!
//! The client supports configuration presets optimized for different data sizes:
//!
//! | Preset | Vector Count | HNSW m | HNSW ef_construct |
//! |--------|--------------|--------|-------------------|
//! | Tiny   | <= 2,000     | -      | - (full scan)     |
//! | Small  | 2,000-10,000 | 16     | 128               |
//! | Medium | 10,000-100,000 | 32   | 256               |
//! | Large  | > 100,000    | 64     | 512               |

pub mod client;
pub mod config;
pub mod error;
pub mod metrics;
pub mod process;
pub mod types;

pub mod estimator;
pub mod operations;
pub mod retrieval;
pub mod scheduler;
pub mod upgrade;

pub use client::{QdrantClient, QdrantDiagnostic, generate_group_id, generate_project_group_id};
pub use config::{
    CollectionPreset, DistanceMetric, HnswConfig, QdrantConfig, VectorStorageConfig, WalConfig,
};
pub use error::QdrantError;
pub use estimator::{
    CollectionSizeEstimate, CollectionSizeEstimator, DEFAULT_AVG_VECTORS_PER_FILE,
    DEFAULT_BYTES_PER_VECTOR, PresetGuideline, PresetGuidelines, SizeDifference,
    SizeEstimateBuilder,
};
pub use metrics::QdrantMetrics;
pub use process::{
    QdrantControlAction, QdrantProcessConfig, QdrantProcessHandle, QdrantProcessManager,
    QdrantProcessStatus,
};
pub use scheduler::{
    ConfigUpgradeScheduler, DEFAULT_CHECK_INTERVAL_SECS, DEFAULT_MAX_CONCURRENT_UPGRADES,
    SchedulerConfig, SchedulerStatus, UpgradeEvent, UpgradeWindow,
};

pub use cce_storage_common::Payload;
pub use retrieval::QdrantRetrieval;
pub use tokio::sync::broadcast::Receiver as UpgradeEventReceiver;
pub use types::{
    CollectionInfo, CollectionStatus, HnswConfigInfo, SearchQuery, SearchResult, SizeEstimation,
    VectorPoint,
};
pub use upgrade::{
    ConfigUpgradeService, StepStatus, UpgradeProgress, UpgradeStatus, UpgradeStep,
    UpgradeThresholds,
};
