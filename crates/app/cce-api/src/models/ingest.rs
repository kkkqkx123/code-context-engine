//! Remote ingest models for gateway-driven indexing.
//!
//! The gateway pushes file manifests and raw content payloads; the server
//! stages them under the project root and runs the existing index pipelines.
//! Content travels base64 encoded so JSON transport preserves raw bytes and
//! encoding detection still runs on the remote side.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Largest single file the ingest endpoints accept, in bytes.
///
/// The gateway filters with the same bound before pushing, so rejected files
/// never consume transfer bandwidth.
pub const MAX_INGEST_FILE_BYTES: u64 = 1_048_576;

/// Largest number of files accepted in one ingest batch.
pub const MAX_INGEST_BATCH_FILES: usize = 200;

/// One manifest entry describing a gateway-side file fingerprint.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestFileMeta {
    /// Canonical project-relative path with forward slashes.
    pub relative_path: String,
    /// File size in bytes.
    pub size: u64,
    /// Modification time as seconds since the unix epoch.
    pub modified_secs: i64,
    /// Full-content hash of the raw bytes, when known.
    #[serde(default)]
    pub content_hash: Option<String>,
}

/// Manifest push request for initial sync comparison.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestManifestRequest {
    /// Files currently visible to the gateway.
    pub files: Vec<IngestFileMeta>,
}

/// Manifest push response listing the paths the gateway must upload.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestManifestResponse {
    pub success: bool,
    pub project_id: i64,
    /// Relative paths whose content the server still needs.
    pub upload: Vec<String>,
    /// Files already up to date on the server.
    pub unchanged: usize,
}

/// One file payload inside an ingest batch.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestedFile {
    /// Canonical project-relative path with forward slashes.
    pub relative_path: String,
    /// Expected full-content hash of the raw bytes.
    #[serde(default)]
    pub content_hash: Option<String>,
    /// Raw file bytes encoded with standard base64.
    pub content_base64: String,
}

/// Batch upload request carrying file contents.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestBatchRequest {
    /// Files to stage on the server.
    pub files: Vec<IngestedFile>,
}

/// Batch upload response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestBatchResponse {
    pub success: bool,
    pub project_id: i64,
    /// Files staged under the project root.
    pub staged: usize,
    /// Files skipped with per-file reasons.
    #[serde(default)]
    pub errors: Vec<String>,
}

/// Commit response after running the full index over staged files.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestCommitResponse {
    pub success: bool,
    pub project_id: i64,
    pub project_name: String,
    pub indexed_files: usize,
    pub total_entities: usize,
    pub total_vectors: usize,
    pub elapsed_ms: u64,
}

/// Incremental change kind inside an ingest event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum IngestEventKind {
    /// New file with content attached.
    Created,
    /// Changed file with content attached.
    Modified,
    /// Removed file without content.
    Deleted,
}

/// One gateway-observed file change.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestEvent {
    /// Canonical project-relative path with forward slashes.
    pub relative_path: String,
    /// Change kind.
    pub kind: IngestEventKind,
    /// Expected full-content hash, required for created and modified.
    #[serde(default)]
    pub content_hash: Option<String>,
    /// Raw file bytes encoded with standard base64, absent for deletions.
    #[serde(default)]
    pub content_base64: Option<String>,
}

/// Incremental push request driven by gateway change observation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestEventRequest {
    /// Changes since the last push.
    pub events: Vec<IngestEvent>,
}

/// Incremental push response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestEventResponse {
    pub success: bool,
    pub project_id: i64,
    /// Files applied through the hot-update path.
    pub applied: usize,
    /// Files removed from the index.
    pub removed: usize,
    #[serde(default)]
    pub errors: Vec<String>,
}
