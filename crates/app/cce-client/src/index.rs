//! Indexing, incremental indexing, single-file parsing and summary generation.

use cce_api::models::{
    IncrementalIndexRequest, IncrementalIndexResponse, IndexRequest, IndexResponse, ParseRequest,
    ParseResponse, SummaryRequest, SummaryResponse,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// Run a full index pass over a directory the server can read.
    pub async fn index(&self, request: &IndexRequest) -> Result<IndexResponse, ClientError> {
        self.post_json(paths::INDEX.to_string(), request, self.long_task_budget())
            .await
    }

    /// Add and remove individual files from an existing index.
    pub async fn incremental_index(
        &self,
        request: &IncrementalIndexRequest,
    ) -> Result<IncrementalIndexResponse, ClientError> {
        self.post_json(
            paths::INDEX_INCREMENTAL.to_string(),
            request,
            self.long_task_budget(),
        )
        .await
    }

    /// Parse one file and report its entities and relations without storing.
    pub async fn parse_file(&self, request: &ParseRequest) -> Result<ParseResponse, ClientError> {
        self.post_json(paths::PARSE.to_string(), request, INTERACTIVE_BUDGET)
            .await
    }

    /// Generate natural-language summaries for files or directories.
    pub async fn summary(&self, request: &SummaryRequest) -> Result<SummaryResponse, ClientError> {
        self.post_json(paths::SUMMARY.to_string(), request, self.long_task_budget())
            .await
    }
}
