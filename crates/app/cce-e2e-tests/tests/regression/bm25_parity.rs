//! Offline differential parity test: the in-memory benchmark scorer
//! (`cce_e2e_tests::infra::score_all`) must reproduce the production tantivy
//! BM25 retrieval (`Bm25Retrieval::search`) ranking and scores exactly.
//!
//! This is the BM25 drift guard: benchmark numbers are only meaningful when
//! the offline scorer mirrors the production query builder (whole/split token
//! expansion, split-token down-weighting, field boosts), the production
//! tokenizer (MixedTokenizer), and tantivy's BM25 statistics semantics
//! (global doc count for idf, average fieldnorm over all docs, fieldnorm
//! quantization).
//!
//! Requirements for exactness (mirrored by the scorer):
//! - All docs carry all three fields (empty-field norms differ between the
//!   scorer and tantivy segment statistics by design).
//! - Single commit => single segment (one average fieldnorm per field).
//! - f32 vs f64: compare with a tolerance.
//!
//! Offline: no embedding API, no network, no Qdrant — only a temp-dir tantivy
//! index.

use cce_storage_common::FulltextSearchOptions;
use std::collections::HashMap;

use cce_e2e_tests::bench_data::production_bm25_config;
use cce_e2e_tests::infra::{build_term_index, expand_query, score_all};
use cce_storage_bm25::{
    Bm25AlgorithmConfig, Bm25Retrieval, IndexManager, IndexManagerConfig, TermOperator,
    batch_add_documents,
};
use tempfile::tempdir;

fn corpus() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    // (document_id, title, keywords, content)
    vec![
        (
            "d1",
            "OnceCell::get_or_init",
            "oncecell get or init",
            "this function initializes the cell with the closure value once and returns the result to the caller without any synchronization overhead",
        ),
        (
            "d2",
            "get_or_init",
            "get_or_init",
            "this function initializes the cell with the closure value once and returns the result to the caller without any synchronization overhead",
        ),
        (
            "d3",
            "read_file",
            "read file",
            "read_file reads the file content from disk and returns the bytes to the caller",
        ),
        (
            "d4",
            "parseQuery",
            "parse query",
            "parseQuery splits the input text into tokens and returns a typed representation",
        ),
        (
            "d5",
            "中文函数",
            "中文 函数",
            "这个函数用于处理数据并返回结果，调用者无需关心内部实现细节",
        ),
        (
            "d6",
            "alpha_manager",
            "alpha manager",
            "alpha does the bookkeeping for every alpha record in the system and the alpha handler dispatches each alpha event to the alpha worker which then persists the alpha state into the alpha store and notifies the alpha observer about the alpha change while the alpha logger records every alpha transition and the alpha config controls the alpha policy and the alpha monitor watches the alpha health and the alpha report summarizes the alpha metrics and the alpha queue buffers the alpha tasks and the alpha scheduler prioritizes the alpha jobs and the alpha cache accelerates the alpha lookups and the alpha index organizes the alpha data and the alpha backup protects the alpha archive",
        ),
        (
            "d7",
            "gamma_worker",
            "gamma worker",
            "the gamma worker consumes gamma jobs from the gamma queue and executes the gamma handler for each gamma task",
        ),
        (
            "d8",
            "beta_helper",
            "beta helper",
            "beta_helper implements the beta protocol that returns the beta payload with parameters and defined in file beta.rs",
        ),
    ]
}

fn queries() -> Vec<(&'static str, TermOperator)> {
    vec![
        ("OnceCell::get_or_init", TermOperator::Or),
        ("get_or_init", TermOperator::Or),
        ("read_file function", TermOperator::Or),
        ("parseQuery", TermOperator::Or),
        ("中文函数", TermOperator::Or),
        ("that returns the value", TermOperator::Or),
        ("alpha manager", TermOperator::Or),
        ("gamma worker", TermOperator::Or),
        ("alpha manager", TermOperator::And),
        ("get_or_init that returns", TermOperator::And),
    ]
}

fn search_options(operator: TermOperator, limit: usize) -> FulltextSearchOptions {
    let mut field_weights = HashMap::new();
    field_weights.insert("title".to_string(), 2.0);
    field_weights.insert("content".to_string(), 1.0);
    field_weights.insert("keywords".to_string(), 2.0);
    FulltextSearchOptions {
        limit,
        offset: 0,
        field_weights,
        project_id: 1,
        epochs: Vec::new(),
        excluded_files: None,
        exclude_test: false,
        exclude_generated: false,
        exclude_dependency: false,
        include_categories: vec![],
        exclude_categories: vec![],
        term_operator: operator,
    }
}

#[test]
fn offline_scorer_matches_production_tantivy_bm25() {
    let corpus = corpus();

    // Production side: real tantivy index in a temp dir.
    let dir = tempdir().expect("tempdir for tantivy index");
    let manager = IndexManager::create_with_config(
        dir.path(),
        IndexManagerConfig::default(),
        Bm25AlgorithmConfig::default(),
    )
    .expect("index manager creation");
    let schema = manager.schema();

    let documents: Vec<(String, HashMap<String, String>)> = corpus
        .iter()
        .map(|(id, title, keywords, content)| {
            let mut fields = HashMap::new();
            fields.insert("title".to_string(), title.to_string());
            fields.insert("keywords".to_string(), keywords.to_string());
            fields.insert("content".to_string(), content.to_string());
            // Production search always enforces the project_id isolation
            // filter; indexed docs must carry it or nothing matches.
            fields.insert("project_id".to_string(), "1".to_string());
            (id.to_string(), fields)
        })
        .collect();
    batch_add_documents(&manager, schema, documents).expect("batch index");
    manager.reload_reader().expect("reader reload after commit");

    // Offline side: in-memory scorer over the same documents.
    let bm25_docs: Vec<cce_storage_common::FulltextDocument> = corpus
        .iter()
        .map(|(id, title, keywords, content)| {
            cce_storage_common::FulltextDocument::new(*id)
                .with_field("title", *title)
                .with_field("keywords", *keywords)
                .with_field("content", *content)
        })
        .collect();
    let index = build_term_index(&bm25_docs);
    let config = production_bm25_config();
    let retrieval = Bm25Retrieval::new();
    let limit = corpus.len();

    for (query, operator) in queries() {
        // Production ranking.
        let results = retrieval
            .search(&manager, schema, query, &search_options(operator, limit))
            .expect("production search");
        let production: Vec<(String, f32)> = results
            .into_iter()
            .map(|r| (r.document_id.clone(), r.score))
            .collect();

        // Offline ranking.
        let forms = expand_query(query, &index);
        let ranked = score_all(&index, &[forms], &config, operator, limit);
        let ranked = &ranked[0];
        let offline: Vec<(String, f64)> = ranked
            .iter()
            .filter(|(_, s)| *s > 0.0)
            .map(|(doc_idx, s)| (bm25_docs[*doc_idx].document_id.clone(), *s))
            .collect();

        // The scored document sets must coincide.
        let prod_ids: std::collections::HashSet<&str> =
            production.iter().map(|(id, _)| id.as_str()).collect();
        let offline_ids: std::collections::HashSet<&str> =
            offline.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            prod_ids, offline_ids,
            "query {query:?} ({operator:?}): scored doc sets diverge",
        );

        // Scores must agree within f32 precision.
        for (prod_id, prod_score) in &production {
            let (_, offline_score) = offline
                .iter()
                .find(|(id, _)| id == prod_id)
                .unwrap_or_else(|| panic!("query {query:?}: missing offline score for {prod_id}"));
            let diff = (*prod_score as f64 - offline_score).abs();
            let tol = 1e-3 * (*prod_score as f64).abs().max(1.0);
            assert!(
                diff <= tol,
                "query {query:?} ({operator:?}): doc {prod_id} score drift: \
                 production={prod_score} offline={offline_score}",
            );
        }

        // Ranking order must agree for distinct score values. Exact ties are
        // broken arbitrarily by tantivy's collector heap, so group scores
        // into 1e-4 buckets and compare the (bucket desc, doc id) signature.
        let mut prod_order: Vec<(i64, String)> = production
            .iter()
            .map(|(id, s)| ((*s as f64 * 1e4).round() as i64, id.clone()))
            .collect();
        prod_order.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut offline_order: Vec<(i64, String)> = offline
            .iter()
            .map(|(id, s)| ((s * 1e4).round() as i64, id.clone()))
            .collect();
        offline_order.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        assert_eq!(
            prod_order, offline_order,
            "query {query:?} ({operator:?}): ranking order diverges",
        );
    }
}
