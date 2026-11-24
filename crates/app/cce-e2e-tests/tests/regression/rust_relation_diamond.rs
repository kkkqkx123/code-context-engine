//! Assertion tests for diamond dependency call graph.
//!
//! This test verifies that a diamond-shaped cross-file call graph
//! (main -> service_a -> repository, main -> service_b -> repository)
//! is correctly indexed and can be traversed in both directions.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps, RelationQueryOps};
use cce_codegraph::{BuildConfigParser, CallChainQuery, UntypedDependency};
use cce_orchestrator::{IndexOptions, IndexOrchestrator};
use cce_types::language::Language;

use crate::helper::{TestFixture, init_minimal_logging};

fn find_entity_id(
    index: &cce_codegraph::RelationIndex,
    name: &str,
    file_suffix: &str,
) -> cce_types::EntityId {
    let mut candidates = index.get_function_ids_by_name(name);
    candidates.sort_by_key(|entity_id| {
        index
            .get_file_path_by_entity(*entity_id)
            .unwrap_or_default()
    });

    candidates
        .into_iter()
        .find(|entity_id| {
            index
                .get_file_path_by_entity(*entity_id)
                .as_deref()
                .is_some_and(|path| path.ends_with(file_suffix))
        })
        .unwrap_or_else(|| {
            let mut available: Vec<String> = index
                .function_index()
                .iter()
                .map(|entry| {
                    let fp = index
                        .get_file_path_by_entity(*entry.key())
                        .unwrap_or_default();
                    format!("{} @ {}", entry.value().name, fp)
                })
                .collect();
            available.sort();
            panic!(
                "Entity `{}` not found in file `{}`; available: {:?}",
                name, file_suffix, available
            )
        })
}

#[tokio::test]
async fn test_rust_relation_diamond_presentation() {
    init_minimal_logging();

    let fixture =
        TestFixture::rust_relation_diamond().expect("Failed to load relation_diamond fixture");
    let project_root = fixture.root_path().to_path_buf();

    let mut config_parser = BuildConfigParser::new();
    config_parser
        .scan_project(&project_root, 0)
        .expect("Build config scan should succeed");
    assert!(
        config_parser
            .dependencies_for_language(Language::Rust)
            .contains(&UntypedDependency::external("serde")),
        "Rust build config should detect the serde dependency in relation_diamond"
    );

    let options = IndexOptions {
        root_dir: project_root.clone(),
        extensions: vec!["rs".to_string()],
        store_vectors: false,
        store_bm25: false,
        store_summaries: false,
        build_relations: true,
        respect_gitignore: false,
        additional_ignore_patterns: Vec::new(),
        custom_gitignore_path: None,
        ..IndexOptions::new(&project_root)
    };

    let mut orchestrator = IndexOrchestrator::new(1).expect("failed to create IndexOrchestrator");
    let result = orchestrator
        .execute(options)
        .await
        .expect("Indexing relation_diamond should succeed");

    assert!(
        result.total_files >= 4,
        "Expected all 4+ fixture files to be indexed, got {}",
        result.total_files
    );
    assert!(
        result.total_entities >= 5,
        "Expected at least 5 entities (main, create_user, print_summary, next_id, save_user, get_user)"
    );
    assert!(
        result.total_relations > 0,
        "Expected relations to be built for diamond call graph"
    );

    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available after indexing");

    assert!(
        relation_index.file_count() >= 4,
        "Relation index should track all fixture files"
    );

    let main_id = find_entity_id(&relation_index, "main", "src/main.rs");
    let create_user_id = find_entity_id(&relation_index, "create_user", "src/service_a.rs");
    let print_summary_id = find_entity_id(&relation_index, "print_summary", "src/service_b.rs");
    let _next_id_id = find_entity_id(&relation_index, "next_id", "src/repository.rs");
    let save_user_id = find_entity_id(&relation_index, "save_user", "src/repository.rs");
    let get_user_id = find_entity_id(&relation_index, "get_user", "src/repository.rs");

    let main_relations = relation_index
        .get_resolved_relations_by_caller(main_id)
        .expect("main should have resolved relations");

    assert!(
        main_relations
            .iter()
            .any(|r| r.callee_name == "create_user" && r.relation_type.is_call()),
        "main should call create_user (service_a)"
    );
    assert!(
        main_relations
            .iter()
            .any(|r| r.callee_name == "print_summary" && r.relation_type.is_call()),
        "main should call print_summary (service_b)"
    );

    let create_user_relations = relation_index
        .get_resolved_relations_by_caller(create_user_id)
        .expect("create_user should have resolved relations");
    assert!(
        create_user_relations
            .iter()
            .any(|r| r.callee_name == "save_user" && r.relation_type.is_call()),
        "create_user should call save_user (repository)"
    );

    let print_summary_relations = relation_index
        .get_resolved_relations_by_caller(print_summary_id)
        .expect("print_summary should have resolved relations");
    assert!(
        print_summary_relations
            .iter()
            .any(|r| r.callee_name == "get_user" && r.relation_type.is_call()),
        "print_summary should call get_user (repository)"
    );

    let call_query = CallChainQuery::from_index(relation_index.clone());
    let forward_main = call_query
        .query_forward_by_entity(main_id, 10)
        .expect("Forward traversal from main should succeed");
    assert!(
        forward_main.len() >= 3,
        "Forward traversal from main should reach at least 3 nodes (service_a, service_b, repository)"
    );

    let backward_save = call_query
        .query_backward_by_entity(save_user_id, 5)
        .expect("Backward traversal from save_user should succeed");
    assert!(
        backward_save
            .iter()
            .any(|n| n.function_name == "create_user"),
        "save_user should have create_user as caller"
    );

    let backward_get = call_query
        .query_backward_by_entity(get_user_id, 5)
        .expect("Backward traversal from get_user should succeed");
    assert!(
        backward_get
            .iter()
            .any(|n| n.function_name == "print_summary"),
        "get_user should have print_summary as caller"
    );
}
