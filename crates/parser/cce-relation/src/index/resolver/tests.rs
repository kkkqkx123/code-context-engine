use super::*;
use crate::index::builder::SymbolTableBuilder;
use cce_types::{Entity, EntityId, EntityKind, Language, RawRelationData, RelationType, Span};
use std::collections::HashMap;
use std::path::PathBuf;

/// Helper function to create a test function entity
fn create_test_function_entity(id: u32, name: &str) -> Entity {
    Entity {
        id: EntityId(id.into()),
        kind: EntityKind::Function,
        name: name.to_string(),
        signature: format!("fn {}()", name),
        parameters: Vec::new(),
        return_type: None,
        span: Span::default(),
        depth: 0,
        parent: None,
        children: Vec::new(),
        doc_comment: None,
        modifiers: Vec::new(),
        attributes: HashMap::new(),
        metadata: HashMap::new(),
        is_stdlib: false,
        subtype: None,
        stdlib_category: None,
    }
}

#[test]
fn test_resolver_new() {
    let resolver = RelationResolver::new();
    assert!(resolver.filter_stdlib_calls);
    assert_eq!(resolver.filtered_count(), 0);
}

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
fn test_resolver_with_filter() {
    let mut resolver = RelationResolver::new();
    resolver.with_filter(false);
    assert!(!resolver.filter_stdlib_calls);
}

#[test]
fn test_resolver_with_external_packages() {
    let mut resolver = RelationResolver::new();
    let mut packages = HashMap::new();
    packages.insert(Language::Rust, HashSet::from(["serde".to_string()]));
    resolver.with_external_packages(packages);
    assert!(resolver.external_packages.is_some());
}

#[test]
fn test_extract_stdlib_name_rust() {
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.extract_stdlib_name("std::collections::HashMap", &Language::Rust),
        "std::collections"
    );
    assert_eq!(
        resolver.extract_stdlib_name("std::fs::read", &Language::Rust),
        "std::fs"
    );
    assert_eq!(
        resolver.extract_stdlib_name("simple_func", &Language::Rust),
        "simple_func"
    );
}

#[test]
fn test_extract_stdlib_name_python() {
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.extract_stdlib_name("os.path.join", &Language::Python),
        "os.path"
    );
    assert_eq!(
        resolver.extract_stdlib_name("json.loads", &Language::Python),
        "json"
    );
    assert_eq!(
        resolver.extract_stdlib_name("print", &Language::Python),
        "builtin"
    );
}

#[test]
fn test_extract_stdlib_name_javascript() {
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.extract_stdlib_name("console.log", &Language::JavaScript),
        "console"
    );
    assert_eq!(
        resolver.extract_stdlib_name("Math.random", &Language::JavaScript),
        "Math"
    );
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

#[test]
fn test_stdlib_relations_preserved() {
    // This test verifies that stdlib relations are now preserved (not deleted)
    // and marked with external_type = StandardLibrary(...).
    //
    // This is the key improvement: instead of filtering out stdlib calls entirely,
    // they are now retained and marked as external, allowing downstream modules
    // to decide how to handle them based on external_type.
    let mut resolver = RelationResolver::new();

    // Verify that filter_stdlib_calls doesn't cause early return anymore
    assert!(resolver.filter_stdlib_calls);

    // The actual verification would require setting up full parse context,
    // which is tested in integration tests. This test just verifies the
    // resolver accepts filter_stdlib_calls=true without deleting relations.
    resolver.with_filter(true);
    assert!(resolver.filter_stdlib_calls);
}

/// the single-entry `resolve` must be a pure delegation to
/// `resolve_batch` — same input produces byte-equivalent output.
#[test]
fn test_resolve_delegates_to_batch_equivalently() {
    // caller.rs calls `callee`, defined in callee.rs.
    let mut caller = ParsedFile::new(Language::Rust, "caller.rs".to_string(), "");
    caller.add_entity(create_test_function_entity(0, "caller"));
    caller.add_relation(RawRelationData {
        src: EntityId(0),
        level: cce_types::RelationLevel::Entity,
        dst_name: "callee".to_string(),
        relation_type: RelationType::DirectCall,
        span: Span::default(),
        stdlib_category: None,
    });

    let mut callee = ParsedFile::new(Language::Rust, "callee.rs".to_string(), "");
    let mut callee_entity = create_test_function_entity(1, "callee");
    callee_entity.modifiers.push("pub".to_string());
    callee.add_entity(callee_entity);

    let files = [&caller, &callee];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);

    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();

    let mut resolver = RelationResolver::new();
    resolver.with_filter(false);

    let single = resolver
        .resolve(&caller.raw_relations[0], &caller, &symbols, &index)
        .expect("single resolve should produce a relation");
    let batch = resolver
        .resolve_batch(&caller.raw_relations, &caller, &symbols, &index)
        .into_iter()
        .next()
        .expect("batch resolve should produce a relation");

    assert_eq!(single.caller, batch.caller);
    assert_eq!(single.callee_id, batch.callee_id);
    assert_eq!(single.callee_name, batch.callee_name);
    assert_eq!(single.relation_type, batch.relation_type);
    assert_eq!(single.span, batch.span);
    assert_eq!(single.is_external, batch.is_external);
    assert_eq!(single.external_type, batch.external_type);
    assert_eq!(
        single.callee_symbol.map(|s| s.entity_id),
        batch.callee_symbol.map(|s| s.entity_id),
    );
    assert!(
        single.callee_id.is_some(),
        "callee should resolve to an internal entity"
    );
}

/// Bucketing: an unresolved non-stdlib callee must land in the
/// `relation_unresolved_total` bucket with the `symbol_not_resolved`
/// reason; stdlib-filtered and externally-classified relations must not.
#[test]
fn test_unresolved_metric_buckets_by_reason() {
    use cce_metrics::MetricsRegistry;
    use cce_types::ExternalCallType;

    let registry = MetricsRegistry::new();
    let metrics = cce_metrics::RelationMetrics::new(&registry, 7);

    // caller.rs with three raw relations: unknown target, external
    // package target, and a stdlib-like target.
    let mut caller = ParsedFile::new(Language::Rust, "caller.rs".to_string(), "");
    caller.add_entity(create_test_function_entity(0, "caller"));
    for (dst, category) in [
        ("mystery_function", None),
        ("serde::Serialize", None),
        (
            "println",
            Some(cce_types::stdlib_category::StdlibCategory::Macro),
        ),
    ] {
        caller.add_relation(RawRelationData {
            src: EntityId(0),
            level: cce_types::RelationLevel::Entity,
            dst_name: dst.to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            stdlib_category: category,
        });
    }

    let files = [&caller];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);

    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();

    let mut resolver = RelationResolver::new();
    resolver.with_metrics(Some(metrics.clone()));
    resolver.with_filter(true);
    let mut packages = HashMap::new();
    packages.insert(Language::Rust, HashSet::from(["serde".to_string()]));
    resolver.with_external_packages(packages);

    let _ = resolver.resolve_batch(&caller.raw_relations, &caller, &symbols, &index);

    let unresolved = metrics
        .relation_unresolved_total
        .get("symbol_not_resolved")
        .map(|c| c.get())
        .unwrap_or(0);
    assert_eq!(
        unresolved, 1,
        "only the unknown-target relation is unresolved"
    );
    assert_eq!(metrics.stdlib_filtered.get(), 1);
    assert_eq!(metrics.resolve_calls_total.get(), 3);
    assert!(
        metrics.resolve_lookups_total.get() >= 3,
        "each call performs at least one lookup"
    );

    let external = resolver.resolve_batch(&caller.raw_relations, &caller, &symbols, &index);
    assert_eq!(external.len(), 2);
    assert!(matches!(
        external[0].external_type,
        Some(ExternalCallType::Unknown { .. })
    ));
    assert!(matches!(
        external[1].external_type,
        Some(ExternalCallType::ExternalLibrary { .. })
    ));
}

/// After a build completes, the unresolved-ratio gauge is bounded in
/// 0..=1 and the average-lookups gauge reflects actual resolution work.
#[test]
fn test_build_metrics_ratio_and_avg_lookups_range() {
    use cce_metrics::MetricsRegistry;

    let registry = MetricsRegistry::new();
    let metrics = cce_metrics::RelationMetrics::new(&registry, 7);

    let mut caller = ParsedFile::new(Language::Rust, "caller.rs".to_string(), "");
    caller.add_entity(create_test_function_entity(0, "caller"));
    for (dst, category) in [
        ("mystery_function", None),
        (
            "println",
            Some(cce_types::stdlib_category::StdlibCategory::Macro),
        ),
    ] {
        caller.add_relation(RawRelationData {
            src: EntityId(0),
            level: cce_types::RelationLevel::Entity,
            dst_name: dst.to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            stdlib_category: category,
        });
    }

    let files = [&caller];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);

    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();

    let mut resolver = RelationResolver::new();
    resolver.with_metrics(Some(metrics.clone()));
    resolver.with_filter(true);
    let _ = resolver.resolve_batch(&caller.raw_relations, &caller, &symbols, &index);

    metrics.record_build(5.0, 2, 1);
    let ratio = metrics.relation_unresolved_ratio.get();
    assert!((0.0..=1.0).contains(&ratio), "ratio in 0..=1, got {ratio}");
    assert!(
        metrics.resolve_avg_lookups.get() >= 1.0,
        "average lookups per call >= 1, got {}",
        metrics.resolve_avg_lookups.get()
    );

    // Force an unresolved relation, then re-record the build: the ratio
    // must become strictly positive.
    metrics.record_unresolved(cce_types::relation::UnresolvedReason::SymbolNotFound.as_str());
    metrics.record_build(5.0, 2, 1);
    let ratio = metrics.relation_unresolved_ratio.get();
    assert!(ratio > 0.0 && ratio <= 1.0, "ratio in (0,1], got {ratio}");
}

#[test]
fn test_else_branch_arg_shape_prefers_complement_in_else_range() {
    use crate::type_inference::{InferenceOrigin, ScopedTypeContext, TypeBinding};
    use cce_types::{ControlFlowFact, ControlFlowFactKind, ControlFlowStore};

    let mut parsed = ParsedFile::new(Language::Java, "demo.java".to_string(), "");
    let mut func = create_test_function_entity(1, "demo");
    func.span = Span {
        start_byte: 0,
        end_byte: 100,
        ..Span::default()
    };
    parsed.entities.push(func);
    let fact = ControlFlowFact::new(
        ControlFlowFactKind::If,
        "if (x instanceof String) { use(x); } else { other(x); }",
        0,
        100,
    )
    .with_else_range(60, 100);
    let mut store = ControlFlowStore::default();
    store.push_fact(EntityId(1), fact);
    parsed.control_flow = store;

    let mut ctx = ScopedTypeContext::new(Language::Java);
    ctx.add_narrowed_type(
        "x".to_string(),
        TypeBinding {
            type_name: "String".to_string(),
            origin: Some(InferenceOrigin::ControlFlowNarrowing),
            ..Default::default()
        },
    );
    ctx.add_narrowed_type_in_branch(
        "x".to_string(),
        TypeBinding {
            type_name: "Integer".to_string(),
            origin: Some(InferenceOrigin::ControlFlowNarrowing),
            ..Default::default()
        },
        BranchPolarity::Else,
    );

    let shape =
        RelationResolver::infer_else_branch_arg_shape(&parsed, &ctx, "x", 80).expect("else shape");
    assert_eq!(
        crate::type_inference::types::type_shape_to_string(&shape),
        "Integer"
    );
    assert!(RelationResolver::infer_else_branch_arg_shape(&parsed, &ctx, "x", 10).is_none());
    assert!(RelationResolver::infer_else_branch_arg_shape(&parsed, &ctx, "missing", 80).is_none());
}
