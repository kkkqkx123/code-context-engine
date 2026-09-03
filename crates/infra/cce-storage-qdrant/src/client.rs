//! Qdrant HTTP client implementation
//!
//! This module provides the main client for interacting with Qdrant vector database
//! via HTTP REST API. The client acts as a facade that coordinates various operations.

use cce_circuit_breaker::CircuitBreaker;
use reqwest::Client;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::metrics::QdrantMetrics;
use crate::{
    config::{HnswConfig, QdrantConfig, QuantizationConfig, VectorStorageConfig, WalConfig},
    error::QdrantError,
    operations::{CollectionOperations, PointOperations, SearchOperations},
    types::{SearchQuery, SearchResult, SizeEstimation, VectorPoint},
};
use cce_config::validation::Validate;
use cce_types::PointKind;
use cce_utils::hash::calculate_hash;

/// Qdrant diagnostic information
#[derive(Debug, Clone, serde::Serialize)]
pub struct QdrantDiagnostic {
    /// Whether the Qdrant server is reachable
    pub reachable: bool,
    /// Qdrant server version (if available)
    pub version: Option<String>,
    /// Whether the target collection exists
    pub collection_exists: bool,
    /// Number of points in the collection
    pub points_count: u64,
    /// Error message if something went wrong
    pub error: Option<String>,
}

/// Generate a deterministic group ID from a workspace path.
pub fn generate_group_id(workspace_path: &str) -> String {
    let hash = calculate_hash(workspace_path.as_bytes());
    format!("proj_{}", &hash[..12])
}

/// Generate a stable Qdrant namespace for one logical project.
///
/// Including the database project ID prevents two projects that share the same
/// workspace path from writing into the same vector partition.
pub fn generate_project_group_id(project_id: i64, workspace_path: &str) -> String {
    format!("project-{project_id}-{}", generate_group_id(workspace_path))
}

/// Qdrant vector storage client
pub struct QdrantClient {
    config: QdrantConfig,
    http_client: Client,
    collection_name: String,
    base_url: String,

    // Operation handlers wrapped in Arc for efficient cloning
    collection_ops: Arc<CollectionOperations>,
    point_ops: Arc<PointOperations>,
    search_ops: Arc<SearchOperations>,

    // Circuit breaker for transient failures
    circuit_breaker: Arc<Mutex<CircuitBreaker>>,

    // Metrics collector
    metrics: Option<Arc<QdrantMetrics>>,
}

impl QdrantClient {
    /// Create a new Qdrant client
    pub fn new(config: QdrantConfig, workspace_path: &str) -> Result<Self, QdrantError> {
        debug!("Creating Qdrant client");

        // Validate config
        config.validate_structured().map_err(|e| {
            error!(error = %e, "Config validation failed");
            QdrantError::config(e.to_string())
        })?;

        // Create HTTP client
        let mut builder = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .user_agent("CodeContextEngine")
            .pool_max_idle_per_host(10)
            .tcp_keepalive(Duration::from_secs(60));

        // Add API key if provided
        if let Some(ref api_key) = config.api_key {
            builder = builder.default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    "api-key",
                    reqwest::header::HeaderValue::from_str(api_key).map_err(|e| {
                        error!(error = %e, "Invalid API key format");
                        QdrantError::config(format!("Invalid API key: {}", e))
                    })?,
                );
                headers
            });
        }

        let http_client = builder.build().map_err(|e| {
            error!(error = %e, "Failed to build HTTP client");
            QdrantError::connection(e.to_string())
        })?;

        // The storage layer now uses a fixed collection name and logical
        // isolation via payload fields.
        let collection_name = Self::generate_collection_name(workspace_path);

        // Normalize URL
        let base_url = config.normalized_url();

        info!(
            collection_name = %collection_name,
            base_url = %base_url,
            "Qdrant client initialized"
        );

        // Create operation handlers
        let collection_ops = Arc::new(CollectionOperations::new(
            config.clone(),
            http_client.clone(),
            collection_name.clone(),
            base_url.clone(),
        ));

        let point_ops = Arc::new(PointOperations::new(
            http_client.clone(),
            collection_name.clone(),
            base_url.clone(),
        ));

        let search_ops = Arc::new(SearchOperations::new(
            http_client.clone(),
            collection_name.clone(),
            base_url.clone(),
        ));

        Ok(Self {
            config,
            http_client,
            collection_name,
            base_url,
            collection_ops,
            point_ops,
            search_ops,
            circuit_breaker: Arc::new(Mutex::new(CircuitBreaker::new(
                3,                       // failure threshold
                Duration::from_secs(30), // reset timeout
            ))),
            metrics: None,
        })
    }

    /// Attach metrics collector to the client
    pub fn with_metrics(mut self, metrics: Arc<QdrantMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Get a reference to the metrics collector
    pub fn metrics(&self) -> Option<&Arc<QdrantMetrics>> {
        self.metrics.as_ref()
    }

    /// Check circuit breaker before executing an operation.
    async fn check_circuit_breaker(&self) -> Result<(), QdrantError> {
        let breaker = self.circuit_breaker.lock().await;
        let state = breaker.state().to_string();
        if breaker.is_open() {
            if let Some(metrics) = &self.metrics {
                metrics.record_circuit_breaker_state(&state);
            }
            return Err(QdrantError::CircuitBreakerOpen(
                "Circuit breaker is open, rejecting request".into(),
            ));
        }
        if let Some(metrics) = &self.metrics {
            metrics.record_circuit_breaker_state(&state);
        }
        Ok(())
    }

    /// Record operation outcome for circuit breaker tracking.
    async fn record_circuit_outcome<T>(&self, result: &Result<T, QdrantError>) {
        let mut breaker = self.circuit_breaker.lock().await;
        match result {
            Ok(_) => breaker.record_success(),
            Err(e) if e.is_retryable() => {
                tracing::warn!(error = %e, "Circuit breaker recording failure");
                breaker.record_failure();
            }
            Err(_) => {
                // Non-retryable errors don't affect circuit breaker state
            }
        }
        if let Some(metrics) = &self.metrics {
            metrics.record_circuit_breaker_state(breaker.state());
        }
    }

    /// Create a new Qdrant client with default config
    pub fn with_default_config(workspace_path: &str) -> Result<Self, QdrantError> {
        Self::new(QdrantConfig::default(), workspace_path)
    }

    /// Generate collection name from workspace path.
    fn generate_collection_name(workspace_path: &str) -> String {
        let _ = workspace_path;
        format!("cce_vectors-i{}", cce_types::INDEX_FORMAT_VERSION)
    }

    /// Get the collection name
    pub fn collection_name(&self) -> &str {
        &self.collection_name
    }

    /// Get the config
    pub fn config(&self) -> &QdrantConfig {
        &self.config
    }

    /// Check if the client is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Quick health check - returns true if Qdrant is reachable
    pub async fn health(&self) -> Result<bool, QdrantError> {
        let start = Instant::now();
        let url = format!("{}/healthz", self.base_url);
        let result = match self.http_client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(e) => {
                if e.is_timeout() {
                    Err(QdrantError::ConnectionTimeout(e.to_string()))
                } else if e.is_connect() {
                    Err(QdrantError::ConnectionRefused {
                        url: self.base_url.clone(),
                        message: e.to_string(),
                    })
                } else {
                    Err(QdrantError::Request(e.to_string()))
                }
            }
        };
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_search(latency_ms, result.is_ok());
        }
        result
    }

    /// Get current circuit breaker state
    pub fn circuit_breaker_state(&self) -> String {
        self.circuit_breaker
            .try_lock()
            .map(|breaker| breaker.state().to_string())
            .unwrap_or_else(|_| "locked".to_string())
    }

    /// Get Qdrant version information
    pub async fn version(&self) -> Result<Option<String>, QdrantError> {
        let start = Instant::now();
        let url = format!("{}/telemetry", self.base_url);

        let response = match self.http_client.get(&url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                let err = if e.is_timeout() {
                    QdrantError::ConnectionTimeout(e.to_string())
                } else {
                    QdrantError::Request(format!("Failed to get version: {}", e))
                };
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                if let Some(metrics) = &self.metrics {
                    metrics.record_search(latency_ms, false);
                }
                return Err(err);
            }
        };

        if !response.status().is_success() {
            let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
            if let Some(metrics) = &self.metrics {
                metrics.record_search(latency_ms, true);
            }
            return Ok(None);
        }

        let json: serde_json::Value = match response.json().await {
            Ok(j) => j,
            Err(e) => {
                let err = QdrantError::ResponseParse(format!("Failed to parse telemetry: {}", e));
                let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
                if let Some(metrics) = &self.metrics {
                    metrics.record_search(latency_ms, false);
                }
                return Err(err);
            }
        };

        let version = json
            .get("result")
            .and_then(|r| r.get("version"))
            .or_else(|| json.get("version"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_search(latency_ms, true);
        }
        Ok(version)
    }

    /// Run a comprehensive diagnostic check
    pub async fn diagnose(&self) -> Result<QdrantDiagnostic, QdrantError> {
        let start = Instant::now();
        let mut diag = QdrantDiagnostic {
            reachable: false,
            version: None,
            collection_exists: false,
            points_count: 0,
            error: None,
        };

        // Check health
        match self.health().await {
            Ok(true) => {
                diag.reachable = true;
            }
            Ok(false) => {
                diag.error = Some("Qdrant health check returned non-success status".to_string());
            }
            Err(e) => {
                diag.error = Some(format!("Health check failed: {}", e));
            }
        }

        // Only continue if reachable
        if diag.reachable {
            // Get version
            diag.version = self.version().await.unwrap_or(None);

            // Get collection info
            match self.get_collection_info().await {
                Ok(info) => {
                    diag.collection_exists = true;
                    diag.points_count = info.points_count;
                }
                Err(QdrantError::CollectionNotFound(_)) => {
                    diag.collection_exists = false;
                }
                Err(e) => {
                    diag.error = Some(format!("Collection check failed: {}", e));
                }
            }
        }

        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_search(latency_ms, true);
        }
        Ok(diag)
    }

    /// Get the HTTP client for retrieval operations
    pub fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }

    /// Get the base URL for retrieval operations
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Initialize the collection
    pub async fn initialize(&self) -> Result<bool, QdrantError> {
        debug!(collection_name = %self.collection_name, "Starting collection initialization");
        self.initialize_with_config(
            self.config.get_hnsw_config(),
            self.config.quantization.clone(),
            Some(self.config.get_wal_config()),
            self.config.vector_storage.clone(),
        )
        .await
    }

    /// Initialize the collection with explicit resolved configuration values
    pub async fn initialize_with_config(
        &self,
        hnsw: Option<HnswConfig>,
        quantization: Option<QuantizationConfig>,
        wal: Option<WalConfig>,
        vector_storage: Option<VectorStorageConfig>,
    ) -> Result<bool, QdrantError> {
        debug!(collection = %self.collection_name, "Initializing collection");
        let start = Instant::now();

        // Check if collection exists first
        let result = match self.collection_ops.get_info().await {
            Ok(_) => {
                info!(
                    collection = %self.collection_name,
                    latency_ms = start.elapsed().as_millis(),
                    "Collection already exists"
                );
                Ok(false)
            }
            Err(QdrantError::CollectionNotFound(_)) => {
                let create_result = self
                    .collection_ops
                    .create_with_config(hnsw, quantization, wal, vector_storage)
                    .await;

                match create_result {
                    Ok(_) => {
                        info!(
                            collection = %self.collection_name,
                            latency_ms = start.elapsed().as_millis(),
                            "Collection created"
                        );
                        Ok(true)
                    }
                    Err(e) => {
                        error!(
                            collection = %self.collection_name,
                            error = %e,
                            latency_ms = start.elapsed().as_millis(),
                            "Collection creation failed"
                        );
                        Err(e)
                    }
                }
            }
            Err(e) => {
                error!(
                    collection = %self.collection_name,
                    error = %e,
                    latency_ms = start.elapsed().as_millis(),
                    "Failed to check collection existence"
                );
                Err(e)
            }
        };

        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_search(latency_ms, result.is_ok());
        }

        result
    }

    /// Get collection information
    pub async fn get_collection_info(&self) -> Result<crate::types::CollectionInfo, QdrantError> {
        self.collection_ops.get_info().await
    }

    /// Check if collection exists
    pub async fn collection_exists(&self) -> Result<bool, QdrantError> {
        self.collection_ops.exists().await
    }

    /// Delete the collection
    pub async fn delete_collection(&self) -> Result<(), QdrantError> {
        self.collection_ops.delete().await
    }

    /// Clear all points from the collection
    pub async fn clear_collection(&self) -> Result<(), QdrantError> {
        debug!(collection = %self.collection_name, "Clearing collection");

        let start = Instant::now();
        let result = self.collection_ops.clear().await;
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        if let Some(metrics) = &self.metrics {
            metrics.record_delete(latency_ms, 0, result.is_ok());
        }

        match &result {
            Ok(_) => {
                info!(
                    collection = %self.collection_name,
                    latency_ms = latency_ms,
                    "Collection cleared successfully"
                );
            }
            Err(e) => {
                error!(
                    collection = %self.collection_name,
                    error = %e,
                    latency_ms = latency_ms,
                    "Collection clear failed"
                );
            }
        }

        result
    }

    /// Upsert vector points
    pub async fn upsert_points(&self, points: &[VectorPoint]) -> Result<(), QdrantError> {
        let point_count = points.len();

        let start = Instant::now();
        let result = self.point_ops.upsert(points).await;
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Record metrics
        if let Some(metrics) = &self.metrics {
            metrics.record_upsert(latency_ms, point_count, result.is_ok());
        }

        match &result {
            Ok(_) => {}
            Err(e) => {
                error!(
                    collection = %self.collection_name,
                    error = %e,
                    "Points upsert failed"
                );
            }
        }

        result
    }

    /// Delete points by file path within a specific group
    pub async fn delete_by_file_path_scoped(
        &self,
        file_path: &str,
        group_id: &str,
        point_type: Option<PointKind>,
    ) -> Result<(), QdrantError> {
        let start = Instant::now();
        let result = self
            .point_ops
            .delete_by_file_path_scoped(file_path, Some(group_id), point_type)
            .await;
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        if let Some(metrics) = &self.metrics {
            metrics.record_delete(latency_ms, 1, result.is_ok());
        }

        result
    }

    /// Delete points for one file in one data epoch.
    pub async fn delete_by_file_path_scoped_epoch(
        &self,
        file_path: &str,
        group_id: &str,
        epoch: i64,
    ) -> Result<(), QdrantError> {
        let start = Instant::now();
        let result = self
            .point_ops
            .delete_by_file_path_scoped_epoch(file_path, group_id, epoch)
            .await;
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_delete(latency_ms, 1, result.is_ok());
        }
        result
    }

    /// Delete all points for one project namespace and data epoch.
    pub async fn delete_by_group_epoch(
        &self,
        group_id: &str,
        epoch: i64,
    ) -> Result<(), QdrantError> {
        let start = Instant::now();
        let result = self.point_ops.delete_by_group_epoch(group_id, epoch).await;
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(metrics) = &self.metrics {
            metrics.record_delete(latency_ms, 1, result.is_ok());
        }
        result
    }

    /// Scroll all points from the collection
    pub async fn scroll_all_points(&self) -> Result<Vec<VectorPoint>, QdrantError> {
        self.point_ops.scroll_all().await
    }

    /// Delete all points for a given group (project namespace)
    pub async fn delete_by_group(&self, group_id: &str) -> Result<(), QdrantError> {
        debug!(
            collection = %self.collection_name,
            group_id = %group_id,
            "Deleting all points for group"
        );

        let start = Instant::now();
        let result = self.point_ops.delete_by_group(group_id).await;
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        if let Some(metrics) = &self.metrics {
            metrics.record_delete(latency_ms, 0, result.is_ok());
        }

        match &result {
            Ok(_) => info!(group_id = %group_id, "All points for group deleted"),
            Err(e) => warn!(group_id = %group_id, error = %e, "Group deletion failed"),
        }

        result
    }

    /// Count points belonging to a group (project namespace)
    pub async fn count_points_by_group(&self, group_id: &str) -> Result<usize, QdrantError> {
        self.point_ops.count_by_group(group_id).await
    }

    /// Count all points in the Qdrant collection
    pub async fn count_all_points(&self) -> Result<usize, QdrantError> {
        self.point_ops.count_all_points().await
    }

    /// Start a background task that periodically samples Qdrant collection size.
    pub fn start_collection_sampling(
        &self,
        interval_secs: u64,
    ) -> Option<tokio::task::JoinHandle<()>> {
        if !self.config.enabled {
            return None;
        }
        let metrics = self.metrics.clone()?;
        let client = Arc::new(self.clone());
        Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                if let Ok(count) = client.count_all_points().await {
                    metrics.record_collection_size(count as u64);
                }
            }
        }))
    }

    /// Delete points by multiple file paths within a specific group
    pub async fn delete_by_file_paths_scoped(
        &self,
        file_paths: &[&str],
        group_id: &str,
        point_type: Option<PointKind>,
    ) -> Result<(), QdrantError> {
        let start = Instant::now();
        let result = self
            .point_ops
            .delete_by_file_paths_scoped(file_paths, Some(group_id), point_type)
            .await;
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        if let Some(metrics) = &self.metrics {
            metrics.record_delete(latency_ms, file_paths.len(), result.is_ok());
        }

        result
    }

    /// Search for similar vectors
    pub async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>, QdrantError> {
        self.check_circuit_breaker().await?;

        let start = Instant::now();
        let result = self.search_ops.search(query).await;
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        self.record_circuit_outcome(&result).await;

        // Record metrics
        if let Some(metrics) = &self.metrics {
            metrics.record_search(latency_ms, result.is_ok());
        }

        match &result {
            Ok(_results) => {}
            Err(e) => {
                error!(
                    collection = %self.collection_name,
                    error = %e,
                    "Search failed"
                );
            }
        }

        result
    }

    /// Estimate collection size from file count
    pub fn estimate_size(&self, file_count: usize, avg_vectors_per_file: f32) -> SizeEstimation {
        SizeEstimation::from_file_count(file_count, avg_vectors_per_file)
    }

    /// Get recommended preset for estimated size
    pub fn get_recommended_preset(&self, vector_count: usize) -> crate::config::CollectionPreset {
        crate::config::CollectionPreset::from_vector_count(vector_count)
    }
}

impl Clone for QdrantClient {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_client: self.http_client.clone(),
            collection_name: self.collection_name.clone(),
            base_url: self.base_url.clone(),
            collection_ops: self.collection_ops.clone(),
            point_ops: self.point_ops.clone(),
            search_ops: self.search_ops.clone(),
            circuit_breaker: self.circuit_breaker.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let config = QdrantConfig::default();
        let client = QdrantClient::new(config, "/test/workspace").expect("Failed to create client");
        assert!(client.is_enabled());
        assert!(client.collection_name().starts_with("cce_"));
    }

    #[test]
    fn test_collection_name_format() {
        let client = QdrantClient::with_default_config("/home/user/myproject")
            .expect("Failed to create client");
        let name = client.collection_name();
        assert_eq!(
            name,
            format!("cce_vectors-i{}", cce_types::INDEX_FORMAT_VERSION)
        );
    }

    #[test]
    fn test_collection_name_special_chars() {
        let client = QdrantClient::with_default_config("/path/My Project@v1.0/src")
            .expect("Failed to create client");
        let name = client.collection_name();
        assert_eq!(
            name,
            format!("cce_vectors-i{}", cce_types::INDEX_FORMAT_VERSION)
        );
    }

    #[test]
    fn test_collection_name_generation() {
        let client1 =
            QdrantClient::with_default_config("/workspace1").expect("Failed to create client");
        let client2 =
            QdrantClient::with_default_config("/workspace2").expect("Failed to create client");

        assert_eq!(client1.collection_name(), client2.collection_name());
    }

    #[test]
    fn test_same_workspace_same_collection() {
        let client1 =
            QdrantClient::with_default_config("/same/workspace").expect("Failed to create client");
        let client2 =
            QdrantClient::with_default_config("/same/workspace").expect("Failed to create client");

        assert_eq!(client1.collection_name(), client2.collection_name());
    }

    #[test]
    fn test_disabled_client() {
        let config = QdrantConfig::default().disabled();
        let client = QdrantClient::new(config, "/test").expect("Failed to create client");
        assert!(!client.is_enabled());
    }

    #[test]
    fn test_circuit_breaker_initial_state() {
        let client = QdrantClient::with_default_config("/test").expect("Failed to create client");
        let state = client.circuit_breaker_state();
        assert_eq!(
            state, "closed",
            "New client should have closed circuit breaker"
        );
    }

    #[test]
    fn test_get_recommended_preset() {
        let client = QdrantClient::with_default_config("/test").expect("Failed to create client");
        let preset = client.get_recommended_preset(100);
        let preset_str = format!("{:?}", preset);
        assert!(!preset_str.is_empty(), "Preset should have a valid name");
    }

    #[test]
    fn test_size_estimation() {
        let client = QdrantClient::with_default_config("/test").expect("Failed to create client");
        let estimation = client.estimate_size(100, 10.0);
        assert_eq!(estimation.estimated_vector_count, 1000);
    }
}
