//! Dart AST parsing tool
//!
//! This tool parses Dart code snippets and prints the AST structure
//! to help understand the correct field names and node types.
//!
//! Usage: cargo run --bin parse_dart

use tree_sitter::Parser;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let test_name = if args.len() > 1 { &args[1] } else { "all" };

    let mut parser = Parser::new();
    let language = tree_sitter_dart::LANGUAGE.into();
    parser
        .set_language(&language)
        .expect("Failed to set language");

    let test_cases = get_test_cases();

    for (name, code) in test_cases {
        if test_name == "all" || test_name == name {
            println!("\n========================================");
            println!("Test: {}", name);
            println!("Code: {}", code);
            println!("========================================\n");

            let tree = parser.parse(code, None).expect("Failed to parse");
            print_tree(tree.root_node(), code, 0);
        }
    }
}

fn get_test_cases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("import", r#"import 'package:flutter/material.dart';"#),
        ("import_alias", r#"import 'dart:core' as core;"#),
        ("export", r#"export 'test.dart';"#),
        ("part", r#"part 'test.dart';"#),
        ("part_of", r#"part of 'lib.dart';"#),
        ("shift_left", r#"void f() { int x = 1 << 2; }"#),
        ("shift_right", r#"void f() { int x = 8 >> 1; }"#),
        ("less_than", r#"void f() { bool b = x < 0; }"#),
    ]
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
