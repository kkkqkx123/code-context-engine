//! Durable file-level index-state projection.
//!
//! The projection mirrors in-memory per-file update states so a restart can
//! restore progress without rescanning. Writes carry a monotonic version:
//! an older version never overwrites a newer one, and deletes are scoped
//! to a single operation.

use serde::{Deserialize, Serialize};

/// One durable file-state row of an indexing operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexStateProjection {
    pub project_id: i64,
    pub operation_id: String,
    pub file_path: String,
    pub version: i64,
    pub state_json: String,
    pub updated_at: String,
}
