//! Qdrant process management models

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub use cce_storage_common::QdrantProcessStatus;

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
