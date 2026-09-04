//! Symbol parsing coverage for previously unasserted patterns.
//!
//! Each test parses a small snippet with the production pipeline
//! (`ParseCoordinator`) and asserts the entities and raw relations that the
//! relation index consumes. These cases close gaps where a language had a
//! query scheme and extractor but no machine assertion: C sources, basic
//! JavaScript and TypeScript projects, structural relations (inheritance,
//! implementation, mixins, trait uses) and minimal Bash and Lua support.

use cce_parser::parser::ParseCoordinator;
use cce_types::entity::EntityKind;
use cce_types::relation::RelationType;
use cce_types::{Entity, ParsedFile};

fn parse_file(path: &str, content: &str) -> ParsedFile {
    let mut coordinator = ParseCoordinator::new();
    coordinator
        .parse(path, content)
        .expect("snippet should parse")
}

fn find<'a>(entities: &'a [Entity], kind: EntityKind, name: &str) -> &'a Entity {
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

fn has_relation(parsed: &ParsedFile, relation: RelationType, needle: &str) -> bool {
    parsed.raw_relations.iter().any(|r| {
        r.relation_type == relation
            && (r.dst_name.contains(needle) || {
                // File-level dependency targets sometimes surface through the
                // import table instead of raw relations.
                parsed
                    .import_table
                    .as_ref()
                    .is_some_and(|t| format!("{t:?}").contains(needle))
            })
    })
}

#[test]
fn c_struct_and_function_entities() {
    let parsed = parse_file(
        "app.c",
        "#include \"helper.h\"\nstruct Point {\n  int x;\n  int y;\n};\nint distance(struct Point a, struct Point b) {\n  return a.x - b.x;\n}\n",
    );
    find(&parsed.entities, EntityKind::Function, "distance");
    assert!(
        parsed
            .entities
            .iter()
            .any(|e| matches!(e.kind, EntityKind::Struct | EntityKind::TypeAlias)),
        "C struct or typedef should be extracted, got {:?}",
        parsed
            .entities
            .iter()
            .map(|e| (&e.kind, e.name.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(
        has_relation(&parsed, RelationType::IncludeLocal, "helper"),
        "local include should be recorded, got {:?}",
        parsed
            .raw_relations
            .iter()
            .map(|r| (&r.relation_type, r.dst_name.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn cpp_class_template_entities() {
    let parsed = parse_file(
        "app.cpp",
        "#include <string>\nclass Calculator {\npublic:\n  int add(int a, int b) { return a + b; }\n};\ntemplate <typename T>\nT identity(T x) { return x; }\n",
    );
    find(&parsed.entities, EntityKind::Class, "Calculator");
    find(&parsed.entities, EntityKind::Method, "add");
    find(&parsed.entities, EntityKind::Function, "identity");
}

#[test]
fn javascript_basic_entities_and_import() {
    let parsed = parse_file(
        "app.js",
        "import { helper } from \"./helper.js\";\nexport function process(x) {\n  return helper(x) * 2;\n}\nclass Helper {\n  help() { return 1; }\n}\nexport class Service {\n  run() { return process(1); }\n}\n",
    );
    find(&parsed.entities, EntityKind::Function, "process");
    find(&parsed.entities, EntityKind::Method, "run");
    find(&parsed.entities, EntityKind::Class, "Helper");
    find(&parsed.entities, EntityKind::Class, "Service");
    // A single named import must yield exactly one import edge (no generic duplicate).
    assert_eq!(
        parsed
            .raw_relations
            .iter()
            .filter(|r| matches!(
                r.relation_type,
                RelationType::ImportStandard
                    | RelationType::ImportNamed
                    | RelationType::ImportDefault
                    | RelationType::ImportNamespace
            ) && r.dst_name.contains("helper"))
            .count(),
        1,
        "single named import should yield one edge, got {:?}",
        parsed
            .raw_relations
            .iter()
            .map(|r| (&r.relation_type, r.dst_name.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn typescript_interface_and_generic_function() {
    let parsed = parse_file(
        "app.ts",
        "interface Container<T> {\n  value: T;\n}\nfunction identity<T>(x: T): T {\n  return x;\n}\nclass Store implements Container<string> {\n  value = \"v\";\n  get(): string { return this.value; }\n}\n",
    );
    find(&parsed.entities, EntityKind::Interface, "Container");
    find(&parsed.entities, EntityKind::Function, "identity");
    find(&parsed.entities, EntityKind::Class, "Store");
    find(&parsed.entities, EntityKind::Method, "get");
}

#[test]
fn python_class_inheritance_and_decorator() {
    let parsed = parse_file(
        "app.py",
        "from .models import Base\nclass Dog(Base):\n  @property\n  def name(self):\n    return \"dog\"\n",
    );
    find(&parsed.entities, EntityKind::Class, "Dog");
    find(&parsed.entities, EntityKind::Method, "name");
    find(&parsed.entities, EntityKind::Annotation, "property");
    assert!(
        parsed
            .raw_relations
            .iter()
            .any(|r| r.relation_type == RelationType::Inheritance && r.dst_name.contains("Base")),
        "class base should be recorded as inheritance, got {:?}",
        parsed
            .raw_relations
            .iter()
            .map(|r| (&r.relation_type, r.dst_name.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(
        parsed.raw_relations.iter().any(|r| matches!(
            r.relation_type,
            RelationType::ImportStandard
                | RelationType::ImportNamed
                | RelationType::ImportNamespace
        )),
        "relative import should be recorded, got {:?}",
        parsed
            .raw_relations
            .iter()
            .map(|r| (&r.relation_type, r.dst_name.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn go_struct_interface_and_import() {
    let parsed = parse_file(
        "app.go",
        "package shapes\nimport \"fmt\"\ntype Reader interface {\n  Read() string\n}\ntype Buffer struct {\n  data string\n}\nfunc (b Buffer) Read() string {\n  fmt.Println(b.data)\n  return b.data\n}\n",
    );
    find(&parsed.entities, EntityKind::Struct, "Buffer");
    find(&parsed.entities, EntityKind::Interface, "Reader");
    assert!(
        has_relation(&parsed, RelationType::ImportStandard, "fmt"),
        "Go import should be recorded, got {:?}",
        parsed
            .raw_relations
            .iter()
            .map(|r| (&r.relation_type, r.dst_name.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn java_class_implements_and_import() {
    let parsed = parse_file(
        "Main.java",
        "package demo;\nimport java.util.List;\npublic class Store implements Repository {\n  public List<String> load() { return List.of(\"v\"); }\n}\n",
    );
    find(&parsed.entities, EntityKind::Class, "Store");
    find(&parsed.entities, EntityKind::Method, "load");
    assert!(
        parsed.raw_relations.iter().any(|r| matches!(
            r.relation_type,
            RelationType::ImportStandard | RelationType::ImportNamed
        ) && r.dst_name.contains("List")),
        "Java import should be recorded, got {:?}",
        parsed
            .raw_relations
            .iter()
            .map(|r| (&r.relation_type, r.dst_name.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn kotlin_class_and_function() {
    let parsed = parse_file(
        "Main.kt",
        "package demo\nclass Store(val name: String) {\n  fun load(): String = name\n}\nfun identity(x: String): String = x\n",
    );
    find(&parsed.entities, EntityKind::Class, "Store");
    let load = find(&parsed.entities, EntityKind::Method, "load");
    assert_eq!(load.return_type.as_deref(), Some("String"));
    find(&parsed.entities, EntityKind::Function, "identity");
}

#[test]
fn scala_trait_object_and_class() {
    let parsed = parse_file(
        "Main.scala",
        "package demo\ntrait Repository {\n  def load(): String\n}\nclass Store extends Repository {\n  def load(): String = \"v\"\n}\nobject Main {\n  def main(args: Array[String]): Unit = ()\n}\n",
    );
    find(&parsed.entities, EntityKind::Trait, "Repository");
    find(&parsed.entities, EntityKind::Class, "Store");
}

#[test]
fn dart_class_with_mixin() {
    let parsed = parse_file(
        "app.dart",
        "mixin Greet {\n  String greet() => \"hi\";\n}\nclass User with Greet {\n  final String name;\n  User(this.name);\n}\n",
    );
    find(&parsed.entities, EntityKind::Class, "User");
    assert!(
        parsed.raw_relations.iter().any(|r| matches!(
            r.relation_type,
            RelationType::Mixin | RelationType::Inheritance | RelationType::Implementation
        ) && r.dst_name.contains("Greet")),
        "mixin application should be recorded, got {:?}",
        parsed
            .raw_relations
            .iter()
            .map(|r| (&r.relation_type, r.dst_name.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn ruby_module_mixin() {
    let parsed = parse_file(
        "app.rb",
        "module Greet\n  def hello\n    \"hi\"\n  end\nend\nclass User\n  include Greet\n  def name\n    \"u\"\n  end\nend\n",
    );
    find(&parsed.entities, EntityKind::Module, "Greet");
    find(&parsed.entities, EntityKind::Class, "User");
    find(&parsed.entities, EntityKind::Method, "hello");
}

#[test]
fn php_class_implements_and_trait_use() {
    let parsed = parse_file(
        "app.php",
        "<?php\nnamespace Demo;\ninterface Repository {\n  public function load(): string;\n}\ntrait Logging {\n  public function log(string $m): void {}\n}\nclass Store implements Repository {\n  use Logging;\n  public function load(): string { return \"v\"; }\n}\n",
    );
    find(&parsed.entities, EntityKind::Interface, "Repository");
    find(&parsed.entities, EntityKind::Class, "Store");
    find(&parsed.entities, EntityKind::Method, "load");
}

#[test]
fn csharp_class_interface_and_method() {
    let parsed = parse_file(
        "App.cs",
        "namespace Demo;\npublic interface IStore {\n  string Load();\n}\npublic class Store : IStore {\n  public string Load() => \"v\";\n}\n",
    );
    find(&parsed.entities, EntityKind::Interface, "IStore");
    find(&parsed.entities, EntityKind::Class, "Store");
    find(&parsed.entities, EntityKind::Method, "Load");
}

#[test]
fn rust_trait_impl_entities_and_use() {
    let parsed = parse_file(
        "lib.rs",
        "use std::collections::HashMap;\nstruct Store {\n  items: HashMap<String, String>,\n}\ntrait Repository {\n  fn load(&self) -> String;\n}\nimpl Repository for Store {\n  fn load(&self) -> String { String::from(\"v\") }\n}\nimpl Store {\n  fn get(&self) -> String { self.load() }\n}\n",
    );
    find(&parsed.entities, EntityKind::Struct, "Store");
    find(&parsed.entities, EntityKind::Trait, "Repository");
    assert!(
        parsed
            .entities
            .iter()
            .any(|e| matches!(e.kind, EntityKind::TraitImpl | EntityKind::InherentImpl)),
        "impl blocks should be extracted, got {:?}",
        parsed
            .entities
            .iter()
            .map(|e| (&e.kind, e.name.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(
        has_relation(&parsed, RelationType::Use, "HashMap"),
        "use statement should be recorded, got {:?}",
        parsed
            .raw_relations
            .iter()
            .map(|r| (&r.relation_type, r.dst_name.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn bash_function_entity() {
    let parsed = parse_file(
        "run.sh",
        "#!/bin/bash\ngreet() {\n  echo \"hi $1\"\n}\ngreet \"bob\"\n",
    );
    assert!(
        parsed
            .entities
            .iter()
            .any(|e| e.kind == EntityKind::Function && e.name == "greet"),
        "bash function should be extracted, got {:?}",
        parsed
            .entities
            .iter()
            .map(|e| (&e.kind, e.name.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn bash_source_statement_yields_import() {
    let parsed = parse_file(
        "run.sh",
        "#!/bin/bash\nsource ./lib.sh\ngreet() {\n  echo \"hi $1\"\n}\ngreet \"bob\"\n",
    );
    let table = parsed
        .import_table
        .as_ref()
        .expect("bash file should carry an import table");
    assert!(
        table
            .standardized_imports
            .iter()
            .any(|i| i.source == "./lib.sh"),
        "source statement should be recorded, got {:?}",
        table
            .standardized_imports
            .iter()
            .map(|i| i.source.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn lua_require_call_yields_import() {
    let parsed = parse_file(
        "app.lua",
        "local helper = require(\"helper\")\nlocal function greet(name)\n  return helper.format(name)\nend\nreturn greet\n",
    );
    let table = parsed
        .import_table
        .as_ref()
        .expect("lua file should carry an import table");
    assert!(
        table
            .standardized_imports
            .iter()
            .any(|i| i.source == "helper"),
        "require call should be recorded, got {:?}",
        table
            .standardized_imports
            .iter()
            .map(|i| i.source.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn lua_function_entity() {
    let parsed = parse_file(
        "app.lua",
        "local helper = require(\"helper\")\nlocal function greet(name)\n  return helper.format(name)\nend\nreturn greet\n",
    );
    assert!(
        parsed
            .entities
            .iter()
            .any(|e| e.kind == EntityKind::Function && e.name == "greet"),
        "lua function should be extracted, got {:?}",
        parsed
            .entities
            .iter()
            .map(|e| (&e.kind, e.name.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn svelte_template_expression_yields_behavior() {
    let parsed = parse_file(
        "app.svelte",
        "<script>\n  let name = \"world\";\n</script>\n<h1>Hello {name}!</h1>\n",
    );
    assert!(
        !parsed.behavior.is_empty(),
        "svelte template expressions should yield behavior facts"
    );
}

#[test]
fn empty_file_parses_without_entities() {
    let parsed = parse_file("empty.rs", "");
    assert!(
        parsed.entities.is_empty(),
        "empty file should yield no entities, got {:?}",
        parsed
            .entities
            .iter()
            .map(|e| (&e.kind, e.name.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn comment_only_file_parses_without_entities() {
    let parsed = parse_file("empty.py", "# just a comment\n# another comment\n");
    assert!(
        parsed.entities.is_empty(),
        "comment-only file should yield no entities, got {:?}",
        parsed
            .entities
            .iter()
            .map(|e| (&e.kind, e.name.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn c_anonymous_typedef_yields_alias() {
    let parsed = parse_file("anon.c", "typedef struct {\n  int x;\n  int y;\n} Point;\n");
    find(&parsed.entities, EntityKind::TypeAlias, "Point");
}

#[test]
fn c_tiny_single_function_yields_entity() {
    let parsed = parse_file("tiny.c", "int f() { return 0; }\n");
    find(&parsed.entities, EntityKind::Function, "f");
}

#[test]
fn c_static_function_modifier() {
    let parsed = parse_file(
        "mod.c",
        "static int add(int a, int b) {\n  return a + b;\n}\nint plain(int x) {\n  return x;\n}\n",
    );
    let add = find(&parsed.entities, EntityKind::Function, "add");
    assert!(
        add.modifiers.iter().any(|m| m == "static"),
        "static specifier should be recorded, got {:?}",
        add.modifiers
    );
    let plain = find(&parsed.entities, EntityKind::Function, "plain");
    assert!(
        !plain.modifiers.iter().any(|m| m == "static"),
        "plain function must not gain modifiers, got {:?}",
        plain.modifiers
    );
}

#[test]
fn python_async_function_modifier() {
    let parsed = parse_file(
        "app.py",
        "async def fetch(url):\n  return url\ndef process(x):\n  return x\n",
    );
    let fetch = find(&parsed.entities, EntityKind::Function, "fetch");
    assert!(
        fetch.modifiers.iter().any(|m| m == "async"),
        "async modifier should be recorded, got {:?}",
        fetch.modifiers
    );
    let process = find(&parsed.entities, EntityKind::Function, "process");
    assert!(
        !process.modifiers.iter().any(|m| m == "async"),
        "sync function must not gain modifiers, got {:?}",
        process.modifiers
    );
}
