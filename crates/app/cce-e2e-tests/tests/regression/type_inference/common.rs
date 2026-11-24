//! Shared helpers for type inference integration tests.

use cce_orchestrator::{IndexOptions, IndexOrchestrator};

use crate::helper::TestFixture;

/// Helper to run the full indexing pipeline on a fixture.
pub(crate) async fn run_index(fixture: TestFixture, extensions: Vec<String>) -> IndexOrchestrator {
    let project_root = fixture.root_path().to_path_buf();

    let options = IndexOptions {
        root_dir: project_root.clone(),
        extensions,
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
    orchestrator
        .execute(options)
        .await
        .expect("Indexing should succeed");
    orchestrator
}

/// Parse every file in a loaded fixture with the same tree-sitter pipeline
/// used by the indexer and return the canonical type snapshot.
pub(crate) fn snapshot_for(
    fixture: &TestFixture,
) -> Vec<cce_e2e_tests::type_inference_assert::CanonicalTypeBinding> {
    use cce_parser::parser::ParseCoordinator;

    let mut files = Vec::new();
    let mut stack = vec![fixture.root_path().to_path_buf()];
    let mut coordinator = ParseCoordinator::new();
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).expect("fixture dir should be readable");
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(content) = std::fs::read_to_string(&path) {
                let rel = path
                    .strip_prefix(fixture.root_path())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if let Ok(parsed) = coordinator.parse(&rel, &content) {
                    files.push(parsed);
                }
            }
        }
    }
    assert!(!files.is_empty(), "Fixture should yield parsed files");
    cce_e2e_tests::type_inference_assert::collect_type_bindings(&files)
}
