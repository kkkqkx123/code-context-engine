//! Summary relevance boost contributor
//!
//! Computes additive boost contributions from file-level summary similarity.
//! Code chunks in files whose summary matches the query receive a contribution
//! proportional to the summary similarity score.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::query::boost::{BoostAggregationConfig, BoostContribution};
use crate::query::error::{QueryError, Result};
use crate::query::types::{SearchConfig, SearchResult};
use cce_llm::Embedder;
use cce_llm_client::LlmError;
use cce_storage_common::{DenseSearchQuery, SearchFilter};
use cce_storage_qdrant::QdrantRetrieval;
use cce_types::PointKind;
use cce_types::error::common::HttpError;

/// Summary relevance boost contributor
///
/// Queries the summary index for file-level query matching and computes
/// additive boost contributions for candidates in matching files.
#[derive(Clone)]
pub struct SummaryBoost {
    qdrant: Arc<QdrantRetrieval>,
    embedder: Arc<dyn Embedder>,
    project_group_id: String,
}

impl SummaryBoost {
    pub fn new(
        qdrant: Arc<QdrantRetrieval>,
        embedder: Arc<dyn Embedder>,
        project_group_id: String,
    ) -> Self {
        Self {
            qdrant,
            embedder,
            project_group_id,
        }
    }

    /// Collect summary relevance boost contributions for the given candidates.
    ///
    /// Returns contributions for candidates in files whose summary matches
    /// the query above the configured threshold.
    pub async fn collect(
        &self,
        candidates: &[SearchResult],
        query: &str,
        config: &SearchConfig,
        boost_config: &BoostAggregationConfig,
    ) -> Result<Vec<BoostContribution>> {
        if candidates.is_empty() || !config.summary.enable_boost {
            return Ok(Vec::new());
        }

        // Extract unique file paths
        let file_paths: HashSet<String> = candidates.iter().map(|c| c.file_path.clone()).collect();
        if file_paths.is_empty() {
            return Ok(Vec::new());
        }

        // Generate query embedding
        let query_embedding = self
            .embedder
            .embed_one(query)
            .await
            .map_err(QueryError::Vector)?;

        // Search summaries from the shared vector collection
        let dense_query = DenseSearchQuery::new(query_embedding, config.summary.top_k)
            .with_score_threshold(config.summary.min_score)
            .with_filter(SearchFilter {
                group_id: Some(self.project_group_id.clone()),
                point_type: Some(PointKind::Summary),
                ..Default::default()
            });

        let summary_results = self
            .qdrant
            .search_dense(dense_query)
            .await
            .map_err(|e| QueryError::Vector(LlmError::Http(HttpError::new(e.to_string()))))?;

        // Build file_path → summary_score map
        let summary_score_map: HashMap<String, f32> = summary_results
            .into_iter()
            .filter(|r| file_paths.contains(&r.payload.file_path))
            .map(|r| (r.payload.file_path, r.score))
            .collect();

        tracing::trace!(
            matched_files = summary_score_map.len(),
            "Summary boost: matched files"
        );

        // Build contributions
        let mut contributions = Vec::new();
        for candidate in candidates {
            if let Some(summary_score) = summary_score_map.get(&candidate.file_path) {
                if *summary_score >= config.summary.min_score {
                    // Normalize summary score to [0, 1] using min_score threshold
                    let normalized = ((summary_score - config.summary.min_score)
                        / (1.0 - config.summary.min_score))
                        .clamp(0.0, 1.0);
                    let boost_value = boost_config.summary_max * normalized;

                    if boost_value > 0.0 {
                        contributions.push(
                            BoostContribution::new(
                                candidate.id.clone(),
                                "summary",
                                boost_value,
                                *summary_score,
                            )
                            .with_reason(format!(
                                "summary(score={:.3}, norm={:.3})",
                                summary_score, normalized
                            )),
                        );
                    }
                }
            }
        }

        Ok(contributions)
    }
}
