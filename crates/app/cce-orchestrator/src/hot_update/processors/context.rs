//! Shared processor context
//!
//! This module provides a shared context for update processors to avoid
//! duplicate resource creation and ensure consistent behavior.

use std::sync::Arc;

use tokio::sync::Mutex as TokioMutex;

use crate::index::{FileProcessor, StorageCoordinator};
use crate::operation::CheckpointManager;
use cce_config::NestProcessorConfig;
use cce_metrics::HotUpdateStorageMetrics;

/// Shared processor context
///
/// This context provides shared resources for update processors,
/// avoiding duplicate resource creation and ensuring consistent behavior.
///
/// Chunked results are shared between processors through the context's
/// single `FileProcessor`: its internal chunk cache (keyed by project,
/// path and source hash) lets the embedding and BM25 modules reuse the
/// chunking work instead of re-processing the same parsed file.
pub struct ProcessorContext {
    /// Storage coordinator for storage operations
    pub storage: Arc<StorageCoordinator>,
    /// File processor for converting parsed files (using async Mutex for Send compatibility)
    pub file_processor: Arc<TokioMutex<FileProcessor>>,
    /// Checkpoint manager used to persist per-file module progress markers so
    /// recovery can skip modules whose work already completed.
    pub checkpoint_manager: Option<Arc<CheckpointManager>>,
    /// Storage-side hot-update metrics; records chunking-drift sweep coverage.
    pub storage_metrics: Option<Arc<HotUpdateStorageMetrics>>,
}

impl ProcessorContext {
    /// Create a new processor context
    pub fn new(
        storage: Arc<StorageCoordinator>,
        pre_processor_config: NestProcessorConfig,
    ) -> Self {
        Self {
            storage,
            file_processor: Arc::new(TokioMutex::new(FileProcessor::with_pre_processor_config(
                pre_processor_config,
            ))),
            checkpoint_manager: None,
            storage_metrics: None,
        }
    }

    /// Create a processor context whose file processor is scoped to one
    /// project.
    pub fn new_with_project(
        storage: Arc<StorageCoordinator>,
        pre_processor_config: NestProcessorConfig,
        project_id: i64,
    ) -> Self {
        Self {
            storage,
            file_processor: Arc::new(TokioMutex::new(
                FileProcessor::with_pre_processor_config(pre_processor_config)
                    .with_project_id(project_id),
            )),
            checkpoint_manager: None,
            storage_metrics: None,
        }
    }

    /// Create a new processor context with default configuration
    pub fn new_default(storage: Arc<StorageCoordinator>) -> Self {
        Self {
            storage,
            file_processor: Arc::new(TokioMutex::new(FileProcessor::new())),
            checkpoint_manager: None,
            storage_metrics: None,
        }
    }

    /// Attach the checkpoint manager used to persist per-module progress.
    pub fn with_checkpoint_manager(mut self, manager: Arc<CheckpointManager>) -> Self {
        self.checkpoint_manager = Some(manager);
        self
    }

    /// Attach storage-side hot-update metrics.
    pub fn with_storage_metrics(mut self, metrics: Arc<HotUpdateStorageMetrics>) -> Self {
        self.storage_metrics = Some(metrics);
        self
    }
}
