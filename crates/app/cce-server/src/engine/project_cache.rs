//! Unified per-project component cache
//!
//! Provides a generic `ProjectCache<T>` to replace the hand-rolled
//! `Arc<RwLock<HashMap<i64, Arc<T>>>>` pattern used throughout the engine.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

/// A lazily-populated, per-project cache keyed by project ID.
///
/// Stores values as `Arc<T>` so the cache does not require `T: Clone`.
/// The cache itself is `Clone` (shared via `Arc`).
///
/// Supports two access patterns:
/// 1. `get_or_create` -- for expensive/fallible construction with double-checked locking
/// 2. `get_or_insert_with` -- for cheap construction (trivial factory)
#[derive(Debug)]
pub struct ProjectCache<T: Send + Sync> {
    inner: Arc<RwLock<HashMap<i64, Arc<T>>>>,
}

impl<T: Send + Sync> ProjectCache<T> {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Fast-path read. Returns `None` on cache miss.
    pub async fn get(&self, project_id: i64) -> Option<Arc<T>> {
        self.inner.read().await.get(&project_id).cloned()
    }

    /// Insert a pre-built value (for use after construction in the caller).
    pub async fn insert(&self, project_id: i64, value: Arc<T>) {
        self.inner.write().await.insert(project_id, value);
    }

    /// Remove entry for a project (used in invalidation).
    pub async fn remove(&self, project_id: i64) -> Option<Arc<T>> {
        self.inner.write().await.remove(&project_id)
    }

    /// Simple get-or-insert for cheap construction (no double-check needed).
    ///
    /// The factory is called synchronously if the cache misses.
    pub async fn get_or_insert_with(
        &self,
        project_id: i64,
        factory: impl FnOnce() -> Arc<T>,
    ) -> Arc<T> {
        // Fast path
        {
            let map = self.inner.read().await;
            if let Some(v) = map.get(&project_id) {
                return v.clone();
            }
        }
        // Slow path (drop read lock first)
        let value = factory();
        let mut map = self.inner.write().await;
        map.entry(project_id).or_insert(value.clone()).clone()
    }

    /// Async double-checked get-or-create for expensive/fallible factories.
    ///
    /// The factory is called only on cache miss, and the result is cached.
    /// If another task inserts a value while the factory runs, the existing
    /// value is returned instead.
    pub async fn get_or_create<F, Fut, E>(&self, project_id: i64, factory: F) -> Result<Arc<T>, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Arc<T>, E>>,
    {
        // Fast path
        if let Some(v) = self.get(project_id).await {
            return Ok(v);
        }
        // Slow path
        let value = factory().await?;
        // Double-check
        {
            let mut map = self.inner.write().await;
            if let Some(existing) = map.get(&project_id) {
                return Ok(existing.clone());
            }
            map.insert(project_id, value.clone());
        }
        Ok(value)
    }

    /// Iterate over all entries (for metrics/cleanup tasks).
    pub async fn for_each<F: FnMut(i64, &Arc<T>)>(&self, mut f: F) {
        let map = self.inner.read().await;
        for (pid, val) in map.iter() {
            f(*pid, val);
        }
    }

    /// Clear all entries.
    pub async fn clear(&self) {
        self.inner.write().await.clear();
    }

    /// Return the number of cached entries.
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    /// Return `true` if the cache is empty.
    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }
}

impl<T: Send + Sync> Default for ProjectCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync> Clone for ProjectCache<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_get_returns_none_on_miss() {
        let cache = ProjectCache::<String>::new();
        assert!(cache.get(1).await.is_none());
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let cache = ProjectCache::new();
        cache.insert(1, Arc::new("hello".to_string())).await;
        assert_eq!(
            cache.get(1).await.as_ref().map(|s| s.as_str()),
            Some("hello")
        );
    }

    #[tokio::test]
    async fn test_remove() {
        let cache = ProjectCache::new();
        cache.insert(1, Arc::new("hello".to_string())).await;
        assert!(cache.remove(1).await.is_some());
        assert!(cache.get(1).await.is_none());
    }

    #[tokio::test]
    async fn test_get_or_insert_with() {
        let cache = ProjectCache::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let v1 = cache
            .get_or_insert_with(1, || {
                counter.fetch_add(1, Ordering::SeqCst);
                Arc::new(42)
            })
            .await;
        assert_eq!(*v1, 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // Second call should hit cache
        let v2 = cache
            .get_or_insert_with(1, || {
                counter.fetch_add(1, Ordering::SeqCst);
                Arc::new(99)
            })
            .await;
        assert_eq!(*v2, 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_get_or_create() {
        let cache = ProjectCache::new();

        let v = cache
            .get_or_create(1, || async {
                Ok::<_, String>(Arc::new("created".to_string()))
            })
            .await
            .unwrap();
        assert_eq!(v.as_str(), "created");

        // Second call should hit cache
        let v2 = cache
            .get_or_create(1, || async {
                Ok::<_, String>(Arc::new("other".to_string()))
            })
            .await
            .unwrap();
        assert_eq!(v2.as_str(), "created");
    }

    #[tokio::test]
    async fn test_get_or_create_error() {
        let cache = ProjectCache::<String>::new();

        let err = cache
            .get_or_create(1, || async { Err("failed".to_string()) })
            .await;
        assert_eq!(err.unwrap_err(), "failed");
        // Cache should be empty after error
        assert!(cache.get(1).await.is_none());
    }

    #[tokio::test]
    async fn test_len_and_is_empty() {
        let cache = ProjectCache::new();
        assert!(cache.is_empty().await);
        assert_eq!(cache.len().await, 0);

        cache.insert(1, Arc::new(42)).await;
        assert!(!cache.is_empty().await);
        assert_eq!(cache.len().await, 1);
    }

    #[tokio::test]
    async fn test_clear() {
        let cache = ProjectCache::new();
        cache.insert(1, Arc::new(42)).await;
        cache.insert(2, Arc::new(43)).await;
        assert_eq!(cache.len().await, 2);

        cache.clear().await;
        assert!(cache.is_empty().await);
    }
}
