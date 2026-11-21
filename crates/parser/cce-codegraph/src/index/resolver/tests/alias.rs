use super::*;

#[test]
fn test_import_alias_redirect_finds_aliased_import() {
    use cce_types::import::{ImportKind, ImportTarget, StandardizedImport, TargetKind};

    let mut caller = ParsedFile::new(Language::JavaScript, "caller.js".to_string(), "");
    let mut imports = cce_types::ImportTable::default();
    imports.add_standardized_import(
        StandardizedImport::new(ImportKind::SymbolImport, "./util")
            .with_target(ImportTarget::new("bar", TargetKind::Function).with_original_name("foo"))
            .with_alias("bar"),
    );
    caller.import_table = Some(imports);

    let resolver = RelationResolver::new();
    // The alias `bar` redirects to the original symbol `foo`.
    assert_eq!(
        resolver.import_alias_redirect("bar", &caller),
        Some("foo".to_string())
    );
    // Non-aliased names never redirect.
    assert_eq!(resolver.import_alias_redirect("foo", &caller), None);
    assert_eq!(resolver.import_alias_redirect("baz", &caller), None);
    // Candidate list tries the literal name first, then the redirect.
    assert_eq!(resolver.resolution_names("bar", &caller), ["bar", "foo"]);
    assert_eq!(resolver.resolution_names("foo", &caller), ["foo"]);
}

#[test]
fn test_alias_call_resolves_under_original_symbol_name() {
    use cce_types::import::{ImportKind, ImportTarget, StandardizedImport, TargetKind};

    // util.js exports `foo`; caller.js imports it under the alias `bar`
    // and calls `bar()`.
    let mut util = ParsedFile::new(Language::JavaScript, "util.js".to_string(), "");
    let mut foo_entity = create_test_function_entity(0, "foo");
    foo_entity.modifiers = vec!["export".to_string()];
    util.add_entity(foo_entity);

    let mut caller = ParsedFile::new(Language::JavaScript, "caller.js".to_string(), "");
    caller.add_entity(create_test_function_entity(0, "caller"));
    caller.add_relation(RawRelationData {
        src: EntityId(0),
        level: cce_types::RelationLevel::Entity,
        dst_name: "bar".to_string(),
        relation_type: RelationType::DirectCall,
        span: Span::default(),
        stdlib_category: None,
    });
    let mut imports = cce_types::ImportTable::default();
    imports.add_standardized_import(
        StandardizedImport::new(ImportKind::SymbolImport, "./util")
            .with_target(ImportTarget::new("bar", TargetKind::Function).with_original_name("foo"))
            .with_alias("bar"),
    );
    caller.import_table = Some(imports);

    let files = [&util, &caller];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();

    let resolver = RelationResolver::new();
    let resolved = resolver
        .resolve_batch(&caller.raw_relations, &caller, &symbols, &index)
        .into_iter()
        .next()
        .expect("alias call must resolve to a relation");
    // The alias redirect resolves the call to `foo`'s entity instead of
    // producing a spurious Unknown external edge.
    assert!(
        resolved.callee_id.is_some(),
        "alias call must resolve internally, got {:?}",
        resolved.external_type
    );
    assert!(!resolved.is_external);
}

#[test]
fn test_paths_equivalent() {
    assert!(RelationResolver::paths_equivalent(
        "src/utils.rs",
        "src/utils.rs"
    ));
    assert!(RelationResolver::paths_equivalent(
        "src/utils.rs",
        "./src/utils.rs"
    ));
    assert!(RelationResolver::paths_equivalent(
        "/src/utils.rs",
        "src/utils.rs"
    ));
    // Strict: absolute and relative forms of *different* paths never match
    assert!(!RelationResolver::paths_equivalent(
        "/workspace/project/src/utils.rs",
        "src/utils.rs"
    ));
    assert!(!RelationResolver::paths_equivalent(
        "src/utils.rs",
        "src/model.rs"
    ));
}
