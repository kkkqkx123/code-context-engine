//! Multi-project integration tests
//!
//! Verifies that query cache is properly isolated per project,
//! preventing cross-project cache pollution.

use cce_orchestrator::query::{
    CacheConfig, CacheKey, QueryCache, QueryConfigBuilder, QueryOptions, QueryResult,
};
use cce_storage_qdrant::generate_project_group_id;
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::{ChunkRecord, ChunkRepository};

// ---------------------------------------------------------------------------
// CacheKey project_id isolation
// ---------------------------------------------------------------------------

#[test]
fn test_cache_key_differs_by_project_id() {
    let base_opts = |pid: i64| -> QueryOptions {
        QueryConfigBuilder::new(pid)
            .build("search function")
            .with_limit(10)
    };

    let key_p1 = CacheKey::from_options(&base_opts(1));
    let key_p2 = CacheKey::from_options(&base_opts(2));

    // Same query text and options but different project → different cache key
    assert_ne!(
        key_p1, key_p2,
        "CacheKey must differ when project_id differs"
    );
}

#[tokio::test]
async fn test_query_cache_does_not_return_another_projects_result() {
    let cache = QueryCache::new(CacheConfig::default());
    let project_one = QueryOptions::new("shared query", 1);
    let project_two = QueryOptions::new("shared query", 2);
    let result = QueryResult {
        total: 17,
        ..Default::default()
    };

    let view = cce_orchestrator::query::QueryFilter::new(3).expect("view");
    cache.put_result_for_view(&project_one, &view, result).await;

    assert!(
        cache
            .get_result_for_view(&project_two, &view)
            .await
            .is_none()
    );
    assert_eq!(
        cache
            .get_result_for_view(&project_one, &view)
            .await
            .expect("project one cache entry")
            .total,
        17
    );
}

#[test]
fn test_cache_key_same_project_same_result() {
    let opts = QueryConfigBuilder::new(1)
        .build("search function")
        .with_limit(10);

    let key_a = CacheKey::from_options(&opts);
    let key_b = CacheKey::from_options(&opts);

    assert_eq!(
        key_a, key_b,
        "CacheKey must be identical for identical project + options"
    );
}

#[test]
fn test_cache_key_project_id_field_set() {
    let opts = QueryConfigBuilder::new(42).build("test").with_limit(5);

    let _key = CacheKey::from_options(&opts);

    assert_eq!(opts.project_id, 42, "QueryOptions should carry project_id");
}

#[test]
fn test_cache_key_three_projects_distinct() {
    let keys: Vec<CacheKey> = (1..=3)
        .map(|pid| {
            CacheKey::from_options(
                &QueryConfigBuilder::new(pid)
                    .build("common query")
                    .with_limit(5),
            )
        })
        .collect();

    // All three keys must be distinct
    assert_ne!(keys[0], keys[1]);
    assert_ne!(keys[1], keys[2]);
    assert_ne!(keys[0], keys[2]);
}

#[test]
fn test_cache_key_limit_still_matters_within_project() {
    let make = |pid: i64, limit: usize| -> CacheKey {
        CacheKey::from_options(
            &QueryConfigBuilder::new(pid)
                .build("query")
                .with_limit(limit),
        )
    };

    // Same project, different limits → different keys
    assert_ne!(make(1, 5), make(1, 10));
    // Different projects, same limit → different keys
    assert_ne!(make(1, 5), make(2, 5));
}

#[test]
fn test_cache_key_differs_by_epoch() {
    let options = QueryOptions::new("query", 1);
    assert_ne!(
        CacheKey::from_options_with_view(
            &options,
            &cce_orchestrator::query::QueryFilter::new(1).expect("view")
        ),
        CacheKey::from_options_with_view(
            &options,
            &cce_orchestrator::query::QueryFilter::new(2).expect("view")
        )
    );
}

#[test]
fn test_same_root_has_distinct_vector_namespaces() {
    assert_ne!(
        generate_project_group_id(1, "/workspace/shared"),
        generate_project_group_id(2, "/workspace/shared")
    );
}

#[test]
fn test_same_chunk_id_is_isolated_across_project_lifecycle() {
    let sqlite = SqliteClient::in_memory().expect("Failed to create SQLite database");
    let now = chrono::Utc::now().timestamp();
    sqlite
        .with_transaction(|tx| {
            tx.execute(
                "INSERT INTO projects (id, name, root_path, created_at, updated_at)
                 VALUES (1, 'project-one', '/project-a', ?1, ?1),
                        (2, 'project-two', '/project-b', ?1, ?1)",
                [now],
            )
            .map_err(|error| cce_types::StorageError::Sqlite(error.to_string()))?;

            let first = ChunkRecord::new(
                "same-chunk".to_string(),
                "src/lib.rs".to_string(),
                "project one".to_string(),
                1,
                1,
            )
            .with_project_id(1);
            let second = ChunkRecord::new(
                "same-chunk".to_string(),
                "src/lib.rs".to_string(),
                "project two".to_string(),
                1,
                1,
            )
            .with_project_id(2);
            ChunkRepository::insert(tx, &first)?;
            ChunkRepository::insert(tx, &second)
        })
        .expect("Failed to store project chunks");

    let conn = sqlite
        .write_connection()
        .expect("Failed to get SQLite connection");
    let first = ChunkRepository::get_by_id(&conn, "same-chunk", 1)
        .expect("Failed to read project one")
        .expect("Project one chunk missing");
    let second = ChunkRepository::get_by_id(&conn, "same-chunk", 2)
        .expect("Failed to read project two")
        .expect("Project two chunk missing");
    assert_eq!(first.content, "project one");
    assert_eq!(second.content, "project two");
    drop(conn);

    sqlite
        .with_transaction(|tx| ChunkRepository::delete_by_id(tx, "same-chunk", 1))
        .expect("Failed to delete project one chunk");
    let conn = sqlite
        .write_connection()
        .expect("Failed to get SQLite connection");
    assert!(
        ChunkRepository::get_by_id(&conn, "same-chunk", 1)
            .expect("Failed to query project one")
            .is_none()
    );
    assert!(
        ChunkRepository::get_by_id(&conn, "same-chunk", 2)
            .expect("Failed to query project two")
            .is_some()
    );
}

// ---------------------------------------------------------------------------
// Multi-project via QueryOptions builder
// ---------------------------------------------------------------------------

#[test]
fn test_query_options_project_id_roundtrip() {
    let opts = QueryOptions::new("test query", 7).with_limit(3);

    assert_eq!(opts.project_id, 7);
    assert_eq!(opts.query, "test query");
    assert_eq!(opts.config.result.limit, 3);
}

#[test]
fn test_query_config_builder_project_id_preserved() {
    let opts = QueryConfigBuilder::new(99).build("find me");

    assert_eq!(opts.project_id, 99);
    assert_eq!(opts.query, "find me");
}

// ---------------------------------------------------------------------------
// AggregatedQueryOptions also carries project_id
// ---------------------------------------------------------------------------

#[test]
fn test_aggregated_query_options_project_id() {
    use cce_orchestrator::query::AggregatedQueryOptions;

    let agg = AggregatedQueryOptions {
        project_id: 5,
        ..Default::default()
    };

    assert_eq!(agg.project_id, 5);
}
