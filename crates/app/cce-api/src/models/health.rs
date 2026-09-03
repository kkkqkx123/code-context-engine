//! Health check models

use serde::{Deserialize, Serialize};

/// Health status response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Overall health
    pub healthy: bool,
    /// Qdrant status
    pub qdrant: ServiceStatus,
    /// BM25 status
    pub bm25: ServiceStatus,
    /// Embedding status
    pub embedding: ServiceStatus,
}

/// Service status
#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceStatus {
    /// Whether the service is reachable
    pub reachable: bool,
    /// Human-readable message
    pub message: String,
}

/// Qdrant health response
#[derive(Debug, Serialize, Deserialize)]
pub struct QdrantHealthResponse {
    pub healthy: bool,
    pub circuit_breaker: String,
    pub diagnostic: QdrantDiagnostic,
}

/// Qdrant diagnostic
#[derive(Debug, Serialize, Deserialize)]
pub struct QdrantDiagnostic {
    #[serde(default)]
    pub reachable: bool,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub collection_exists: bool,
    #[serde(default)]
    pub points_count: u64,
    #[serde(default)]
    pub error: Option<String>,
}

/// Embedding health response
#[derive(Debug, Serialize, Deserialize)]
pub struct EmbeddingHealthResponse {
    pub healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    pub message: String,
}

/// BM25 health response
#[derive(Debug, Serialize, Deserialize)]
pub struct Bm25HealthResponse {
    pub enabled: bool,
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_path: Option<String>,
}

/// Retry queue status response
#[derive(Debug, Serialize, Deserialize)]
pub struct RetryQueueStatusResponse {
    pub pending_count: usize,
    pub is_empty: bool,
}

/// Retry queue process response
#[derive(Debug, Serialize, Deserialize)]
pub struct RetryQueueProcessResponse {
    pub processed: usize,
    pub message: String,
}

/// Retry queue clear response
#[derive(Debug, Serialize, Deserialize)]
pub struct RetryQueueClearResponse {
    pub cleared: usize,
    pub message: String,
}
