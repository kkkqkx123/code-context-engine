//! Regression tests for tree-sitter query false negatives.
//!
//! Each test indexes a minimal `query_regression` fixture project and
//! asserts the file-level dependency edges (and, for C#, the resolved
//! same-file call edges) that previously went missing: grouped Go imports,
//! plain C# usings, C# this/base calls, short-path Rust uses, Ruby require
//! forms, PHP single uses, C system includes, bare Python relative imports
//! and Scala renamed selectors.

use cce_codegraph::index::{EntityIndexOps, RelationQueryOps};
use cce_orchestrator::{IndexOptions, IndexOrchestrator};

use crate::helper::{TestFixture, init_minimal_logging};

async fn index_fixture(
    fixture: TestFixture,
    extensions: Vec<String>,
) -> cce_codegraph::RelationIndex {
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
    let result = orchestrator
        .execute(options)
        .await
        .expect("indexing query_regression fixture should succeed");
    assert!(result.total_files >= 1, "fixture files should be indexed");
    orchestrator
        .get_relation_index()
        .expect("relation index should be available")
}

/// File-level dependency targets recorded for the file ending with `suffix`.
fn file_dep_targets(index: &cce_codegraph::RelationIndex, suffix: &str) -> Vec<String> {
    let records = index.file_records();
    let guard = records.read();
    let key = guard
        .keys()
        .find(|k| k.ends_with(suffix))
        .unwrap_or_else(|| panic!("indexed file with suffix '{suffix}' should exist"))
        .clone();
    drop(guard);
    index
        .file_relations(&key)
        .unwrap_or_default()
        .iter()
        .map(|r| r.callee_name.clone())
        .collect()
}

fn assert_targets(index: &cce_codegraph::RelationIndex, suffix: &str, expected: &[&str]) {
    let targets = file_dep_targets(index, suffix);
    for needle in expected {
        assert!(
            targets.iter().any(|t| t.contains(needle)),
            "file '{suffix}' should depend on '{needle}', got {targets:?}"
        );
    }
}

#[tokio::test]
async fn test_go_grouped_imports_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::go_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["go".to_string()]).await;
    assert_targets(
        &index,
        "main.go",
        &[
            "fmt",
            "os",
            "example.com/foo",
            "example.com/dot",
            "example.com/blank",
        ],
    );
}

#[tokio::test]
async fn test_csharp_usings_and_calls_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::csharp_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["cs".to_string()]).await;
    assert_targets(
        &index,
        "App.cs",
        &[
            "System",
            "System.IO",
            "Bar.Baz",
            "System.Math",
            "System.Linq",
            "System.Text",
            "System.Text.StringBuilder",
            "Inner.Space",
        ],
    );
    let targets = file_dep_targets(&index, "App.cs");
    assert!(
        !targets.iter().any(|t| t == "Foo"),
        "using alias must not become a dependency target, got {targets:?}"
    );
    // Inner qualified-name fragments eliminated by span-containment dedup.
    for junk in ["Math", "Text", "List", "Collections", "Generic", "Space"] {
        assert!(
            !targets.iter().any(|t| t == junk),
            "fragment '{junk}' must not become a target, got {targets:?}"
        );
    }
    // Exactly one constructor call: `new Widget()`; the target-typed
    // `new()` carries no type information and must stay silent.
    let app_key = index
        .file_records()
        .read()
        .keys()
        .find(|k| k.ends_with("App.cs"))
        .cloned()
        .expect("App.cs indexed");
    let mut ctor_edges = Vec::new();
    for entity_id in index.get_entities_in_line_range(&app_key, 1, 300) {
        if let Some(relations) = index.get_resolved_relations_by_caller(entity_id) {
            ctor_edges.extend(
                relations
                    .iter()
                    .filter(|r| {
                        r.relation_type == cce_types::relation::RelationType::ConstructorCall
                    })
                    .map(|r| r.callee_name.clone()),
            );
        }
    }
    assert_eq!(
        ctor_edges,
        vec!["Widget"],
        "only new Widget() should be a constructor edge"
    );

    // this.H() and base.M() resolve to same-file methods H and M.
    let mut callees = Vec::new();
    let records = index.file_records();
    let key = records
        .read()
        .keys()
        .find(|k| k.ends_with("App.cs"))
        .cloned()
        .expect("App.cs indexed");
    for entity_id in index.get_entities_in_line_range(&key, 1, 200) {
        if let Some(relations) = index.get_resolved_relations_by_caller(entity_id) {
            callees.extend(relations.iter().map(|r| r.callee_name.clone()));
        }
    }
    for needle in ["H", "M"] {
        assert!(
            callees.iter().any(|c| c == needle),
            "call to '{needle}' should resolve, got {callees:?}"
        );
    }
}

#[tokio::test]
async fn test_rust_short_uses_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::rust_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["rs".to_string()]).await;
    // Unresolvable uses stay external; self-resolving uses land on their
    // own import entities with the full written path as the target name.
    assert_targets(
        &index,
        "main.rs",
        &[
            "foo",
            "plain",
            "a::b",
            "a::c",
            "x::y::z",
            "g",
            "self::shape",
            "super::sibling",
            "crate::root_mod",
            "outer::inner::deep",
            "outer::inner::deeper",
            "shared::Reexport",
            "with_self",
            "once_cell",
        ],
    );
    let file_key = index
        .file_records()
        .read()
        .keys()
        .find(|k| k.ends_with("main.rs"))
        .cloned()
        .expect("main.rs indexed");
    let file_relations = index.file_relations(&file_key).unwrap_or_default();
    let use_edges: Vec<_> = file_relations
        .iter()
        .filter(|r| r.relation_type == cce_types::relation::RelationType::Use)
        .collect();
    assert_eq!(
        use_edges.len(),
        13,
        "all thirteen uses should reach the index, got {:?}",
        file_dep_targets(&index, "main.rs")
    );
    // Glob targets carry the prefix only; the star must not leak.
    assert!(
        !use_edges.iter().any(|r| r.callee_name.contains('*')),
        "glob star must not leak into targets"
    );
    // L3: the self-resolving `use plain;` must land on its own import
    // entity, not on the same-named function resolved from a stale ID
    // space. This pins the local-to-global ID translation fix.
    let main_fn = index
        .get_function_ids_by_name("main")
        .into_iter()
        .find(|id| {
            index
                .get_file_path_by_entity(*id)
                .as_deref()
                .is_some_and(|p| p.ends_with("main.rs"))
        })
        .expect("main fn indexed");
    let import_plain = index
        .function_index()
        .iter()
        .find(|entry| {
            entry.value().kind == cce_types::entity::EntityKind::Import
                && entry.value().name == "plain"
        })
        .map(|entry| *entry.key())
        .expect("plain import entity indexed");
    let plain_edge = use_edges
        .iter()
        .find(|r| r.callee_name == "plain")
        .expect("use plain edge should exist");
    assert_eq!(
        plain_edge.callee_id,
        Some(import_plain),
        "use plain must resolve to its import entity, not {main_fn:?}"
    );
    assert_ne!(
        plain_edge.callee_id,
        Some(main_fn),
        "use plain must not resolve to fn main"
    );
}

#[tokio::test]
async fn test_ruby_requires_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::ruby_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["rb".to_string()]).await;
    assert_targets(
        &index,
        "app.rb",
        &["foo", "./bar", "baz.rb", "helper", "blueberry"],
    );
    // Mixins are entity-level edges; the file level must not show them
    // as inclusions or bare calls.
    let targets = file_dep_targets(&index, "app.rb");
    for junk in ["Fetch", "Tricks", "Loud"] {
        assert!(
            !targets.iter().any(|t| t == junk),
            "mixin '{junk}' must not appear as a file dependency, got {targets:?}"
        );
    }
}

#[tokio::test]
async fn test_php_uses_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::php_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["php".to_string()]).await;
    let targets = file_dep_targets(&index, "app.php");
    for needle in [
        "Foo\\Bar",
        "Quux",
        "Corge",
        "Acme\\helper",
        "Acme\\VERSION",
        "Vendor\\Package",
    ] {
        assert!(
            targets.iter().any(|t| t.contains(needle)),
            "use '{needle}' should be recorded, got {targets:?}"
        );
    }
    for alias in ["C", "VP", "Baz"] {
        assert!(
            !targets.iter().any(|t| t == alias),
            "alias '{alias}' must not become a dependency target, got {targets:?}"
        );
    }
}

#[tokio::test]
async fn test_c_includes_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::c_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["c".to_string(), "h".to_string()]).await;
    // The system header is stdlib-filtered by design; the local includes
    // prove the pattern split (previously the quoted form matched twice).
    assert_targets(&index, "app.c", &["helper.h", "extra.h"]);
    let local_edges = file_dep_targets(&index, "app.c")
        .into_iter()
        .filter(|t| t.contains("helper.h"))
        .count();
    assert_eq!(local_edges, 1, "quoted include must yield one edge");
}

#[tokio::test]
async fn test_python_relative_imports_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::python_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["py".to_string()]).await;
    // `os` is stdlib-filtered by design; the relative and item forms are
    // the point. Local aliases never become targets.
    assert_targets(
        &index,
        "app.py",
        &[
            "sibling",
            "models",
            "Base",
            "pkg.loader",
            "pkg",
            "tool",
            "other",
            "pkg.sub",
            "alpha",
            "beta",
            "shared",
            "common",
            "top",
            "Optional",
            "fallback",
            "impl",
        ],
    );
    let targets = file_dep_targets(&index, "app.py");
    for alias in ["loader", "renamed"] {
        assert!(
            !targets.iter().any(|t| t == alias),
            "alias '{alias}' must not become a dependency target, got {targets:?}"
        );
    }
    assert!(
        !targets.iter().any(|t| t.contains('*')),
        "star must not leak into targets, got {targets:?}"
    );
}

#[tokio::test]
async fn test_scala_selectors_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::scala_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["scala".to_string()]).await;
    assert_targets(
        &index,
        "App.scala",
        &["a", "b", "c", "util", "conf", "host", "port", "store"],
    );
    let targets = file_dep_targets(&index, "App.scala");
    for alias in ["d", "servicePort", "backend"] {
        assert!(
            !targets.iter().any(|t| t == alias),
            "selector alias '{alias}' must not become a target, got {targets:?}"
        );
    }
}

#[tokio::test]
async fn test_cpp_usings_and_calls_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::cpp_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["cpp".to_string(), "h".to_string()]).await;
    // Using targets are names, never whole statements.
    // The type using stays external so its full path survives the
    // internal-edge identity merge (same-file internal targets collapse
    // by entity ID by design).
    assert_targets(&index, "main.cpp", &["helper.h", "mylib", "extlib::Widget"]);
    let targets = file_dep_targets(&index, "main.cpp");
    assert!(
        !targets
            .iter()
            .any(|t| t.contains("using ") || t.contains(';')),
        "using statements must not become raw targets, got {targets:?}"
    );
}

#[tokio::test]
async fn test_java_imports_and_calls_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::java_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["java".to_string()]).await;
    assert_targets(
        &index,
        "Main.java",
        &["java.util.List", "java.lang.Math", "java.util"],
    );
}

#[tokio::test]
async fn test_javascript_imports_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::javascript_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["js".to_string()]).await;
    // Module specifiers keep their source quotes, matching extractor output.
    assert_targets(
        &index,
        "app.js",
        &[
            "\"mod-default\"",
            "\"mod-ns\"",
            "\"mod-named\"",
            "\"mod-side\"",
            "\"mod-reexport\"",
            "\"mod-dynamic\"",
            "\"mod-cjs\"",
        ],
    );
}

#[tokio::test]
async fn test_typescript_imports_and_heritage_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::typescript_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["ts".to_string()]).await;
    assert_targets(&index, "app.ts", &["\"./config\"", "\"./server\""]);
    // Heritage edges are entity-level; the file level must not show the
    // class itself as a dependency (entity-name pollution regression).
    let targets = file_dep_targets(&index, "app.ts");
    assert!(
        !targets.iter().any(|t| t == "App" || t == "C"),
        "class name must not become a dependency target, got {targets:?}"
    );
}

#[tokio::test]
async fn test_kotlin_imports_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::kotlin_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["kt".to_string(), "kts".to_string()]).await;
    assert_targets(&index, "App.kt", &["shop.cart.Item", "shop.cart"]);
}

#[tokio::test]
async fn test_lua_requires_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::lua_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["lua".to_string()]).await;
    assert_targets(
        &index,
        "app.lua",
        &["\"util\"", "\"path\"", "\"extra.lua\""],
    );
}

#[tokio::test]
async fn test_dart_imports_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::dart_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["dart".to_string()]).await;
    assert_targets(
        &index,
        "app.dart",
        &["\"util.dart\"", "\"api.dart\"", "\"impl.dart\""],
    );
}

#[tokio::test]
async fn test_bash_sources_indexed() {
    init_minimal_logging();
    let fixture = TestFixture::bash_query_regression().expect("fixture should load");
    let index = index_fixture(fixture, vec!["sh".to_string()]).await;
    assert_targets(&index, "run.sh", &["./lib.sh", "./env.sh"]);
    // Inclusion roots stay whole paths here, never bare command text.
    let targets = file_dep_targets(&index, "run.sh");
    assert!(
        !targets.iter().any(|t| t.contains("source ")),
        "inclusion must not keep the command text, got {targets:?}"
    );
}
