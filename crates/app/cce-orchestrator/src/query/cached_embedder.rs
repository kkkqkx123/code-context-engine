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

use cce_llm::{EmbeddingResult, LlmError};
use moka::future::Cache;

/// Maximum number of cached query embeddings per searcher.
const CACHE_MAX_ENTRIES: u64 = 512;

/// Time-to-live for one cached query embedding.
const CACHE_TTL: Duration = Duration::from_secs(600);

/// Wrapper that deduplicates `embed_one` calls per query text.
pub struct CachedEmbedder<P = cce_llm_client::OpenAICompatibleProvider> {
    inner: Arc<cce_llm_client::OpenAICompatibleProvider<P>>,
    cache: Cache<String, Vec<f32>>,
}

impl<P: llm_embedding::EmbeddingProvider> CachedEmbedder<P> {
    /// Wrap the given embedder with a small TTL cache.
    pub fn new(inner: Arc<cce_llm_client::OpenAICompatibleProvider<P>>) -> Self {
        Self {
            inner,
            cache: Self::build_cache(CACHE_TTL),
        }
    }

    /// Wrap with an explicit TTL; test-only, used for expiry testing.
    #[cfg(test)]
    fn with_ttl(inner: Arc<cce_llm_client::OpenAICompatibleProvider<P>>, ttl: Duration) -> Self {
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

impl<P: llm_embedding::EmbeddingProvider> CachedEmbedder<P> {
    pub async fn embed(&self, texts: &[&str]) -> Result<EmbeddingResult, LlmError> {
        self.inner.embed(texts).await
    }

    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        let key = text.to_owned();
        let inner = Arc::clone(&self.inner);
        let pending = {
            let key = key.clone();
            async move { inner.embed_one(&key).await }
        };
        self.cache
            .try_get_with(key, pending)
            .await
            .map_err(|err| Arc::try_unwrap(err).unwrap_or_else(|shared| (*shared).clone()))
    }

    pub async fn embed_vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
        self.inner.embed_vectors(texts).await
    }

    pub fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    pub fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    pub fn is_healthy(&self) -> bool {
        self.inner.is_healthy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llm_embedding::EmbeddingError;
    use llm_embedding::mock::{MockEmbeddingProvider, MockEmbeddingStep};

    type MockEmbedder = cce_llm_client::OpenAICompatibleProvider<MockEmbeddingProvider>;

    fn create_test_embedder(mock: MockEmbeddingProvider) -> Arc<MockEmbedder> {
        Arc::new(MockEmbedder::from_embed_provider(mock, "test-model", 3))
    }

    #[tokio::test]
    async fn repeated_text_hits_cache_once() {
        let embedder = create_test_embedder(MockEmbeddingProvider::new("test-model", 3));
        let cached = CachedEmbedder::new(Arc::clone(&embedder));

        let first = cached.embed_one("same query").await.expect("first embed");
        let second = cached.embed_one("same query").await.expect("second embed");

        assert_eq!(first, second);
        assert_eq!(
            embedder.inner_provider().recorded_batch_sizes(),
            vec![1],
            "identical text must be embedded exactly once"
        );

        let _ = cached.embed_one("other query").await.expect("other embed");
        assert_eq!(embedder.inner_provider().recorded_batch_sizes(), vec![1, 1]);
    }

    #[tokio::test]
    async fn failed_embed_is_not_cached() {
        let mock = MockEmbeddingProvider::with_steps(vec![MockEmbeddingStep::Fail(
            EmbeddingError::Provider {
                status: 429,
                message: "rate limit exceeded".to_string(),
                retry_after_ms: None,
            },
        )])
        .with_dimension(3);
        let embedder = create_test_embedder(mock);
        let cached = CachedEmbedder::new(Arc::clone(&embedder));

        let err = cached
            .embed_one("flaky query")
            .await
            .expect_err("first call fails");
        assert_eq!(err.error_code(), "LLM_RATE_LIMIT_EXCEEDED_ERROR");

        let first = cached.embed_one("flaky query").await.expect("retry embed");
        let second = cached.embed_one("flaky query").await.expect("cached embed");
        assert_eq!(first, second);

        assert_eq!(
            embedder.inner_provider().recorded_batch_sizes(),
            vec![1, 1],
            "failure must not be cached; success must be"
        );
    }

    #[tokio::test]
    async fn concurrent_identical_texts_share_one_remote_call() {
        let mock =
            MockEmbeddingProvider::with_steps(vec![MockEmbeddingStep::Delayed { delay_ms: 20 }])
                .with_dimension(3);
        let embedder = create_test_embedder(mock);
        let cached = Arc::new(CachedEmbedder::new(embedder));

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
            cached.inner.inner_provider().recorded_batch_sizes(),
            vec![1],
            "concurrent identical texts must coalesce into a single remote call"
        );
    }

    #[tokio::test]
    async fn expired_entry_is_reembedded() {
        let embedder = create_test_embedder(MockEmbeddingProvider::new("test-model", 3));
        let cached = CachedEmbedder::with_ttl(Arc::clone(&embedder), Duration::from_millis(50));

        let _ = cached.embed_one("aging query").await.expect("first embed");
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = cached.embed_one("aging query").await.expect("second embed");

        assert_eq!(
            embedder.inner_provider().recorded_batch_sizes(),
            vec![1, 1],
            "expired entry must trigger a fresh remote call"
        );
    }

    #[tokio::test]
    async fn metadata_delegates_to_inner() {
        let embedder = create_test_embedder(MockEmbeddingProvider::new("test-model", 3));
        let cached = CachedEmbedder::new(embedder);
        assert_eq!(cached.dimension(), 3);
        assert_eq!(cached.model_name(), "test-model");
        assert!(cached.is_healthy());
    }
}
