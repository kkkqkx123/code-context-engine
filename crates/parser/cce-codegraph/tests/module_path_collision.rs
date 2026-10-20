//! Regression tests for module path uniqueness across C/C++ translation units.
//!
//! A C/C++ header and its implementation share a stem (`foo.h` / `foo.c`).
//! The module path is the identity of a translation unit, so the two files
//! must map to distinct module paths; otherwise the package module index
//! silently drops one of them and `#include "foo.h"` resolves to the wrong
//! file.

use std::collections::HashMap;
use std::path::PathBuf;

use cce_codegraph::index::builder::SymbolTableBuilder;
use cce_types::Span;
use cce_types::entity::{Entity, EntityId, EntityKind, ParsedFile};
use cce_types::language::Language;

const PROJECT_ROOT: &str = "/project/media-server";

fn make_parsed_file(language: Language, path: &str, entities: Vec<Entity>) -> ParsedFile {
    ParsedFile {
        path: path.to_string(),
        language,
        source: std::sync::Arc::from(""),
        entities,
        raw_relations: Vec::new(),
        behavior: Default::default(),
        control_flow: Default::default(),
        local_symbols: HashMap::new(),
        import_table: None,
        reexports: Vec::new(),
        embedded_blocks: Vec::new(),
        block_relations: Vec::new(),
        file_doc_comment: None,
        file_doc_span: None,
        file_hash: None,
    }
}

fn function(id: u64, name: &str) -> Entity {
    Entity::new(
        EntityId(id),
        EntityKind::Function,
        name.to_string(),
        Span::default(),
    )
}

fn build_package(
    files: &[ParsedFile],
) -> std::sync::Arc<cce_codegraph::symbol_table::package::PackageSymbolTable> {
    let builder = SymbolTableBuilder::new(PathBuf::from(PROJECT_ROOT));
    let refs: Vec<&ParsedFile> = files.iter().collect();
    let project = builder.build(&refs);
    project
        .get_package_by_name("media-server")
        .expect("package should exist")
}

#[test]
fn c_header_and_source_get_distinct_module_paths() {
    let header = make_parsed_file(
        Language::Cpp,
        "libmov/test/mov-file-buffer.h",
        vec![function(1, "mov_file_buffer_alloc")],
    );
    let source = make_parsed_file(
        Language::C,
        "libmov/test/mov-file-buffer.c",
        vec![function(2, "mov_file_buffer_alloc")],
    );

    let package = build_package(&[header, source]);

    assert_eq!(package.module_count(), 2);
    assert!(
        package
            .get_module_by_path("libmov/test/mov-file-buffer.h")
            .is_some(),
        "header must be reachable by its own module path"
    );
    assert!(
        package
            .get_module_by_path("libmov/test/mov-file-buffer.c")
            .is_some(),
        "source must be reachable by its own module path"
    );
}

#[test]
fn c_header_and_source_are_both_reachable_by_file_path() {
    let header = make_parsed_file(
        Language::Cpp,
        "libmov/test/mov-file-buffer.h",
        vec![function(1, "mov_file_buffer_alloc")],
    );
    let source = make_parsed_file(
        Language::C,
        "libmov/test/mov-file-buffer.c",
        vec![function(2, "mov_file_buffer_alloc")],
    );

    let package = build_package(&[header, source]);

    assert!(package.has_module("libmov/test/mov-file-buffer.h"));
    assert!(package.has_module("libmov/test/mov-file-buffer.c"));
}

#[test]
fn include_of_a_header_resolves_to_the_header_module() {
    let header = make_parsed_file(
        Language::Cpp,
        "libmov/test/mov-file-buffer.h",
        vec![function(1, "mov_file_buffer_alloc")],
    );
    let source = make_parsed_file(
        Language::C,
        "libmov/test/mov-file-buffer.c",
        vec![function(2, "mov_file_buffer_alloc")],
    );

    let package = build_package(&[header, source]);

    let resolved = package
        .resolve_module_path("mov-file-buffer.h", Some("libmov/test/mov-file-buffer.c"))
        .expect("include must resolve to the header module");
    assert_eq!(resolved.file_path, "libmov/test/mov-file-buffer.h");
}
