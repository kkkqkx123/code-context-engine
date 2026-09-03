//! Rust macro AST parsing tool
//!
//! This tool parses Rust code snippets and prints the tree-sitter AST
//! structure for macro definitions to help understand the correct node
//! types, field names, and byte ranges for macro bodies.
//!
//! Usage: cargo run --bin parse_rust_macro -- [test_name]
//!
//! Test names:
//!   message     - The `message` macro from ripgrep's messages.rs
//!   simple      - A simple macro_rules! definition
//!   nested      - A macro with nested token trees

use tree_sitter::Parser;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let test_name = if args.len() > 1 { &args[1] } else { "message" };

    let test_cases = get_test_cases();

    let Some((name, code)) = test_cases.iter().find(|(n, _)| *n == test_name) else {
        let names: Vec<&str> = test_cases.iter().map(|(n, _)| *n).collect();
        eprintln!("Unknown test: {}. Available: {}", test_name, names.join(", "));
        std::process::exit(1);
    };

    println!("Test: {}", name);
    println!("Code:\n{}\n", code);

    let mut parser = Parser::new();
    let language = tree_sitter_rust::LANGUAGE.into();
    parser
        .set_language(&language)
        .expect("Failed to set Rust language");

    let tree = parser.parse(code, None).expect("Failed to parse");

    // Print full tree
    println!("=== FULL AST ===");
    print_tree(tree.root_node(), code, 0);

    // Find and print macro_rules definitions specifically
    println!("\n=== MACRO RULES DEFINITIONS ===");
    find_macro_rules(tree.root_node(), code, 0);
}

fn get_test_cases() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "message",
            r#"macro_rules! message {
    ($($tt:tt)*) => {
        if crate::messages::messages() {
            eprintln_locked!($($tt)*);
        }
    };
}

macro_rules! err_message {
    ($($tt:tt)*) => {
        message!(err $($tt)*);
        crate::messages::set_errored();
    };
}

macro_rules! ignore_message {
    ($($tt:tt)*) => {
        if crate::messages::ignore_messages() {
            message!(note $($tt)*);
        }
    };
}"#,
        ),
        (
            "simple",
            r#"macro_rules! vec {
    ($x:expr) => {
        {
            let mut v = Vec::new();
            v.push($x);
            v
        }
    };
}"#,
        ),
        (
            "nested",
            r#"macro_rules! write {
    ($dst:expr, $($arg:tt)*) => {
        $dst.write_fmt(format_args!($($arg)*))
    };
    ($dst:expr, $($arg:tt)*) => {
        writeln!($dst, $($arg)*)
    };
}"#,
        ),
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

    println!("{}{}: {:?} [{}-{}]", indent, node.kind(), text, node.start_byte(), node.end_byte());

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            if let Some(field_name) = node.field_name_for_child(i as u32) {
                println!("{}  [field: {}]", indent, field_name);
            }
            print_tree(child, source, depth + 1);
        }
    }
}

fn find_macro_rules(node: tree_sitter::Node, source: &str, depth: usize) {
    if node.kind() == "macro_definition" {
        println!("\n--- macro_definition ---");
        println!("Byte range: [{}, {}]", node.start_byte(), node.end_byte());
        println!("Full text: {:?}", &source[node.start_byte()..node.end_byte()]);

        // Print children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                let field = node.field_name_for_child(i as u32);
                println!("\n  Child {}: kind={}, field={:?}, range=[{}, {}]", i, child.kind(), field, child.start_byte(), child.end_byte());

                if child.kind() == "token_tree" || child.kind() == "macro_rule" {
                    println!("    {} text (first 200 chars): {:?}", child.kind(), &source[child.start_byte()..(child.start_byte() + 200.min(child.end_byte() - child.start_byte()))]);

                    // Print token_tree structure
                    for j in 0..child.child_count() {
                        if let Some(grandchild) = child.child(j as u32) {
                            let field = child.field_name_for_child(j as u32);
                            let text = if grandchild.child_count() == 0 {
                                &source[grandchild.start_byte()..grandchild.end_byte()]
                            } else {
                                ""
                            };
                            println!("      {} child {}: kind={}, field={:?}, text={:?}, range=[{}, {}]",
                                child.kind(), j, grandchild.kind(), field, text, grandchild.start_byte(), grandchild.end_byte());
                        }
                    }
                } else {
                    let text = if child.child_count() == 0 {
                        &source[child.start_byte()..child.end_byte()]
                    } else {
                        ""
                    };
                    println!("    text: {:?}", text);
                }
            }
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            find_macro_rules(child, source, depth + 1);
        }
    }
}
