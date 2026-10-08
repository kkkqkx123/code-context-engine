//! SQLite-specific metrics (stub).
//!
//! This module provides a lightweight metrics wrapper for SQLite operations.
//! When wired to a `MetricsRegistry`, it records transaction counts and latency.

use std::sync::Arc;

/// SQLite metadata storage monitoring metrics.
///
/// Tracks performance and capacity metrics for SQLite database operations.
#[derive(Debug)]
pub struct SqliteMetrics;

impl SqliteMetrics {
    /// Create new SQLite metrics (no-op stub).
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }

    /// Record a completed transaction.
    pub fn record_transaction(&self, _latency_ms: f64, _is_write: bool, _success: bool) {}
}
