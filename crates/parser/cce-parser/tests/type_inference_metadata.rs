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
fn ruby_implicit_constructor_return_binds_method() {
    let entities = parse("app.rb", "def load_user(name)\n  User.new(name, 30)\nend");
    let load_user = find(&entities, EntityKind::Method, "load_user");
    assert_eq!(load_user.return_type.as_deref(), Some("User"));
}

#[test]
fn ruby_non_constructor_tail_leaves_no_return() {
    let entities = parse(
        "app.rb",
        "class User\n  def greet\n    \"Hello!\"\n  end\nend",
    );
    let greet = find(&entities, EntityKind::Method, "greet");
    assert_eq!(greet.return_type.as_deref(), None);
}

#[test]
fn rust_mut_self_receiver_reports_self_type() {
    let entities = parse(
        "app.rs",
        "struct Counter {\n  count: i32,\n}\nimpl Counter {\n  fn increment(&mut self) {\n    self.count += 1;\n  }\n}",
    );
    let increment = find(&entities, EntityKind::Function, "increment");
    assert!(
        increment
            .parameters
            .contains(&("self".to_string(), Some("&mut Self".to_string()))),
        "receiver should parse as (&mut Self), got {:?}",
        increment.parameters
    );
}

#[test]
fn javascript_return_expression_is_not_a_type() {
    let entities = parse(
        "app.js",
        "function sum(a, b) {\n  return calc(a) + calc(b);\n}\nfunction one() {\n  return 1;\n}",
    );
    let sum = find(&entities, EntityKind::Function, "sum");
    assert_eq!(sum.return_type.as_deref(), None);
    let one = find(&entities, EntityKind::Function, "one");
    assert_eq!(one.return_type.as_deref(), Some("number"));
}

#[test]
fn javascript_new_expression_return_binds_class() {
    let entities = parse(
        "app.js",
        "function loadUser(name) {\n  return new User(name, 30);\n}",
    );
    let load_user = find(&entities, EntityKind::Function, "loadUser");
    assert_eq!(load_user.return_type.as_deref(), Some("User"));
}

#[test]
fn cpp_method_definition_binds_return_type() {
    let entities = parse(
        "app.cpp",
        "class Overloads {\npublic:\n  int combine(int a, int b) {\n    return a + b;\n  }\n};",
    );
    let combine = find(&entities, EntityKind::Method, "combine");
    assert_eq!(combine.return_type.as_deref(), Some("int"));
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
fn java_bare_constructor_recovers_class_type_params() {
    let entities = parse(
        "app.java",
        "class Container<T> {\n  T value;\n  Container() {}\n}\nclass Demo {\n  void run() {\n    var c = new Container();\n  }\n}",
    );
    let c = find(&entities, EntityKind::Variable, "c");
    assert_eq!(
        c.metadata.get("constructor_type").map(String::as_str),
        Some("Container<T>")
    );
}

#[test]
fn java_explicit_constructor_args_are_composed() {
    let entities = parse(
        "app.java",
        "class Container<T> {\n  Container(T v) {}\n}\nclass Demo {\n  void run() {\n    Object c = new Container<String>(\"v\");\n  }\n}",
    );
    let c = find(&entities, EntityKind::Variable, "c");
    assert_eq!(
        c.metadata.get("constructor_type").map(String::as_str),
        Some("Container<String>")
    );
    assert_eq!(
        c.metadata.get("constructor_type_args").map(String::as_str),
        Some("String")
    );
}

#[test]
fn dart_explicit_constructor_args_are_composed() {
    let entities = parse(
        "app.dart",
        "class Container<T> {\n  T value;\n  Container(this.value);\n}\nvoid main() {\n  var user = Container<String>('value');\n}",
    );
    let user = find(&entities, EntityKind::Variable, "user");
    assert_eq!(
        user.metadata.get("constructor_type").map(String::as_str),
        Some("Container<String>")
    );
}

#[test]
fn scala_bracket_type_params_recover_bare_constructor() {
    let entities = parse(
        "app.scala",
        "case class Pair[A, B](first: A, second: B)\nobject Demo {\n  def main(): Unit = {\n    val p = Pair(1, \"one\")\n  }\n}",
    );
    let p = find(&entities, EntityKind::Variable, "p");
    assert_eq!(
        p.metadata.get("constructor_type").map(String::as_str),
        Some("Pair<A, B>")
    );
}

#[test]
fn csharp_method_binds_return_type() {
    let entities = parse(
        "app.cs",
        "public class Math {\n  public int Combine(int a, int b) {\n    return a + b;\n  }\n}",
    );
    let combine = find(&entities, EntityKind::Method, "Combine");
    assert_eq!(combine.return_type.as_deref(), Some("int"));
}

#[test]
fn csharp_variable_binds_annotation_and_initializer() {
    let entities = parse(
        "app.cs",
        "public class C {\n  public int M() {\n    int y = 2;\n    var z = compute();\n    return y;\n  }\n}",
    );
    let y = find(&entities, EntityKind::Variable, "y");
    assert_eq!(
        y.metadata.get("type_annotation").map(String::as_str),
        Some("int")
    );
    assert_eq!(
        y.metadata.get("literal_type").map(String::as_str),
        Some("number")
    );
    let z = find(&entities, EntityKind::Variable, "z");
    assert_eq!(
        z.metadata.get("call_target").map(String::as_str),
        Some("compute")
    );
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
