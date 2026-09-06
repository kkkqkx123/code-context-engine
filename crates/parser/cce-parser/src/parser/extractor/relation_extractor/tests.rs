//! Tests for relation extraction.
//!
//! Covers call extraction, callee naming, dependency edges and
//! import edge cases, plus entity binding regressions exercised through
//! the extractor harness.

use super::*;
use crate::parser::ast_parser::AstParser;
use crate::parser::extractor::entity_extractor::EntityExtractor;
use crate::tree_sitter_query::executor::{Capture, QueryMatch};
use cce_types::language::Language;
use cce_types::{RelationType, StdlibCategory};

#[test]
fn test_extract_rust_calls() {
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = r#"
fn foo() -> i32 {
1
}

fn bar() -> i32 {
foo() + 1
}
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let entities = entity_extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract entities");

    let relations = relation_extractor
        .extract(&tree, code, &Language::Rust, &entities, Some(1))
        .expect("Failed to extract relations");

    // Should find the call from bar to foo
    let _calls: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type.is_call())
        .collect();

    // Note: The actual number depends on query results
    // This test just verifies the extraction doesn't fail
    assert!(!entities.is_empty());
}

#[test]
fn test_extract_rust_trait_impl_relations() {
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = r#"
trait MyTrait {
fn f(&self);
}

struct MyStruct;

impl MyStruct {
fn inherent(&self) {}
}

impl MyTrait for MyStruct {
fn f(&self) {}
}
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;

    let entities = entity_extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract entities");

    let relations = relation_extractor
        .extract(&tree, code, &Language::Rust, &entities, Some(1))
        .expect("Failed to extract relations");

    let implementations: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type == RelationType::Implementation)
        .collect();
    let impl_associations: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type == RelationType::ImplAssociation)
        .collect();

    // Trait impl block yields exactly one Implementation (callee = trait)
    assert_eq!(implementations.len(), 1, "relations: {relations:?}");
    assert_eq!(
        implementations[0].dst_name(),
        "MyTrait",
        "Implementation callee should be the trait name, got {:?}",
        implementations[0].dst_name()
    );

    // Inherent + trait impl blocks each yield one ImplAssociation (callee = target type)
    assert_eq!(impl_associations.len(), 2, "relations: {relations:?}");
    let targets: Vec<_> = impl_associations
        .iter()
        .map(|r| r.dst_name().to_string())
        .collect();
    assert_eq!(
        targets,
        vec!["MyStruct".to_string(), "MyStruct".to_string()]
    );
}

#[test]
fn test_extract_python_calls() {
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = r#"
def foo():
return 1

def bar():
return foo() + 1
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Python)
        .expect("Failed to parse")
        .0;

    let entities = entity_extractor
        .extract(&tree, code, &Language::Python)
        .expect("Failed to extract entities");

    let _relations = relation_extractor
        .extract(&tree, code, &Language::Python, &entities, Some(1))
        .expect("Failed to extract relations");

    // This test just verifies the extraction doesn't fail
    assert!(!entities.is_empty());
}

#[test]
fn test_extract_full_scoped_type_reference() {
    // Regression: a type-position reference like
    // `std::collections::HashMap` must keep the full scoped path instead
    // of truncating to the path segment `std::collections`.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = r#"
fn main() {
let map: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
}
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract entities");
    let relations = relation_extractor
        .extract(&tree, code, &Language::Rust, &entities, Some(1))
        .expect("Failed to extract relations");

    let refs: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type == RelationType::TypeReference)
        .collect();
    assert!(
        refs.iter()
            .any(|r| r.dst_name() == "std::collections::HashMap"),
        "expected full scoped type reference, got {refs:?}"
    );
}

#[test]
fn test_extract_full_callee_name_rust_associated_and_method() {
    // Regression: `Vec::new()` and `v.push(...)` must preserve the
    // type path / receiver instead of truncating to the bare function
    // name.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = r#"
fn main() {
let mut v = Vec::new();
v.push(1);
}
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("Failed to parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Rust)
        .expect("Failed to extract entities");
    let relations = relation_extractor
        .extract(&tree, code, &Language::Rust, &entities, Some(1))
        .expect("Failed to extract relations");

    let calls: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type.is_call())
        .collect();
    let names: Vec<&str> = calls.iter().map(|r| r.dst_name()).collect();
    assert!(
        names.contains(&"Vec::new"),
        "expected Vec::new in {names:?}"
    );
    assert!(names.contains(&"v.push"), "expected v.push in {names:?}");
    // The stdlib category must be derived from the type path prefix.
    let vec_new = calls
        .iter()
        .find(|r| r.dst_name() == "Vec::new")
        .expect("Vec::new call present");
    assert_eq!(
        vec_new.stdlib_category,
        Some(StdlibCategory::Collection),
        "Vec::new category should come from the `Vec` prefix"
    );
}

#[test]
fn test_extract_full_callee_name_javascript_method() {
    // Regression: `console.log(...)` and `Math.max(...)` must keep
    // their receivers so stdlib detection sees the qualified name.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = r#"
function main() {
console.log("hello");
Math.max(1, 2);
}
"#;

    let tree = ast_parser
        .parse_with_tree(code, &Language::JavaScript)
        .expect("Failed to parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::JavaScript)
        .expect("Failed to extract entities");
    let relations = relation_extractor
        .extract(&tree, code, &Language::JavaScript, &entities, Some(1))
        .expect("Failed to extract relations");

    let calls: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type.is_call())
        .collect();
    let names: Vec<&str> = calls.iter().map(|r| r.dst_name()).collect();
    assert!(
        names.contains(&"console.log"),
        "expected console.log in {names:?}"
    );
    assert!(
        names.contains(&"Math.max"),
        "expected Math.max in {names:?}"
    );
    let console_log = calls
        .iter()
        .find(|r| r.dst_name() == "console.log")
        .expect("console.log call present");
    assert_eq!(
        console_log.stdlib_category,
        Some(StdlibCategory::Io),
        "console.log category should come from the `console` prefix"
    );
}

#[test]
fn test_build_full_callee_name_trivial_receiver_falls_back() {
    // Regression: `this.method()` inside a JS method must still
    // produce `method` (the receiver is trivial) so local resolution
    // keeps working.
    let mat = QueryMatch {
        index: 0,
        pattern_index: 0,
        captures: vec![
            Capture {
                name: "call.method.object".to_string(),
                text: "this".to_string(),
                start_byte: 0,
                end_byte: 4,
                start_point: (0, 0),
                end_point: (0, 4),
            },
            Capture {
                name: "call.method.function".to_string(),
                text: "method".to_string(),
                start_byte: 5,
                end_byte: 11,
                start_point: (0, 5),
                end_point: (0, 11),
            },
        ],
    };
    // Create a minimal tree for AST-based name extraction.
    let mut ast_parser = AstParser::new();
    let source = "this.method()";
    let (tree, _) = ast_parser
        .parse_with_tree(source, &Language::JavaScript)
        .expect("Failed to parse");
    assert_eq!(
        relation_handlers::build_full_callee_name(&mat, &Language::JavaScript, &tree, source)
            .as_deref(),
        Some("method"),
        "trivial receiver must not be preserved"
    );
    // Rust `self.method()` must be preserved for type-member dispatch.
    assert_eq!(
        relation_handlers::build_full_callee_name(&mat, &Language::Rust, &tree, source).as_deref(),
        Some("this.method"),
        "rust trivial this should be preserved as qualified"
    );
    let mut rust_self_mat = QueryMatch {
        index: 0,
        pattern_index: 0,
        captures: vec![
            Capture {
                name: "call.method.object".to_string(),
                text: "self".to_string(),
                start_byte: 0,
                end_byte: 4,
                start_point: (0, 0),
                end_point: (0, 4),
            },
            Capture {
                name: "call.method.function".to_string(),
                text: "clone".to_string(),
                start_byte: 5,
                end_byte: 10,
                start_point: (0, 5),
                end_point: (0, 10),
            },
        ],
    };
    let rust_source = "self.clone()";
    let (rust_tree, _) = ast_parser
        .parse_with_tree(rust_source, &Language::Rust)
        .expect("Failed to parse");
    assert_eq!(
        relation_handlers::build_full_callee_name(
            &rust_self_mat,
            &Language::Rust,
            &rust_tree,
            rust_source
        )
        .as_deref(),
        Some("self.clone"),
        "rust self.clone must be preserved"
    );
    rust_self_mat.captures[0].text = "Self".to_string();
    assert_eq!(
        relation_handlers::build_full_callee_name(
            &rust_self_mat,
            &Language::Rust,
            &rust_tree,
            rust_source
        )
        .as_deref(),
        Some("Self.clone"),
        "rust self.clone must be preserved"
    );
}

#[test]
fn test_python_tuple_unpacking_entities() {
    // Tuple unpacking yields one comma-separated variable entity
    // carrying the right-hand side as its source.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "def f(pair):\n    first, second = pair\n    return first\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Python)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Python)
        .expect("extract");
    let unpacked = entities
        .iter()
        .find(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name.contains("first"))
        .expect("tuple unpacking entity should exist");
    assert_eq!(unpacked.name, "first, second");
    assert_eq!(
        unpacked.metadata.get("source_type").map(String::as_str),
        Some("pair")
    );
}

#[test]
fn test_python_pattern_binding_entities() {
    // Loop and exception bindings exist as entities.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "for k, v in items:\n    pass\ntry:\n    pass\nexcept ValueError as e:\n    pass\nwith open('f') as fh:\n    pass\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Python)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Python)
        .expect("extract");
    let names: Vec<&str> = entities
        .iter()
        .filter(|e| e.kind == cce_types::entity::EntityKind::Variable)
        .map(|e| e.name.as_str())
        .collect();
    for expected in ["k", "v", "e", "fh"] {
        assert!(
            names.contains(&expected),
            "pattern binding '{expected}' should exist, got {names:?}"
        );
    }
    let except_entity = entities
        .iter()
        .find(|e| e.name == "e")
        .expect("except binding should exist");
    assert_eq!(
        except_entity
            .metadata
            .get("source_type")
            .map(String::as_str),
        Some("ValueError")
    );

    let case_code = "def f(value):\n    match value:\n        case (x, 0):\n            return x\n        case (x, y):\n            return y\n";
    let case_tree = ast_parser
        .parse_with_tree(case_code, &Language::Python)
        .expect("parse")
        .0;
    let case_entities = entity_extractor
        .extract(&case_tree, case_code, &Language::Python)
        .expect("extract");
    let case_names: Vec<&str> = case_entities
        .iter()
        .filter(|e| e.kind == cce_types::entity::EntityKind::Variable)
        .map(|e| e.name.as_str())
        .collect();
    for expected in ["x", "y"] {
        assert!(
            case_names.contains(&expected),
            "case binding '{expected}' should exist, got {case_names:?}"
        );
    }
}

#[test]
fn test_string_argument_call_is_not_require_import() {
    // A call with a string literal argument must not produce
    // an ImportStandard edge to that literal.
    for language in [Language::JavaScript, Language::TypeScript] {
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();

        let code = r#"
function createUser(name, id) {
  return { kind: 'user', name, id };
}
const user = createUser('Alice', 1);
"#;

        let tree = ast_parser
            .parse_with_tree(code, &language)
            .expect("Failed to parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &language)
            .expect("Failed to extract entities");
        let relations = relation_extractor
            .extract(&tree, code, &language, &entities, Some(1))
            .expect("Failed to extract relations");

        assert!(
            relations
                .iter()
                .filter(|r| r.relation_type == RelationType::ImportStandard)
                .all(|r| !r.dst_name().contains("Alice")),
            "string literal must not become an import edge in {language:?}: {relations:?}"
        );
        assert!(
            relations.iter().any(|r| r.dst_name() == "createUser"),
            "real call edge must be preserved in {language:?}: {relations:?}"
        );
    }
}

#[test]
fn test_real_require_import_still_detected() {
    // The predicate fix must not break genuine require() detection.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = "const { loadUser } = require(\"./models\");\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::JavaScript)
        .expect("Failed to parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::JavaScript)
        .expect("Failed to extract entities");
    let relations = relation_extractor
        .extract(&tree, code, &Language::JavaScript, &entities, Some(1))
        .expect("Failed to extract relations");

    assert!(
        relations
            .iter()
            .any(|r| r.relation_type == RelationType::ImportStandard
                && r.dst_name().contains("models")),
        "genuine require() import must still be detected: {relations:?}"
    );
}

#[test]
fn test_assigned_require_yields_single_import_edge() {
    // The bare-call require pattern fires on the inner call_expression
    // regardless of its parent, so no declarator-level pattern may exist:
    // such a pattern would emit a second edge with a different span that
    // survives span-grouped dedup. `const x = require()` must yield
    // exactly one ImportStandard edge.
    for (language, code) in [
        (Language::JavaScript, "const x = require(\"./m\");\n"),
        (Language::JavaScript, "var y = require(\"./m\");\n"),
        (Language::TypeScript, "const x: any = require(\"./m\");\n"),
    ] {
        let mut ast_parser = AstParser::new();
        let entity_extractor = EntityExtractor::new();
        let relation_extractor = RelationExtractor::new();

        let tree = ast_parser
            .parse_with_tree(code, &language)
            .expect("Failed to parse")
            .0;
        let entities = entity_extractor
            .extract(&tree, code, &language)
            .expect("Failed to extract entities");
        let relations = relation_extractor
            .extract(&tree, code, &language, &entities, Some(1))
            .expect("Failed to extract relations");

        let imports: Vec<_> = relations
            .iter()
            .filter(|r| {
                r.relation_type == RelationType::ImportStandard && r.dst_name().contains("./m")
            })
            .collect();
        assert_eq!(
            imports.len(),
            1,
            "assigned require() must yield exactly one import edge in {language:?}: {relations:?}"
        );
    }
}

#[test]
fn test_shadowed_require_param_yields_no_import() {
    // A shadowed builtin produces no import edge, while a top-level
    // call in the same file is still kept.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();
    let code = "function f(require) { return require(\"./x\"); }\nrequire(\"./y\");\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::JavaScript)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::JavaScript)
        .expect("extract");
    let relations = relation_extractor
        .extract(&tree, code, &Language::JavaScript, &entities, Some(1))
        .expect("relations");
    let imports: Vec<_> = relations
        .iter()
        .filter(|r| r.relation_type == RelationType::ImportStandard)
        .collect();
    assert!(
        imports.iter().all(|r| !r.dst_name().contains("./x")),
        "shadowed require must not import: {relations:?}"
    );
    assert!(
        imports.iter().any(|r| r.dst_name().contains("./y")),
        "top-level require must still import: {relations:?}"
    );
}

#[test]
fn test_js_destructuring_entities() {
    // Object and array destructuring yield comma-folded entities.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "const {name, age} = user;\nconst [first, second] = pair;\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::JavaScript)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::JavaScript)
        .expect("extract");
    let vars: Vec<&str> = entities
        .iter()
        .filter(|e| e.kind == cce_types::entity::EntityKind::Variable)
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        vars.iter().any(|n| n.contains("name") && n.contains("age")),
        "object destructuring entity missing, got {vars:?}"
    );
    assert!(
        vars.iter()
            .any(|n| n.contains("first") && n.contains("second")),
        "array destructuring entity missing, got {vars:?}"
    );
}

#[test]
fn test_rust_pattern_entities() {
    // Tuple and struct patterns with branch bindings exist as entities.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "fn f(pair: (i32, i32), p: Point, opt: Option<i32>, x: Option<i32>) { let (a, b) = pair; if let Some(v) = opt {} match x { Some(y) => {}, _ => {} } }\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Rust)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Rust)
        .expect("extract");
    let vars: Vec<&str> = entities
        .iter()
        .filter(|e| e.kind == cce_types::entity::EntityKind::Variable)
        .map(|e| e.name.as_str())
        .collect();
    assert!(
        vars.iter().any(|n| n.contains('a') && n.contains('b')),
        "tuple pattern entity missing, got {vars:?}"
    );
    assert!(vars.contains(&"v"), "if-let binding missing, got {vars:?}");
}

#[test]
fn test_c_enum_member_entities() {
    // C enumerators are distinct enum variant entities.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "enum Color { RED, GREEN = 2, BLUE };\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::C)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::C)
        .expect("extract");
    for expected in ["RED", "GREEN", "BLUE"] {
        assert!(
            entities
                .iter()
                .any(|e| e.kind == cce_types::entity::EntityKind::EnumVariant
                    && e.name == expected),
            "enum member '{expected}' missing, got {:?}",
            entities
                .iter()
                .map(|e| (&e.kind, &e.name))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_dart_function_params_extracted() {
    // Function and method signatures carry parameters.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "String greet(String name, int age) { return name; }\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Dart)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Dart)
        .expect("extract");
    let func = entities
        .iter()
        .find(|e| e.name == "greet")
        .expect("greet entity should exist");
    assert_eq!(func.parameters.len(), 2, "params: {:?}", func.parameters);
    assert!(func.parameters.iter().any(|(n, _)| n == "name"));
    assert!(func.parameters.iter().any(|(n, _)| n == "age"));
}

#[test]
fn test_java_record_and_pattern_entities() {
    // Record components, pattern variables and loop variables
    // exist as entities.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "public record Point(String name, int age) {}\nclass A {\n    String f(Object obj, String[] args) {\n        if (obj instanceof String s) { return s; }\n        for (String current : args) { }\n        return \"x\";\n    }\n}\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Java)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Java)
        .expect("extract");
    let names: Vec<(&cce_types::entity::EntityKind, &str)> = entities
        .iter()
        .map(|e| (&e.kind, e.name.as_str()))
        .collect();
    for expected in ["name", "age"] {
        assert!(
            entities
                .iter()
                .any(|e| e.kind == cce_types::entity::EntityKind::Field && e.name == expected),
            "record component '{expected}' missing, got {names:?}"
        );
    }
    assert!(
        entities
            .iter()
            .any(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name == "s"),
        "instanceof pattern var 's' missing, got {names:?}"
    );
    assert!(
        entities
            .iter()
            .any(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name == "current"),
        "enhanced-for var 'current' missing, got {names:?}"
    );
}

#[test]
fn test_kotlin_for_in_loop_variable_entities() {
    // For-in loop variables exist as entities with source provenance.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "fun f(items: List<String>, entries: Map<String, Int>) {\n    for (item in items) { println(item) }\n    for ((key, value) in entries) { println(key) }\n}\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Kotlin)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Kotlin)
        .expect("extract");
    let names: Vec<(&cce_types::entity::EntityKind, &str)> = entities
        .iter()
        .map(|e| (&e.kind, e.name.as_str()))
        .collect();
    assert!(
        entities
            .iter()
            .any(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name == "item"),
        "for-in var 'item' missing, got {names:?}"
    );
    for expected in ["key", "value"] {
        assert!(
            entities
                .iter()
                .any(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name == expected),
            "destructuring loop var '{expected}' missing, got {names:?}"
        );
    }
    assert!(
        entities.iter().any(|e| e.name == "item"
            && e.metadata
                .get("source_type")
                .is_some_and(|s| s.contains("items"))),
        "for-in var 'item' lacks collection provenance, got {names:?}"
    );
}

#[test]
fn test_go_range_loop_variable_entities() {
    // Range loop variables exist as entities with source provenance.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "package main\nfunc f(items []string) {\n    for i, v := range items { println(i, v) }\n    for _, w := range items { println(w) }\n}\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Go)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Go)
        .expect("extract");
    let names: Vec<(&cce_types::entity::EntityKind, &str)> = entities
        .iter()
        .map(|e| (&e.kind, e.name.as_str()))
        .collect();
    for expected in ["i", "v", "w"] {
        assert!(
            entities
                .iter()
                .any(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name == expected),
            "range var '{expected}' missing, got {names:?}"
        );
    }
    assert!(
        entities.iter().any(|e| e.name == "v"
            && e.metadata
                .get("source_type")
                .is_some_and(|s| s.contains("items"))),
        "range var 'v' lacks collection provenance, got {names:?}"
    );
}

#[test]
fn test_dart_for_in_loop_variable_entities() {
    // For-in loop variables exist as entities with source provenance.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "void f(List<String> items) {\n    for (var item in items) { print(item); }\n}\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Dart)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Dart)
        .expect("extract");
    let names: Vec<(&cce_types::entity::EntityKind, &str)> = entities
        .iter()
        .map(|e| (&e.kind, e.name.as_str()))
        .collect();
    assert!(
        entities
            .iter()
            .any(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name == "item"),
        "for-in var 'item' missing, got {names:?}"
    );
    assert!(
        entities.iter().any(|e| e.name == "item"
            && e.metadata
                .get("source_type")
                .is_some_and(|s| s.contains("items"))),
        "for-in var 'item' lacks collection provenance, got {names:?}"
    );
}

#[test]
fn test_scala_tuple_destructuring_entities() {
    // Tuple destructuring binds each component with source provenance.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "object Main {\n  def f(pair: (String, Int)): Unit = {\n    val (first, second) = pair\n  }\n}\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Scala)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Scala)
        .expect("extract");
    let names: Vec<(&cce_types::entity::EntityKind, &str)> = entities
        .iter()
        .map(|e| (&e.kind, e.name.as_str()))
        .collect();
    assert!(
        entities
            .iter()
            .any(|e| e.kind == cce_types::entity::EntityKind::Variable
                && e.name.contains("first")
                && e.name.contains("second")),
        "tuple destructuring entity missing, got {names:?}"
    );
    assert!(
        entities.iter().any(|e| e
            .metadata
            .get("source_type")
            .is_some_and(|s| s.contains("pair"))),
        "tuple destructuring lacks source provenance, got {names:?}"
    );
}

#[test]
fn test_dart_record_destructuring_entities() {
    // Record destructuring binds each component with source provenance.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "void f((String, int) record) {\n    var (first, second) = record;\n}\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Dart)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Dart)
        .expect("extract");
    let names: Vec<(&cce_types::entity::EntityKind, &str)> = entities
        .iter()
        .map(|e| (&e.kind, e.name.as_str()))
        .collect();
    assert!(
        entities
            .iter()
            .any(|e| e.kind == cce_types::entity::EntityKind::Variable
                && e.name.contains("first")
                && e.name.contains("second")),
        "record destructuring entity missing, got {names:?}"
    );
    assert!(
        entities.iter().any(|e| e
            .metadata
            .get("source_type")
            .is_some_and(|s| s.contains("record"))),
        "record destructuring lacks source provenance, got {names:?}"
    );
}

#[test]
fn test_cpp_range_for_and_structured_binding_entities() {
    // Range loop variables and structured bindings exist as entities.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "int f(std::vector<int>& v) {\n    for (auto& elem : v) { }\n    auto [a, b] = p;\n    return 0;\n}\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Cpp)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Cpp)
        .expect("extract");
    let names: Vec<(&cce_types::entity::EntityKind, &str)> = entities
        .iter()
        .map(|e| (&e.kind, e.name.as_str()))
        .collect();
    assert!(
        entities
            .iter()
            .any(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name == "elem"),
        "range-for var 'elem' missing, got {names:?}"
    );
    assert!(
        entities
            .iter()
            .any(|e| e.kind == cce_types::entity::EntityKind::Variable
                && e.name.contains('a')
                && e.name.contains('b')),
        "structured binding entity missing, got {names:?}"
    );
}

#[test]
fn test_kotlin_destructuring_entities() {
    // Destructuring declarations fold into one entity carrying
    // the right-hand side as its source.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "fun f(pair: Pair<Int, String>) {\n    val (first, second) = pair\n}\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::Kotlin)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::Kotlin)
        .expect("extract");
    let folded = entities
        .iter()
        .find(|e| {
            e.kind == cce_types::entity::EntityKind::Variable
                && e.name.contains("first")
                && e.name.contains("second")
        })
        .expect("folded destructuring entity should exist");
    assert_eq!(
        folded.metadata.get("source_type").map(String::as_str),
        Some("pair")
    );
}

#[test]
fn test_csharp_pattern_entities() {
    // Loop variables, pattern designations, output variables and
    // tuple deconstruction exist as entities.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let code = "class D {\n    void M(object obj, string[] items) {\n        foreach (var current in items) { }\n        if (obj is string s) { }\n        var (a, b) = (1, 2);\n        if (int.TryParse(\"1\", out var result)) { }\n    }\n}\n";
    let tree = ast_parser
        .parse_with_tree(code, &Language::CSharp)
        .expect("parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::CSharp)
        .expect("extract");
    let names: Vec<(&cce_types::entity::EntityKind, &str)> = entities
        .iter()
        .map(|e| (&e.kind, e.name.as_str()))
        .collect();
    for expected in ["current", "s", "result"] {
        assert!(
            entities
                .iter()
                .any(|e| e.kind == cce_types::entity::EntityKind::Variable && e.name == expected),
            "pattern var '{expected}' missing, got {names:?}"
        );
    }
    assert!(
        entities
            .iter()
            .any(|e| e.kind == cce_types::entity::EntityKind::Variable
                && e.name.contains('a')
                && e.name.contains('b')),
        "tuple deconstruction entity missing, got {names:?}"
    );
}

#[test]
fn test_typescript_import_require_clause_detected() {
    // `import x = require("m")` parses as `import_require_clause`, not a
    // `call_expression`, so it needs its own dependency pattern.
    let mut ast_parser = AstParser::new();
    let entity_extractor = EntityExtractor::new();
    let relation_extractor = RelationExtractor::new();

    let code = "import x = require(\"./m\");\n";

    let tree = ast_parser
        .parse_with_tree(code, &Language::TypeScript)
        .expect("Failed to parse")
        .0;
    let entities = entity_extractor
        .extract(&tree, code, &Language::TypeScript)
        .expect("Failed to extract entities");
    let relations = relation_extractor
        .extract(&tree, code, &Language::TypeScript, &entities, Some(1))
        .expect("Failed to extract relations");

    assert!(
        relations.iter().any(
            |r| r.relation_type == RelationType::ImportStandard && r.dst_name().contains("./m")
        ),
        "TS import-require must be detected as an import edge: {relations:?}"
    );
}
