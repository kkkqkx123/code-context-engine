//! Full index pipeline performance probe.
//!
//! Indexes a deterministic synthetic project through the production query
//! workflow: scanner, parser, dual-path text generation, summaries, mock
//! embeddings, local vector store, local BM25 index, SQLite metadata and
//! relation publication. Resource counters are sampled before and after the
//! timed phase so the cost of all backend stages is visible in one row.
//!
//! Run with `cargo run --release -p cce-e2e-tests --example full_pipeline_perf`.
//! `CCE_PERF_SIZE=medium` switches from the 500-file default to 5,000 files;
//! `CCE_PERF_SIZE=large` uses 50,000 files.
//! Overall trends are appended to `outputs/benchmark/full_pipeline_perf.tsv`;
//! per-stage timings are appended to `outputs/benchmark/full_pipeline_stages.tsv`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use cce_bench_kit::{
    DEFAULT_LANG_DIST, ProjectSize, SyntheticProject, cpu_ms, resource_delta, resource_snapshot,
};
use cce_e2e_tests::fixture::FixtureAccess;
use cce_e2e_tests::mock_embedding_server::{MockEmbeddingServer, mock_embedding_config};
use cce_e2e_tests::{EmbeddingConfig, QueryWorkflowTest};
use cce_llm_client::OpenAICompatibleProvider;
use cce_orchestrator::SearchSources;

struct SyntheticFixture {
    root: PathBuf,
}

impl FixtureAccess for SyntheticFixture {
    fn root(&self) -> &Path {
        &self.root
    }

    fn file<P: AsRef<Path>>(&self, relative_path: P) -> PathBuf {
        self.root.join(relative_path)
    }

    fn add_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &self,
        relative_path: P,
        content: C,
    ) -> io::Result<PathBuf> {
        let path = self.file(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, content)?;
        Ok(path)
    }

    fn modify_file<P: AsRef<Path>, C: AsRef<[u8]>>(
        &self,
        relative_path: P,
        content: C,
    ) -> io::Result<PathBuf> {
        self.add_file(relative_path, content)
    }

    fn delete_file<P: AsRef<Path>>(&self, relative_path: P) -> io::Result<()> {
        fs::remove_file(self.file(relative_path))
    }
}

fn size_from_env() -> ProjectSize {
    match std::env::var("CCE_PERF_SIZE").as_deref() {
        Ok("medium") => ProjectSize::Medium,
        Ok("large") => ProjectSize::Large,
        _ => ProjectSize::Small,
    }
}

const PERF_COLUMNS: &[&str] = &[
    "tier",
    "files",
    "indexed",
    "failed",
    "entities",
    "relations",
    "vectors",
    "reported_ms",
    "wall_ms",
    "files_per_sec",
    "cpu_ms",
    "peak_rss_kb",
    "peak_rss_delta_kb",
    "read_bytes_delta",
    "write_bytes_delta",
];

const STAGE_COLUMNS: &[&str] = &[
    "tier",
    "files",
    "wall_ms",
    "scan_ms",
    "batch_process_ms",
    "batch_setup_ms",
    "entity_store_ms",
    "vector_store_ms",
    "bm25_store_ms",
    "summary_ms",
    "relation_ms",
    "relation_symbol_register_ms",
    "symbol_propagation_ms",
    "relation_register_ms",
    "relation_resolve_ms",
    "relation_artifact_ms",
    "relation_publish_ms",
    "relation_publish_store_ms",
    "relation_state_mark_ms",
    "finalize_index_ms",
    "relation_spool_ms",
    "relation_spool_write_ms",
    "relation_symbols_ms",
    "symbol_modules_ms",
    "symbol_type_index_ms",
    "symbol_overload_ms",
    "symbol_flat_ms",
    "symbol_cross_file_ms",
    "export_spool_ms",
    "export_ms",
    "manifest_ms",
    "batch_overhead_ms",
    "state_projection_ms",
    "checkpoint_ms",
    "progress_ms",
    "batch_dispatch_ms",
    "write_commit_ms",
    "batches",
    "vector_calls",
    "bm25_calls",
    "metadata",
    "entity_store_calls",
    "vector_txns_applied",
    "vector_points_written",
    "vector_wal_flushes",
    "vector_wal_txns",
    "vector_commit_wait_ms",
    "vector_hnsw_builds",
    "vector_hnsw_build_ms",
    "vector_checkpoints",
    "vector_checkpoint_ms",
    "bm25_commits",
    "bm25_documents",
    "bm25_document_ms",
    "bm25_commit_ms",
    "bm25_refresh_ms",
    "bm25_merge_ms",
    "bm25_staged_documents",
    "rss_after_scan_kb",
    "rss_after_batches_kb",
    "rss_after_relation_kb",
    "rss_final_kb",
    "rss_peak_batch_kb",
    "vector_pending_points_max",
    "bm25_pending_documents_max",
    "vector_embed_ms",
    "vector_point_build_ms",
    "vector_metadata_ms",
    "vector_work_unit_ms",
    "vector_microbatches",
    "vector_chunk_rows",
    "vector_entity_mappings",
    "vector_staged_records_max",
    "vector_flush_ms",
    "vector_stage_push_ms",
    "vector_entity_lookup_ms",
    "vector_delay_ms",
    "vector_work_unit_queries",
    "vector_work_unit_query_ms",
    "relation_spool_entries",
    "relation_spool_bytes",
    "symbol_table_entries",
    "symbol_table_estimated_bytes",
    "symbol_view_module_peak_entries",
    "symbol_view_module_peak_bytes",
    "symbol_view_name_peak_entries",
    "symbol_view_name_peak_bytes",
    "symbol_view_queries",
    "symbol_view_candidate_truncated",
    "cross_file_candidate_truncated",
    "relation_nodes",
    "relation_edges",
    "relation_estimated_bytes",
    "relation_snapshot_entries",
    "relation_snapshot_estimated_bytes",
    "canonical_write_batches",
    "canonical_write_buffer_peak_bytes",
    "relation_shard_count",
    "relation_shard_directory_entries",
    "relation_shard_directory_estimated_bytes",
    "published_snapshot_entries",
    "export_spool_entries",
    "export_spool_bytes",
    "entity_id_map_entries",
    "entity_id_map_estimated_bytes",
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let size = size_from_env();
    let root = tempfile::TempDir::new()?;
    let project = SyntheticProject::generate(size, DEFAULT_LANG_DIST);
    let file_count = project.files.len();
    project.materialize(root.path())?;
    println!(
        "full pipeline perf: {file_count} files at {:?}",
        root.path()
    );

    // A loopback mock server keeps the index path on the same embedder as
    // the service benchmarks; without it the fixture silently degrades to
    // metadata-only chunk writes and the engine write counters stay at zero.
    let mock_server = MockEmbeddingServer::start().await;
    let app_config = mock_embedding_config(&mock_server.base_url);
    let embedder = Arc::new(
        OpenAICompatibleProvider::from_model(&app_config, "mock")
            .map_err(|e| format!("create mock embedder: {e}"))?,
    );

    let fixture = SyntheticFixture {
        root: root.path().to_path_buf(),
    };
    let mut workflow = QueryWorkflowTest::new(fixture, EmbeddingConfig::mock())
        .with_embedder(embedder)
        .with_sources(SearchSources::none().with_vector().with_bm25())
        .with_extensions(vec![
            "rs".to_string(),
            "py".to_string(),
            "go".to_string(),
            "ts".to_string(),
        ]);

    let before = resource_snapshot();
    let started = Instant::now();
    let index_result = workflow.index().await?;
    let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
    let after = resource_snapshot();
    let resources = resource_delta(before, after);

    let indexed_files = index_result.indexed_files;
    let failed_files = index_result.failed_files;
    let entities = index_result.total_entities;
    let relations = index_result.total_relations;
    let vectors = index_result.total_vectors;
    let reported_ms = index_result.elapsed_ms;
    let stages = &index_result.stages;

    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("outputs/benchmark");
    fs::create_dir_all(&output_dir)?;
    let output = output_dir.join("full_pipeline_perf.tsv");
    let mut report = cce_bench_kit::TsvReport::open_at(&output, PERF_COLUMNS);
    report.write_row(&[
        size.label().to_string(),
        file_count.to_string(),
        indexed_files.to_string(),
        failed_files.to_string(),
        entities.to_string(),
        relations.to_string(),
        vectors.to_string(),
        reported_ms.to_string(),
        format!("{wall_ms:.1}"),
        format!("{:.1}", indexed_files as f64 / (wall_ms / 1000.0)),
        cpu_ms(&resources).unwrap_or(0).to_string(),
        after.peak_rss_kb.unwrap_or(0).to_string(),
        resources.peak_rss_kb.unwrap_or(0).to_string(),
        resources.read_bytes.unwrap_or(0).to_string(),
        resources.write_bytes.unwrap_or(0).to_string(),
    ])?;

    let mut stage_report = cce_bench_kit::TsvReport::open_at(
        &output_dir.join("full_pipeline_stages.tsv"),
        STAGE_COLUMNS,
    );
    stage_report.write_row(&[
        size.label().to_string(),
        file_count.to_string(),
        format!("{wall_ms:.1}"),
        stages.scan_ms.to_string(),
        stages.batch_process_ms.to_string(),
        stages.batch_setup_ms.to_string(),
        stages.entity_store_ms.to_string(),
        stages.vector_store_ms.to_string(),
        stages.bm25_store_ms.to_string(),
        stages.summary_ms.to_string(),
        stages.relation_ms.to_string(),
        stages.relation_symbol_register_ms.to_string(),
        stages.symbol_propagation_ms.to_string(),
        stages.relation_register_ms.to_string(),
        stages.relation_resolve_ms.to_string(),
        stages.relation_artifact_ms.to_string(),
        stages.relation_publish_ms.to_string(),
        stages.relation_publish_store_ms.to_string(),
        stages.relation_state_mark_ms.to_string(),
        stages.finalize_index_ms.to_string(),
        stages.relation_spool_ms.to_string(),
        stages.relation_spool_write_ms.to_string(),
        stages.relation_symbols_ms.to_string(),
        stages.symbol_modules_ms.to_string(),
        stages.symbol_type_index_ms.to_string(),
        stages.symbol_overload_ms.to_string(),
        stages.symbol_flat_ms.to_string(),
        stages.symbol_cross_file_ms.to_string(),
        stages.export_spool_ms.to_string(),
        stages.export_ms.to_string(),
        stages.manifest_ms.to_string(),
        stages.batch_overhead_ms.to_string(),
        stages.state_projection_ms.to_string(),
        stages.checkpoint_ms.to_string(),
        stages.progress_ms.to_string(),
        stages.batch_dispatch_ms.to_string(),
        stages.write_commit_ms.to_string(),
        stages.batches.to_string(),
        stages.vector_store_calls.to_string(),
        stages.bm25_store_calls.to_string(),
        "memory".to_string(),
        stages.entity_store_calls.to_string(),
        stages.vector_txns_applied.to_string(),
        stages.vector_points_written.to_string(),
        stages.vector_wal_flushes.to_string(),
        stages.vector_wal_txns.to_string(),
        stages.vector_commit_wait_ms.to_string(),
        stages.vector_hnsw_builds.to_string(),
        stages.vector_hnsw_build_ms.to_string(),
        stages.vector_checkpoints.to_string(),
        stages.vector_checkpoint_ms.to_string(),
        stages.bm25_commits.to_string(),
        stages.bm25_documents.to_string(),
        stages.bm25_document_ms.to_string(),
        stages.bm25_commit_ms.to_string(),
        stages.bm25_refresh_ms.to_string(),
        stages.bm25_merge_ms.to_string(),
        stages.bm25_staged_documents.to_string(),
        stages.rss_after_scan_kb.to_string(),
        stages.rss_after_batches_kb.to_string(),
        stages.rss_after_relation_kb.to_string(),
        stages.rss_final_kb.to_string(),
        stages.rss_peak_batch_kb.to_string(),
        stages.vector_pending_points_max.to_string(),
        stages.bm25_pending_documents_max.to_string(),
        stages.vector_embed_ms.to_string(),
        stages.vector_point_build_ms.to_string(),
        stages.vector_metadata_ms.to_string(),
        stages.vector_work_unit_ms.to_string(),
        stages.vector_microbatches.to_string(),
        stages.vector_chunk_rows.to_string(),
        stages.vector_entity_mappings.to_string(),
        stages.vector_staged_records_max.to_string(),
        stages.vector_flush_ms.to_string(),
        stages.vector_stage_push_ms.to_string(),
        stages.vector_entity_lookup_ms.to_string(),
        stages.vector_delay_ms.to_string(),
        stages.vector_work_unit_queries.to_string(),
        stages.vector_work_unit_query_ms.to_string(),
        stages.relation_spool_entries.to_string(),
        stages.relation_spool_bytes.to_string(),
        stages.symbol_table_entries.to_string(),
        stages.symbol_table_estimated_bytes.to_string(),
        stages.symbol_view_module_peak_entries.to_string(),
        stages.symbol_view_module_peak_bytes.to_string(),
        stages.symbol_view_name_peak_entries.to_string(),
        stages.symbol_view_name_peak_bytes.to_string(),
        stages.symbol_view_queries.to_string(),
        stages.symbol_view_candidate_truncated.to_string(),
        stages.cross_file_candidate_truncated.to_string(),
        stages.relation_nodes.to_string(),
        stages.relation_edges.to_string(),
        stages.relation_estimated_bytes.to_string(),
        stages.relation_snapshot_entries.to_string(),
        stages.relation_snapshot_estimated_bytes.to_string(),
        stages.canonical_write_batches.to_string(),
        stages.canonical_write_buffer_peak_bytes.to_string(),
        stages.relation_shard_count.to_string(),
        stages.relation_shard_directory_entries.to_string(),
        stages.relation_shard_directory_estimated_bytes.to_string(),
        stages.published_snapshot_entries.to_string(),
        stages.export_spool_entries.to_string(),
        stages.export_spool_bytes.to_string(),
        stages.entity_id_map_entries.to_string(),
        stages.entity_id_map_estimated_bytes.to_string(),
    ])?;

    println!(
        "indexed {indexed_files}/{file_count} files in {wall_ms:.1} ms, {} entities, {} vectors, peak RSS {} KiB",
        entities,
        vectors,
        after.peak_rss_kb.unwrap_or(0)
    );
    workflow.cleanup().await;
    Ok(())
}
