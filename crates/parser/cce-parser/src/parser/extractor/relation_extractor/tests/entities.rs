use super::*;

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
