//! Qdrant subprocess status shared between the Qdrant backend and the API wire models.
//!
//! Lives here (backend-agnostic contract crate) so the Qdrant backend can own
//! process management without depending on the application layer, while the
//! API contract crate reuses the exact same enum for its wire DTOs.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Qdrant process status
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
            QdrantProcessStatus::Failed(message) => write!(f, "Failed: {}", message),
        }
    }
}
