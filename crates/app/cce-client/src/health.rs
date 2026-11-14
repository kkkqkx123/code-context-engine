//! Service health, retry-queue state and admission counters.

use cce_api::models::{
    AdmissionStatsResponse, Bm25HealthResponse, EmbeddingHealthResponse, HealthStatus,
    QdrantHealthResponse, RetryQueueClearResponse, RetryQueueDeadClearResponse,
    RetryQueueDeadResponse, RetryQueueProcessResponse, RetryQueueQuery, RetryQueueStatusResponse,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// Unified health of every backing service.
    pub async fn health(&self) -> Result<HealthStatus, ClientError> {
        self.get_json(paths::HEALTH.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Vector backend diagnostics.
    pub async fn health_qdrant(&self) -> Result<QdrantHealthResponse, ClientError> {
        self.get_json(paths::HEALTH_QDRANT.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Embedding provider diagnostics.
    pub async fn health_embedding(&self) -> Result<EmbeddingHealthResponse, ClientError> {
        self.get_json(paths::HEALTH_EMBEDDING.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Full-text index diagnostics.
    pub async fn health_bm25(&self) -> Result<Bm25HealthResponse, ClientError> {
        self.get_json(paths::HEALTH_BM25.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Pending count of the retry queue.
    pub async fn retry_queue_status(
        &self,
        query: &RetryQueueQuery,
    ) -> Result<RetryQueueStatusResponse, ClientError> {
        self.get_json_query(paths::RETRY_QUEUE.to_string(), query, INTERACTIVE_BUDGET)
            .await
    }

    /// Trigger one retry-queue processing pass.
    pub async fn retry_queue_process(
        &self,
        query: &RetryQueueQuery,
    ) -> Result<RetryQueueProcessResponse, ClientError> {
        self.post_json_with_query(
            paths::RETRY_QUEUE_PROCESS.to_string(),
            query,
            self.long_task_budget(),
        )
        .await
    }

    /// Discard all queued retries.
    pub async fn retry_queue_clear(
        &self,
        query: &RetryQueueQuery,
    ) -> Result<RetryQueueClearResponse, ClientError> {
        self.delete_json_with_query(paths::RETRY_QUEUE.to_string(), query, INTERACTIVE_BUDGET)
            .await
    }

    /// List entries that exhausted their retries.
    pub async fn retry_queue_dead(
        &self,
        query: &RetryQueueQuery,
    ) -> Result<RetryQueueDeadResponse, ClientError> {
        self.get_json_query(
            paths::RETRY_QUEUE_DEAD.to_string(),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Discard dead retry-queue entries.
    pub async fn retry_queue_dead_clear(
        &self,
        query: &RetryQueueQuery,
    ) -> Result<RetryQueueDeadClearResponse, ClientError> {
        self.delete_json_with_query(
            paths::RETRY_QUEUE_DEAD.to_string(),
            query,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Admission counters and per-token audit.
    pub async fn admission_stats(&self) -> Result<AdmissionStatsResponse, ClientError> {
        self.get_json(paths::ADMISSION_STATS.to_string(), INTERACTIVE_BUDGET)
            .await
    }
}
