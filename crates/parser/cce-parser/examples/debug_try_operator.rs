//! Debug tool: verify tree-sitter `try_expression` / `try_statement` parsing across languages
//!
//! Parses code snippets containing error-propagation operators (`?`, `try`, etc.)
//! and prints the AST to confirm correct node kind detection.
//!
//! Usage:
//!   cargo run --example debug_try_operator -p cce-parser

use tree_sitter::Node;
use tree_sitter::Parser;

fn main() {
    let snippets: Vec<(&str, &str, tree_sitter::Language)> = vec![
        (
            "Rust",
            "fn f() -> Result<i32, ()> { Ok(42?) }",
            tree_sitter_rust::LANGUAGE.into(),
        ),
        (
            "Rust chain",
            "fn f() -> Result<i32, ()> { let x = foo()?.bar()?; Ok(x) }",
            tree_sitter_rust::LANGUAGE.into(),
        ),
        (
            "Rust index_sidecar",
            r#"fn f() -> Result<usize, std::io::Error> {
    let _ = std::result::Result::<usize, std::io::Error>::Ok(0)?;
    Ok(0)
}"#,
            tree_sitter_rust::LANGUAGE.into(),
        ),
        (
            "Kotlin",
            "fun f(): Int = try { 42 } catch (e: Exception) { 0 }",
            tree_sitter_kotlin_ng::LANGUAGE.into(),
        ),
        (
            "Scala",
            "def f: Int = try { 42 } catch { case _ => 0 }",
            tree_sitter_scala::LANGUAGE.into(),
        ),
        (
            "JavaScript",
            "try { risky(); } catch (e) { handle(e); }",
            tree_sitter_javascript::LANGUAGE.into(),
        ),
        (
            "TypeScript",
            "try { risky(); } catch (e) { handle(e); }",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        ),
        (
            "Python",
            "try:\n    risky()\nexcept:\n    pass",
            tree_sitter_python::LANGUAGE.into(),
        ),
        (
            "Java",
            "void f() { try { risky(); } catch (Exception e) {} }",
            tree_sitter_java::LANGUAGE.into(),
        ),
        (
            "C#",
            "void f() { try { risky(); } catch (Exception e) {} }",
            tree_sitter_c_sharp::LANGUAGE.into(),
        ),
        (
            "C++",
            "void f() { try { risky(); } catch (int e) {} }",
            tree_sitter_cpp::LANGUAGE.into(),
        ),
        (
            "Go",
            "func f() (int, error) { return 0, nil }",
            tree_sitter_go::LANGUAGE.into(),
        ),
        (
            "Dart",
            "void f() { try { risky(); } catch (e) {} }",
            tree_sitter_dart::LANGUAGE.into(),
        ),
        (
            "PHP",
            "<?php try { risky(); } catch (Exception $e) {}",
            tree_sitter_php::LANGUAGE_PHP.into(),
        ),
        (
            "Ruby",
            "begin\n  risky()\nrescue\nend",
            tree_sitter_ruby::LANGUAGE.into(),
        ),
        (
            "Bash",
            "if true; then echo ok; fi",
            tree_sitter_bash::LANGUAGE.into(),
        ),
        (
            "Lua",
            "local ok, err = pcall(risky)",
            tree_sitter_lua::LANGUAGE.into(),
        ),
    ];

    for (name, code, language) in snippets {
        println!("========================================");
        println!("Language: {}", name);
        println!("Code: {}", code);
        println!("----------------------------------------");
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(code, None).unwrap();
        print_node(tree.root_node(), 0, code);
        println!();
    }
}

fn print_node(node: Node, depth: usize, source: &str) {
    let indent = "  ".repeat(depth);
    let text = node.utf8_text(source.as_bytes()).unwrap_or("");
    let text_preview = if text.len() > 50 {
        format!(" {:?}...", &text[..50])
    } else if !text.is_empty() {
        format!(" {:?}", text)
    } else {
        String::new()
    };
    let is_try = node.kind().contains("try");
    let has_q = text.contains('?');
    println!(
        "{}{} [{}, {}]{}{}",
        indent,
        node.kind(),
        node.start_position(),
        node.end_position(),
        text_preview,
        if is_try || has_q { " ← TRY/?" } else { "" },
    );
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        print_node(child, depth + 1, source);
    }
}
