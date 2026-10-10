//! Signature audit script: report empty and overlong entity signatures.
//!
//! Walks source files, extracts entities per language, and prints an
//! observational report (no assertions):
//! - per-language entity counts
//! - empty signatures on body-owning kinds (query gaps)
//! - signatures longer than a threshold (default 200 chars, real bloat
//!   candidates for manual review)
//!
//! Usage:
//!   cargo run --example sig_audit -p cce-parser -- [DIR...] [--threshold N]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use cce_parser::parser::{AstParser, EntityExtractor};
use cce_types::language::Language;

fn language_for(path: &Path) -> Option<Language> {
    match path.extension().and_then(|e| e.to_str())? {
        "py" => Some(Language::Python),
        "rs" => Some(Language::Rust),
        "js" | "jsx" | "mjs" | "cjs" => Some(Language::JavaScript),
        "ts" | "mts" | "cts" => Some(Language::TypeScript),
        "tsx" => Some(Language::Tsx),
        "go" => Some(Language::Go),
        "java" => Some(Language::Java),
        "c" | "h" => Some(Language::C),
        "cpp" | "hpp" | "cc" | "cxx" | "hxx" => Some(Language::Cpp),
        "cs" => Some(Language::CSharp),
        "rb" => Some(Language::Ruby),
        "php" => Some(Language::Php),
        "kt" | "kts" => Some(Language::Kotlin),
        "dart" => Some(Language::Dart),
        "scala" => Some(Language::Scala),
        "lua" => Some(Language::Lua),
        "sh" | "bash" => Some(Language::Bash),
        _ => None,
    }
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else if path.is_file() && language_for(&path).is_some() {
            out.push(path);
        }
    }
}

fn main() {
    let mut roots: Vec<String> = Vec::new();
    let mut threshold: usize = 200;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--threshold" {
            if let Some(value) = args.next() {
                threshold = value.parse().unwrap_or(threshold);
            }
        } else {
            roots.push(arg);
        }
    }
    if roots.is_empty() {
        roots.push("crates/app/cce-e2e-tests/fixtures".to_string());
    }

    let mut files = Vec::new();
    for root in &roots {
        let path = PathBuf::from(root);
        if path.is_file() {
            files.push(path);
        } else {
            collect_files(&path, &mut files);
        }
    }
    files.sort();

    let mut parser = AstParser::new();
    let extractor = EntityExtractor::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut empty: Vec<String> = Vec::new();
    let mut overlong: Vec<(usize, String)> = Vec::new();
    let mut parse_failures = 0usize;
    let mut files_scanned = 0usize;

    for file in &files {
        let language = match language_for(file) {
            Some(language) => language,
            None => continue,
        };
        let source = match fs::read_to_string(file) {
            Ok(source) => source,
            Err(_) => continue,
        };
        files_scanned += 1;
        let tree = match parser.parse_with_tree(&source, &language) {
            Ok(parsed) => parsed.0,
            Err(_) => {
                parse_failures += 1;
                continue;
            }
        };
        let entities = match extractor.extract(&tree, &source, &language) {
            Ok(entities) => entities,
            Err(_) => {
                parse_failures += 1;
                continue;
            }
        };
        for entity in &entities {
            *counts
                .entry(format!("{:?}/{:?}", language, entity.kind))
                .or_insert(0) += 1;
            let len = entity.signature.chars().count();
            if entity.signature.trim().is_empty() {
                empty.push(format!(
                    "{} {:?} {:?} L{}",
                    file.display(),
                    entity.kind,
                    entity.name,
                    entity.span.start_position.row + 1
                ));
            } else if len > threshold {
                let preview: String = entity.signature.chars().take(120).collect();
                overlong.push((
                    len,
                    format!(
                        "{} {:?} {:?} len={len} {preview:?}",
                        file.display(),
                        entity.kind,
                        entity.name
                    ),
                ));
            }
        }
    }

    overlong.sort_by_key(|b| std::cmp::Reverse(b.0));

    println!("files={files_scanned} parse_failures={parse_failures}");
    println!("--- entities per language/kind ---");
    for (key, count) in &counts {
        println!("{count:>6}  {key}");
    }
    println!("--- empty signatures ({}) ---", empty.len());
    for line in empty.iter().take(100) {
        println!("EMPTY  {line}");
    }
    println!(
        "--- signatures over {threshold} chars ({}) ---",
        overlong.len()
    );
    for (_, line) in overlong.iter().take(100) {
        println!("LONG  {line}");
    }
}
