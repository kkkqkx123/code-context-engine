//! Transaction management for the export processor.
//!
//! Handles backup, commit, abort, and recovery operations for NL document exports.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::export::nl_exporter::NlDocumentExporter;
use crate::hot_update::{HotUpdateError, Result};
use crate::operation::OperationContext;
use crate::operation::checkpoint::CheckpointManager;

use super::export_staging::{ExportStaging, StagedWrite};

/// Normalize an operation ID for use in a filesystem directory name.
pub fn sanitize_operation_id(operation_id: &str) -> String {
    operation_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Move a pre-existing document to a backup location if it exists.
///
/// Returns the backup path, or `None` when no previous document existed.
pub async fn backup_existing(
    exporter: &NlDocumentExporter,
    ctx: &OperationContext,
    output: &Path,
) -> Result<Option<PathBuf>> {
    if !output.exists() {
        return Ok(None);
    }
    let backup_dir = exporter.config().output_dir().join(format!(
        ".export-backup-{}",
        sanitize_operation_id(&ctx.operation_id)
    ));
    let file_name = output
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "doc.md".to_string());
    let backup = backup_dir.join(file_name);
    if let Some(parent) = backup.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| HotUpdateError::export(e.to_string()))?;
    }
    tokio::fs::rename(output, &backup)
        .await
        .map_err(|e| HotUpdateError::export(e.to_string()))?;
    Ok(Some(backup))
}

/// Flush one staged deletion to disk, backing up any existing document.
pub async fn flush_deletion(
    exporter: &NlDocumentExporter,
    ctx: &OperationContext,
    output: &Path,
    staging: &mut ExportStaging,
) -> Result<()> {
    if let Some(backup) = backup_existing(exporter, ctx, output).await? {
        staging.backups.push((output.to_path_buf(), backup));
        staging.backed_up.push(output.to_path_buf());
    }
    staging.committed_deletions.push(output.to_path_buf());
    Ok(())
}

/// Flush one staged write to disk, backing up any existing document and
/// persisting the `export_path` checkpoint marker.
pub async fn flush_write(
    exporter: &NlDocumentExporter,
    ctx: &OperationContext,
    write: StagedWrite,
    staging: &mut ExportStaging,
    checkpoint_manager: &Option<Arc<CheckpointManager>>,
) -> Result<()> {
    if let Some(backup) = backup_existing(exporter, ctx, &write.output_path).await? {
        staging.backups.push((write.output_path.clone(), backup));
        staging.backed_up.push(write.output_path.clone());
    }
    staging.committed.push(write.clone());
    crate::export::path_utils::write_file_atomic(&write.output_path, &write.content)
        .await
        .map_err(|e| HotUpdateError::export(e.to_string()))?;
    persist_export_path(ctx, &write, checkpoint_manager).await?;
    Ok(())
}

/// Persist the relative output path and its render fingerprint on the file
/// checkpoint so a later resume can skip files whose documents were already
/// exported with the same rendering inputs.
async fn persist_export_path(
    ctx: &OperationContext,
    write: &StagedWrite,
    checkpoint_manager: &Option<Arc<CheckpointManager>>,
) -> Result<()> {
    let Some(cm) = checkpoint_manager else {
        return Ok(());
    };
    let mut record = cm
        .create_file_checkpoint(&ctx.operation_id, 0, &write.source_path)
        .await
        .map_err(|e| HotUpdateError::export(e.to_string()))?;
    record.export_path = Some(write.relative_output.clone());
    record.render_fingerprint = Some(write.render_fingerprint.clone());
    cm.save_file_checkpoint(&record)
        .await
        .map_err(|e| HotUpdateError::export(e.to_string()))
}

/// Restore backed-up documents and remove newly-written ones during abort.
pub async fn restore_from_backup(
    exporter: &NlDocumentExporter,
    ctx: &OperationContext,
    staging: &mut ExportStaging,
) -> Result<()> {
    // Restore pre-existing documents that were backed up during commit;
    // remove documents that did not exist before this operation.
    for (output, backup) in staging.drain_stale_backups() {
        if backup.exists() {
            let _ = tokio::fs::rename(&backup, &output).await;
        }
    }
    for write in staging.committed.drain(..) {
        if !staging.backed_up.contains(&write.output_path) {
            let _ = tokio::fs::remove_file(&write.output_path).await;
        }
    }
    for output in staging.committed_deletions.drain(..) {
        if !staging.backed_up.contains(&output) {
            let _ = tokio::fs::remove_file(&output).await;
        }
    }
    staging.backed_up.clear();

    // Remove the operation's backup directory.
    let backup_dir = exporter.config().output_dir().join(format!(
        ".export-backup-{}",
        sanitize_operation_id(&ctx.operation_id)
    ));
    let _ = tokio::fs::remove_dir_all(&backup_dir).await;

    Ok(())
}
