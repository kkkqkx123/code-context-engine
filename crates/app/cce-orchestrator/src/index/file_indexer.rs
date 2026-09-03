//! File indexer for deterministic file processing
//!
//! This module provides file indexing functionality with:
//! - Deterministic file sorting
//! - Batch splitting with clear boundaries
//! - Checkpoint-based recovery
//!
//! # Key Design Points
//!
//! 1. **Deterministic Sorting**: Files are sorted by path to ensure reproducible batching
//! 2. **File List Hashing**: Hash of sorted file list is computed for verification
//! 3. **Batch Boundary Tracking**: First and last file paths are recorded for validation

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

use crate::CheckpointManager;
use cce_metrics::ScannerMetrics;
use cce_scanner::{FSScanner, FileEntry, ScanOptions};
use cce_storage_sqlite::types::{BatchCheckpointRecord, CheckpointRecord, CheckpointStatus};
use cce_types::{OperationKind, StorageError};

/// Error returned when recovery from an existing checkpoint is not possible
#[derive(Debug)]
pub struct RecoveryValidation {
    reason: String,
}

impl RecoveryValidation {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// File indexer for deterministic file processing with checkpoint support
#[derive(Clone)]
pub struct FileIndexer {
    /// Sorted file list (for reproducibility)
    sorted_files: Vec<FileEntry>,
    /// Hash of the sorted file list (for verification)
    file_list_hash: String,
    /// Checkpoint record
    checkpoint: CheckpointRecord,
}

impl FileIndexer {
    /// Initialize the file indexer
    ///
    /// This will:
    /// 1. Scan files from the root directory
    /// 2. Sort files deterministically by path
    /// 3. Compute file list hash for verification
    /// 4. Create and persist an initial checkpoint
    pub async fn initialize(
        root_dir: &Path,
        batch_size: usize,
        scan_options: &ScanOptions,
        checkpoint_manager: Option<Arc<CheckpointManager>>,
        scanner_metrics: Option<Arc<ScannerMetrics>>,
        plugin_registry: Option<Arc<cce_plugin::PluginRegistry>>,
    ) -> Result<Self, StorageError> {
        // 1. Scan files
        let mut scanner = FSScanner::new();
        if let Some(ref metrics) = scanner_metrics {
            scanner = scanner.with_scanner_metrics(metrics.clone());
        }
        if let Some(registry) = plugin_registry {
            scanner = scanner.with_plugin_registry(registry);
        }
        let mut files = scanner
            .scan(scan_options)
            .map_err(|e| StorageError::query(format!("Failed to scan files: {}", e)))?;

        info!(
            count = files.len(),
            root_dir = %root_dir.display(),
            "Scanned files, now sorting for deterministic processing"
        );

        // 2. Sort files deterministically by path
        files.sort_by(|a, b| a.path.cmp(&b.path));

        debug!(
            count = files.len(),
            "Files sorted by path for deterministic batching"
        );

        // 3. Compute file list hash
        let file_list_hash = Self::compute_file_list_hash(&files);

        // 4. Create and persist initial checkpoint (skip if no checkpoint_manager)
        let operation_id = format!("full_{}", chrono::Utc::now().timestamp_millis());

        let project_id = if let Some(ref cm) = checkpoint_manager {
            cm.create_checkpoint(crate::operation::checkpoint::CreateCheckpointParams {
                operation_id: &operation_id,
                operation_type: OperationKind::FullIndex,
                root_dir: &root_dir.to_string_lossy(),
                total_files: files.len() as u32,
                batch_size: batch_size as u32,
                file_list_hash: &file_list_hash,
            })
            .await?;
            cm.project_id()
        } else {
            tracing::warn!("CheckpointManager not configured, skipping checkpoint persistence");
            0
        };

        let checkpoint = CheckpointRecord {
            id: None,
            project_id,
            operation_id,
            operation_type: OperationKind::FullIndex.to_string(),

            root_dir: root_dir.to_string_lossy().to_string(),
            total_files: files.len() as u32,
            batch_size: batch_size as u32,

            current_batch_index: 0,
            current_phase: "Scanning".to_string(),

            file_list_hash: Some(file_list_hash.clone()),

            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            last_error: None,
            failure_count: 0,

            status: CheckpointStatus::InProgress,
            active_flag: false,
            priority: 3,
            last_heartbeat: None,
            failed_at: None,
        };

        info!(
            operation_id = %checkpoint.operation_id,
            total_files = files.len(),
            batch_size = batch_size,
            file_list_hash = %file_list_hash,
            "FileIndexer initialized"
        );

        Ok(Self {
            sorted_files: files,
            file_list_hash,
            checkpoint,
        })
    }

    /// Compute the hash of the sorted file list (path + content hash)
    ///
    /// This hash is used to detect if the file list or file contents
    /// have changed between scans. Content hashes are included when
    /// available to detect content changes even when paths are the same.
    fn compute_file_list_hash(files: &[FileEntry]) -> String {
        let mut hasher = DefaultHasher::new();
        for file in files {
            file.path.to_string_lossy().hash(&mut hasher);
            file.content_hash.hash(&mut hasher);
        }
        format!("{:x}", hasher.finish())
    }

    /// Validate that a checkpoint is compatible with the current file state
    ///
    /// Checks:
    /// 1. File count is the same
    /// 2. Batch size is the same
    /// 3. Root directory is the same
    /// 4. File list hash matches (path + content hash fingerprint)
    /// 5. First and last files of the resume-start batch match the persisted
    ///    batch checkpoint boundary; when they do not (the file set changed
    ///    at that boundary), the previous completed batch's boundary is
    ///    re-validated as a fallback, since completed batches are
    ///    authoritative for the already-processed prefix.
    ///
    /// `batch_boundaries` must contain the persisted batch checkpoint records
    /// of the operation, at least for the resume-start batch and its
    /// predecessor. Records missing from the slice (batches never started)
    /// impose no constraint.
    pub fn validate_checkpoint(
        &self,
        checkpoint: &CheckpointRecord,
        batch_boundaries: &[BatchCheckpointRecord],
    ) -> Result<(), String> {
        // Check basic parameters
        if checkpoint.batch_size != self.checkpoint.batch_size {
            return Err(format!(
                "Batch size mismatch: {} vs {}",
                checkpoint.batch_size, self.checkpoint.batch_size
            ));
        }

        if checkpoint.root_dir != self.checkpoint.root_dir {
            return Err(format!(
                "Root directory mismatch: {} vs {}",
                checkpoint.root_dir, self.checkpoint.root_dir
            ));
        }

        // Check file count
        if checkpoint.total_files != self.sorted_files.len() as u32 {
            return Err(format!(
                "File count changed: {} -> {}",
                checkpoint.total_files,
                self.sorted_files.len()
            ));
        }

        // Check the persisted file list hash against the current scan.
        // A mismatch means the file set (paths and/or content hashes)
        // changed between the crash and the resume; the old batch boundaries
        // are no longer trustworthy and recovery must start over.
        if let Some(ref stored_hash) = checkpoint.file_list_hash {
            if stored_hash != &self.file_list_hash {
                return Err(format!(
                    "File list hash mismatch: checkpoint={stored_hash}, current={}",
                    self.file_list_hash
                ));
            }
        }

        // Check batch boundary files. The resume-start batch may carry a
        // stale boundary (it was being processed when the run died), so its
        // predecessor's boundary is validated as a fallback.
        let start_batch = checkpoint.current_batch_index;
        let mut boundary_failure: Option<String> = None;
        for probe in [start_batch, start_batch.saturating_sub(1)] {
            let Some(record) = batch_boundaries
                .iter()
                .find(|record| record.batch_index == probe)
            else {
                // No persisted boundary for this batch (never started or
                // never persisted): nothing stale to validate.
                continue;
            };
            match self.validate_batch_boundary(probe, record) {
                Ok(()) => {
                    boundary_failure = None;
                    break;
                }
                Err(error) => {
                    tracing::warn!(batch = probe, %error, "Batch boundary mismatch, probing previous batch");
                    boundary_failure = Some(error);
                }
            }
        }
        if let Some(error) = boundary_failure {
            return Err(error);
        }

        debug!(
            operation_id = %checkpoint.operation_id,
            "Checkpoint validation passed"
        );

        Ok(())
    }

    /// Validate the recorded first/last file boundary of a batch against the
    /// currently computed batch.
    pub fn validate_batch_boundary(
        &self,
        batch_index: u32,
        recorded: &BatchCheckpointRecord,
    ) -> Result<(), String> {
        let batch = self.get_batch(batch_index as usize)?;
        let current_first = batch
            .first()
            .map(|file| file.path.to_string_lossy().to_string())
            .unwrap_or_default();
        let current_last = batch
            .last()
            .map(|file| file.path.to_string_lossy().to_string())
            .unwrap_or_default();
        if recorded.first_file != current_first || recorded.last_file != current_last {
            return Err(format!(
                "Batch {batch_index} boundary mismatch: recorded=[{}, {}], current=[{}, {}]",
                recorded.first_file, recorded.last_file, current_first, current_last
            ));
        }
        Ok(())
    }

    /// Get files for a specific batch
    ///
    /// Returns a slice of the sorted file list for the given batch index.
    /// Returns an error if batch_index is out of range.
    pub fn get_batch(&self, batch_index: usize) -> Result<&[FileEntry], String> {
        let start = batch_index * self.checkpoint.batch_size as usize;
        let end =
            ((batch_index + 1) * self.checkpoint.batch_size as usize).min(self.sorted_files.len());

        if start >= self.sorted_files.len() {
            return Err(format!(
                "Batch index {} out of range (total batches: {})",
                batch_index,
                self.total_batches()
            ));
        }

        Ok(&self.sorted_files[start..end])
    }

    /// Get the total number of batches
    pub fn total_batches(&self) -> usize {
        self.sorted_files
            .len()
            .div_ceil(self.checkpoint.batch_size as usize)
    }

    /// Recover from an existing checkpoint
    ///
    /// This will:
    /// 1. Scan files from the root directory (same as initialize)
    /// 2. Sort files deterministically by path
    /// 3. Compute file list hash
    /// 4. Validate the existing checkpoint against current file state
    /// 5. If valid, return FileIndexer without creating a new DB entry
    ///
    /// Returns Err if the checkpoint is incompatible and a fresh start is needed.
    pub fn recover(
        root_dir: &Path,
        batch_size: usize,
        scan_options: &ScanOptions,
        existing_checkpoint: CheckpointRecord,
        scanner_metrics: Option<Arc<ScannerMetrics>>,
    ) -> Result<Self, RecoveryValidation> {
        // 1. Scan files
        let mut scanner = FSScanner::new();
        if let Some(ref metrics) = scanner_metrics {
            scanner = scanner.with_scanner_metrics(metrics.clone());
        }
        let mut files = scanner
            .scan(scan_options)
            .map_err(|e| RecoveryValidation::new(format!("Failed to scan files: {}", e)))?;

        // 2. Sort files deterministically by path
        files.sort_by(|a, b| a.path.cmp(&b.path));

        // 3. Compute file list hash
        let file_list_hash = Self::compute_file_list_hash(&files);

        // 4. Validate existing checkpoint against current file state
        Self::validate_existing_checkpoint(
            &existing_checkpoint,
            &files,
            &file_list_hash,
            batch_size,
            root_dir,
        )?;

        info!(
            operation_id = %existing_checkpoint.operation_id,
            current_batch_index = existing_checkpoint.current_batch_index,
            total_files = files.len(),
            "Recovered from existing checkpoint"
        );

        Ok(Self {
            sorted_files: files,
            file_list_hash,
            checkpoint: existing_checkpoint,
        })
    }

    /// Validate an existing checkpoint against current file state
    fn validate_existing_checkpoint(
        checkpoint: &CheckpointRecord,
        files: &[FileEntry],
        file_list_hash: &str,
        batch_size: usize,
        root_dir: &Path,
    ) -> Result<(), RecoveryValidation> {
        if checkpoint.root_dir != root_dir.to_string_lossy() {
            return Err(RecoveryValidation::new(format!(
                "Root directory mismatch: checkpoint={}, current={}",
                checkpoint.root_dir,
                root_dir.display()
            )));
        }

        if checkpoint.batch_size as usize != batch_size {
            return Err(RecoveryValidation::new(format!(
                "Batch size mismatch: checkpoint={}, current={}",
                checkpoint.batch_size, batch_size
            )));
        }

        if checkpoint.total_files as usize != files.len() {
            return Err(RecoveryValidation::new(format!(
                "File count changed: checkpoint={}, current={}",
                checkpoint.total_files,
                files.len()
            )));
        }

        if let Some(ref hash) = checkpoint.file_list_hash {
            if hash != file_list_hash {
                return Err(RecoveryValidation::new(
                    "File list hash changed, file list differs from checkpoint",
                ));
            }
        }

        Ok(())
    }

    /// Set a new operation_id on the checkpoint (for recovery reuse)
    pub fn set_operation_id(&mut self, operation_id: String) {
        self.checkpoint.operation_id = operation_id;
    }

    /// Get the operation ID
    pub fn operation_id(&self) -> &str {
        &self.checkpoint.operation_id
    }

    /// Get the checkpoint record
    pub fn checkpoint(&self) -> &CheckpointRecord {
        &self.checkpoint
    }

    /// Get the sorted files list
    pub fn files(&self) -> &[FileEntry] {
        &self.sorted_files
    }

    /// Get the file list hash
    pub fn file_list_hash(&self) -> &str {
        &self.file_list_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_compute_file_list_hash() {
        let file1 = FileEntry {
            path: PathBuf::from("/a.rs"),
            relative_path: PathBuf::from("a.rs"),
            size: 100,
            modified: chrono::Utc::now(),
            content_hash: None,
            language_info: None,
        };
        let file2 = FileEntry {
            path: PathBuf::from("/b.rs"),
            relative_path: PathBuf::from("b.rs"),
            size: 200,
            modified: chrono::Utc::now(),
            content_hash: None,
            language_info: None,
        };

        let files = vec![file1.clone(), file2.clone()];
        let hash1 = FileIndexer::compute_file_list_hash(&files);

        // Same order should produce same hash
        let files2 = vec![file1, file2];
        let hash2 = FileIndexer::compute_file_list_hash(&files2);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_file_list_hash_order_matters() {
        let file1 = FileEntry {
            path: PathBuf::from("/a.rs"),
            relative_path: PathBuf::from("a.rs"),
            size: 100,
            modified: chrono::Utc::now(),
            content_hash: None,
            language_info: None,
        };
        let file2 = FileEntry {
            path: PathBuf::from("/b.rs"),
            relative_path: PathBuf::from("b.rs"),
            size: 200,
            modified: chrono::Utc::now(),
            content_hash: None,
            language_info: None,
        };

        let files1 = vec![file1.clone(), file2.clone()];
        let hash1 = FileIndexer::compute_file_list_hash(&files1);

        // Reversed order should produce different hash
        let files2 = vec![file2, file1];
        let hash2 = FileIndexer::compute_file_list_hash(&files2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_recovery_validation_rejects_different_root() {
        let checkpoint = CheckpointRecord {
            id: None,
            project_id: 1,
            operation_id: "full-test".to_string(),
            operation_type: OperationKind::FullIndex.to_string(),
            root_dir: "/project-a".to_string(),
            total_files: 0,
            batch_size: 10,
            current_batch_index: 0,
            current_phase: "Scanning".to_string(),
            file_list_hash: Some("empty".to_string()),
            created_at: String::new(),
            updated_at: String::new(),
            last_error: None,
            failure_count: 0,
            status: CheckpointStatus::InProgress,
            active_flag: false,
            priority: 3,
            last_heartbeat: None,
            failed_at: None,
        };

        let result = FileIndexer::validate_existing_checkpoint(
            &checkpoint,
            &[],
            "empty",
            10,
            Path::new("/project-b"),
        );

        let error = result.expect_err("A checkpoint from another root must be rejected");
        assert!(error.reason().contains("Root directory mismatch"));
    }

    // -----------------------------------------------------------------------
    // validate_checkpoint: file_list_hash + batch boundary validation
    // -----------------------------------------------------------------------

    fn test_file_entry(path: &str) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            relative_path: PathBuf::from(path.trim_start_matches('/')),
            size: 100,
            modified: chrono::Utc::now(),
            content_hash: None,
            language_info: None,
        }
    }

    fn test_indexer_with_hash(files: &[FileEntry], file_list_hash: &str) -> FileIndexer {
        FileIndexer {
            sorted_files: files.to_vec(),
            file_list_hash: file_list_hash.to_string(),
            checkpoint: CheckpointRecord {
                id: None,
                project_id: 1,
                operation_id: "full-test".to_string(),
                operation_type: OperationKind::FullIndex.to_string(),
                root_dir: "/project-a".to_string(),
                total_files: files.len() as u32,
                batch_size: 2,
                current_batch_index: 1,
                current_phase: "Scanning".to_string(),
                file_list_hash: Some(file_list_hash.to_string()),
                created_at: String::new(),
                updated_at: String::new(),
                last_error: None,
                failure_count: 0,
                status: CheckpointStatus::InProgress,
                active_flag: false,
                priority: 3,
                last_heartbeat: None,
                failed_at: None,
            },
        }
    }

    fn test_checkpoint_record(
        current_batch_index: u32,
        file_list_hash: &str,
        total_files: u32,
    ) -> CheckpointRecord {
        CheckpointRecord {
            id: None,
            project_id: 1,
            operation_id: "full-test".to_string(),
            operation_type: OperationKind::FullIndex.to_string(),
            root_dir: "/project-a".to_string(),
            total_files,
            batch_size: 2,
            current_batch_index,
            current_phase: "Parsing".to_string(),
            file_list_hash: Some(file_list_hash.to_string()),
            created_at: String::new(),
            updated_at: String::new(),
            last_error: None,
            failure_count: 0,
            status: CheckpointStatus::InProgress,
            active_flag: false,
            priority: 3,
            last_heartbeat: None,
            failed_at: None,
        }
    }

    fn test_batch_record(batch_index: u32, first: &str, last: &str) -> BatchCheckpointRecord {
        BatchCheckpointRecord {
            id: None,
            operation_id: "full-test".to_string(),
            batch_index,
            first_file: first.to_string(),
            last_file: last.to_string(),
            file_count: 2,
            processed_files: 0,
            failed_files: 0,
            entities_extracted: 0,
            relations_found: 0,
            chunks_generated: 0,
            vectors_stored: 0,
            start_time: String::new(),
            end_time: None,
            duration_ms: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn test_validate_checkpoint_rejects_hash_mismatch() {
        let files = vec![test_file_entry("/a.rs"), test_file_entry("/b.rs")];
        let indexer = test_indexer_with_hash(&files, "current-hash");
        let checkpoint = test_checkpoint_record(1, "stale-hash", files.len() as u32);
        let boundaries = vec![test_batch_record(1, "/a.rs", "/b.rs")];

        let error = indexer
            .validate_checkpoint(&checkpoint, &boundaries)
            .expect_err("A checkpoint with a stale file list hash must be rejected");
        assert!(error.contains("File list hash mismatch"));
    }

    #[test]
    fn test_validate_checkpoint_accepts_matching_boundary() {
        let files = vec![
            test_file_entry("/a.rs"),
            test_file_entry("/b.rs"),
            test_file_entry("/c.rs"),
            test_file_entry("/d.rs"),
        ];
        let indexer = test_indexer_with_hash(&files, "current-hash");
        let checkpoint = test_checkpoint_record(1, "current-hash", files.len() as u32);
        // Batch 1 (the resume start) is [c.rs, d.rs] with batch_size 2
        let boundaries = vec![test_batch_record(1, "/c.rs", "/d.rs")];

        indexer
            .validate_checkpoint(&checkpoint, &boundaries)
            .expect("Matching start-batch boundary must validate");
    }

    #[test]
    fn test_validate_checkpoint_falls_back_to_previous_boundary() {
        let files = vec![
            test_file_entry("/a.rs"),
            test_file_entry("/b.rs"),
            test_file_entry("/c.rs"),
            test_file_entry("/d.rs"),
        ];
        let indexer = test_indexer_with_hash(&files, "current-hash");
        let checkpoint = test_checkpoint_record(1, "current-hash", files.len() as u32);
        // The start batch carries a stale boundary (file set changed while
        // keeping the same count); the completed predecessor batch [a.rs, b.rs]
        // still matches, so recovery is safe.
        let boundaries = vec![
            test_batch_record(1, "/x.rs", "/y.rs"),
            test_batch_record(0, "/a.rs", "/b.rs"),
        ];

        indexer
            .validate_checkpoint(&checkpoint, &boundaries)
            .expect("Fallback to the previous completed batch boundary must validate");
    }

    #[test]
    fn test_validate_checkpoint_rejects_drifted_boundaries() {
        let files = vec![
            test_file_entry("/a.rs"),
            test_file_entry("/b.rs"),
            test_file_entry("/c.rs"),
            test_file_entry("/d.rs"),
        ];
        let indexer = test_indexer_with_hash(&files, "current-hash");
        let checkpoint = test_checkpoint_record(1, "current-hash", files.len() as u32);
        // Neither the start batch nor its predecessor matches the current
        // batching: the file list has drifted and recovery must restart.
        let boundaries = vec![
            test_batch_record(1, "/x.rs", "/y.rs"),
            test_batch_record(0, "/w.rs", "/z.rs"),
        ];

        let error = indexer
            .validate_checkpoint(&checkpoint, &boundaries)
            .expect_err("Drifted batch boundaries must reject recovery");
        assert!(error.contains("boundary mismatch"));
    }

    #[test]
    fn test_validate_checkpoint_passes_without_boundary_records() {
        let files = vec![
            test_file_entry("/a.rs"),
            test_file_entry("/b.rs"),
            test_file_entry("/c.rs"),
            test_file_entry("/d.rs"),
        ];
        let indexer = test_indexer_with_hash(&files, "current-hash");
        let checkpoint = test_checkpoint_record(1, "current-hash", files.len() as u32);

        // No persisted boundary records: the batch was never started, so
        // nothing stale exists and validation passes.
        indexer
            .validate_checkpoint(&checkpoint, &[])
            .expect("Missing boundary records must not block recovery");
    }
}
