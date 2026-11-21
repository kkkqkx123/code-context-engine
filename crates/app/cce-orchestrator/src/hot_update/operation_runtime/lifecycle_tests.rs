//! Operation-finalization and config-change lifecycle tests.
//!
//! Covers the two branches that the e2e suite only reaches on the happy path:
//! result finalization (status and resume flag derived from the failure list)
//! and the config-change operation's failure handling (checkpoint marked
//! failed, candidate retired, queue released).

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;

use crate::hot_update::BatchChangeResult;
use crate::hot_update::error::{HotUpdateError, Result};
use crate::hot_update::operation_runtime::HotUpdateOperationRuntime;
use crate::hot_update::processors::UpdateProcessor;
use crate::index::in_memory_metadata_store;
use crate::operation::checkpoint::{CheckpointManager, CreateCheckpointParams};
use crate::operation::{ModuleFailure, OperationContext, OperationProcessResult, OperationType};
use cce_storage_common::metadb::CheckpointStatus;
use cce_types::OperationKind;

const OP_ID: &str = "lifecycle-op";

fn build_runtime() -> (HotUpdateOperationRuntime, Arc<CheckpointManager>) {
    let store = in_memory_metadata_store().expect("in-memory metadata store");
    let manager = Arc::new(CheckpointManager::new_for_project(1, store.clone()));
    let mut runtime =
        HotUpdateOperationRuntime::new(1, store.clone(), cce_scanner::ScanOptions::default());
    runtime.set_metadata_store(store);
    runtime.set_checkpoint_manager(manager.clone());
    (runtime, manager)
}

fn context(operation_type: OperationType) -> OperationContext {
    OperationContext::new(1, OP_ID.to_string(), operation_type, 1)
}

/// Processor that always fails its processing stage.
struct FailingProcessor {
    abort_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl UpdateProcessor for FailingProcessor {
    fn name(&self) -> &'static str {
        "failing"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn process_operation(
        &self,
        _ctx: &OperationContext,
        _batch_result: &mut BatchChangeResult,
    ) -> Result<OperationProcessResult> {
        Err(HotUpdateError::hot_update("simulated module failure"))
    }

    async fn abort_operation(&self, _ctx: &OperationContext, _reason: &str) -> Result<()> {
        self.abort_calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Result finalization
// ---------------------------------------------------------------------------

#[tokio::test]
async fn successful_batch_finalizes_as_completed() {
    let (runtime, _manager) = build_runtime();
    let ctx = context(OperationType::HotUpdate);

    let result = runtime
        .finalize_operation(&ctx, &BatchChangeResult::new(), Vec::new())
        .await
        .expect("finalization succeeds");

    assert!(result.is_successful());
    assert!(
        !result.summary.can_resume,
        "a clean batch has nothing to resume"
    );
    assert_eq!(
        result.metrics.avg_file_duration_ms, 0.0,
        "an empty batch must not divide by zero"
    );
}

#[tokio::test]
async fn failed_batch_is_partial_and_resumable() {
    let (runtime, _manager) = build_runtime();
    let ctx = context(OperationType::HotUpdate);

    let failures = vec![
        ModuleFailure {
            file_path: "src/lib.rs".to_string(),
            module_name: "bm25".to_string(),
            error: "boom".to_string(),
            retry_count: 0,
            next_retry_time: None,
        },
        ModuleFailure {
            file_path: "src/util.rs".to_string(),
            module_name: "embedding".to_string(),
            error: "boom".to_string(),
            retry_count: 0,
            next_retry_time: None,
        },
    ];

    let result = runtime
        .finalize_operation(&ctx, &BatchChangeResult::new(), failures)
        .await
        .expect("finalization succeeds");

    assert_eq!(result.failed_module_count(), 2);
    assert!(
        result.summary.can_resume,
        "failed modules are work a later pass must redo"
    );
    assert_eq!(result.failed_modules.len(), 2);
}

// ---------------------------------------------------------------------------
// Config-change operation lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn config_change_without_processors_completes() {
    let (runtime, manager) = build_runtime();
    let mut ctx = context(OperationType::ConfigChange);
    ctx.config_path = Some(std::path::PathBuf::from("Cargo.toml"));

    let result = runtime
        .run_config_change_operation(&mut ctx, &[], Path::new("Cargo.toml"))
        .await
        .expect("config change runs");

    assert!(result.is_successful());
    let checkpoint = manager
        .get_checkpoint(OP_ID)
        .await
        .expect("read checkpoint")
        .expect("checkpoint exists");
    assert_eq!(
        checkpoint.operation_type,
        OperationKind::ConfigChange.as_str(),
        "config changes are recorded under their own kind"
    );
    assert_eq!(
        checkpoint.status,
        CheckpointStatus::Completed,
        "a successful config change must reach a terminal state"
    );
}

#[tokio::test]
async fn config_change_failure_marks_failed_and_retires_candidate() {
    let (runtime, manager) = build_runtime();
    let mut ctx = context(OperationType::ConfigChange);
    ctx.config_path = Some(std::path::PathBuf::from("Cargo.toml"));

    let abort_calls = Arc::new(AtomicUsize::new(0));
    let failing = FailingProcessor {
        abort_calls: abort_calls.clone(),
    };
    let processors: Vec<&dyn UpdateProcessor> = vec![&failing];

    let result = runtime
        .run_config_change_operation(&mut ctx, &processors, Path::new("Cargo.toml"))
        .await
        .expect("a failing module is reported, not raised");

    assert!(!result.is_successful(), "the module failure must surface");
    assert_eq!(result.failed_module_count(), 1);
    assert_eq!(
        abort_calls.load(Ordering::Relaxed),
        1,
        "the candidate must be retired after a failure"
    );

    let checkpoint = manager
        .get_checkpoint(OP_ID)
        .await
        .expect("read checkpoint")
        .expect("checkpoint exists");
    assert_eq!(
        checkpoint.status,
        CheckpointStatus::Failed,
        "a failed config change must never stay in_progress"
    );
}

// ---------------------------------------------------------------------------
// Config-change checkpoints stay out of hot-update recovery
// ---------------------------------------------------------------------------

#[tokio::test]
async fn config_change_checkpoint_is_not_recovered_as_hot_update() {
    let (_runtime, manager) = build_runtime();
    manager
        .create_checkpoint(CreateCheckpointParams {
            operation_id: "config-change-op",
            operation_type: OperationKind::ConfigChange,
            root_dir: "Cargo.toml",
            total_files: 0,
            batch_size: 1,
            file_list_hash: "",
        })
        .await
        .expect("create config-change checkpoint");

    let recovered = manager
        .validate_and_recover_checkpoint("new-op", OperationKind::HotUpdate, "Cargo.toml")
        .await
        .expect("recovery query succeeds");
    assert!(
        recovered.is_none(),
        "a crashed config change must not be replayed as a file-change operation"
    );
}
