//! Debug tool: reproduce tree-sitter query false negatives from the issue list
//!
//! For each reported case it prints the real syntax tree (with field names)
//! and then runs the current cce scheme query (via public QueryLoader),
//! reporting captures per match.
//!
//! Usage:
//!   cargo run --example debug_issue_queries -p cce-parser

use cce_parser::tree_sitter_query::{QueryLoader, QueryType};
use cce_types::Language;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Parser, QueryCursor};

fn print_tree(node: Node, source: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    let named = if node.is_named() { "" } else { " (anonymous)" };
    let text = if node.child_count() == 0 {
        format!(" {:?}", &source[node.start_byte()..node.end_byte()])
    } else {
        String::new()
    };
    println!("{}{}{}{}", indent, node.kind(), named, text);
    let mut cursor = node.walk();
    for (i, child) in node.children(&mut cursor).enumerate() {
        if let Some(field) = node.field_name_for_child(i as u32) {
            println!("{}  [field: {}]", indent, field);
        }
        print_tree(child, source, depth + 1);
    }
}

fn ts_lang(lang: &Language) -> tree_sitter::Language {
    match lang {
        Language::Go => tree_sitter_go::LANGUAGE.into(),
        Language::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        _ => panic!("unsupported"),
    }
}

fn run_query(title: &str, lang: &Language, qtype: QueryType, code: &str, loader: &QueryLoader) {
    println!("----------------------------------------");
    println!("Query run: {}", title);
    println!("Code:\n{}", code);
    let query = match loader.get_query(lang, qtype) {
        Ok(q) => q,
        Err(e) => {
            println!("QUERY LOAD ERROR: {:?}", e);
            return;
        }
    };
    let mut parser = Parser::new();
    parser.set_language(&ts_lang(lang)).unwrap();
    let tree = parser.parse(code, None).unwrap();
    let names = query.capture_names().to_vec();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), code.as_bytes());
    let mut n = 0;
    while let Some(m) = matches.next() {
        n += 1;
        let mut caps = vec![];
        for c in m.captures {
            let node = c.node;
            caps.push(format!(
                "{} = {:?}",
                names[c.index as usize],
                &code[node.start_byte()..node.end_byte()]
            ));
        }
        println!(
            "  match #{} (pattern {}): {}",
            n,
            m.pattern_index,
            caps.join(" | ")
        );
    }
    println!("  total matches: {}", n);
}

fn dump_ast(title: &str, lang: &Language, code: &str) {
    println!("========================================");
    println!("AST: {}", title);
    println!("Code:\n{}", code);
    println!("----------------------------------------");
    let mut parser = Parser::new();
    parser.set_language(&ts_lang(lang)).unwrap();
    let tree = parser.parse(code, None).unwrap();
    print_tree(tree.root_node(), code, 0);
    println!();
}

fn main() {
    let loader = QueryLoader::new();
    let go_lang = Language::Go;
    let cs_lang = Language::CSharp;
    let rs_lang = Language::Rust;

    // ---- Issue 1: Go grouped imports ----
    let go_single = "package main\n\nimport \"fmt\"\n\nfunc main() { fmt.Println(\"hi\") }\n";
    let go_grouped = "package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n\talias \"example.com/foo\"\n)\n\nfunc main() { fmt.Println(\"hi\") }\n";
    dump_ast("Go single import", &go_lang, go_single);
    dump_ast("Go grouped import", &go_lang, go_grouped);
    run_query(
        "Go dep / single import",
        &go_lang,
        QueryType::Dependency,
        go_single,
        &loader,
    );
    run_query(
        "Go dep / grouped import",
        &go_lang,
        QueryType::Dependency,
        go_grouped,
        &loader,
    );
    run_query(
        "Go entity / grouped import",
        &go_lang,
        QueryType::Entity,
        go_grouped,
        &loader,
    );

    // ---- Issue 2: C# usings ----
    let cs_usings = "using System;\nusing System.IO;\nusing Foo = Bar.Baz;\n\nclass C { }\n";
    dump_ast("C# usings", &cs_lang, cs_usings);
    run_query(
        "C# dep / usings",
        &cs_lang,
        QueryType::Dependency,
        cs_usings,
        &loader,
    );
    run_query(
        "C# entity / usings",
        &cs_lang,
        QueryType::Entity,
        cs_usings,
        &loader,
    );

    // ---- Issue 3: C# this-call ----
    let cs_calls =
        "class C {\n    void H() { }\n    void G() { this.H(); }\n    void F() { obj.M(); }\n}\n";
    dump_ast("C# this-call vs obj-call", &cs_lang, cs_calls);
    run_query(
        "C# call / this + obj calls",
        &cs_lang,
        QueryType::Call,
        cs_calls,
        &loader,
    );

    // ---- Issue 4: Rust use aliases ----
    let rs_uses = "use foo as bar;\nuse plain;\nuse a::{b, c as d};\nuse x::y::z;\n";
    dump_ast("Rust use aliases", &rs_lang, rs_uses);
    run_query(
        "Rust dep / uses",
        &rs_lang,
        QueryType::Dependency,
        rs_uses,
        &loader,
    );

    // ---- Secondary: C# type bodies ----
    let cs_types = "class A { void M() { } }\ninterface I { void M(); }\nstruct S { int x; }\nrecord R(int X);\n";
    dump_ast("C# type bodies", &cs_lang, cs_types);
    run_query(
        "C# entity / type bodies",
        &cs_lang,
        QueryType::Entity,
        cs_types,
        &loader,
    );

    // ---- Secondary: C# duplicate call hits ----
    let cs_dup = "class C { void F() { obj.M(); } }\n";
    run_query(
        "C# call / duplicate hits",
        &cs_lang,
        QueryType::Call,
        cs_dup,
        &loader,
    );
}
