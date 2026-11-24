//! Assertion tests for Rust relation indexing and call chain traversal.
//!
//! These tests verify that the relation index correctly resolves cross-file
//! function calls, call chains, and file dependencies for the relation_demo fixture.

use cce_codegraph::index::{EntityIndexOps, FileIndexOps, RelationQueryOps};
use cce_codegraph::{BuildConfigParser, CallChainQuery, UntypedDependency};
use cce_orchestrator::{IndexOptions, IndexOrchestrator};
use cce_types::EntityId;

use crate::helper::{TestFixture, init_minimal_logging};

fn find_entity_id(index: &cce_codegraph::RelationIndex, name: &str, file_suffix: &str) -> EntityId {
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
            let mut candidates = index
                .function_index()
                .iter()
                .map(|entry| {
                    let file_path = index
                        .get_file_path_by_entity(*entry.key())
                        .unwrap_or_default();
                    format!("{} @ {}", entry.value().name, file_path)
                })
                .collect::<Vec<_>>();
            candidates.sort();
            panic!(
                "Failed to find entity `{}` in file suffix `{}`; available entities: {:?}",
                name, file_suffix, candidates
            )
        })
}

#[tokio::test]
async fn test_rust_relation_presentation() {
    init_minimal_logging();

    let fixture = TestFixture::rust_relation_demo().expect("Failed to load relation_demo fixture");
    let project_root = fixture.root_path().to_path_buf();

    let mut config_parser = BuildConfigParser::new();
    config_parser
        .scan_project(&project_root, 0)
        .expect("Build config scan should succeed");
    assert!(
        config_parser
            .dependencies_for_language(cce_types::language::Language::Rust)
            .contains(&UntypedDependency::external("serde")),
        "Rust build config should detect the serde dependency"
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
        .expect("Indexing relation demo should succeed");

    assert!(
        result.total_files >= 5,
        "Expected all fixture files to be indexed"
    );
    assert!(
        result.total_entities >= 5,
        "Expected at least one entity per file"
    );
    assert!(
        result.total_relations > 0,
        "Expected relation data to be built"
    );

    let relation_index = orchestrator
        .get_relation_index()
        .expect("Relation index should be available after indexing");

    assert!(
        relation_index.file_count() >= 5,
        "Relation index should track all fixture files"
    );
    assert!(
        relation_index.resolved_relation_count() > 0,
        "Relation index should contain resolved relations"
    );

    let call_query = CallChainQuery::from_index(relation_index.clone());

    let main_id = find_entity_id(&relation_index, "main", "src/main.rs");
    let load_user_id = find_entity_id(&relation_index, "load_user", "src/repository.rs");
    let _format_user_id = find_entity_id(&relation_index, "format_user", "src/utils.rs");
    let create_user_id = find_entity_id(&relation_index, "create", "src/model.rs");

    let main_relations = relation_index
        .get_resolved_relations_by_caller(main_id)
        .expect("Main should have resolved relations");
    let load_and_format_user_relation_id = main_relations
        .iter()
        .find(|relation| relation.callee_name == "load_and_format_user")
        .and_then(|relation| relation.callee_id)
        .unwrap_or_else(|| {
            panic!(
                "Main should resolve load_and_format_user; actual outgoing edges: {:?}",
                main_relations
                    .iter()
                    .map(|relation| {
                        format!(
                            "{} -> {:?} (internal: {:?})",
                            relation.callee_name, relation.relation_type, relation.callee_id
                        )
                    })
                    .collect::<Vec<_>>()
            )
        });
    let load_and_format_user_path = relation_index
        .get_file_path_by_entity(load_and_format_user_relation_id)
        .unwrap_or_default();
    assert!(
        load_and_format_user_path.ends_with("src/repository.rs"),
        "load_and_format_user should live in repository.rs, got {}",
        load_and_format_user_path
    );
    assert!(
        main_relations
            .iter()
            .any(|relation| relation.callee_name == "load_and_format_user"
                && relation.relation_type.is_call()),
        "Main should call load_and_format_user directly"
    );

    let repository_relations = relation_index
        .get_resolved_relations_by_caller(load_and_format_user_relation_id)
        .expect("load_and_format_user should have resolved relations");
    assert!(
        repository_relations.iter().any(|relation| relation.callee_name == "load_user"
            && relation.relation_type.is_call()),
        "load_and_format_user should call load_user"
    );

    let load_user_relations = relation_index
        .get_resolved_relations_by_caller(load_user_id)
        .expect("load_user should have resolved relations");
    let normalize_relation = load_user_relations
        .iter()
        .find(|relation| relation.callee_name == "normalize_name")
        .expect("load_user should resolve normalize_name");
    assert!(
        normalize_relation.callee_id.is_some(),
        "normalize_name should resolve to an entity id"
    );
    assert!(
        normalize_relation.callee_name == "normalize_name",
        "normalize_name relation should resolve to correct function name, got {}",
        normalize_relation.callee_name
    );
    if let Some(symbol) = normalize_relation.callee_symbol.as_ref() {
        assert!(
            symbol.location.file_path.ends_with("src/utils.rs"),
            "normalize_name should keep its symbol location, got {}",
            symbol.location.file_path
        );
    } else {
        panic!("normalize_name relation should preserve symbol snapshot");
    }

    let _forward_main = call_query
        .query_forward_by_entity(main_id, 6)
        .expect("Forward traversal from main should succeed");
    let _forward_repo = call_query
        .query_forward_by_entity(load_and_format_user_relation_id, 5)
        .expect("Forward traversal from load_and_format_user should succeed");
    let _backward_load_user = call_query
        .query_backward_by_entity(load_user_id, 5)
        .expect("Backward traversal from load_user should succeed");
    let path_main_to_load_user = call_query
        .find_call_chain(main_id, load_user_id, 6, 10_000)
        .expect("Path query from main to load_user should not error");
    assert!(
        path_main_to_load_user.is_some(),
        "Expected a path from main to load_user through load_and_format_user"
    );

    assert!(
        relation_index
            .get_function_by_entity_id(create_user_id)
            .is_some(),
        "create function should still be present in the relation index"
    );
}
