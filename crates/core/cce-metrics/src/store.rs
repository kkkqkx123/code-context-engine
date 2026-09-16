//! Trait abstracting SQLite persistence operations needed by the metrics aggregator.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Trait abstracting SQLite persistence operations needed by the aggregator.
pub trait SqliteStore: Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;

    fn execute_write(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<usize, Self::Error>;
    fn query_rows(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
        f: &mut dyn FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<AggregatedMetric>,
    ) -> Result<Vec<AggregatedMetric>, Self::Error>;

    /// Insert a batch of rows, one parameter set per row.
    ///
    /// Implementations should wrap the batch in a single transaction when
    /// possible. The default implementation falls back to repeated
    /// `execute_write` calls so in-memory fakes keep working.
    fn execute_write_batch(
        &self,
        sql: &str,
        batch: &[Vec<Box<dyn rusqlite::ToSql>>],
    ) -> Result<usize, Self::Error> {
        let mut inserted = 0;
        for params in batch {
            let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            if self.execute_write(sql, &refs).is_ok() {
                inserted += 1;
            }
        }
        Ok(inserted)
    }
}

/// Aggregated metric record for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedMetric {
    pub timestamp: DateTime<Utc>,
    pub metric_name: String,
    pub metric_type: String,
    pub labels_json: Option<String>,
    pub count: i64,
    pub avg: Option<f64>,
    pub median: Option<f64>,
    pub max: Option<f64>,
    pub p90: Option<f64>,
    pub p99: Option<f64>,
    pub project_id: Option<i64>,
    pub operation_type: Option<String>,
}
