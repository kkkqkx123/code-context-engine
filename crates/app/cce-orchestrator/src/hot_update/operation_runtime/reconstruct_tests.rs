//! Reconstruction tests for the resumed hot-update batch.
//!
//! These cover the durable-state mapping in
//! [`super::reconstruct::HotUpdateOperationRuntime::reconstruct_batch_from_checkpoints`]
//! without a coordinator: a checkpoint manager, a scan root and an in-memory
//! metadata store are enough to rebuild a batch, so every branch of the
//! mapping (tombstone, reused envelope, vanished file, incompatible payload,
//! batch selection) is asserted directly instead of through the e2e suite.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::hot_update::change::FileChangeType;
use crate::hot_update::operation_runtime::HotUpdateOperationRuntime;
use crate::index::{MetadataStore, in_memory_metadata_store};
use crate::operation::checkpoint::{
    CheckpointManager, CreateCheckpointParams, ParsedCheckpointEnvelope, ParsedCheckpointPayload,
    SummaryCheckpointPayload, encode_parsed_checkpoint, encode_summary_checkpoint,
};
use cce_types::OperationKind;

const OP_ID: &str = "resume-reconstruct-op";

/// Build a runtime wired to a temporary scan root and checkpoint manager.
fn build_runtime(
    root: &Path,
) -> (
    HotUpdateOperationRuntime,
    Arc<CheckpointManager>,
    MetadataStore,
) {
    let store = in_memory_metadata_store().expect("in-memory metadata store");
    let manager = Arc::new(CheckpointManager::new_for_project(1, store.clone()));

    let scan_options = cce_scanner::ScanOptions {
        root_path: root.to_string_lossy().to_string(),
        ..Default::default()
    };

    let mut runtime = HotUpdateOperationRuntime::new(1, store.clone(), scan_options);
    runtime.set_metadata_store(store.clone());
    runtime.set_watch_root(root.to_path_buf());
    runtime.set_checkpoint_manager(manager.clone());
    (runtime, manager, store)
}

/// Create the operation and batch checkpoints a crashed run would leave.
async fn open_batch(manager: &CheckpointManager, batch_index: u32, files: &[&str]) {
    manager
        .create_checkpoint(CreateCheckpointParams {
            operation_id: OP_ID,
            operation_type: OperationKind::HotUpdate,
            root_dir: "/test",
            total_files: files.len() as u32,
            batch_size: 1,
            file_list_hash: "hash",
        })
        .await
        .expect("create operation checkpoint");
    let first = files.first().copied().unwrap_or_default();
    let last = files.last().copied().unwrap_or_default();
    manager
        .create_batch_checkpoint(OP_ID, batch_index, first, last, files.len() as u32)
        .await
        .expect("create batch checkpoint");
}

/// Persist one file checkpoint with the given parsed payload.
async fn save_file(
    manager: &CheckpointManager,
    batch_index: u32,
    path: &str,
    payload: Option<ParsedCheckpointPayload>,
    content_hash: Option<String>,
) {
    let mut record = manager
        .create_file_checkpoint(OP_ID, batch_index, path)
        .await
        .expect("create file checkpoint");
    record.content_hash = content_hash;
    record.parsed_data = payload.map(|payload| {
        encode_parsed_checkpoint(&payload).expect("encode parsed checkpoint payload")
    });
    manager
        .save_file_checkpoint(&record)
        .await
        .expect("save file checkpoint");
}

fn parsed_envelope(change_type: FileChangeType, source: &str) -> ParsedCheckpointPayload {
    let parsed_file = cce_types::ParsedFile {
        path: "ignored".to_string(),
        source: source.to_string().into(),
        language: cce_types::Language::Rust,
        ..Default::default()
    };
    ParsedCheckpointPayload::Parsed(Box::new(ParsedCheckpointEnvelope::new(
        change_type,
        parsed_file,
    )))
}

// ---------------------------------------------------------------------------
// Deletion tombstones survive the reconstruction
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleted_tombstone_becomes_file_change() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (runtime, manager, _store) = build_runtime(dir.path());
    open_batch(&manager, 0, &["src/gone.rs"]).await;
    save_file(
        &manager,
        0,
        "src/gone.rs",
        Some(ParsedCheckpointPayload::Deleted),
        Some("deadbeef".to_string()),
    )
    .await;

    let batch = runtime
        .reconstruct_batch_from_checkpoints(OP_ID)
        .await
        .expect("reconstruction succeeds");

    assert!(
        batch.parse_results.is_empty(),
        "a tombstone must never be turned back into a parse result"
    );
    assert_eq!(batch.file_changes.len(), 1, "exactly one deletion");
    let change = &batch.file_changes[0];
    assert_eq!(change.change_type, FileChangeType::Deleted);
    assert_eq!(
        change.path,
        Path::new("src/gone.rs"),
        "deletions keep the project-relative identity"
    );
    assert_eq!(change.content_hash, "deadbeef");
}

// ---------------------------------------------------------------------------
// A file that vanished between crash and resume is a deletion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vanished_file_becomes_file_change() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (runtime, manager, _store) = build_runtime(dir.path());
    open_batch(&manager, 0, &["src/vanished.rs"]).await;
    save_file(
        &manager,
        0,
        "src/vanished.rs",
        Some(parsed_envelope(
            FileChangeType::Modified,
            "pub fn vanished() {}",
        )),
        Some("hash-of-a-file-that-is-gone".to_string()),
    )
    .await;

    let batch = runtime
        .reconstruct_batch_from_checkpoints(OP_ID)
        .await
        .expect("reconstruction succeeds");

    assert!(
        batch.parse_results.is_empty(),
        "no source on disk means nothing to parse"
    );
    assert_eq!(batch.file_changes.len(), 1);
    assert_eq!(batch.file_changes[0].change_type, FileChangeType::Deleted);
    assert_eq!(batch.file_changes[0].path, Path::new("src/vanished.rs"));
}

// ---------------------------------------------------------------------------
// A valid envelope is reused instead of re-parsed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn matching_envelope_is_reused_with_its_persisted_state() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (runtime, manager, _store) = build_runtime(dir.path());

    let source = "pub fn alpha() -> i32 { 1 }";
    std::fs::create_dir_all(dir.path().join("src")).expect("create src dir");
    std::fs::write(dir.path().join("src/lib.rs"), source).expect("write source file");
    let content_hash = cce_utils::hash::calculate_hash(source.as_bytes());

    open_batch(&manager, 0, &["src/lib.rs"]).await;
    save_file(
        &manager,
        0,
        "src/lib.rs",
        Some(parsed_envelope(FileChangeType::Modified, source)),
        Some(content_hash.clone()),
    )
    .await;

    // Attach the summary, render fingerprint, export marker and module
    // progress a crashed run would have written.
    let summary = cce_parser::summary::FileSummary::new("src/lib.rs").with_summary("alpha");
    let mut record = manager
        .get_file_checkpoint(OP_ID, "src/lib.rs")
        .await
        .expect("read file checkpoint")
        .expect("file checkpoint exists");
    record.summary_data = Some(
        encode_summary_checkpoint(&SummaryCheckpointPayload::new(
            summary,
            None,
            Some("summary-fp".to_string()),
        ))
        .expect("encode summary"),
    );
    record.render_fingerprint = Some("render-fp".to_string());
    record.export_path = Some("docs/lib.md".to_string());
    record.module_progress = Some(r#"{"bm25":"bm25-fp"}"#.to_string());
    manager
        .save_file_checkpoint(&record)
        .await
        .expect("save enriched checkpoint");

    std::fs::create_dir_all(dir.path().join("docs")).expect("create docs dir");
    std::fs::write(dir.path().join("docs/lib.md"), "# lib").expect("write exported document");

    let parse_probe = Arc::new(AtomicUsize::new(0));
    let mut runtime = runtime;
    runtime.set_parse_probe(parse_probe.clone());

    let batch = runtime
        .reconstruct_batch_from_checkpoints(OP_ID)
        .await
        .expect("reconstruction succeeds");

    assert_eq!(
        parse_probe.load(Ordering::Relaxed),
        0,
        "a reusable envelope must not trigger a parse"
    );
    assert_eq!(batch.parse_results.len(), 1);
    let reused = &batch.parse_results[0];
    assert_eq!(reused.file_path, Path::new("src/lib.rs"));
    assert_eq!(
        reused.stored_content_hash.as_deref(),
        Some(content_hash.as_str())
    );
    assert_eq!(
        reused.stored_render_fingerprint.as_deref(),
        Some("render-fp")
    );
    assert_eq!(
        reused.stored_summary_fingerprint.as_deref(),
        Some("summary-fp"),
        "the summary configuration fingerprint decides regeneration later"
    );
    let restored_summary = reused.file_summary.as_ref().expect("summary restored");
    assert_eq!(restored_summary.summary_text, "alpha");
    assert_eq!(
        reused.module_progress.get("bm25").map(String::as_str),
        Some("bm25-fp"),
        "module markers decide which modules skip the file"
    );
    assert!(
        reused.already_exported,
        "an existing exported document marks the file as exported"
    );
}

// ---------------------------------------------------------------------------
// An unusable envelope falls back to a fresh parse
// ---------------------------------------------------------------------------

#[tokio::test]
async fn incompatible_envelope_is_reparsed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (runtime, manager, _store) = build_runtime(dir.path());

    let source = "pub fn beta() -> i32 { 2 }";
    std::fs::create_dir_all(dir.path().join("src")).expect("create src dir");
    std::fs::write(dir.path().join("src/util.rs"), source).expect("write source file");
    let content_hash = cce_utils::hash::calculate_hash(source.as_bytes());

    open_batch(&manager, 0, &["src/util.rs"]).await;

    // A version from the future is rejected by the compatibility filter.
    let mut payload = parsed_envelope(FileChangeType::Modified, source);
    if let ParsedCheckpointPayload::Parsed(ref mut envelope) = payload {
        envelope.schema_version = 999;
    }
    save_file(
        &manager,
        0,
        "src/util.rs",
        Some(payload),
        Some(content_hash),
    )
    .await;

    let parse_probe = Arc::new(AtomicUsize::new(0));
    let mut runtime = runtime;
    runtime.set_parse_probe(parse_probe.clone());

    let batch = runtime
        .reconstruct_batch_from_checkpoints(OP_ID)
        .await
        .expect("reconstruction succeeds");

    assert_eq!(
        parse_probe.load(Ordering::Relaxed),
        1,
        "an incompatible envelope must be re-parsed exactly once"
    );
    assert_eq!(batch.parse_results.len(), 1);
    assert!(batch.file_changes.is_empty());
}

// ---------------------------------------------------------------------------
// Only the hot-update batch is reconstructed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn only_the_recovery_batch_is_reconstructed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (runtime, manager, _store) = build_runtime(dir.path());
    open_batch(&manager, 0, &["src/first.rs"]).await;
    save_file(
        &manager,
        0,
        "src/first.rs",
        Some(ParsedCheckpointPayload::Deleted),
        Some("hash-first".to_string()),
    )
    .await;

    // A second batch entry, as a hypothetical multi-batch writer would leave.
    manager
        .create_batch_checkpoint(OP_ID, 1, "src/second.rs", "src/second.rs", 1)
        .await
        .expect("create second batch checkpoint");
    save_file(
        &manager,
        1,
        "src/second.rs",
        Some(ParsedCheckpointPayload::Deleted),
        Some("hash-second".to_string()),
    )
    .await;

    let batch = runtime
        .reconstruct_batch_from_checkpoints(OP_ID)
        .await
        .expect("reconstruction succeeds");

    assert_eq!(
        batch.file_changes.len(),
        1,
        "recovery reads the single hot-update batch only"
    );
    assert_eq!(batch.file_changes[0].path, Path::new("src/first.rs"));
}

// ---------------------------------------------------------------------------
// Reconstruction requires the checkpoint manager
// ---------------------------------------------------------------------------

#[tokio::test]
async fn missing_checkpoint_manager_is_an_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = in_memory_metadata_store().expect("in-memory metadata store");
    let scan_options = cce_scanner::ScanOptions {
        root_path: dir.path().to_string_lossy().to_string(),
        ..Default::default()
    };
    let runtime = HotUpdateOperationRuntime::new(1, store, scan_options);

    let error = runtime
        .reconstruct_batch_from_checkpoints(OP_ID)
        .await
        .expect_err("recovery without checkpoints cannot proceed");
    assert!(
        error.to_string().contains("checkpoint manager"),
        "the error must name the missing binding: {error}"
    );
}
