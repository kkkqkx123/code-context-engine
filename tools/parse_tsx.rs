//! TSX AST parsing tool
//!
//! This tool parses TSX code snippets and prints the AST structure
//! to help understand the correct field names and node types.
//!
//! Usage: cargo run --bin parse_tsx

use tree_sitter::Parser;

fn main() {
    let test_cases = vec![
        ("JSX Attribute", r#"<Component prop="value" />"#),
        ("JSX Expression", r#"<Component>{expression}</Component>"#),
        ("JSX Event Handler", r#"<Component onClick={handler} />"#),
        ("JSX Ref", r#"<Component ref={refCallback} />"#),
        ("JSX Spread Attribute", r#"<Component {...props} />"#),
    ];

    let mut parser = Parser::new();
    let language = tree_sitter_typescript::LANGUAGE_TSX.into();
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
