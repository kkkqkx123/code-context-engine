//! Hybrid alignment review report.
//!
//! Generates a human-review alignment report under
//! `outputs/scenarios/rust/alignment/`:
//!
//! - `run_manifest.txt`: fixture, queries, vector backend, alignment key
//!   convention and metric definitions.
//! - `alignment_summary.md`: per-query and overall match, single-path and
//!   unalignable ratios.
//! - `per_query_alignment.md`: per-query vector/BM25/hybrid key lists.
//!
//! Uses the deterministic mock embedding server, so no LLM service is needed.
//! Hybrid queries run on the embedded local vector engine; no external
//! service is required and the manifest records the local backend.
//!
//! Run: `cargo run --example alignment_report -p cce-e2e-tests`

use std::sync::Arc;

use cce_e2e_tests::embedding::EmbeddingConfig;
use cce_e2e_tests::mock_embedding_server::MockEmbeddingServer;
use cce_e2e_tests::output_manager::{OutputBuilder, OutputCategory};
use cce_e2e_tests::query_test::QueryWorkflowTest;
use cce_llm_client::OpenAICompatibleProvider;
use cce_orchestrator::query::retrieval::primitives::fusion::compute_alignment_coverage;
use cce_orchestrator::query::searcher::expand_multi_entity_results;
use cce_orchestrator::query::types::SearchResult;

const FIXTURE: &str = "rust/basic";
const QUERIES: &[&str] = &["process", "internal", "process internal result"];

fn no_threshold_config() -> cce_orchestrator::SearchConfig {
    let mut config = cce_orchestrator::SearchConfig::default();
    config.vector.similarity_threshold = 0.0;
    config.result.min_score = 0.0;
    config
}

fn alignment_key_of(item: &SearchResult) -> String {
    cce_types::alignment_key(&item.entity_ids, item.segment_id.as_deref(), &item.id)
        .unwrap_or_else(|| format!("c:{}", item.id))
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn fmt_keys(items: &[SearchResult]) -> String {
    let mut keys: Vec<String> = items.iter().map(alignment_key_of).collect();
    keys.sort_unstable();
    keys.dedup();
    if keys.is_empty() {
        "(none)".to_string()
    } else {
        keys.join(", ")
    }
}

fn main() {
    let manager = OutputBuilder::new()
        .category(OutputCategory::Scenarios)
        .language("rust")
        .scenario("alignment")
        .build();

    let _ = manager.clean();
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let server = rt.block_on(MockEmbeddingServer::start());

    // Load the existing fixture.
    let fixture =
        cce_e2e_tests::fixture::TestFixture::load(cce_e2e_tests::fixture::FixtureSpec::new(
            cce_e2e_tests::fixture::FixtureCategory::Rust,
            "basic",
        ))
        .expect("load rust/basic fixture");

    let embedder = Arc::new(
        OpenAICompatibleProvider::from_model(
            &cce_e2e_tests::mock_embedding_server::mock_embedding_config(&server.base_url),
            "mock",
        )
        .expect("mock embedder"),
    );

    let mut query_test = QueryWorkflowTest::new(fixture, EmbeddingConfig::mock())
        .with_embedder(embedder)
        .with_config(no_threshold_config());

    rt.block_on(query_test.index()).expect("index failed");

    let mut manifest = String::new();
    manifest.push_str(&format!("fixture: {FIXTURE}\n"));
    manifest.push_str("vector_backend: local (embedded simvec, per-run temp data dir)\n");
    manifest.push_str("embedder: deterministic mock (no LLM service)\n");
    manifest.push_str("alignment_key: e:<entity_id> | s:<segment_id> | c:<chunk_id>\n");
    manifest.push_str(
        "metrics: match=matched/vector_keys; single_path=single_keys/distinct_keys; \
         unalignable=unalignable/(all_keys+unalignable); computed with production FusionAlignmentStats\n",
    );
    manifest.push_str(&format!(
        "queries: {}\n",
        QUERIES
            .iter()
            .map(|q| format!("\"{q}\""))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    let mut summary = String::from("# Hybrid Fusion Alignment Summary\n\n");
    let mut per_query = String::from("# Per-Query Alignment Detail\n\n");
    let mut totals = (0usize, 0usize, 0usize, 0usize, 0usize);

    for query in QUERIES {
        let vector_items: Vec<SearchResult> = rt
            .block_on(query_test.search_vector(query, 20))
            .expect("vector search")
            .items
            .clone();
        let bm25_items: Vec<SearchResult> = rt
            .block_on(query_test.search_bm25(query, 20))
            .expect("bm25 search")
            .items
            .clone();
        let hybrid_items: Vec<SearchResult> = rt
            .block_on(query_test.search_hybrid(query, 20))
            .expect("hybrid search")
            .items
            .clone();

        let vector_items = expand_multi_entity_results(vector_items);
        let bm25_items = expand_multi_entity_results(bm25_items);
        let hybrid_items = expand_multi_entity_results(hybrid_items);

        let stats = compute_alignment_coverage(&vector_items, &bm25_items);
        let vector_keys: std::collections::HashSet<String> =
            vector_items.iter().map(alignment_key_of).collect();
        let bm25_keys: std::collections::HashSet<String> =
            bm25_items.iter().map(alignment_key_of).collect();

        totals.0 += stats.vector_keys;
        totals.1 += stats.bm25_keys;
        totals.2 += stats.matched_keys;
        totals.3 += stats.single_path_keys();
        totals.4 += stats.unalignable();

        summary.push_str(&format!("## Query: \"{query}\"\n"));
        summary.push_str(&format!(
            "- vector keys: {} ({} items)\n",
            stats.vector_keys,
            vector_items.len()
        ));
        summary.push_str(&format!(
            "- bm25 keys: {} ({} items)\n",
            stats.bm25_keys,
            bm25_items.len()
        ));
        summary.push_str(&format!(
            "- matched keys: {} (match ratio {:.2})\n",
            stats.matched_keys,
            stats.match_ratio()
        ));
        summary.push_str(&format!(
            "- single-path keys: {} (ratio {:.2})\n",
            stats.single_path_keys(),
            stats.single_path_ratio()
        ));
        summary.push_str(&format!(
            "- unalignable entries: {} (ratio {:.2})\n",
            stats.unalignable(),
            stats.unalignable_ratio()
        ));
        summary.push_str(&format!(
            "- hybrid items: {}; hybrid key ⊆ (vector ∪ bm25): {}\n",
            hybrid_items.len(),
            hybrid_items.iter().all(|r| {
                let k = alignment_key_of(r);
                vector_keys.contains(&k) || bm25_keys.contains(&k)
            })
        ));
        summary.push('\n');

        per_query.push_str(&format!("## Query: \"{query}\"\n"));
        per_query.push_str(&format!("- vector keys: {}\n", fmt_keys(&vector_items)));
        per_query.push_str(&format!("- bm25 keys: {}\n", fmt_keys(&bm25_items)));
        per_query.push_str(&format!("- hybrid keys: {}\n", fmt_keys(&hybrid_items)));
        per_query.push('\n');
    }

    let distinct = totals.0 + totals.1 - totals.2;
    let observed = totals.0 + totals.1 + totals.4;
    summary.push_str("## Overall\n\n");
    summary.push_str(&format!(
        "- keys per path: vector {}, bm25 {}; matched {}\n",
        totals.0, totals.1, totals.2
    ));
    summary.push_str(&format!(
        "- match ratio: {:.2} (matched / vector keys)\n",
        ratio(totals.2, totals.0)
    ));
    summary.push_str(&format!(
        "- single-path ratio: {:.2} ({} / {} distinct keys)\n",
        ratio(totals.3, distinct),
        totals.3,
        distinct
    ));
    summary.push_str(&format!(
        "- unalignable ratio: {:.2} ({} / {} observed entries)\n\n",
        ratio(totals.4, observed),
        totals.4,
        observed
    ));
    summary.push_str(
        "Overall ratios aggregate counts across queries; the online histograms record one value per query.\n",
    );

    manager
        .write("run_manifest.txt", &manifest)
        .expect("write run_manifest.txt");
    manager
        .write("alignment_summary.md", &summary)
        .expect("write alignment_summary.md");
    manager
        .write("per_query_alignment.md", &per_query)
        .expect("write per_query_alignment.md");

    println!(
        "Alignment report written to {}",
        manager.output_dir().display()
    );

    rt.block_on(query_test.cleanup());
    rt.block_on(server.stop());
}
