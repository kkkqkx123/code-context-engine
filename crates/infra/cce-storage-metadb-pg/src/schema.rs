pub const POSTGRES_SCHEMA_VERSION: i64 = 2;

/// Incremental migration bringing pre-V2 databases to the current shape.
/// Fresh databases get the column from `V1_DDL` directly.
pub const V2_DDL: &str = r#"
ALTER TABLE chunks ADD COLUMN IF NOT EXISTS entity_kinds TEXT NOT NULL DEFAULT '[]';
ALTER TABLE chunks ADD COLUMN IF NOT EXISTS group_title TEXT NOT NULL DEFAULT '';
"#;

pub const V1_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS projects (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    root_path TEXT NOT NULL UNIQUE,
    config_file_path TEXT NOT NULL DEFAULT '.cce/config.json',
    language TEXT,
    extensions TEXT,
    exclude_dirs TEXT,
    respect_gitignore BIGINT,
    ignore_patterns TEXT,
    last_indexed TEXT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS project_meta (
    project_id BIGINT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (project_id, key)
);
CREATE TABLE IF NOT EXISTS project_index_manifests (
    project_id BIGINT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    publication_epoch BIGINT NOT NULL,
    data_epoch BIGINT NOT NULL,
    relation_epoch BIGINT NOT NULL,
    operation_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('building', 'active', 'failed')),
    input_fingerprint TEXT,
    created_at BIGINT NOT NULL,
    activated_at BIGINT,
    failure_reason TEXT,
    candidate_ready BIGINT NOT NULL DEFAULT 0,
    parent_data_epoch BIGINT,
    PRIMARY KEY (project_id, publication_epoch),
    UNIQUE (project_id, operation_id)
);
CREATE INDEX IF NOT EXISTS idx_project_index_manifest_active
    ON project_index_manifests (project_id, state, publication_epoch DESC);
CREATE TABLE IF NOT EXISTS generation_overrides (
    project_id BIGINT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    epoch BIGINT NOT NULL,
    file_path TEXT NOT NULL,
    disposition TEXT NOT NULL CHECK (disposition IN ('replaced', 'deleted')),
    PRIMARY KEY (project_id, epoch, file_path)
);
CREATE TABLE IF NOT EXISTS admission_audit (
    token_fingerprint TEXT PRIMARY KEY,
    projects TEXT NOT NULL DEFAULT '',
    quota_bytes BIGINT,
    bytes_used BIGINT NOT NULL DEFAULT 0,
    admitted BIGINT NOT NULL DEFAULT 0,
    auth_rejections BIGINT NOT NULL DEFAULT 0,
    scope_rejections BIGINT NOT NULL DEFAULT 0,
    rate_rejections BIGINT NOT NULL DEFAULT 0,
    body_rejections BIGINT NOT NULL DEFAULT 0,
    quota_rejections BIGINT NOT NULL DEFAULT 0,
    last_used BIGINT,
    last_reject_reason TEXT,
    updated_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS files (
    id BIGSERIAL PRIMARY KEY,
    path TEXT NOT NULL,
    language TEXT NOT NULL,
    category BIGINT NOT NULL DEFAULT 4,
    last_modified BIGINT NOT NULL,
    created_at BIGINT NOT NULL,
    project_id BIGINT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    content_hash TEXT,
    epoch BIGINT NOT NULL DEFAULT 0,
    batch_id BIGINT NOT NULL DEFAULT 0,
    UNIQUE (project_id, epoch, path)
);
CREATE INDEX IF NOT EXISTS idx_files_project ON files (project_id);
CREATE INDEX IF NOT EXISTS idx_files_project_path ON files (project_id, path);
CREATE TABLE IF NOT EXISTS entities (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    file_id BIGINT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    signature TEXT,
    span_start_row BIGINT,
    span_end_row BIGINT,
    span_start_column BIGINT,
    span_end_column BIGINT,
    span_start_byte BIGINT,
    span_end_byte BIGINT,
    scoped_name TEXT,
    depth BIGINT,
    parent_id BIGINT REFERENCES entities(id) ON DELETE SET NULL,
    metadata TEXT,
    parameters_json TEXT,
    return_type TEXT,
    doc_comment TEXT,
    modifiers_json TEXT,
    project_id BIGINT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    epoch BIGINT NOT NULL DEFAULT 0,
    batch_id BIGINT NOT NULL DEFAULT 0,
    rank DOUBLE PRECISION NOT NULL DEFAULT 0,
    UNIQUE (project_id, epoch, file_id, scoped_name, kind)
);
CREATE INDEX IF NOT EXISTS idx_entities_project ON entities (project_id);
CREATE INDEX IF NOT EXISTS idx_entities_project_name ON entities (project_id, name);
CREATE INDEX IF NOT EXISTS idx_entities_project_kind ON entities (project_id, kind);
CREATE INDEX IF NOT EXISTS idx_entities_project_file ON entities (project_id, file_id);
CREATE INDEX IF NOT EXISTS idx_entities_file_epoch ON entities (file_id, epoch);
CREATE TABLE IF NOT EXISTS entity_detail_mappings (
    id BIGSERIAL PRIMARY KEY,
    entity_id BIGINT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    project_id BIGINT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    epoch BIGINT NOT NULL DEFAULT 0,
    qdrant_point_ids TEXT NOT NULL DEFAULT '[]',
    bm25_doc_ids TEXT NOT NULL DEFAULT '[]',
    chunk_count BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    UNIQUE (project_id, epoch, entity_id)
);
CREATE INDEX IF NOT EXISTS idx_detail_mappings_project_epoch_entity
    ON entity_detail_mappings (project_id, epoch, entity_id);
CREATE TABLE IF NOT EXISTS chunks (
    chunk_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    content TEXT NOT NULL,
    start_line BIGINT NOT NULL,
    end_line BIGINT NOT NULL,
    entity_ids TEXT NOT NULL DEFAULT '[]',
    entity_names TEXT NOT NULL DEFAULT '[]',
    entity_kinds TEXT NOT NULL DEFAULT '[]',
    group_title TEXT NOT NULL DEFAULT '',
    chunk_type TEXT NOT NULL,
    test_status BIGINT NOT NULL DEFAULT 0,
    test_source BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    project_id BIGINT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    epoch BIGINT NOT NULL DEFAULT 0,
    batch_id BIGINT NOT NULL DEFAULT 0,
    path TEXT NOT NULL DEFAULT 'emb',
    bm25_keywords TEXT NOT NULL DEFAULT '',
    segment_id TEXT NOT NULL DEFAULT '',
    truncated BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (project_id, epoch, chunk_id)
);
CREATE INDEX IF NOT EXISTS idx_chunks_project ON chunks (project_id);
CREATE TABLE IF NOT EXISTS file_summaries (
    id BIGSERIAL PRIMARY KEY,
    file_id BIGINT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    epoch BIGINT NOT NULL DEFAULT 0,
    summary_json TEXT,
    summary_text TEXT GENERATED ALWAYS AS (COALESCE(summary_json::jsonb ->> 'summary_text', '')) STORED,
    language TEXT GENERATED ALWAYS AS (COALESCE(summary_json::jsonb ->> 'language', 'unknown')) STORED,
    entity_count BIGINT GENERATED ALWAYS AS (COALESCE((summary_json::jsonb ->> 'entity_count')::BIGINT, 0)) STORED,
    line_count BIGINT GENERATED ALWAYS AS (COALESCE((summary_json::jsonb ->> 'line_count')::BIGINT, 0)) STORED,
    qdrant_point_id TEXT,
    bm25_doc_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (file_id, epoch)
);
CREATE INDEX IF NOT EXISTS idx_file_summaries_file_epoch ON file_summaries (file_id, epoch);
CREATE TABLE IF NOT EXISTS checkpoint (
    id BIGSERIAL PRIMARY KEY,
    project_id BIGINT NOT NULL,
    operation_id TEXT NOT NULL,
    operation_type TEXT NOT NULL,
    root_dir TEXT NOT NULL,
    total_files BIGINT NOT NULL,
    batch_size BIGINT NOT NULL,
    current_batch_index BIGINT NOT NULL DEFAULT 0,
    current_phase TEXT NOT NULL DEFAULT 'Scanning',
    file_list_hash TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_error TEXT,
    failure_count BIGINT NOT NULL DEFAULT 0,
    status TEXT DEFAULT 'in_progress',
    operation_mode TEXT,
    active_flag BIGINT DEFAULT 0,
    priority BIGINT NOT NULL DEFAULT 0,
    last_heartbeat TEXT,
    failed_at TEXT,
    UNIQUE (project_id, operation_id),
    CHECK (current_batch_index >= 0),
    CHECK (batch_size > 0),
    CHECK (active_flag IN (0, 1)),
    CHECK (priority IN (0, 1, 2, 3)),
    CHECK (status IN ('in_progress', 'completed', 'failed'))
);
CREATE INDEX IF NOT EXISTS idx_checkpoint_project_operation ON checkpoint (project_id, operation_id);
CREATE INDEX IF NOT EXISTS idx_checkpoint_status ON checkpoint (status);
CREATE TABLE IF NOT EXISTS checkpoint_batch (
    id BIGSERIAL PRIMARY KEY,
    project_id BIGINT NOT NULL,
    operation_id TEXT NOT NULL,
    batch_index BIGINT NOT NULL,
    first_file TEXT NOT NULL,
    last_file TEXT NOT NULL,
    file_count BIGINT NOT NULL,
    processed_files BIGINT DEFAULT 0,
    failed_files BIGINT DEFAULT 0,
    entities_extracted BIGINT DEFAULT 0,
    relations_found BIGINT DEFAULT 0,
    chunks_generated BIGINT DEFAULT 0,
    vectors_stored BIGINT DEFAULT 0,
    start_time TEXT NOT NULL,
    end_time TEXT,
    duration_ms BIGINT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (project_id, operation_id, batch_index),
    FOREIGN KEY (project_id, operation_id) REFERENCES checkpoint (project_id, operation_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS checkpoint_file (
    id BIGSERIAL PRIMARY KEY,
    project_id BIGINT NOT NULL,
    operation_id TEXT NOT NULL,
    batch_index BIGINT NOT NULL,
    file_path TEXT NOT NULL,
    file_id BIGINT,
    language TEXT,
    file_size BIGINT,
    content_hash TEXT,
    parsed_data BYTEA,
    parse_error TEXT,
    summary_data BYTEA,
    embedding_count BIGINT DEFAULT 0,
    bm25_doc_id TEXT,
    export_path TEXT,
    render_fingerprint TEXT,
    module_progress TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (project_id, operation_id, file_path)
);
CREATE INDEX IF NOT EXISTS idx_checkpoint_file_batch ON checkpoint_file (operation_id, batch_index);
CREATE TABLE IF NOT EXISTS work_unit_checkpoint (
    id BIGSERIAL PRIMARY KEY,
    project_id BIGINT NOT NULL,
    operation_id TEXT NOT NULL,
    stage TEXT NOT NULL,
    target_epoch BIGINT NOT NULL,
    work_unit_hash TEXT NOT NULL,
    status TEXT DEFAULT 'pending',
    item_count BIGINT DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (project_id, operation_id, stage, work_unit_hash)
);
CREATE INDEX IF NOT EXISTS idx_work_unit_op_stage
    ON work_unit_checkpoint (project_id, operation_id, stage);
CREATE TABLE IF NOT EXISTS index_state_projection (
    project_id BIGINT NOT NULL,
    operation_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    version BIGINT NOT NULL,
    state_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (project_id, operation_id, file_path)
);
CREATE TABLE IF NOT EXISTS relation_snapshot_manifest (
    project_id BIGINT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    relation_epoch BIGINT NOT NULL,
    operation_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('building', 'ready', 'active', 'failed', 'delta')),
    schema_version BIGINT NOT NULL,
    parser_version BIGINT NOT NULL,
    resolver_version BIGINT NOT NULL,
    path_normalization_version BIGINT NOT NULL,
    config_fingerprint TEXT NOT NULL,
    input_fingerprint TEXT,
    snapshot_fingerprint TEXT,
    file_count BIGINT,
    entity_count BIGINT,
    relation_count BIGINT,
    dependency_count BIGINT,
    created_at BIGINT NOT NULL,
    validated_at BIGINT,
    activated_at BIGINT,
    failure_reason TEXT,
    symbol_key_conflict_count BIGINT NOT NULL DEFAULT 0,
    symbol_key_conflict_samples_json TEXT,
    PRIMARY KEY (project_id, relation_epoch)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_relation_manifest_operation
    ON relation_snapshot_manifest (project_id, operation_id);
CREATE TABLE IF NOT EXISTS relation_snapshot_files (
    id BIGSERIAL PRIMARY KEY,
    project_id BIGINT NOT NULL,
    relation_epoch BIGINT NOT NULL,
    path TEXT NOT NULL,
    language TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    file_size BIGINT NOT NULL,
    imports_json TEXT NOT NULL,
    UNIQUE (project_id, relation_epoch, path),
    FOREIGN KEY (project_id, relation_epoch)
        REFERENCES relation_snapshot_manifest (project_id, relation_epoch) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS relation_snapshot_entities (
    id BIGSERIAL PRIMARY KEY,
    project_id BIGINT NOT NULL,
    relation_epoch BIGINT NOT NULL,
    file_id BIGINT NOT NULL REFERENCES relation_snapshot_files (id) ON DELETE CASCADE,
    scoped_name TEXT NOT NULL,
    kind_json TEXT NOT NULL,
    overload_discriminator TEXT NOT NULL,
    entity_id BIGINT,
    name TEXT NOT NULL,
    signature TEXT NOT NULL,
    parameters_json TEXT NOT NULL,
    return_type TEXT,
    span_json TEXT NOT NULL,
    depth BIGINT NOT NULL,
    parent_symbol_id BIGINT REFERENCES relation_snapshot_entities (id),
    doc_comment TEXT,
    modifiers_json TEXT NOT NULL,
    attributes_json TEXT NOT NULL,
    metadata_json TEXT NOT NULL,
    is_stdlib BIGINT NOT NULL,
    stdlib_category_json TEXT,
    subtype TEXT,
    UNIQUE (project_id, relation_epoch, file_id, scoped_name, kind_json, overload_discriminator),
    FOREIGN KEY (project_id, relation_epoch)
        REFERENCES relation_snapshot_manifest (project_id, relation_epoch) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_relation_snapshot_entities_scoped_name
    ON relation_snapshot_entities (project_id, relation_epoch, scoped_name);
CREATE INDEX IF NOT EXISTS idx_relation_snapshot_entities_file
    ON relation_snapshot_entities (project_id, relation_epoch, file_id);
CREATE TABLE IF NOT EXISTS relation_snapshot_relations (
    id BIGSERIAL PRIMARY KEY,
    project_id BIGINT NOT NULL,
    relation_epoch BIGINT NOT NULL,
    caller_symbol_id BIGINT NOT NULL REFERENCES relation_snapshot_entities (id),
    target_symbol_id BIGINT REFERENCES relation_snapshot_entities (id),
    target_state TEXT NOT NULL CHECK (target_state IN ('internal', 'external', 'unresolved')),
    raw_target TEXT NOT NULL,
    relation_type_json TEXT NOT NULL,
    span_json TEXT NOT NULL,
    external_type_json TEXT,
    unresolved_reason TEXT,
    stdlib_category_json TEXT,
    call_context_json TEXT,
    owner_type TEXT,
    overload_signature TEXT,
    callee_symbol_json TEXT,
    call_frequency BIGINT NOT NULL DEFAULT 1,
    cfg_condition TEXT,
    FOREIGN KEY (project_id, relation_epoch)
        REFERENCES relation_snapshot_manifest (project_id, relation_epoch) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_relation_snapshot_relations_caller
    ON relation_snapshot_relations (project_id, relation_epoch, caller_symbol_id);
CREATE INDEX IF NOT EXISTS idx_relation_snapshot_relations_target
    ON relation_snapshot_relations (project_id, relation_epoch, target_symbol_id);
CREATE TABLE IF NOT EXISTS relation_snapshot_exports (
    project_id BIGINT NOT NULL,
    relation_epoch BIGINT NOT NULL,
    file_id BIGINT NOT NULL REFERENCES relation_snapshot_files (id) ON DELETE CASCADE,
    symbol_id BIGINT NOT NULL REFERENCES relation_snapshot_entities (id),
    export_type TEXT NOT NULL,
    PRIMARY KEY (project_id, relation_epoch, file_id, symbol_id, export_type),
    FOREIGN KEY (project_id, relation_epoch)
        REFERENCES relation_snapshot_manifest (project_id, relation_epoch) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS relation_snapshot_dependencies (
    project_id BIGINT NOT NULL,
    relation_epoch BIGINT NOT NULL,
    source_file_id BIGINT NOT NULL REFERENCES relation_snapshot_files (id) ON DELETE CASCADE,
    target_path TEXT NOT NULL,
    source TEXT NOT NULL,
    PRIMARY KEY (project_id, relation_epoch, source_file_id, target_path, source),
    FOREIGN KEY (project_id, relation_epoch)
        REFERENCES relation_snapshot_manifest (project_id, relation_epoch) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS relation_snapshot_deltas (
    project_id BIGINT NOT NULL,
    base_epoch BIGINT NOT NULL,
    delta_epoch BIGINT NOT NULL,
    delta_data BYTEA NOT NULL,
    size_bytes BIGINT NOT NULL,
    PRIMARY KEY (project_id, delta_epoch),
    FOREIGN KEY (project_id, base_epoch)
        REFERENCES relation_snapshot_manifest (project_id, relation_epoch) ON DELETE CASCADE,
    FOREIGN KEY (project_id, delta_epoch)
        REFERENCES relation_snapshot_manifest (project_id, relation_epoch) ON DELETE CASCADE
);
"#;
