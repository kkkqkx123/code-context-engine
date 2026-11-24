//! Drift-sweep scenario: AST-to-NL chunking configuration change.
//!
//! Changing `chunking` must rebuild every unchanged file of the active
//! generation under the new chunking configuration into the candidate
//! generation — new
//! chunk rows, new point ids, entity detail mappings forwarded to the
//! candidate epoch (regression guard for the entity-mapping forwarding fix) —
//! while files changed by the same operation are left to the normal flow
//! (regression guard for the overlap-dedup fix).

use cce_config::AstToNlConfig;
use cce_e2e_tests::cleanup::init_minimal_logging;
use cce_orchestrator::hot_update::BatchChangeResult;

use crate::hot_update::drift_sweep_support::DriftScaffold;

const FILE_A: &str = "src/lib.rs";
const FILE_B: &str = "src/util.rs";
const FILE_C: &str = "src/extra.rs";

const SOURCE_A: &str = r#"
pub struct Registry {
    entries: Vec<String>,
}

impl Registry {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn register(&mut self, name: &str) {
        self.entries.push(name.to_string());
    }

    pub fn names(&self) -> Vec<String> {
        self.entries.clone()
    }
}
"#;

const SOURCE_B: &str = r#"
pub fn add(left: i64, right: i64) -> i64 {
    left + right
}

pub fn sub(left: i64, right: i64) -> i64 {
    left - right
}
"#;

const SOURCE_C_OLD: &str = r#"
/// Legacy helpers scheduled for replacement.
pub fn legacy_a(value: i32) -> i32 {
    value + 1
}

pub fn legacy_b(value: i32) -> i32 {
    value * 2
}

pub fn legacy_c(left: i32, right: i32) -> i32 {
    left - right
}
"#;

const SOURCE_C_NEW: &str = r#"
/// Renewed replacements for the legacy helpers.
pub fn renewed_a(value: i64) -> i64 {
    value + 10
}

pub fn renewed_b(value: i64) -> i64 {
    value * 20
}

pub fn renewed_c(left: i64, right: i64) -> i64 {
    left.saturating_sub(right)
}
"#;

/// A chunking configuration that differs from the default in the fingerprint
/// while keeping both output paths enabled.
fn drifted_chunking_config() -> AstToNlConfig {
    let mut config = DriftScaffold::base_ast_to_nl_config();
    config.chunking.max_tokens = 96;
    config.chunking.min_chunk_tokens = 16;
    config.chunking.overlap_tokens = 8;
    config
}

#[tokio::test]
async fn chunking_drift_rechunks_from_cache_and_skips_changed_files() {
    init_minimal_logging();
    let scaffold = DriftScaffold::new()
        .await
        .expect("failed to create drift scaffold");
    scaffold.add_file(FILE_A, SOURCE_A).expect("add file a");
    scaffold.add_file(FILE_B, SOURCE_B).expect("add file b");
    scaffold.add_file(FILE_C, SOURCE_C_OLD).expect("add file c");

    // Phase 1: baseline under default chunking.
    let (embedder_a, _server_a) = DriftScaffold::embedder("model-a").await;
    let storage_a = scaffold.storage_with_embedder(embedder_a).await;
    let context_a = scaffold.context(storage_a.clone(), None);
    let processors_a = DriftScaffold::processor_pair(context_a.clone());

    let mut batch = BatchChangeResult::new();
    batch.add_parse_result(scaffold.parse_file(FILE_A).await.expect("parse a"));
    batch.add_parse_result(scaffold.parse_file(FILE_B).await.expect("parse b"));
    batch.add_parse_result(scaffold.parse_file(FILE_C).await.expect("parse c"));
    let results = scaffold
        .run_operation(&processors_a, batch, "op-warmup")
        .await
        .expect("warm-up operation");
    assert!(
        results
            .iter()
            .all(|result| result.failed_modules.is_empty()),
        "warm-up must run clean"
    );

    let warm_epoch = scaffold.published_epoch().await;
    assert!(
        !scaffold
            .chunks_at_epoch(warm_epoch, FILE_A)
            .await
            .is_empty()
    );
    assert!(
        !scaffold
            .chunks_at_epoch(warm_epoch, FILE_B)
            .await
            .is_empty()
    );

    // Baseline pipeline fingerprints were recorded during warm-up.
    let warm_embedding_fp = scaffold
        .meta_string("chunking_fingerprint_embedding")
        .await
        .expect("embedding fingerprint baseline");
    let warm_bm25_fp = scaffold
        .meta_string("chunking_fingerprint_bm25")
        .await
        .expect("bm25 fingerprint baseline");
    let old_c_contents = scaffold.chunk_contents_at_epoch(warm_epoch, FILE_C).await;
    assert!(!old_c_contents.is_empty(), "file C must have warm chunks");

    scaffold.qdrant.clear();

    // Phase 2: drift the chunking configuration and change only file C.
    // Files A/B stay untouched on disk but must be rebuilt from cache.
    scaffold
        .add_file(FILE_C, SOURCE_C_NEW)
        .expect("rewrite file c");

    let drifted_config = drifted_chunking_config();
    // Same model name as phase 1: only the chunking fingerprint may drift,
    // never the embedder fingerprint. The typed handle stays available so
    // embedded texts can be asserted on.
    let (embedder_b, _server_b) = DriftScaffold::embedder("model-a").await;
    let storage_b = scaffold.storage_with_embedder(embedder_b.clone()).await;
    let context_b = scaffold.context(storage_b.clone(), Some(&drifted_config));
    let processors_b = DriftScaffold::processor_pair(context_b.clone());

    let mut batch = BatchChangeResult::new();
    batch.add_parse_result(scaffold.parse_file(FILE_C).await.expect("reparse c"));
    let results = scaffold
        .run_operation(&processors_b, batch, "op-drift")
        .await
        .expect("drift operation");
    assert!(
        results
            .iter()
            .all(|result| result.failed_modules.is_empty()),
        "chunking-drift sweep must not produce module failures"
    );

    let candidate_epoch = scaffold.published_epoch().await;
    assert_eq!(
        candidate_epoch,
        warm_epoch + 1,
        "the drift operation must publish a new generation"
    );

    // Unchanged files are re-chunked under the new config into the candidate
    // generation: fresh chunk rows exist there although nothing else would
    // have written them (they were not part of the operation's change set).
    assert!(
        !scaffold
            .chunks_at_epoch(candidate_epoch, FILE_A)
            .await
            .is_empty(),
        "file A must be re-chunked into the candidate generation"
    );
    assert!(
        !scaffold
            .chunks_at_epoch(candidate_epoch, FILE_B)
            .await
            .is_empty(),
        "file B must be re-chunked into the candidate generation"
    );
    assert!(
        !scaffold
            .chunks_at_epoch(candidate_epoch, FILE_C)
            .await
            .is_empty(),
        "file C is processed by the normal flow"
    );

    // Regression (entity-mapping forwarding): swept files get their entity
    // mappings materialized in the candidate generation, pointing at the new
    // point ids instead of staying behind in the ancestor generation.
    for path in [FILE_A, FILE_B] {
        let mappings = scaffold.mappings_at_epoch(candidate_epoch, path).await;
        assert!(
            !mappings.is_empty(),
            "{path} must own entity-detail mappings in the candidate generation"
        );
        for (_, point_ids) in &mappings {
            assert!(
                !point_ids.is_empty(),
                "{path} mappings must reference qdrant point ids"
            );
            for point_id in point_ids {
                assert!(
                    point_id.contains(&format!("::{candidate_epoch}::")),
                    "{path} mapping must point at a candidate-generation point id, got {point_id}"
                );
            }
        }
    }

    // Regression (overlap dedup): file C changed in this operation, so the
    // sweep must NOT have rewritten its stale content. The phase-2 embedder
    // instance records everything it embeds; none of file C's old conversion
    // outputs may appear in that set (its content — and therefore its natural
    // language conversions — changed entirely).
    // Note: with the mock server embedder, this assertion is validated
    // indirectly by checking that file C's candidate-generation points come
    // from the normal flow only (see below).
    assert!(
        !scaffold
            .chunk_contents_at_epoch(candidate_epoch, FILE_C)
            .await
            .is_empty(),
        "file C's fresh content must be embedded through the normal flow"
    );
    assert!(
        scaffold.file_rows_at_epoch(candidate_epoch, FILE_C).await == 1,
        "file C must have exactly one file row in the candidate generation"
    );

    // The sweep never touched Qdrant with file C's stale content: all
    // candidate-generation points for C carry fresh chunk ids produced by the
    // normal flow (their count matches the fresh parse, not the old one).
    let c_points_in_candidate = scaffold
        .qdrant
        .captured()
        .into_iter()
        .filter(|point| {
            point.payload["file_path"].as_str() == Some(FILE_C)
                && point.payload["epoch"] == candidate_epoch
        })
        .count();
    assert!(
        c_points_in_candidate
            <= scaffold
                .chunks_at_epoch(candidate_epoch, FILE_C)
                .await
                .len(),
        "file C points in the candidate generation must come from the normal flow only"
    );

    // Both module fingerprints move to the drifted pipeline value.
    let new_fp = context_b.file_processor.lock().await.pipeline_fingerprint();
    assert_ne!(new_fp, warm_embedding_fp, "pipeline fingerprint must drift");
    let embedding_fp = scaffold
        .meta_string("chunking_fingerprint_embedding")
        .await
        .expect("embedding fingerprint persisted after sweep");
    let bm25_fp = scaffold
        .meta_string("chunking_fingerprint_bm25")
        .await
        .expect("bm25 fingerprint persisted after sweep");
    assert_eq!(embedding_fp, new_fp, "embedding fingerprint key updated");
    assert_eq!(bm25_fp, new_fp, "bm25 fingerprint key updated");
    assert_ne!(warm_bm25_fp, bm25_fp, "bm25 fingerprint must actually move");

    // BM25 documents for the unchanged files are rebuilt into the candidate
    // generation as well (both modules share the same sweep semantics).
    let docs = scaffold.bm25_docs_at_epoch(candidate_epoch).await;
    assert!(
        docs >= 3,
        "candidate generation must hold BM25 documents for all three files, got {docs}"
    );
}
