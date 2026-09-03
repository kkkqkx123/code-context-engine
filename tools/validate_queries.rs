//! Query validation tool
//!
//! This script validates Tree-sitter query syntax for frontend languages
//! and reports errors without running through cargo test.
//!
//! Usage: cargo run --bin validate_queries

use std::collections::HashMap;
use tree_sitter::Query;

/// Query source to validate
struct QuerySource {
    language_name: &'static str,
    query_name: &'static str,
    query_str: &'static str,
    language: tree_sitter::Language,
}

fn main() {
    println!("========================================");
    println!("Tree-sitter Query Validation Tool");
    println!("========================================\n");

    let sources = vec![
        QuerySource {
            language_name: "HTML",
            query_name: "entity_query",
            query_str: include_str!("../src/tree_sitter_query/scheme/html.rs"),
            language: tree_sitter_html::LANGUAGE.into(),
        },
        QuerySource {
            language_name: "CSS",
            query_name: "entity_query",
            query_str: include_str!("../src/tree_sitter_query/scheme/css.rs"),
            language: tree_sitter_css::LANGUAGE.into(),
        },
        QuerySource {
            language_name: "TSX",
            query_name: "entity_query",
            query_str: include_str!("../src/tree_sitter_query/scheme/tsx.rs"),
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
        },
    ];

    let mut total_errors = 0;

    for source in &sources {
        println!(
            "Validating {} {}...",
            source.language_name, source.query_name
        );

        // Extract queries from source file
        let queries = extract_queries(source.query_str);

        for (name, query_str) in queries {
            match Query::new(&source.language, query_str) {
                Ok(_) => {
                    println!("  ✓ {} - OK", name);
                }
                Err(e) => {
                    total_errors += 1;
                    println!("  ✗ {} - ERROR", name);
                    println!(
                        "    Row: {}, Column: {}, Offset: {}",
                        e.row, e.column, e.offset
                    );
                    println!("    Message: {}", e.message);

                    // Show context with more lines
                    let lines: Vec<&str> = query_str.lines().collect();
                    if e.row > 0 && e.row <= lines.len() {
                        println!("    Context:");
                        let start = e.row.saturating_sub(5);
                        let end = (e.row + 5).min(lines.len());
                        for (idx, line) in lines.iter().enumerate().take(end).skip(start) {
                            let marker = if idx == e.row - 1 { ">>> " } else { "    " };
                            println!("{}{}: {}", marker, idx + 1, line);
                        }
                    }
                    println!();
                }
            }
        }
        println!();
    }

    if total_errors == 0 {
        println!("========================================");
        println!("✓ All queries validated successfully!");
        println!("========================================");
        std::process::exit(0);
    } else {
        println!("========================================");
        println!("✗ Found {} errors", total_errors);
        println!("========================================");
        std::process::exit(1);
    }
}

/// Extract queries from source file content
fn extract_queries(source: &str) -> HashMap<String, &str> {
    let mut queries = HashMap::new();

    // Find entity_query function
    if let Some(start) = source.find("pub fn entity_query()") {
        if let Some(query_start) = source[start..].find("r#\"") {
            let actual_start = start + query_start + 3;
            if let Some(query_end) = source[actual_start..].find("\"#") {
                queries.insert(
                    "entity_query".to_string(),
                    &source[actual_start..actual_start + query_end],
                );
            }
        }
    }

    // Find structural_query function
    if let Some(start) = source.find("pub fn structural_query()") {
        if let Some(query_start) = source[start..].find("r#\"") {
            let actual_start = start + query_start + 3;
            if let Some(query_end) = source[actual_start..].find("\"#") {
                queries.insert(
                    "structural_query".to_string(),
                    &source[actual_start..actual_start + query_end],
                );
            }
        }
    }

    // Find dependency_query function
    if let Some(start) = source.find("pub fn dependency_query()") {
        if let Some(query_start) = source[start..].find("r#\"") {
            let actual_start = start + query_start + 3;
            if let Some(query_end) = source[actual_start..].find("\"#") {
                queries.insert(
                    "dependency_query".to_string(),
                    &source[actual_start..actual_start + query_end],
                );
            }
        }
    }

    queries
}
