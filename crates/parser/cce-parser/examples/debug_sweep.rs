//! Cross-language sweep: run dependency/call queries over edge-case snippets.
//!
//! Reports per-case match lists so missing coverage (false negatives) is
//! visible at a glance. Pass a case-name substring to also dump the AST:
//!
//!   cargo run --example debug_sweep -p cce-parser
//!   cargo run --example debug_sweep -p cce-parser -- csharp-using-alias

use cce_parser::tree_sitter_init::get_tree_sitter_language;
use cce_parser::tree_sitter_query::{QueryLoader, QueryType};
use cce_types::Language;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Parser};

struct Case {
    name: &'static str,
    lang: Language,
    qtype: QueryType,
    code: &'static str,
}

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
        if let Some(f) = node.field_name_for_child(i as u32) {
            println!("{}  [field: {}]", indent, f);
        }
        print_tree(child, source, depth + 1);
    }
}

fn cases() -> Vec<Case> {
    vec![
        // ---------- Go ----------
        Case {
            name: "go-import-single",
            lang: Language::Go,
            qtype: QueryType::Dependency,
            code: "package p\n\nimport \"fmt\"\n",
        },
        Case {
            name: "go-import-grouped",
            lang: Language::Go,
            qtype: QueryType::Dependency,
            code: "package p\n\nimport (\n\t\"fmt\"\n\t\"os\"\n\talias \"example.com/foo\"\n\t. \"example.com/dot\"\n\t_ \"example.com/blank\"\n)\n",
        },
        // ---------- C# ----------
        Case {
            name: "csharp-using-plain",
            lang: Language::CSharp,
            qtype: QueryType::Dependency,
            code: "using System;\n",
        },
        Case {
            name: "csharp-using-qualified",
            lang: Language::CSharp,
            qtype: QueryType::Dependency,
            code: "using System.IO;\n",
        },
        Case {
            name: "csharp-using-alias",
            lang: Language::CSharp,
            qtype: QueryType::Dependency,
            code: "using Foo = Bar.Baz;\n",
        },
        Case {
            name: "csharp-using-static",
            lang: Language::CSharp,
            qtype: QueryType::Dependency,
            code: "using static System.Math;\n",
        },
        Case {
            name: "csharp-using-global",
            lang: Language::CSharp,
            qtype: QueryType::Dependency,
            code: "global using System;\n",
        },
        Case {
            name: "csharp-call-this",
            lang: Language::CSharp,
            qtype: QueryType::Call,
            code: "class C {\n void H() {}\n void G() { this.H(); }\n}\n",
        },
        Case {
            name: "csharp-call-base",
            lang: Language::CSharp,
            qtype: QueryType::Call,
            code: "class C : B {\n void G() { base.M(); }\n}\n class B { public void M() {} }\n",
        },
        Case {
            name: "csharp-call-obj",
            lang: Language::CSharp,
            qtype: QueryType::Call,
            code: "class C {\n void F() { obj.M(); }\n}\n",
        },
        Case {
            name: "csharp-call-static",
            lang: Language::CSharp,
            qtype: QueryType::Call,
            code: "class C {\n void F() { Math.Max(1, 2); }\n}\n",
        },
        // ---------- Rust ----------
        Case {
            name: "rust-use-plain",
            lang: Language::Rust,
            qtype: QueryType::Dependency,
            code: "use plain;\n",
        },
        Case {
            name: "rust-use-alias-ident",
            lang: Language::Rust,
            qtype: QueryType::Dependency,
            code: "use foo as bar;\n",
        },
        Case {
            name: "rust-use-alias-scoped",
            lang: Language::Rust,
            qtype: QueryType::Dependency,
            code: "use x::y as z;\n",
        },
        Case {
            name: "rust-use-scoped",
            lang: Language::Rust,
            qtype: QueryType::Dependency,
            code: "use x::y::z;\n",
        },
        Case {
            name: "rust-use-list",
            lang: Language::Rust,
            qtype: QueryType::Dependency,
            code: "use a::{b, c as d};\n",
        },
        Case {
            name: "rust-use-list-self",
            lang: Language::Rust,
            qtype: QueryType::Dependency,
            code: "use a::{self, b};\n",
        },
        Case {
            name: "rust-use-wildcard",
            lang: Language::Rust,
            qtype: QueryType::Dependency,
            code: "use a::*;\n",
        },
        Case {
            name: "rust-use-crate",
            lang: Language::Rust,
            qtype: QueryType::Dependency,
            code: "use crate::m::N;\n",
        },
        Case {
            name: "rust-call-self",
            lang: Language::Rust,
            qtype: QueryType::Call,
            code: "struct S;\nimpl S {\n fn f(&self) { self.g(); }\n fn g(&self) {}\n}\n",
        },
        Case {
            name: "rust-call-associated",
            lang: Language::Rust,
            qtype: QueryType::Call,
            code: "fn f() { let _ = Vec::new(); }\n",
        },
        // ---------- Java ----------
        Case {
            name: "java-import-single",
            lang: Language::Java,
            qtype: QueryType::Dependency,
            code: "import java.util.List;\nclass A {}\n",
        },
        Case {
            name: "java-import-static",
            lang: Language::Java,
            qtype: QueryType::Dependency,
            code: "import static java.lang.Math.*;\nclass A {}\n",
        },
        Case {
            name: "java-import-wildcard",
            lang: Language::Java,
            qtype: QueryType::Dependency,
            code: "import java.util.*;\nclass A {}\n",
        },
        Case {
            name: "java-call-this",
            lang: Language::Java,
            qtype: QueryType::Call,
            code: "class A {\n void f() { this.g(); }\n void g() {}\n}\n",
        },
        Case {
            name: "java-call-obj",
            lang: Language::Java,
            qtype: QueryType::Call,
            code: "class A {\n void f() { obj.g(); }\n}\n",
        },
        // ---------- Python ----------
        Case {
            name: "python-import",
            lang: Language::Python,
            qtype: QueryType::Dependency,
            code: "import os\n",
        },
        Case {
            name: "python-from-import",
            lang: Language::Python,
            qtype: QueryType::Dependency,
            code: "from os import path\n",
        },
        Case {
            name: "python-from-import-list",
            lang: Language::Python,
            qtype: QueryType::Dependency,
            code: "from os import (path, getcwd)\n",
        },
        Case {
            name: "python-from-relative",
            lang: Language::Python,
            qtype: QueryType::Dependency,
            code: "from . import sibling\n",
        },
        Case {
            name: "python-call-self",
            lang: Language::Python,
            qtype: QueryType::Call,
            code: "class A:\n def f(self):\n self.g()\n def g(self):\n pass\n",
        },
        // ---------- JavaScript / TypeScript ----------
        Case {
            name: "js-call-this",
            lang: Language::JavaScript,
            qtype: QueryType::Call,
            code: "class A {\n f() { this.g(); }\n g() {}\n}\n",
        },
        Case {
            name: "ts-import-named",
            lang: Language::TypeScript,
            qtype: QueryType::Dependency,
            code: "import { a, b as c } from \"./mod\";\n",
        },
        Case {
            name: "ts-import-default",
            lang: Language::TypeScript,
            qtype: QueryType::Dependency,
            code: "import d from \"./mod\";\n",
        },
        Case {
            name: "ts-import-namespace",
            lang: Language::TypeScript,
            qtype: QueryType::Dependency,
            code: "import * as ns from \"./mod\";\n",
        },
        // ---------- PHP ----------
        Case {
            name: "php-use-single",
            lang: Language::Php,
            qtype: QueryType::Dependency,
            code: "<?php\nuse Foo\\Bar;\n",
        },
        Case {
            name: "php-use-group",
            lang: Language::Php,
            qtype: QueryType::Dependency,
            code: "<?php\nuse Foo\\{Bar, Baz as B};\n",
        },
        Case {
            name: "php-call-this",
            lang: Language::Php,
            qtype: QueryType::Call,
            code: "<?php\nclass A {\n public function f() { $this->g(); }\n public function g() {}\n}\n",
        },
        // ---------- Scala ----------
        Case {
            name: "scala-import-single",
            lang: Language::Scala,
            qtype: QueryType::Dependency,
            code: "import a.b\n",
        },
        Case {
            name: "scala-import-braces",
            lang: Language::Scala,
            qtype: QueryType::Dependency,
            code: "import a.{b, c => d}\n",
        },
        Case {
            name: "scala-import-wildcard",
            lang: Language::Scala,
            qtype: QueryType::Dependency,
            code: "import a._\n",
        },
        // ---------- Kotlin ----------
        Case {
            name: "kotlin-import-single",
            lang: Language::Kotlin,
            qtype: QueryType::Dependency,
            code: "import a.b\n",
        },
        Case {
            name: "kotlin-import-alias",
            lang: Language::Kotlin,
            qtype: QueryType::Dependency,
            code: "import a.b as c\n",
        },
        // ---------- Ruby ----------
        Case {
            name: "ruby-require",
            lang: Language::Ruby,
            qtype: QueryType::Dependency,
            code: "require \"foo\"\n",
        },
        Case {
            name: "ruby-require-relative",
            lang: Language::Ruby,
            qtype: QueryType::Dependency,
            code: "require_relative \"./bar\"\n",
        },
        // ---------- Dart ----------
        Case {
            name: "dart-import",
            lang: Language::Dart,
            qtype: QueryType::Dependency,
            code: "import 'package:flutter/material.dart';\n",
        },
        Case {
            name: "dart-import-alias",
            lang: Language::Dart,
            qtype: QueryType::Dependency,
            code: "import 'dart:core' as core;\n",
        },
        // ---------- C / C++ ----------
        Case {
            name: "c-include-local",
            lang: Language::C,
            qtype: QueryType::Dependency,
            code: "#include \"foo.h\"\n",
        },
        Case {
            name: "c-include-system",
            lang: Language::C,
            qtype: QueryType::Dependency,
            code: "#include <stdio.h>\n",
        },
        Case {
            name: "cpp-using-namespace",
            lang: Language::Cpp,
            qtype: QueryType::Dependency,
            code: "using namespace std;\n",
        },
        Case {
            name: "cpp-call-this",
            lang: Language::Cpp,
            qtype: QueryType::Call,
            code: "struct A {\n void f() { this->g(); }\n void g() {}\n};\n",
        },
        // ---------- Bash / Lua ----------
        Case {
            name: "bash-source",
            lang: Language::Bash,
            qtype: QueryType::Dependency,
            code: "source ./lib.sh\n",
        },
        Case {
            name: "lua-require",
            lang: Language::Lua,
            qtype: QueryType::Dependency,
            code: "local x = require(\"foo\")\n",
        },
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let filter = args.get(1).map(|s| s.as_str()).unwrap_or("");
    let loader = QueryLoader::new();
    for case in cases() {
        if !filter.is_empty() && !case.name.contains(filter) {
            continue;
        }
        let Some(ts_lang) = get_tree_sitter_language(&case.lang) else {
            println!("=== {}: no tree-sitter language", case.name);
            continue;
        };
        println!("=== {} [{:?}/{:?}]", case.name, case.lang, case.qtype);
        println!("--- code ---\n{}", case.code);
        if !filter.is_empty() {
            let mut parser = Parser::new();
            parser.set_language(&ts_lang).unwrap();
            let tree = parser.parse(case.code, None).unwrap();
            println!("--- ast ---");
            print_tree(tree.root_node(), case.code, 0);
        }
        let query = match loader.get_query(&case.lang, case.qtype) {
            Ok(q) => q,
            Err(e) => {
                println!("QUERY LOAD ERROR: {:?}", e);
                continue;
            }
        };
        let mut parser = Parser::new();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(case.code, None).unwrap();
        let names = query.capture_names().to_vec();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), case.code.as_bytes());
        let mut n = 0;
        while let Some(m) = matches.next() {
            n += 1;
            let mut caps = vec![];
            for c in m.captures {
                let node = c.node;
                caps.push(format!(
                    "{} = {:?}",
                    names[c.index as usize],
                    &case.code[node.start_byte()..node.end_byte()]
                ));
            }
            println!(
                "  match #{} (p{}): {}",
                n,
                m.pattern_index,
                caps.join(" | ")
            );
        }
        println!("  total: {}\n", n);
    }
}
