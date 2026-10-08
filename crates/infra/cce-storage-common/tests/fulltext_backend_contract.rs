//! Dual-backend fulltext storage contract tests.
//!
//! The same case suite runs against both backends so batch idempotency,
//! project isolation, generation exclusion, test/category filtering, scoped
//! deletes, snapshot readback, and counts stay identical: the local backend
//! always runs (embedded), the search-service branch runs when
//! `CCE_TEST_ES_URL` points at a live service and skips otherwise. Each run
//! starts from `clear_index`, so no cross-run state leaks. Single-term
//! queries only: phrase behavior is a documented cross-branch approximation
//! and stays out of this suite.

use std::collections::HashMap;

use cce_storage_common::{FulltextDocument, FulltextSearchOptions, FulltextStorage, TermOperator};
use cce_types::FileCategory;

const INDEX: &str = "contract";

fn doc(
    id: &str,
    project_id: i64,
    epoch: i64,
    title: &str,
    file_path: &str,
    test: bool,
    category: FileCategory,
) -> FulltextDocument {
    FulltextDocument::new(id)
        .with_field("chunk_id", id)
        .with_field("title", title)
        .with_field("content", title)
        .with_field("keywords", title)
        .with_field("file_path", file_path)
        .with_field("project_id", project_id.to_string())
        .with_field("epoch", epoch.to_string())
        .with_field("test", if test { "1" } else { "0" }.to_string())
        .with_field("category", (category.as_u8()).to_string())
}

fn options(project_id: i64, epochs: Vec<i64>) -> FulltextSearchOptions {
    FulltextSearchOptions {
        limit: 10,
        offset: 0,
        field_weights: HashMap::new(),
        project_id,
        epochs,
        excluded_files: None,
        exclude_test: false,
        include_categories: vec![],
        exclude_categories: vec![],
        term_operator: TermOperator::Or,
    }
}

/// Shared contract suite: identical assertions for every backend.
async fn run_contract_suite(store: &impl FulltextStorage, tag: &str, index: &str) {
    store.clear_index(index).await.expect("clear index");
    assert_eq!(store.document_count().await.expect("count"), 0);

    // Batch write plus idempotent replay.
    let group = format!("{tag}-roundtrip");
    let docs = vec![
        doc(
            &format!("{group}-a"),
            7,
            1,
            "zephyrwind alpha calculator",
            "src/a.rs",
            false,
            FileCategory::Code,
        ),
        doc(
            &format!("{group}-b"),
            7,
            1,
            "zephyrwind beta calculator",
            "src/b.rs",
            false,
            FileCategory::Code,
        ),
        doc(
            &format!("{group}-c"),
            7,
            1,
            "quartz gamma renderer",
            "src/c.rs",
            false,
            FileCategory::Code,
        ),
    ];
    assert_eq!(store.batch_index(index, &docs).await.expect("index"), 3);
    store.flush().await.expect("flush");
    assert_eq!(store.batch_index(index, &docs).await.expect("replay"), 3);
    store.flush().await.expect("flush");
    assert_eq!(store.document_count().await.expect("count"), 3);

    let hits = store
        .search("zephyrwind", &options(7, vec![1]))
        .await
        .expect("search");
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| h.fields.get("file_path").is_some()));

    // Project isolation: another project's documents stay invisible, and a
    // project that was never indexed reads empty.
    let other = doc(
        &format!("{tag}-other"),
        8,
        1,
        "zephyrwind delta calculator",
        "src/d.rs",
        false,
        FileCategory::Code,
    );
    store
        .batch_index(index, &[other])
        .await
        .expect("index other");
    store.flush().await.expect("flush");
    let hits = store
        .search("zephyrwind", &options(7, vec![1]))
        .await
        .expect("scoped search");
    assert_eq!(hits.len(), 2);
    assert_eq!(
        store
            .document_count_by_project(7)
            .await
            .expect("project count"),
        3
    );
    assert_eq!(
        store
            .document_count_by_project(9)
            .await
            .expect("missing project count"),
        0
    );
    let hits = store
        .search("zephyrwind", &options(9, vec![1]))
        .await
        .expect("missing project search");
    assert!(hits.is_empty());

    // Generation exclusion: the parent row of an overridden file is hidden.
    let generation = format!("{tag}-generation");
    store
        .batch_index(
            index,
            &[
                doc(
                    &format!("{generation}-parent"),
                    7,
                    4,
                    "zephyrwind parent ledger",
                    "src/ledger.rs",
                    false,
                    FileCategory::Code,
                ),
                doc(
                    &format!("{generation}-own"),
                    7,
                    5,
                    "zephyrwind own ledger",
                    "src/ledger.rs",
                    false,
                    FileCategory::Code,
                ),
            ],
        )
        .await
        .expect("index generations");
    store.flush().await.expect("flush");
    let mut generation_options = options(7, vec![4, 5]);
    generation_options.excluded_files = Some(vec!["src/ledger.rs".to_string()]);
    let hits = store
        .search("zephyrwind", &generation_options)
        .await
        .expect("generation search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].document_id, format!("{generation}-own"));

    // Test-code exclusion plus category filtering.
    let flags = format!("{tag}-flags");
    store
        .batch_index(
            index,
            &[
                doc(
                    &format!("{flags}-test"),
                    7,
                    1,
                    "zephyrwind test helper",
                    "src/t.rs",
                    true,
                    FileCategory::Code,
                ),
                doc(
                    &format!("{flags}-doc"),
                    7,
                    1,
                    "zephyrwind usage guide",
                    "docs/g.md",
                    false,
                    FileCategory::Documentation,
                ),
            ],
        )
        .await
        .expect("index flags");
    store.flush().await.expect("flush");
    let mut no_test = options(7, vec![1]);
    no_test.exclude_test = true;
    let hits = store
        .search("zephyrwind", &no_test)
        .await
        .expect("exclude test");
    assert!(
        hits.iter()
            .all(|h| h.document_id != format!("{flags}-test")),
        "test document must be hidden"
    );
    let mut docs_only = options(7, vec![1]);
    docs_only.include_categories = vec![FileCategory::Documentation];
    let hits = store
        .search("zephyrwind", &docs_only)
        .await
        .expect("category search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].document_id, format!("{flags}-doc"));

    // Snapshot readback carries the stored fields for an epoch copy.
    let snapshot = store.snapshot_documents(7, 1).await.expect("snapshot");
    assert!(!snapshot.is_empty());
    assert!(
        snapshot
            .iter()
            .all(|d| d.get_field("project_id").map(String::as_str) == Some("7"))
    );
    let sample = snapshot
        .iter()
        .find(|d| d.document_id == format!("{group}-a"))
        .expect("roundtrip document present");
    assert_eq!(
        sample.get_field("title").map(String::as_str),
        Some("zephyrwind alpha calculator")
    );
    assert_eq!(
        sample.get_field("chunk_id").map(String::as_str),
        Some(format!("{group}-a").as_str())
    );

    // Epoch enumeration follows the indexed generations.
    let mut epochs = store.epochs_by_project(7).await.expect("epochs");
    epochs.sort_unstable();
    assert!(epochs.contains(&1), "epoch 1 present: {epochs:?}");
    assert!(epochs.contains(&5), "epoch 5 present: {epochs:?}");

    // Scoped deletes remove exactly their scope; counts follow along.
    let removed = store
        .delete_by_file_path_scoped(index, "src/a.rs", 7)
        .await
        .expect("delete file");
    assert_eq!(removed, 1);
    store.flush().await.expect("flush");
    let hits = store
        .search("zephyrwind", &options(7, vec![1, 4, 5]))
        .await
        .expect("search after file delete");
    assert!(
        hits.iter()
            .all(|h| h.fields.get("file_path").map(String::as_str) != Some("src/a.rs"))
    );
    let removed = store
        .delete_by_file_path_scoped_epoch(index, "src/ledger.rs", 7, 5)
        .await
        .expect("delete file epoch");
    assert_eq!(removed, 1);
    let removed = store
        .delete_by_project_epoch(index, 7, 4)
        .await
        .expect("delete project epoch");
    assert_eq!(removed, 1);
    let removed = store
        .delete_all_project_docs(index, 8)
        .await
        .expect("delete project");
    assert_eq!(removed, 1);
    store.flush().await.expect("flush");
    assert_eq!(
        store
            .document_count_by_project(8)
            .await
            .expect("project count"),
        0
    );
}

#[tokio::test]
async fn local_backend_contract() {
    use cce_config::modules::Bm25Config;
    use cce_storage_bm25::Bm25Client;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = Bm25Config::default()
        .enabled()
        .with_index_name(INDEX)
        .with_index_path(dir.path().to_string_lossy().as_ref());
    let mut client = Bm25Client::new(config);
    client.connect().await.expect("connect");
    run_contract_suite(&client, "local", INDEX).await;
}

#[tokio::test]
async fn remote_backend_contract() {
    let Some(url) = std::env::var("CCE_TEST_ES_URL")
        .ok()
        .filter(|v| !v.is_empty())
    else {
        eprintln!("skipping remote fulltext contract: CCE_TEST_ES_URL is not set");
        return;
    };
    use cce_config::modules::{Bm25Config, FulltextRemoteConfig};
    use cce_storage_bm25::{ElasticsearchClient, ElasticsearchConfig};
    let index =
        std::env::var("CCE_TEST_ES_INDEX").unwrap_or_else(|_| "cce_contract_test".to_string());
    let remote = FulltextRemoteConfig {
        url: Some(url),
        index_name: Some(index.clone()),
        ..FulltextRemoteConfig::default()
    };
    let config =
        ElasticsearchConfig::from_remote(&remote, &Bm25Config::default()).expect("remote config");
    let client = ElasticsearchClient::new(config).expect("remote client must build");
    run_contract_suite(&client, "remote", &index).await;
    client.clear_index(&index).await.expect("cleanup index");
}
