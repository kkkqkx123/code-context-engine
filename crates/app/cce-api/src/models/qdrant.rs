//! Qdrant process management models
//!
//! The wire copy of the backend lifecycle state. The backend-owned domain
//! type lives in `cce-storage-vector-qdrant`; `cce-server-shared` converts
//! between them so infrastructure never depends on this contract. `utoipa`
//! derives stay here because this crate is the single schema source for
//! the server OpenAPI document and the generated TypeScript projection.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Wire copy of the managed Qdrant subprocess lifecycle state.
///
/// Mirrors the backend domain type without pulling in storage machinery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(tag = "type", content = "message")]
pub enum QdrantProcessStatus {
    Idle,
    Starting,
    Running,
    Stopping,
    Crashed,
    Stopped,
    Failed(String),
}

impl std::fmt::Display for QdrantProcessStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QdrantProcessStatus::Idle => write!(f, "Idle"),
            QdrantProcessStatus::Starting => write!(f, "Starting"),
            QdrantProcessStatus::Running => write!(f, "Running"),
            QdrantProcessStatus::Stopping => write!(f, "Stopping"),
            QdrantProcessStatus::Crashed => write!(f, "Crashed"),
            QdrantProcessStatus::Stopped => write!(f, "Stopped"),
            QdrantProcessStatus::Failed(message) => write!(f, "Failed: {message}"),
        }
    }
}

/// Qdrant process status response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QdrantProcessStatusResponse {
    pub managed: bool,
    pub status: QdrantProcessStatus,
}

/// Qdrant process action response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct QdrantActionResponse {
    pub success: bool,
    pub message: String,
    pub status: QdrantProcessStatus,
}
