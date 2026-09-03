//! Preset histogram bucket configurations for common scenarios.

/// Latency buckets for HTTP requests and storage operations (in milliseconds).
pub const LATENCY_BUCKETS: &[f64] = &[1.0, 5.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0];

/// Latency buckets for embedding API calls (in milliseconds).
pub const EMBEDDING_BUCKETS: &[f64] = &[
    50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0, 30000.0, 60000.0,
];

/// Buckets for batch size distributions (number of items).
pub const THROUGHPUT_BUCKETS: &[f64] = &[1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0];
