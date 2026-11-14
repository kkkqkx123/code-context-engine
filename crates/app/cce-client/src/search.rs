//! Semantic, keyword and aggregated retrieval.

use cce_api::models::{
    AggregatedSearchRequest, EntitySearchRequest, EntitySearchResponse, SearchRequest,
    SearchResponse,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// Run one retrieval request.
    pub async fn search(&self, request: &SearchRequest) -> Result<SearchResponse, ClientError> {
        self.post_json(paths::SEARCH.to_string(), request, INTERACTIVE_BUDGET)
            .await
    }

    /// Run several weighted sub-queries in one request.
    pub async fn search_aggregated(
        &self,
        request: &AggregatedSearchRequest,
    ) -> Result<SearchResponse, ClientError> {
        self.post_json(
            paths::SEARCH_AGGREGATED.to_string(),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Search entities by name over the full-text symbol index.
    pub async fn entity_search(
        &self,
        request: &EntitySearchRequest,
    ) -> Result<EntitySearchResponse, ClientError> {
        self.post_json(
            paths::ENTITY_SEARCH.to_string(),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }
}
