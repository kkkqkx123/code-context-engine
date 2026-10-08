//! Dual-backend vector storage contract tests.
//!
//! The same case suite runs against both backends so generation, group,
//! type, directory, test-code and category semantics stay identical:
//! the local backend always runs (embedded), the Qdrant branch runs when
//! `CCE_TEST_QDRANT_URL` points at a live service and skips otherwise.
//! Switching backends requires a reindex, so the suite only locks
//! per-group behavior, never cross-run global counts.

use cce_storage_common::{DenseSearchQuery, Payload, SearchFilter, VectorPoint, VectorStorage};
use cce_types::{FileCategory, PointKind};

const DIM: usize = 4;

#[allow(clippy::too_many_arguments)]
fn point(
    id: &str,
    vector: Vec<f32>,
    file: &str,
    group: &str,
    epoch: i64,
    kind: PointKind,
    category: FileCategory,
    test: bool,
) -> VectorPoint {
    VectorPoint::new(
        id.to_string(),
        vector,
        Payload::new(file)
            .with_source_id(id.to_string())
            .with_group_id(group)
            .with_type(kind)
            .with_category(category)
            .with_epoch(epoch)
            .with_test(test),
    )
}

fn code_point(id: &str, vector: Vec<f32>, file: &str, group: &str, epoch: i64) -> VectorPoint {
    point(
        id,
        vector,
        file,
        group,
        epoch,
        PointKind::Chunk,
        FileCategory::Code,
        false,
    )
}

/// Shared contract suite: identical assertions for every backend.
async fn run_contract_suite(store: &impl VectorStorage, tag: &str) {
    store.ensure_collection().await.expect("ensure collection");
    assert!(store.collection_exists().await.expect("exists"));
    assert!(store.health().await.expect("health"));

    // Upsert plus similarity roundtrip: the nearest point wins.
    let group = format!("{tag}-roundtrip");
    store
        .upsert_points(&[
            code_point("a", vec![1.0, 0.0, 0.0, 0.0], "src/a.rs", &group, 1),
            code_point("b", vec![0.0, 1.0, 0.0, 0.0], "src/b.rs", &group, 1),
            code_point("c", vec![0.0, 0.0, 1.0, 0.0], "src/c.rs", &group, 1),
        ])
        .await
        .expect("upsert");
    let hits = store
        .search_dense(
            DenseSearchQuery::new(vec![1.0, 0.0, 0.0, 0.0], 10).with_filter(SearchFilter {
                group_id: Some(group.clone()),
                ..Default::default()
            }),
        )
        .await
        .expect("search");
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].id, "a");

    // Group isolation: another group's points stay invisible.
    let other = format!("{tag}-other");
    store
        .upsert_points(&[code_point(
            "x",
            vec![1.0, 0.0, 0.0, 0.0],
            "src/x.rs",
            &other,
            1,
        )])
        .await
        .expect("upsert other group");
    let hits = store
        .search_dense(
            DenseSearchQuery::new(vec![1.0, 0.0, 0.0, 0.0], 10).with_filter(SearchFilter {
                group_id: Some(group.clone()),
                ..Default::default()
            }),
        )
        .await
        .expect("group-scoped search");
    assert!(
        hits.iter()
            .all(|h| h.payload.group_id.as_deref() == Some(group.as_str()))
    );
    assert_eq!(store.count_points_by_group(&group).await.expect("count"), 3);

    // Generation exclusion: the parent row of an overridden file is hidden.
    let generation = format!("{tag}-generation");
    store
        .upsert_points(&[
            code_point(
                "parent",
                vec![1.0, 0.0, 0.0, 0.0],
                "src/a.rs",
                &generation,
                4,
            ),
            code_point("own", vec![1.0, 0.1, 0.0, 0.0], "src/a.rs", &generation, 5),
        ])
        .await
        .expect("upsert generations");
    let hits = store
        .search_dense(
            DenseSearchQuery::new(vec![1.0, 0.0, 0.0, 0.0], 10).with_filter(SearchFilter {
                group_id: Some(generation.clone()),
                epochs: vec![4, 5],
                excluded_files: Some(vec!["src/a.rs".to_string()]),
                ..Default::default()
            }),
        )
        .await
        .expect("generation search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "own");

    // Point type filter.
    let types = format!("{tag}-types");
    store
        .upsert_points(&[
            code_point("chunk-1", vec![1.0, 0.0, 0.0, 0.0], "src/a.rs", &types, 1),
            point(
                "summary-1",
                vec![1.0, 0.05, 0.0, 0.0],
                "src/a.rs",
                &types,
                1,
                PointKind::Summary,
                FileCategory::Code,
                false,
            ),
        ])
        .await
        .expect("upsert types");
    let hits = store
        .search_dense(
            DenseSearchQuery::new(vec![1.0, 0.0, 0.0, 0.0], 10).with_filter(SearchFilter {
                group_id: Some(types.clone()),
                point_type: Some(PointKind::Summary),
                ..Default::default()
            }),
        )
        .await
        .expect("type search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "summary-1");

    // Directory prefix plus test-code exclusion plus categories.
    let docs = format!("{tag}-docs");
    store
        .upsert_points(&[
            code_point("code-1", vec![1.0, 0.0, 0.0, 0.0], "src/lib/a.rs", &docs, 1),
            point(
                "test-1",
                vec![1.0, 0.05, 0.0, 0.0],
                "src/lib/a_test.rs",
                &docs,
                1,
                PointKind::Chunk,
                FileCategory::Code,
                true,
            ),
            point(
                "doc-1",
                vec![1.0, 0.02, 0.0, 0.0],
                "docs/b.md",
                &docs,
                1,
                PointKind::Chunk,
                FileCategory::Documentation,
                false,
            ),
        ])
        .await
        .expect("upsert docs");
    let hits = store
        .search_dense(
            DenseSearchQuery::new(vec![1.0, 0.0, 0.0, 0.0], 10).with_filter(SearchFilter {
                group_id: Some(docs.clone()),
                directory_prefix: Some("src".to_string()),
                exclude_test: true,
                ..Default::default()
            }),
        )
        .await
        .expect("directory search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "code-1");
    let hits = store
        .search_dense(
            DenseSearchQuery::new(vec![1.0, 0.0, 0.0, 0.0], 10).with_filter(SearchFilter {
                group_id: Some(docs.clone()),
                include_categories: Some(vec![FileCategory::Documentation]),
                ..Default::default()
            }),
        )
        .await
        .expect("category search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "doc-1");

    // Scoped file delete then group delete, counts follow along.
    store
        .delete_by_file_path_scoped("src/a.rs", &group, None)
        .await
        .expect("delete file");
    assert_eq!(store.count_points_by_group(&group).await.expect("count"), 2);
    for cleanup in [&group, &other, &generation, &types, &docs] {
        store.delete_by_group(cleanup).await.expect("delete group");
        assert_eq!(
            store.count_points_by_group(cleanup).await.expect("count"),
            0,
            "group {cleanup} must be empty after delete"
        );
    }

    // Dimension mismatches are rejected, never stored silently.
    let bad = format!("{tag}-bad");
    let err = store
        .upsert_points(&[code_point("bad", vec![1.0], "src/bad.rs", &bad, 1)])
        .await
        .expect_err("wrong-dimension upsert must fail");
    assert!(
        err.to_string().contains("dimension"),
        "unexpected error: {err}"
    );
    let err = store
        .search_dense(
            DenseSearchQuery::new(vec![1.0], 10).with_filter(SearchFilter {
                group_id: Some(bad.clone()),
                ..Default::default()
            }),
        )
        .await
        .expect_err("wrong-dimension search must fail");
    assert!(
        err.to_string().contains("dimension"),
        "unexpected error: {err}"
    );
    assert_eq!(store.count_points_by_group(&bad).await.expect("count"), 0);
}

#[tokio::test]
async fn local_backend_contract() {
    use cce_config::modules::{DistanceMetric, LocalVectorConfig};
    let dir = tempfile::tempdir().expect("tempdir");
    let config = LocalVectorConfig {
        data_dir: None,
        vector_size: DIM,
        distance_metric: DistanceMetric::Cosine,
        hnsw_m: None,
        hnsw_ef_construct: None,
        hnsw_ef_search: None,
        full_scan_threshold: None,
    };
    let store = cce_storage_vector_local::LocalVectorStore::open_at(dir.path(), &config)
        .expect("open store");
    run_contract_suite(&store, "local").await;
}

#[tokio::test]
async fn qdrant_backend_contract() {
    let Some(url) = std::env::var("CCE_TEST_QDRANT_URL")
        .ok()
        .filter(|v| !v.is_empty())
    else {
        eprintln!("skipping qdrant contract: CCE_TEST_QDRANT_URL is not set");
        return;
    };
    use cce_config::modules::{DistanceMetric, QdrantConfig};
    use cce_storage_vector_qdrant::QdrantClient;
    let config = QdrantConfig {
        url,
        vector_size: DIM,
        distance_metric: DistanceMetric::Cosine,
        timeout_ms: 5000,
        max_retries: 0,
        retry_delay_ms: 10,
        enabled: true,
        ..Default::default()
    };
    // Unique workspace isolates the collection from any developer data.
    let workspace = format!(
        "vector-contract-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    let client = QdrantClient::new(config, &workspace).expect("qdrant client must build");
    run_contract_suite(&client, "qdrant").await;
    client
        .delete_collection()
        .await
        .expect("cleanup collection");
}
