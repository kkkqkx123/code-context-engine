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

/// Raw bytes per transfer chunk. Files larger than this are split into
/// fixed-size pieces with per-chunk hashes so interrupted pushes resume
/// without retransmitting received pieces.
pub const INGEST_CHUNK_BYTES: usize = 256 * 1024;

/// Largest number of chunks accepted in one ingest batch. The bound keeps a
/// single request body inside the admission body limit after base64 growth.
pub const MAX_INGEST_CHUNKS_PER_BATCH: usize = 16;

/// Number of chunks needed to transfer `size` raw bytes. Empty files still
/// use one chunk so hash verification has a stable shape.
pub fn total_chunks_for_size(size: u64) -> u32 {
    let chunks = size.div_ceil(INGEST_CHUNK_BYTES as u64);
    chunks.max(1).min(u32::MAX as u64) as u32
}

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
    /// Gateway-chosen manifest version identifying this sync pass. The
    /// server keys received-chunk bookkeeping on it so an interrupted push
    /// can resume without retransmitting stored pieces.
    #[serde(default)]
    pub manifest_version: u64,
}

/// One missing chunk the gateway must upload.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MissingChunk {
    /// Canonical project-relative path with forward slashes.
    pub relative_path: String,
    /// Zero-based chunk index within the file.
    pub chunk_index: u32,
    /// Total chunks for the file at manifest time.
    pub total_chunks: u32,
}

/// Manifest push response listing the paths the gateway must upload.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestManifestResponse {
    pub success: bool,
    pub project_id: i64,
    /// Echo of the request manifest version.
    #[serde(default)]
    pub manifest_version: u64,
    /// Relative paths whose content the server still needs.
    pub upload: Vec<String>,
    /// Chunk-granular misses for partially received files.
    #[serde(default)]
    pub missing_chunks: Vec<MissingChunk>,
    /// Files already up to date on the server.
    pub unchanged: usize,
}

/// One file chunk payload inside an ingest batch.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestedFile {
    /// Canonical project-relative path with forward slashes.
    pub relative_path: String,
    /// Expected full-content hash of the raw bytes.
    #[serde(default)]
    pub content_hash: Option<String>,
    /// Raw file bytes encoded with standard base64. When `compressed` is
    /// true the bytes hold the compressed form and the server decompresses
    /// before hash verification.
    pub content_base64: String,
    /// Zero-based chunk index within the file.
    #[serde(default)]
    pub chunk_index: u32,
    /// Total chunks for the file.
    #[serde(default = "default_total_chunks")]
    pub total_chunks: u32,
    /// Full-content chunk hash of the uncompressed bytes.
    #[serde(default)]
    pub chunk_hash: Option<String>,
    /// Whether `content_base64` carries compressed bytes. Negotiation stays
    /// off by default; the gateway enables it explicitly per batch.
    #[serde(default)]
    pub compressed: bool,
}

fn default_total_chunks() -> u32 {
    1
}

/// Batch upload request carrying file contents.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestBatchRequest {
    /// File chunks to stage on the server.
    pub files: Vec<IngestedFile>,
    /// Manifest version this batch belongs to. Zero means the legacy
    /// whole-file pass without resume bookkeeping.
    #[serde(default)]
    pub manifest_version: u64,
}

/// Batch upload response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IngestBatchResponse {
    pub success: bool,
    pub project_id: i64,
    /// Echo of the request manifest version.
    #[serde(default)]
    pub manifest_version: u64,
    /// Files fully staged under the project root.
    pub staged: usize,
    /// Chunks accepted in this batch, including partial files.
    #[serde(default)]
    pub staged_chunks: usize,
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
