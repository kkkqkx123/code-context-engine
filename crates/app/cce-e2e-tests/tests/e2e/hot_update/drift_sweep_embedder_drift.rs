//! Drift-sweep scenario: embedder model change.
//!
//! Replacing the embedder (different model fingerprint) must regenerate every
//! vector of the active generation in place — same point ids, payloads rebuilt
//! from persisted records, refreshed summary vectors, and the new fingerprint
//! persisted — without re-chunking or re-parsing anything.

use std::collections::HashMap;

use cce_e2e_tests::cleanup::init_minimal_logging;
use cce_orchestrator::hot_update::BatchChangeResult;

use crate::hot_update::drift_sweep_support::DriftScaffold;

const FILE_A: &str = "src/lib.rs";
const FILE_B: &str = "src/util.rs";

const SOURCE_A: &str = r#"
pub struct Counter {
    value: i64,
}

impl Counter {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn increment(&mut self) {
        self.value += 1;
    }

    pub fn value(&self) -> i64 {
        self.value
    }
}
"#;

const SOURCE_B: &str = r#"
pub fn clamp(value: i32, low: i32, high: i32) -> i32 {
    if value < low {
        return low;
    }
    if value > high {
        return high;
    }
    value
}

pub fn scale(value: f64, factor: f64) -> f64 {
    value * factor
}
"#;

/// Vectors keyed by captured UUID point id.
fn vectors_by_id(
    points: &[cce_e2e_tests::mock_qdrant::CapturedPoint],
) -> HashMap<String, Vec<f32>> {
    points
        .iter()
        .map(|point| (point.point_id.clone(), point.vector.clone()))
        .collect()
}

#[tokio::test]
async fn embedder_drift_reembeds_active_generation_in_place() {
    init_minimal_logging();
    let scaffold = DriftScaffold::new()
        .await
        .expect("failed to create drift scaffold");
    scaffold.add_file(FILE_A, SOURCE_A).expect("add file a");
    scaffold.add_file(FILE_B, SOURCE_B).expect("add file b");

    // Phase 1: normal processing under "model-a" publishes the baseline.
    let (embedder_a, _server_a) = DriftScaffold::embedder("model-a").await;
    let storage_a = scaffold.storage_with_embedder(embedder_a).await;
    let context_a = scaffold.context(storage_a.clone(), None);
    let processors_a = DriftScaffold::processor_pair(context_a);

    let mut batch = BatchChangeResult::new();
    batch.add_parse_result(scaffold.parse_file(FILE_A).await.expect("parse a"));
    batch.add_parse_result(scaffold.parse_file(FILE_B).await.expect("parse b"));
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
    assert!(warm_epoch >= 1, "warm-up must publish a generation");

    let warm_points = scaffold.qdrant.captured();
    assert!(
        !warm_points.is_empty(),
        "warm-up must upsert vectors into Qdrant"
    );
    let warm_ids = scaffold.qdrant.captured_point_ids();
    let warm_vectors = vectors_by_id(&warm_points);

    // The first operation only records the embedder fingerprint as baseline.
    let warm_fp = scaffold
        .meta_string("embedding_model_fingerprint")
        .await
        .expect("baseline fingerprint recorded");

    scaffold.qdrant.clear();

    // Phase 2: switch to "model-b" and run an operation with no file changes.
    let (embedder_b, _server_b) = DriftScaffold::embedder("model-b").await;
    let storage_b = scaffold.storage_with_embedder(embedder_b).await;
    let context_b = scaffold.context(storage_b.clone(), None);
    let processors_b = DriftScaffold::processor_pair(context_b);

    let results = scaffold
        .run_operation(&processors_b, BatchChangeResult::new(), "op-drift")
        .await
        .expect("drift operation");
    assert!(
        results
            .iter()
            .all(|result| result.failed_modules.is_empty()),
        "drift sweep must not produce module failures"
    );

    // Every active-generation chunk is rewritten under its original point id.
    let reembedded = scaffold.qdrant.captured();
    assert!(!reembedded.is_empty(), "sweep must rewrite all vectors");
    let re_ids = scaffold.qdrant.captured_point_ids();
    assert_eq!(
        warm_ids, re_ids,
        "point id set must be identical before and after the sweep"
    );

    // Vector values are byte-exactly different (deterministic stub output),
    // proving actual regeneration rather than a no-op replay.
    let re_vectors = vectors_by_id(&reembedded);
    for (id, old_vector) in &warm_vectors {
        let new_vector = re_vectors
            .get(id)
            .unwrap_or_else(|| panic!("point {id} missing after sweep"));
        assert_ne!(
            old_vector, new_vector,
            "vector of point {id} must change under the new model"
        );
    }

    // Payloads are rebuilt from persisted records with epoch semantics intact.
    for point in &reembedded {
        assert_eq!(
            point.payload["group_id"],
            DriftScaffold::GROUP_ID,
            "payload group id must be preserved"
        );
        assert_eq!(
            point.payload["epoch"], warm_epoch,
            "regenerated points stay attached to the generation that owns them"
        );
        assert!(
            !point.payload["source_id"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "source_id must be rebuilt from the chunk record"
        );
    }

    // The new embedder fingerprint is persisted after a successful sweep.
    let drift_fp = scaffold
        .meta_string("embedding_model_fingerprint")
        .await
        .expect("fingerprint persisted");
    assert_ne!(drift_fp, warm_fp, "fingerprint must move to model-b");

    // A follow-up operation under the same model must be a no-op: the stored
    // fingerprint matches, so no regeneration sweep runs.
    scaffold.qdrant.clear();
    let results = scaffold
        .run_operation(&processors_b, BatchChangeResult::new(), "op-stable")
        .await
        .expect("stable operation");
    assert!(
        results
            .iter()
            .all(|result| result.failed_modules.is_empty()),
        "stable operation must run clean"
    );
    assert!(
        scaffold.qdrant.captured().is_empty(),
        "no vectors must be rewritten when the embedder fingerprint is unchanged"
    );
}
