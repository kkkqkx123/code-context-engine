//! Scala AST parsing tool
//!
//! This tool parses Scala code snippets and prints the AST structure
//! to help understand the correct field names and node types.
//!
//! Usage: cargo run --bin parse_scala

use tree_sitter::Parser;

fn main() {
    let test_cases = vec![
        ("Class Definition", r#"class MyClass {
  def method(): Int = 42
}"#),
        ("Trait Definition", r#"trait MyTrait {
  def method(): Int
}"#),
        ("Object Definition", r#"object MyObject {
  val constant = 42
}"#),
        ("Case Class", r#"case class Person(name: String, age: Int)"#),
        ("Function Definition", r#"def add(a: Int, b: Int): Int = a + b"#),
        ("Val Definition", r#"val x = 10"#),
        ("Var Definition", r#"var y = 20"#),
        ("Package Declaration", r#"package com.example"#),
        ("Import Declaration", r#"import scala.collection.mutable"#),
        ("Import with Selectors", r#"import scala.collection.{Map, Set}"#),
        ("Inheritance", r#"class Child extends Parent with Trait1 with Trait2"#),
        ("Method Call", r#"obj.method(arg)"#),
        ("Constructor Call", r#"new MyClass(arg)"#),
        ("Lambda", r#"(x: Int) => x * 2"#),
        ("Enum (Scala 3)", r#"enum Color:
  case Red, Green, Blue"#),
    ];

    let mut parser = Parser::new();
    let language = tree_sitter_scala::LANGUAGE.into();
    parser
        .set_language(&language)
        .expect("Failed to set language");

    for (name, code) in test_cases {
        println!("\n========================================");
        println!("Test: {}", name);
        println!("Code: {}", code);
        println!("========================================\n");

        let tree = parser.parse(code, None).expect("Failed to parse");
        print_tree(tree.root_node(), code, 0);
    }
}

fn print_tree(node: tree_sitter::Node, source: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    let text = if node.child_count() == 0 {
        let start = node.start_byte();
        let end = node.end_byte();
        &source[start..end]
    } else {
        ""
    };

    println!("{}{}: {:?}", indent, node.kind(), text);

    // Print field names if any
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            if let Some(field_name) = node.field_name_for_child(i as u32) {
                println!("{}  [field: {}]", indent, field_name);
            }
            print_tree(child, source, depth + 1);
        }
    }
}
