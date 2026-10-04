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
use cce_llm_client::OpenAICompatibleProvider;
use moka::future::Cache;

/// Maximum number of cached query embeddings per searcher.
const CACHE_MAX_ENTRIES: u64 = 512;

/// Time-to-live for one cached query embedding.
const CACHE_TTL: Duration = Duration::from_secs(600);

/// Wrapper that deduplicates `embed_one` calls per query text.
pub struct CachedEmbedder {
    inner: Arc<OpenAICompatibleProvider>,
    cache: Cache<String, Vec<f32>>,
}

impl CachedEmbedder {
    /// Wrap the given embedder with a small TTL cache.
    pub fn new(inner: Arc<OpenAICompatibleProvider>) -> Self {
        Self {
            inner,
            cache: Self::build_cache(CACHE_TTL),
        }
    }

    /// Wrap with an explicit TTL; test-only, used for expiry testing.
    #[cfg(test)]
    fn with_ttl(inner: Arc<OpenAICompatibleProvider>, ttl: Duration) -> Self {
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

impl CachedEmbedder {
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
    use cce_llm_client::services::embedding::mock_server::{MockEmbeddingServer, MockResponse};

    fn create_test_embedder(
        server: &MockEmbeddingServer,
    ) -> Arc<cce_llm_client::OpenAICompatibleProvider> {
        let config = server.app_config("test-model", 3);
        let provider = cce_llm_client::OpenAICompatibleProvider::from_model(&config, "test-model")
            .expect("create embedder");
        Arc::new(provider)
    }

    #[tokio::test]
    async fn repeated_text_hits_cache_once() {
        let server = MockEmbeddingServer::start();
        let embedder = create_test_embedder(&server);
        let cached = CachedEmbedder::new(embedder);

        let first = cached.embed_one("same query").await.expect("first embed");
        let second = cached.embed_one("same query").await.expect("second embed");

        assert_eq!(first, second);
        assert_eq!(
            server.request_count(),
            1,
            "identical text must be embedded exactly once"
        );

        let _ = cached.embed_one("other query").await.expect("other embed");
        assert_eq!(server.request_count(), 2);
    }

    #[tokio::test]
    async fn failed_embed_is_not_cached() {
        let server = MockEmbeddingServer::start();
        server.queue_response(MockResponse::RateLimit);
        server.queue_response(MockResponse::Success { dimension: 3 });

        let embedder = create_test_embedder(&server);
        let cached = CachedEmbedder::new(embedder);

        let err = cached
            .embed_one("flaky query")
            .await
            .expect_err("first call fails");
        assert_eq!(err.error_code(), "LLM_RATE_LIMIT_EXCEEDED");

        let vector = cached.embed_one("flaky query").await.expect("retry embed");
        assert_eq!(vector, vec![0.5, 0.5, 0.5]);

        let _ = cached.embed_one("flaky query").await.expect("cached embed");
        assert_eq!(
            server.request_count(),
            2,
            "failure must not be cached; success must be"
        );
    }

    #[tokio::test]
    async fn concurrent_identical_texts_share_one_remote_call() {
        let server = MockEmbeddingServer::start();
        server.queue_response(MockResponse::Delayed {
            delay: Duration::from_millis(20),
            dimension: 3,
        });

        let embedder = create_test_embedder(&server);
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
            server.request_count(),
            1,
            "concurrent identical texts must coalesce into a single remote call"
        );
    }

    #[tokio::test]
    async fn expired_entry_is_reembedded() {
        let server = MockEmbeddingServer::start();
        let embedder = create_test_embedder(&server);
        let cached = CachedEmbedder::with_ttl(embedder, Duration::from_millis(50));

        let _ = cached.embed_one("aging query").await.expect("first embed");
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = cached.embed_one("aging query").await.expect("second embed");

        assert_eq!(
            server.request_count(),
            2,
            "expired entry must trigger a fresh remote call"
        );
    }

    #[tokio::test]
    async fn metadata_delegates_to_inner() {
        let server = MockEmbeddingServer::start();
        let embedder = create_test_embedder(&server);
        let cached = CachedEmbedder::new(embedder);
        assert_eq!(cached.dimension(), 3);
        assert_eq!(cached.model_name(), "test-model");
        assert!(cached.is_healthy());

        let result = cached.embed(&["a", "b"]).await.expect("batch embed");
        assert_eq!(result.embeddings.len(), 2);
    }
}
