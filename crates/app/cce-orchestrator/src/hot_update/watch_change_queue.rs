//! Bounded queue for accumulated watch changes with overflow backpressure.
//!
//! File-watch event streams are unbounded by nature: an editor saving a file
//! several times or a build tool touching many files can generate more events
//! than a single hot-update operation can consume. Instead of buffering them
//! without limit (unbounded memory) or blocking the watcher thread, this queue:
//!
//! - deduplicates repeated paths with event-merge semantics (the first
//!   occurrence keeps its position, the last event's deletion flag wins,
//!   because it describes the most recent on-disk state), and
//! - when the queue reaches its capacity, drops the incoming event and marks
//!   the queue as `needs_full_rescan` so the next operation falls back to a
//!   full filesystem scan (`HotUpdateOperationRuntime::update`) instead of
//!   trusting the incomplete event list.
//!
//! The full-rescan fallback is lossless with respect to the filesystem: the
//! change detector re-hashes every file against the persisted cache, so events
//! dropped here are still detected.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
use tracing::warn;

/// Default maximum number of distinct pending watch paths.
pub const DEFAULT_WATCH_CHANGE_CAPACITY: usize = 4096;

struct WatchChangeQueueInner {
    entries: Vec<(PathBuf, bool)>,
}

/// A bounded, deduplicating queue of pending watch changes.
///
/// All mutation happens through `push`/`extend` under the inner mutex; the
/// overflow flag is an atomic so read-only paths (`needs_full_rescan`) never
/// contend with the event loop.
pub struct WatchChangeQueue {
    inner: Mutex<WatchChangeQueueInner>,
    capacity: usize,
    needs_full_rescan: AtomicBool,
}

impl WatchChangeQueue {
    /// Create a queue with a fixed capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(WatchChangeQueueInner {
                entries: Vec::new(),
            }),
            capacity,
            needs_full_rescan: AtomicBool::new(false),
        }
    }

    /// Create a queue with the default capacity.
    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_WATCH_CHANGE_CAPACITY)
    }

    /// Push a single path into the queue.
    ///
    /// Repeats of an already-queued path update its deletion flag in place.
    /// When the queue is at capacity the entry is dropped and the queue is
    /// flagged for a full rescan; once flagged, further pushes are dropped so
    /// memory stays bounded.
    pub async fn push(&self, path: PathBuf, is_deletion: bool) {
        let mut inner = self.inner.lock().await;
        if self.needs_full_rescan.load(Ordering::Relaxed) {
            return;
        }
        self.insert_inner(&mut inner, path, is_deletion);
    }

    /// Extend the queue with a batch of paths (event-loop forwarding).
    pub async fn extend(&self, incoming: Vec<(PathBuf, bool)>) {
        let mut inner = self.inner.lock().await;
        if self.needs_full_rescan.load(Ordering::Relaxed) {
            return;
        }
        for (path, is_deletion) in incoming {
            self.insert_inner(&mut inner, path, is_deletion);
        }
    }

    /// Drain all queued changes, returning the entries.
    ///
    /// The overflow flag is cleared so a subsequent operation starts with a
    /// clean slate; if a full rescan is required the caller must read
    /// `needs_full_rescan()` *before* this call.
    pub async fn take(&self) -> Vec<(PathBuf, bool)> {
        let mut inner = self.inner.lock().await;
        self.needs_full_rescan.store(false, Ordering::Relaxed);
        std::mem::take(&mut inner.entries)
    }

    /// Whether the queue overflowed and the next operation must fall back to a
    /// full filesystem scan.
    pub fn needs_full_rescan(&self) -> bool {
        self.needs_full_rescan.load(Ordering::Relaxed)
    }

    /// Mark the queue as requiring a full rescan.
    ///
    /// Used when the upstream event channel overflowed: events dropped there
    /// never reached this queue, so the filesystem must be scanned to recover
    /// them.
    pub fn mark_full_rescan(&self) {
        self.needs_full_rescan.store(true, Ordering::Relaxed);
    }

    /// Number of queued entries.
    pub async fn len(&self) -> usize {
        self.inner.lock().await.entries.len()
    }

    /// Whether the queue holds no entries.
    pub async fn is_empty(&self) -> bool {
        self.inner.lock().await.entries.is_empty()
    }

    /// Drop all queued entries without affecting the overflow flag.
    pub async fn clear(&self) {
        self.inner.lock().await.entries.clear();
    }

    fn insert_inner(&self, inner: &mut WatchChangeQueueInner, path: PathBuf, is_deletion: bool) {
        match inner
            .entries
            .iter_mut()
            .find(|(existing, _)| *existing == path)
        {
            Some(existing) => existing.1 = is_deletion,
            None => {
                if inner.entries.len() >= self.capacity {
                    warn!(
                        capacity = self.capacity,
                        path = %path.display(),
                        "Watch change queue overflow, scheduling full rescan"
                    );
                    self.needs_full_rescan.store(true, Ordering::Relaxed);
                } else {
                    inner.entries.push((path, is_deletion));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_watch_change_queue_push_dedup_keep_last_flag() {
        let queue = WatchChangeQueue::with_default_capacity();

        queue.push(PathBuf::from("/p/a.rs"), false).await;
        queue.push(PathBuf::from("/p/b.rs"), true).await;
        // Repeat of a.rs keeps first position, deletion flag wins.
        queue.push(PathBuf::from("/p/a.rs"), true).await;

        let taken = queue.take().await;
        assert_eq!(taken.len(), 2);
        assert_eq!(taken[0], (PathBuf::from("/p/a.rs"), true));
        assert_eq!(taken[1], (PathBuf::from("/p/b.rs"), true));
        assert!(!queue.needs_full_rescan());
        assert!(queue.is_empty().await);
    }

    #[tokio::test]
    async fn test_watch_change_queue_overflow_marks_full_rescan() {
        let queue = WatchChangeQueue::new(2);

        queue.push(PathBuf::from("/p/a.rs"), false).await;
        queue.push(PathBuf::from("/p/b.rs"), false).await;
        assert!(!queue.needs_full_rescan());

        // At capacity: the third distinct path is dropped and flagged.
        queue.push(PathBuf::from("/p/c.rs"), false).await;
        assert!(queue.needs_full_rescan());

        // After the flag, further pushes are dropped.
        queue.push(PathBuf::from("/p/d.rs"), false).await;

        assert!(queue.needs_full_rescan());
        let taken = queue.take().await;
        assert_eq!(taken.len(), 2, "entries before overflow are preserved");
        assert!(!queue.needs_full_rescan(), "take clears the overflow flag");
    }

    #[tokio::test]
    async fn test_watch_change_queue_extend_batch() {
        let queue = WatchChangeQueue::new(4);

        queue
            .extend(vec![
                (PathBuf::from("/p/a.rs"), false),
                (PathBuf::from("/p/b.rs"), false),
                (PathBuf::from("/p/a.rs"), true),
            ])
            .await;

        assert_eq!(queue.len().await, 2);
        assert!(!queue.needs_full_rescan());

        let taken = queue.take().await;
        assert_eq!(taken[0], (PathBuf::from("/p/a.rs"), true));
    }

    #[tokio::test]
    async fn test_watch_change_queue_mark_full_rescan_from_upstream() {
        let queue = WatchChangeQueue::with_default_capacity();
        assert!(!queue.needs_full_rescan());

        // Simulates the event channel overflowing before events reached us.
        queue.mark_full_rescan();
        assert!(queue.needs_full_rescan());

        // New pushes are dropped while the flag is set.
        queue.push(PathBuf::from("/p/a.rs"), false).await;
        assert!(queue.take().await.is_empty());
    }
}
