//! Metrics trait for BM25 storage operations

/// Trait for recording BM25 storage metrics.
///
/// This trait decouples the BM25 storage client from any specific metrics
/// implementation, allowing consumers to provide their own metrics backend.
pub trait Bm25Metrics: Send + Sync {
    /// Record a completed document indexing operation
    fn record_index(&self, latency_ms: f64, document_count: usize, success: bool);

    /// Record a completed deletion operation
    fn record_delete(&self, latency_ms: f64, document_count: usize, success: bool);

    /// Record current BM25 index disk usage in bytes
    fn record_disk_usage(&self, bytes: u64);
}
