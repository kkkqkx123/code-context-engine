//! Query-side embedding memoization.
//!
//! A single search flow can require embedding the same query text more than
//! once: dense retrieval and summary boost each call `Embedder::embed_one`
//! with the identical text. This wrapper shares one remote call across all
//! consumers of a project's [`Searcher`](super::Searcher) by caching results
//! per query text; concurrent identical lookups additionally coalesce into a
//! single in-flight remote call (single-flight).
//!
//! The embedder instance is fixed for the searcher's lifetime (the server
//! rebuilds searchers when the configuration changes), so the query text is
//! a sufficient cache key. Batch methods are delegated uncached: the query
//! path only uses `embed_one`.

use std::sync::Arc;
use std::time::Duration;

use cce_llm::{Embedder, EmbeddingResult, LlmError};
use moka::future::Cache;

/// Maximum number of cached query embeddings per searcher.
const CACHE_MAX_ENTRIES: u64 = 512;

/// Time-to-live for one cached query embedding.
const CACHE_TTL: Duration = Duration::from_secs(600);

/// [`Embedder`] wrapper that deduplicates `embed_one` calls per query text.
pub struct CachedEmbedder {
    inner: Arc<dyn Embedder>,
    cache: Cache<String, Vec<f32>>,
}

impl CachedEmbedder {
    /// Wrap the given embedder with a small TTL cache.
    pub fn new(inner: Arc<dyn Embedder>) -> Self {
        Self {
            inner,
            cache: Self::build_cache(CACHE_TTL),
        }
    }

    /// Wrap with an explicit TTL; test-only, used for expiry testing.
    #[cfg(test)]
    fn with_ttl(inner: Arc<dyn Embedder>, ttl: Duration) -> Self {
        Self {
            inner,
            cache: Self::build_cache(ttl),
        }
    }

    fn build_cache(ttl: Duration) -> Cache<String, Vec<f32>> {
        Cache::builder()
            .max_capacity(CACHE_MAX_ENTRIES)
            .time_to_live(ttl)
            .build()
    }
}

#[async_trait::async_trait]
impl Embedder for CachedEmbedder {
    async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, LlmError> {
        self.inner.embed(texts).await
    }

    async fn embed_one(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        // `try_get_with` provides single-flight semantics: concurrent
        // identical texts share one remote call instead of racing separate
        // ones, and a failed result is never inserted, so the error stays
        // observable and the next lookup retries against the remote instead
        // of replaying it from the cache for the whole TTL.
        let key = text.to_owned();
        let inner = Arc::clone(&self.inner);
        let pending = {
            let key = key.clone();
            async move { inner.embed_one(&key).await }
        };
        self.cache
            .try_get_with(key, pending)
            .await
            // Single caller gets the error back without cloning; extra
            // awaiters of the same in-flight call clone the shared error.
            .map_err(|err| Arc::try_unwrap(err).unwrap_or_else(|shared| (*shared).clone()))
    }

    async fn embed_vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
        self.inner.embed_vectors(texts).await
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn is_healthy(&self) -> bool {
        self.inner.is_healthy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inner embedder that counts `embed_one` invocations.
    struct CountingEmbedder {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl CountingEmbedder {
        fn new() -> Self {
            Self {
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl Embedder for CountingEmbedder {
        async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, LlmError> {
            Ok(EmbeddingResult {
                embeddings: texts.iter().map(|_| vec![0.0; 3]).collect(),
                prompt_tokens: 0,
                total_tokens: 0,
            })
        }

        async fn embed_one(&self, _text: &str) -> Result<Vec<f32>, LlmError> {
            self.calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(vec![1.0, 2.0, 3.0])
        }

        async fn embed_vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
            Ok(texts.iter().map(|_| vec![0.0; 3]).collect())
        }

        fn dimension(&self) -> usize {
            3
        }

        fn model_name(&self) -> &str {
            "counting"
        }

        fn is_healthy(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn repeated_text_hits_cache_once() {
        let counting = Arc::new(CountingEmbedder::new());
        let cached = CachedEmbedder::new(counting.clone());

        let first = cached.embed_one("same query").await.expect("first embed");
        let second = cached.embed_one("same query").await.expect("second embed");

        assert_eq!(first, second);
        assert_eq!(
            counting.calls.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "identical text must be embedded exactly once"
        );

        // Different text goes through to the inner embedder.
        let _ = cached.embed_one("other query").await.expect("other embed");
        assert_eq!(counting.calls.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    /// Inner embedder whose first call fails and subsequent calls succeed.
    struct FailFirstEmbedder {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl FailFirstEmbedder {
        fn new() -> Self {
            Self {
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl Embedder for FailFirstEmbedder {
        async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, LlmError> {
            Ok(EmbeddingResult {
                embeddings: texts.iter().map(|_| vec![0.0; 3]).collect(),
                prompt_tokens: 0,
                total_tokens: 0,
            })
        }

        async fn embed_one(&self, _text: &str) -> Result<Vec<f32>, LlmError> {
            let call = self
                .calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if call == 0 {
                Err(LlmError::internal("remote unavailable"))
            } else {
                Ok(vec![1.0, 2.0, 3.0])
            }
        }

        async fn embed_vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
            Ok(texts.iter().map(|_| vec![0.0; 3]).collect())
        }

        fn dimension(&self) -> usize {
            3
        }

        fn model_name(&self) -> &str {
            "fail-first"
        }

        fn is_healthy(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn failed_embed_is_not_cached() {
        let flaky = Arc::new(FailFirstEmbedder::new());
        let cached = CachedEmbedder::new(flaky.clone());

        // The failure must surface to the caller.
        let err = cached
            .embed_one("flaky query")
            .await
            .expect_err("first call fails");
        assert_eq!(err.error_code(), "LLM_INTERNAL_ERROR");

        // The retry goes back to the remote instead of replaying the error.
        let vector = cached.embed_one("flaky query").await.expect("retry embed");
        assert_eq!(vector, vec![1.0, 2.0, 3.0]);

        // The successful result is now memoized.
        let _ = cached.embed_one("flaky query").await.expect("cached embed");
        assert_eq!(
            flaky.calls.load(std::sync::atomic::Ordering::Relaxed),
            2,
            "failure must not be cached; success must be"
        );
    }

    /// Inner embedder that counts invocations and delays each response so
    /// concurrent lookups can pile up on one in-flight remote call.
    struct SlowEmbedder {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl SlowEmbedder {
        fn new() -> Self {
            Self {
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl Embedder for SlowEmbedder {
        async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, LlmError> {
            Ok(EmbeddingResult {
                embeddings: texts.iter().map(|_| vec![0.0; 3]).collect(),
                prompt_tokens: 0,
                total_tokens: 0,
            })
        }

        async fn embed_one(&self, _text: &str) -> Result<Vec<f32>, LlmError> {
            self.calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(vec![7.0, 8.0, 9.0])
        }

        async fn embed_vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
            Ok(texts.iter().map(|_| vec![0.0; 3]).collect())
        }

        fn dimension(&self) -> usize {
            3
        }

        fn model_name(&self) -> &str {
            "slow"
        }

        fn is_healthy(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn concurrent_identical_texts_share_one_remote_call() {
        let slow = Arc::new(SlowEmbedder::new());
        let cached = Arc::new(CachedEmbedder::new(slow.clone()));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let cached = Arc::clone(&cached);
            handles.push(tokio::spawn(async move {
                cached.embed_one("shared query").await.expect("embed")
            }));
        }
        for handle in handles {
            handle.await.expect("task join");
        }

        assert_eq!(
            slow.calls.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "concurrent identical texts must coalesce into a single remote call"
        );
    }

    #[tokio::test]
    async fn expired_entry_is_reembedded() {
        let counting = Arc::new(CountingEmbedder::new());
        let cached = CachedEmbedder::with_ttl(counting.clone(), Duration::from_millis(50));

        let _ = cached.embed_one("aging query").await.expect("first embed");
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = cached.embed_one("aging query").await.expect("second embed");

        assert_eq!(
            counting.calls.load(std::sync::atomic::Ordering::Relaxed),
            2,
            "expired entry must trigger a fresh remote call"
        );
    }

    #[tokio::test]
    async fn metadata_delegates_to_inner() {
        let cached = CachedEmbedder::new(Arc::new(CountingEmbedder::new()));
        assert_eq!(cached.dimension(), 3);
        assert_eq!(cached.model_name(), "counting");
        assert!(cached.is_healthy());

        let result = cached.embed(&["a", "b"]).await.expect("batch embed");
        assert_eq!(result.embeddings.len(), 2);
    }
}
