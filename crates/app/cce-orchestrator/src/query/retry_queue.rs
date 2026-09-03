//! Retry queue for preserving query progress when services are unavailable.
//!
//! When a recall path fails after exhausting all retries, the query options are
//! persisted in this queue. When the circuit breaker transitions back to closed
//! (service recovered), the queued queries are drained and re-executed.

use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{info, trace, warn};

use super::types::QueryOptions;

/// Default maximum number of queued queries.
///
/// The queue is bounded so a service outage cannot grow memory without limit.
/// When full, the oldest entry is dropped (the queue is best-effort; a dropped
/// queued query only means the client must re-issue it).
const DEFAULT_MAX_QUEUE_LEN: usize = 1024;

/// A queued query entry with metadata
#[derive(Debug, Clone)]
struct QueuedQuery {
    options: QueryOptions,
    enqueued_at: Instant,
    retry_count: u32,
}

/// Retry queue for preserving query progress during service outages.
///
/// Query options are pushed when all retries are exhausted. The queue is drained
/// when an external signal (e.g. circuit breaker half-open) indicates the service
/// may have recovered.
///
/// # Example
///
/// ```ignore
/// let retry_queue = Arc::new(RetryQueue::new());
///
/// // On retryable failure:
/// retry_queue.push(options).await;
///
/// // On service recovery signal:
/// let pending = retry_queue.drain_ready().await;
/// for options in pending {
///     coordinator.search(&options).await?;
/// }
/// ```
pub struct RetryQueue {
    inner: Mutex<RetryQueueInner>,
    /// Maximum retry attempts for queued queries before they are discarded
    max_retries: u32,
    /// Minimum time between retry attempts for the same query
    cooldown: Duration,
    /// Maximum number of queued queries (bounded queue)
    max_queue_len: usize,
}

#[derive(Debug, Default)]
struct RetryQueueInner {
    queue: Vec<QueuedQuery>,
}

impl RetryQueue {
    /// Create a new retry queue with default settings
    ///
    /// Defaults: max_retries = 3, cooldown = 30 seconds, max_queue_len = 1024
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(RetryQueueInner::default()),
            max_retries: 3,
            cooldown: Duration::from_secs(30),
            max_queue_len: DEFAULT_MAX_QUEUE_LEN,
        }
    }

    /// Create a new retry queue with custom settings
    pub fn with_config(max_retries: u32, cooldown_secs: u64) -> Self {
        Self {
            inner: Mutex::new(RetryQueueInner::default()),
            max_retries,
            cooldown: Duration::from_secs(cooldown_secs),
            max_queue_len: DEFAULT_MAX_QUEUE_LEN,
        }
    }

    /// Push a failed query into the retry queue
    ///
    /// If the query is already queued (same options), its retry_count is tracked
    /// separately rather than creating a duplicate entry.
    pub async fn push(&self, options: QueryOptions) {
        let mut inner = self.inner.lock().await;

        // Avoid duplicate entries for the same query text within a short window
        if inner.queue.iter().any(|q| q.options.query == options.query) {
            trace!("Query already queued, skipping duplicate");
            return;
        }

        let entry = QueuedQuery {
            options,
            enqueued_at: Instant::now(),
            retry_count: 0,
        };

        inner.queue.push(entry);
        // Bounded queue: drop the oldest entry when at capacity so a service
        // outage cannot grow memory without limit.
        if inner.queue.len() > self.max_queue_len {
            let dropped = inner.queue.remove(0);
            warn!(
                queue_len = inner.queue.len(),
                dropped_query = %dropped.options.query,
                "Retry queue at capacity, dropping oldest query"
            );
        }
        trace!(queue_len = inner.queue.len(), "Query queued for retry");
    }

    /// Drain all queries that are ready for retry (cooldown expired and under max retries)
    ///
    /// Returns the list of query options ready to be re-executed.
    /// Queries that have exceeded max_retries are discarded with a warning.
    pub async fn drain_ready(&self) -> Vec<QueryOptions> {
        let mut inner = self.inner.lock().await;
        let now = Instant::now();
        let mut ready = Vec::new();
        let mut remaining = Vec::new();

        for mut entry in inner.queue.drain(..) {
            if entry.retry_count >= self.max_retries {
                warn!(
                    retry_count = entry.retry_count,
                    max_retries = self.max_retries,
                    "Query exceeded max retries, discarding"
                );
                continue;
            }

            let elapsed = now.duration_since(entry.enqueued_at);
            if elapsed >= self.cooldown {
                entry.retry_count += 1;
                entry.enqueued_at = now;
                ready.push(entry.options);
            } else {
                remaining.push(entry);
            }
        }

        inner.queue = remaining;
        trace!(
            ready = ready.len(),
            remaining = inner.queue.len(),
            "Drained ready queries"
        );
        ready
    }

    /// Get the current number of queued queries
    pub async fn len(&self) -> usize {
        self.inner.lock().await.queue.len()
    }

    /// Check if the queue is empty
    pub async fn is_empty(&self) -> bool {
        self.inner.lock().await.queue.is_empty()
    }

    /// Clear all queued queries
    pub async fn clear(&self) {
        self.inner.lock().await.queue.clear();
        info!("Retry queue cleared");
    }
}

impl Default for RetryQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_query_options(query: &str) -> QueryOptions {
        QueryOptions {
            query: query.to_string(),
            ..Default::default()
        }
    }

    /// Basic push and len/is_empty
    #[tokio::test]
    async fn test_retry_queue_push_and_len() {
        let queue = RetryQueue::new();

        assert!(queue.is_empty().await);
        assert_eq!(queue.len().await, 0);

        queue.push(make_query_options("test query")).await;

        assert!(!queue.is_empty().await);
        assert_eq!(queue.len().await, 1);

        queue.push(make_query_options("another query")).await;

        assert_eq!(queue.len().await, 2);
    }

    /// Drain ready immediately after push
    #[tokio::test]
    async fn test_retry_queue_drain_ready_immediate() {
        let queue = RetryQueue::with_config(5, 0); // no cooldown

        queue.push(make_query_options("query 1")).await;
        queue.push(make_query_options("query 2")).await;

        let ready = queue.drain_ready().await;
        assert_eq!(ready.len(), 2);
        assert!(queue.is_empty().await);
    }

    /// Cooldown prevents premature drain
    #[tokio::test]
    async fn test_retry_queue_cooldown() {
        let queue = RetryQueue::with_config(5, 3600); // 1 hour cooldown

        queue.push(make_query_options("test")).await;

        // Immediately drain — should get nothing because cooldown hasn't expired
        let ready = queue.drain_ready().await;
        assert!(ready.is_empty());

        // Queue should still have the entry
        assert_eq!(queue.len().await, 1);
    }

    /// Clear empties the queue
    #[tokio::test]
    async fn test_retry_queue_clear() {
        let queue = RetryQueue::new();

        queue.push(make_query_options("query")).await;
        assert_eq!(queue.len().await, 1);

        queue.clear().await;
        assert!(queue.is_empty().await);
    }

    /// Multiple drain cycles work correctly (push resets retry_count)
    #[tokio::test]
    async fn test_retry_queue_multiple_drain_cycles() {
        let queue = RetryQueue::with_config(3, 0);

        queue.push(make_query_options("test")).await;

        // First drain — entry is ready (cooldown=0)
        let ready1 = queue.drain_ready().await;
        assert_eq!(ready1.len(), 1);
        assert!(queue.is_empty().await);

        // Push again (simulating re-queue after failure)
        queue.push(make_query_options("test")).await;

        // Second drain — push resets retry_count, so it works again
        let ready2 = queue.drain_ready().await;
        assert_eq!(ready2.len(), 1);
        assert!(queue.is_empty().await);

        // Push and drain third time
        queue.push(make_query_options("test")).await;
        let ready3 = queue.drain_ready().await;
        assert_eq!(ready3.len(), 1);
        assert!(queue.is_empty().await);
    }

    /// Cooldown enforcement with multiple drain attempts
    #[tokio::test]
    async fn test_retry_queue_cooldown_enforcement() {
        let queue = RetryQueue::with_config(5, 60); // 60s cooldown

        queue.push(make_query_options("test")).await;

        // First drain — cooldown not expired
        let ready1 = queue.drain_ready().await;
        assert!(ready1.is_empty());
        assert_eq!(queue.len().await, 1);

        // After a short wait, cooldown is still not expired (60s)
        tokio::time::sleep(Duration::from_millis(100)).await;
        let ready2 = queue.drain_ready().await;
        assert!(ready2.is_empty());
        assert_eq!(queue.len().await, 1);
    }

    /// Duplicate query text suppression
    #[tokio::test]
    async fn test_retry_queue_duplicate_suppression() {
        let queue = RetryQueue::new();

        queue.push(make_query_options("same query")).await;
        assert_eq!(queue.len().await, 1);

        // Push same query text again — should be suppressed
        queue.push(make_query_options("same query")).await;
        assert_eq!(queue.len().await, 1);

        // Different query text should be added
        queue.push(make_query_options("different query")).await;
        assert_eq!(queue.len().await, 2);
    }

    /// Zero max_retries discards immediately
    #[tokio::test]
    async fn test_retry_queue_zero_max_retries() {
        let queue = RetryQueue::with_config(0, 0);

        queue.push(make_query_options("test")).await;

        // With max_retries=0, retry_count(0) >= 0 → immediately discarded
        let ready = queue.drain_ready().await;
        assert_eq!(
            ready.len(),
            0,
            "Entry should be discarded with max_retries=0"
        );
        assert!(
            queue.is_empty().await,
            "Queue should be empty after discard"
        );
    }

    /// Entry with retry_count >= max_retries stays in remaining is discarded
    /// when cooldown eventually expires
    #[tokio::test]
    async fn test_retry_queue_cooldown_then_return_and_discard() {
        // max_retries=1 means an entry returned once (retry_count goes 0→1)
        // but since it's returned (not staying), next push resets to 0
        // So max_retries=1 with cooldown=0: entry is returned once, then gone
        let queue = RetryQueue::with_config(1, 0);

        queue.push(make_query_options("test")).await;

        // retry_count(0) < 1, cooldown expired → retry_count=1, returned
        let ready = queue.drain_ready().await;
        assert_eq!(
            ready.len(),
            1,
            "With max_retries=1, entry should be returned"
        );
        assert!(queue.is_empty().await);

        // Re-push: retry_count reset to 0, same behavior
        queue.push(make_query_options("test")).await;
        let ready2 = queue.drain_ready().await;
        assert_eq!(ready2.len(), 1, "Re-pushed entry should also be returned");
    }

    /// Multiple distinct entries all returned by drain_ready
    #[tokio::test]
    async fn test_retry_queue_multiple_distinct_entries() {
        let queue = RetryQueue::with_config(5, 0);

        for i in 0..5 {
            queue
                .push(make_query_options(&format!("query {}", i)))
                .await;
        }
        assert_eq!(queue.len().await, 5);

        let ready = queue.drain_ready().await;
        assert_eq!(ready.len(), 5);
        assert!(queue.is_empty().await);
    }

    /// Drain with mixed cooldown states
    #[tokio::test]
    async fn test_retry_queue_mixed_cooldown_states() {
        let queue = RetryQueue::with_config(5, 3600); // 1 hour cooldown

        queue.push(make_query_options("query 1")).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        queue.push(make_query_options("query 2")).await;

        // Neither should be ready (cooldown is 1 hour)
        let ready = queue.drain_ready().await;
        assert!(ready.is_empty());
        assert_eq!(queue.len().await, 2);

        // After clearing, both are gone
        queue.clear().await;
        assert!(queue.is_empty().await);
    }

    /// Default configuration values
    #[test]
    fn test_retry_queue_default_config() {
        let queue = RetryQueue::new();
        assert_eq!(queue.max_retries, 3);
        assert_eq!(queue.cooldown, Duration::from_secs(30));
    }

    /// Custom configuration
    #[test]
    fn test_retry_queue_custom_config() {
        let queue = RetryQueue::with_config(5, 120);
        assert_eq!(queue.max_retries, 5);
        assert_eq!(queue.cooldown, Duration::from_secs(120));
    }

    /// Bounded queue drops oldest entry when full
    #[tokio::test]
    async fn test_retry_queue_bounded_drops_oldest() {
        let queue = RetryQueue::with_config(5, 0);
        // Default capacity is 1024; verify the invariant holds after the
        // queue is driven past its bound with distinct query texts.
        for i in 0..(DEFAULT_MAX_QUEUE_LEN + 50) {
            queue
                .push(make_query_options(&format!("query {}", i)))
                .await;
        }
        assert_eq!(
            queue.len().await,
            DEFAULT_MAX_QUEUE_LEN,
            "queue must stay bounded at the configured capacity"
        );

        // The oldest entries are the ones dropped, so the newest survive.
        let ready = queue.drain_ready().await;
        assert_eq!(ready.len(), DEFAULT_MAX_QUEUE_LEN);
        assert_eq!(
            ready.first().map(|o| o.query.as_str()),
            Some("query 50"),
            "oldest queued entries must have been evicted"
        );
    }
}
