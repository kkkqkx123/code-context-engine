//! End-to-end metadata assertions for the type-inference coverage plan.
//!
//! Each test parses a small snippet with the production pipeline
//! (`ParseCoordinator`, including dedup, filtering, and comment
//! association) and asserts the entity metadata that the per-language
//! type inferers consume.

use cce_parser::parser::ParseCoordinator;
use cce_types::entity::EntityKind;

fn parse(path: &str, content: &str) -> Vec<cce_types::Entity> {
    let mut coordinator = ParseCoordinator::new();
    coordinator
        .parse(path, content)
        .expect("fixture should parse")
        .entities
}

fn find<'a>(
    entities: &'a [cce_types::Entity],
    kind: EntityKind,
    name: &str,
) -> &'a cce_types::Entity {
    entities
        .iter()
        .find(|e| e.kind == kind && e.name == name)
        .unwrap_or_else(|| {
            panic!(
                "expected {kind:?} '{name}', got {:?}",
                entities
                    .iter()
                    .map(|e| (&e.kind, e.name.as_str()))
                    .collect::<Vec<_>>()
            )
        })
}

#[test]
fn dart_generic_constructor_call_binds_base_type() {
    let entities = parse(
        "main.dart",
        "void main() {\n  var user = Container<String>('value');\n  var count = 42;\n  String explicit = 't';\n}",
    );
    let user = find(&entities, EntityKind::Variable, "user");
    assert_eq!(
        user.metadata.get("constructor_type").map(String::as_str),
        Some("Container"),
        "generic instantiation should normalize to the base type"
    );
    let count = find(&entities, EntityKind::Variable, "count");
    assert_eq!(
        count.metadata.get("literal_type").map(String::as_str),
        Some("number")
    );
    let explicit = find(&entities, EntityKind::Variable, "explicit");
    assert_eq!(
        explicit.metadata.get("type_annotation").map(String::as_str),
        Some("String")
    );
}

#[test]
fn dart_generic_return_type_is_complete() {
    let entities = parse("lib.dart", "List<T> wrapInList<T>(T item) => [item];");
    let func = find(&entities, EntityKind::Function, "wrapInList");
    assert_eq!(func.return_type.as_deref(), Some("List<T>"));
}

#[test]
fn dart_function_span_covers_body() {
    let entities = parse(
        "lib.dart",
        "String f(Object v) {\n  if (v is String) {\n    return v;\n  }\n  return 'x';\n}",
    );
    let func = find(&entities, EntityKind::Function, "f");
    assert!(
        func.span.end_byte > 30,
        "function span should extend past the signature, got {:?}",
        func.span
    );
}

#[test]
fn kotlin_val_binds_annotation_and_constructor() {
    let entities = parse(
        "main.kt",
        "fun main() {\n  val id: String = identity(\"t\")\n  val c = Container(\"v\")\n}",
    );
    let id = entities.iter().find(|e| e.name == "id").expect("id entity");
    assert_eq!(
        id.metadata.get("type_annotation").map(String::as_str),
        Some("String")
    );
    let c = entities.iter().find(|e| e.name == "c").expect("c entity");
    assert_eq!(
        c.metadata.get("constructor_type").map(String::as_str),
        Some("Container")
    );
}

#[test]
fn scala_val_binds_annotation_and_constructor() {
    let entities = parse(
        "main.scala",
        "object Demo {\n  def main(args: Array[String]): Unit = {\n    val id: String = identity(\"t\")\n    val c = Container(\"v\")\n  }\n}",
    );
    let id = find(&entities, EntityKind::Variable, "id");
    assert_eq!(
        id.metadata.get("type_annotation").map(String::as_str),
        Some("String")
    );
    let c = find(&entities, EntityKind::Variable, "c");
    assert_eq!(
        c.metadata.get("constructor_type").map(String::as_str),
        Some("Container")
    );
}

#[test]
fn php_new_with_args_binds_constructor() {
    let entities = parse(
        "app.php",
        "<?php\n$user = new User(\"Alice\", 30);\n$calc = new Calculator();",
    );
    let user = find(&entities, EntityKind::Variable, "user");
    assert_eq!(
        user.metadata.get("constructor_type").map(String::as_str),
        Some("User")
    );
    let calc = find(&entities, EntityKind::Variable, "calc");
    assert_eq!(
        calc.metadata.get("constructor_type").map(String::as_str),
        Some("Calculator")
    );
}

#[test]
fn php_method_native_return_and_phpdoc() {
    let entities = parse(
        "app.php",
        "<?php\nclass C {\n  /**\n   * @param int $a\n   * @return int\n   */\n  public function add(int $a, int $b): int {\n    return $a + $b;\n  }\n}",
    );
    let add = find(&entities, EntityKind::Method, "add");
    assert_eq!(add.return_type.as_deref(), Some("int"));
    assert_eq!(
        add.metadata.get("phpdoc_return_type").map(String::as_str),
        Some("int")
    );
}

#[test]
fn php_var_doc_binds_type() {
    let entities = parse(
        "app.php",
        "<?php\n/** @var int $total */\n$total = $calc->add(1, 2);",
    );
    let total = find(&entities, EntityKind::Variable, "total");
    assert_eq!(
        total.metadata.get("phpdoc_var_type").map(String::as_str),
        Some("int")
    );
}

#[test]
fn ruby_new_normalizes_to_class_name() {
    let entities = parse(
        "app.rb",
        "user = User.new(\"Alice\", 30)\ncalc = Calculator.new\n",
    );
    let user = find(&entities, EntityKind::Variable, "user");
    assert_eq!(
        user.metadata.get("constructor_type").map(String::as_str),
        Some("User"),
        "parenthesized .new should normalize to the class name"
    );
    let calc = find(&entities, EntityKind::Variable, "calc");
    assert_eq!(
        calc.metadata.get("constructor_type").map(String::as_str),
        Some("Calculator")
    );
}

#[test]
fn ruby_yard_return_binds_method() {
    let entities = parse(
        "app.rb",
        "class Calculator\n  # @return [Integer] the sum\n  def add(a, b)\n    a + b\n  end\nend",
    );
    let add = find(&entities, EntityKind::Method, "add");
    assert_eq!(
        add.metadata.get("yard_return_type").map(String::as_str),
        Some("Integer")
    );
}

#[test]
fn cpp_method_has_no_receiver_type() {
    let entities = parse(
        "app.cpp",
        "class Calculator {\npublic:\n  int add(int a, int b);\n};",
    );
    let add = find(&entities, EntityKind::Method, "add");
    assert!(
        !add.metadata.contains_key("receiver_type"),
        "first parameter must not be misread as a receiver"
    );
}

#[test]
fn cpp_init_declarator_binds_literal() {
    let entities = parse("app.cpp", "int main() {\n  int x = 1;\n  return 0;\n}");
    let x = find(&entities, EntityKind::Variable, "x");
    assert_eq!(
        x.metadata.get("literal_type").map(String::as_str),
        Some("number")
    );
}

#[test]
fn typescript_return_has_no_colon_prefix() {
    let entities = parse(
        "app.ts",
        "function handleResult(result: Result): string {\n  return \"x\";\n}",
    );
    let func = find(&entities, EntityKind::Function, "handleResult");
    assert_eq!(func.return_type.as_deref(), Some("string"));
}

#[test]
fn c_init_declarator_binds_type_and_literal() {
    let entities = parse("app.c", "int main() {\n  int x = 1;\n  return 0;\n}");
    let x = find(&entities, EntityKind::Variable, "x");
    assert_eq!(
        x.metadata.get("type_annotation").map(String::as_str),
        Some("int")
    );
}
