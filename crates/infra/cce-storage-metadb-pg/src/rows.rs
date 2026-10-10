use cce_types::StorageError;
use tokio_postgres::Row;

use cce_storage_common::metadb::{
    AdmissionAuditRecord, CheckpointRecord, CheckpointStatus, ChunkRecord, FileCheckpointRecord,
    ProjectIndexManifest, ProjectIndexManifestState, ProjectRecord, WorkUnitCheckpointRecord,
    WorkUnitStatus,
};

use crate::error::classify_pg;

pub fn project_from_row(row: &Row) -> Result<ProjectRecord, StorageError> {
    let respect_gitignore: Option<i64> = row.try_get("respect_gitignore").map_err(classify_pg)?;
    Ok(ProjectRecord {
        id: row.try_get("id").map_err(classify_pg)?,
        name: row.try_get("name").map_err(classify_pg)?,
        root_path: row.try_get("root_path").map_err(classify_pg)?,
        config_file_path: row.try_get("config_file_path").map_err(classify_pg)?,
        language: row.try_get("language").map_err(classify_pg)?,
        extensions: row.try_get("extensions").map_err(classify_pg)?,
        exclude_dirs: row.try_get("exclude_dirs").map_err(classify_pg)?,
        respect_gitignore: respect_gitignore.map(|flag| flag != 0),
        ignore_patterns: row.try_get("ignore_patterns").map_err(classify_pg)?,
        last_indexed: row.try_get("last_indexed").map_err(classify_pg)?,
        created_at: row.try_get("created_at").map_err(classify_pg)?,
        updated_at: row.try_get("updated_at").map_err(classify_pg)?,
    })
}

pub const PROJECT_COLUMNS: &str = "id, name, root_path, config_file_path, language, extensions, \
     exclude_dirs, respect_gitignore, ignore_patterns, last_indexed, created_at, updated_at";

pub fn manifest_state_from(value: &str) -> Result<ProjectIndexManifestState, StorageError> {
    match value {
        "building" => Ok(ProjectIndexManifestState::Building),
        "active" => Ok(ProjectIndexManifestState::Active),
        "failed" => Ok(ProjectIndexManifestState::Failed),
        other => Err(StorageError::query(format!(
            "invalid project index manifest state: {other}"
        ))),
    }
}

pub const MANIFEST_COLUMNS: &str = "project_id, publication_epoch, data_epoch, relation_epoch, \
     operation_id, state, input_fingerprint, candidate_ready, parent_data_epoch";

pub fn manifest_from_row(row: &Row) -> Result<ProjectIndexManifest, StorageError> {
    let state: String = row.try_get("state").map_err(classify_pg)?;
    let candidate_ready: i64 = row.try_get("candidate_ready").map_err(classify_pg)?;
    Ok(ProjectIndexManifest {
        project_id: row.try_get("project_id").map_err(classify_pg)?,
        publication_epoch: row.try_get("publication_epoch").map_err(classify_pg)?,
        data_epoch: row.try_get("data_epoch").map_err(classify_pg)?,
        relation_epoch: row.try_get("relation_epoch").map_err(classify_pg)?,
        operation_id: row.try_get("operation_id").map_err(classify_pg)?,
        state: manifest_state_from(&state)?,
        input_fingerprint: row.try_get("input_fingerprint").map_err(classify_pg)?,
        candidate_ready: candidate_ready != 0,
        parent_data_epoch: row.try_get("parent_data_epoch").map_err(classify_pg)?,
    })
}

pub fn chunk_from_row(row: &Row) -> Result<ChunkRecord, StorageError> {
    let test_status: i64 = row.try_get("test_status").map_err(classify_pg)?;
    let test_source: i64 = row.try_get("test_source").map_err(classify_pg)?;
    let truncated: i64 = row.try_get("truncated").map_err(classify_pg)?;
    Ok(ChunkRecord {
        chunk_id: row.try_get("chunk_id").map_err(classify_pg)?,
        file_path: row.try_get("file_path").map_err(classify_pg)?,
        content: row.try_get("content").map_err(classify_pg)?,
        start_line: row.try_get("start_line").map_err(classify_pg)?,
        end_line: row.try_get("end_line").map_err(classify_pg)?,
        entity_ids: row.try_get("entity_ids").map_err(classify_pg)?,
        entity_names: row.try_get("entity_names").map_err(classify_pg)?,
        entity_kinds: row.try_get("entity_kinds").map_err(classify_pg)?,
        group_title: row.try_get("group_title").map_err(classify_pg)?,
        chunk_type: row.try_get("chunk_type").map_err(classify_pg)?,
        test_status: test_status as u8,
        test_source: test_source as u8,
        created_at: row.try_get("created_at").map_err(classify_pg)?,
        updated_at: row.try_get("updated_at").map_err(classify_pg)?,
        project_id: row.try_get("project_id").map_err(classify_pg)?,
        epoch: row.try_get("epoch").map_err(classify_pg)?,
        batch_id: row.try_get("batch_id").map_err(classify_pg)?,
        path: row.try_get("path").map_err(classify_pg)?,
        bm25_keywords: row.try_get("bm25_keywords").map_err(classify_pg)?,
        segment_id: row.try_get("segment_id").map_err(classify_pg)?,
        truncated: truncated as u8,
    })
}

pub const CHUNK_COLUMNS: &str = "chunk_id, file_path, content, start_line, end_line, entity_ids, \
     entity_names, entity_kinds, group_title, chunk_type, test_status, test_source, created_at, updated_at, project_id, \
     epoch, batch_id, path, bm25_keywords, segment_id, truncated";

pub fn admission_from_row(row: &Row) -> Result<AdmissionAuditRecord, StorageError> {
    Ok(AdmissionAuditRecord {
        token_fingerprint: row.try_get("token_fingerprint").map_err(classify_pg)?,
        projects: row.try_get("projects").map_err(classify_pg)?,
        quota_bytes: row.try_get("quota_bytes").map_err(classify_pg)?,
        bytes_used: row.try_get("bytes_used").map_err(classify_pg)?,
        admitted: row.try_get("admitted").map_err(classify_pg)?,
        auth_rejections: row.try_get("auth_rejections").map_err(classify_pg)?,
        scope_rejections: row.try_get("scope_rejections").map_err(classify_pg)?,
        rate_rejections: row.try_get("rate_rejections").map_err(classify_pg)?,
        body_rejections: row.try_get("body_rejections").map_err(classify_pg)?,
        quota_rejections: row.try_get("quota_rejections").map_err(classify_pg)?,
        last_used: row.try_get("last_used").map_err(classify_pg)?,
        last_reject_reason: row.try_get("last_reject_reason").map_err(classify_pg)?,
    })
}

pub fn checkpoint_from_row(row: &Row) -> Result<CheckpointRecord, StorageError> {
    let status: String = row.try_get("status").map_err(classify_pg)?;
    let status: CheckpointStatus = status
        .parse()
        .map_err(|_| StorageError::query(format!("invalid checkpoint status: {status}")))?;
    let total_files: i64 = row.try_get("total_files").map_err(classify_pg)?;
    let batch_size: i64 = row.try_get("batch_size").map_err(classify_pg)?;
    let current_batch_index: i64 = row.try_get("current_batch_index").map_err(classify_pg)?;
    let failure_count: i64 = row.try_get("failure_count").map_err(classify_pg)?;
    let active_flag: i64 = row.try_get("active_flag").map_err(classify_pg)?;
    let priority: i64 = row.try_get("priority").map_err(classify_pg)?;
    Ok(CheckpointRecord {
        id: row.try_get("id").map_err(classify_pg)?,
        project_id: row.try_get("project_id").map_err(classify_pg)?,
        operation_id: row.try_get("operation_id").map_err(classify_pg)?,
        operation_type: row.try_get("operation_type").map_err(classify_pg)?,
        root_dir: row.try_get("root_dir").map_err(classify_pg)?,
        total_files: total_files as u32,
        batch_size: batch_size as u32,
        current_batch_index: current_batch_index as u32,
        current_phase: row.try_get("current_phase").map_err(classify_pg)?,
        file_list_hash: row.try_get("file_list_hash").map_err(classify_pg)?,
        created_at: row.try_get("created_at").map_err(classify_pg)?,
        updated_at: row.try_get("updated_at").map_err(classify_pg)?,
        last_error: row.try_get("last_error").map_err(classify_pg)?,
        failure_count: failure_count as u32,
        status,
        active_flag: active_flag != 0,
        priority: priority as i32,
        last_heartbeat: row.try_get("last_heartbeat").map_err(classify_pg)?,
        failed_at: row.try_get("failed_at").map_err(classify_pg)?,
    })
}

pub fn file_checkpoint_from_row(
    project_id: i64,
    row: &Row,
) -> Result<FileCheckpointRecord, StorageError> {
    let batch_index: i64 = row.try_get("batch_index").map_err(classify_pg)?;
    let embedding_count: i64 = row.try_get("embedding_count").map_err(classify_pg)?;
    let _ = project_id;
    Ok(FileCheckpointRecord {
        id: row.try_get("id").map_err(classify_pg)?,
        operation_id: row.try_get("operation_id").map_err(classify_pg)?,
        batch_index: batch_index as u32,
        file_path: row.try_get("file_path").map_err(classify_pg)?,
        file_id: row.try_get("file_id").map_err(classify_pg)?,
        language: row.try_get("language").map_err(classify_pg)?,
        file_size: row.try_get("file_size").map_err(classify_pg)?,
        content_hash: row.try_get("content_hash").map_err(classify_pg)?,
        parsed_data: row.try_get("parsed_data").map_err(classify_pg)?,
        parse_error: row.try_get("parse_error").map_err(classify_pg)?,
        summary_data: row.try_get("summary_data").map_err(classify_pg)?,
        embedding_count: embedding_count as u32,
        bm25_doc_id: row.try_get("bm25_doc_id").map_err(classify_pg)?,
        export_path: row.try_get("export_path").map_err(classify_pg)?,
        render_fingerprint: row.try_get("render_fingerprint").map_err(classify_pg)?,
        module_progress: row.try_get("module_progress").map_err(classify_pg)?,
        created_at: row.try_get("created_at").map_err(classify_pg)?,
        updated_at: row.try_get("updated_at").map_err(classify_pg)?,
    })
}

pub fn work_unit_from_row(row: &Row) -> Result<WorkUnitCheckpointRecord, StorageError> {
    let status: String = row.try_get("status").map_err(classify_pg)?;
    let status: WorkUnitStatus = status
        .parse()
        .map_err(|_| StorageError::query(format!("invalid work unit status: {status}")))?;
    let item_count: i64 = row.try_get("item_count").map_err(classify_pg)?;
    Ok(WorkUnitCheckpointRecord {
        id: row.try_get("id").map_err(classify_pg)?,
        project_id: row.try_get("project_id").map_err(classify_pg)?,
        operation_id: row.try_get("operation_id").map_err(classify_pg)?,
        stage: row.try_get("stage").map_err(classify_pg)?,
        target_epoch: row.try_get("target_epoch").map_err(classify_pg)?,
        work_unit_hash: row.try_get("work_unit_hash").map_err(classify_pg)?,
        status,
        item_count: item_count as u32,
        created_at: row.try_get("created_at").map_err(classify_pg)?,
        updated_at: row.try_get("updated_at").map_err(classify_pg)?,
    })
}
