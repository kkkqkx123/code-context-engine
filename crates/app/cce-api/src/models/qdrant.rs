//! Qdrant process management models

use serde::{Deserialize, Serialize};

/// Qdrant process status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

/// Qdrant process status response
#[derive(Debug, Serialize, Deserialize)]
pub struct QdrantProcessStatusResponse {
    pub managed: bool,
    pub status: QdrantProcessStatus,
}

/// Qdrant process action response
#[derive(Debug, Serialize, Deserialize)]
pub struct QdrantActionResponse {
    pub success: bool,
    pub message: String,
    pub status: QdrantProcessStatus,
}
