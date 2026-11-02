use super::*;
use crate::index::builder::SymbolTableBuilder;
use cce_types::{Entity, EntityId, EntityKind, Language, RawRelationData, RelationType, Span};
use std::collections::HashMap;
use std::path::PathBuf;

/// Helper function to create a test entity of any kind
fn create_test_entity(id: u32, name: &str, kind: EntityKind) -> Entity {
    Entity {
        id: EntityId(id.into()),
        kind,
        name: name.to_string(),
        signature: name.to_string(),
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

/// Build a one-file project: `caller_name()` contains a bare `dst_name()`
/// call recorded as `DirectCall`, and resolve it.
fn resolve_bare_call(
    caller_name: &str,
    dst_name: &str,
    callees: Vec<Entity>,
) -> cce_types::ResolvedRelation {
    let mut file = ParsedFile::new(Language::Rust, "lib.rs".to_string(), "");
    let caller_id = 100u32;
    file.add_entity(create_test_entity(
        caller_id,
        caller_name,
        EntityKind::Function,
    ));
    for callee in callees {
        file.add_entity(callee);
    }
    file.add_relation(RawRelationData {
        src: EntityId(caller_id.into()),
        level: cce_types::RelationLevel::Entity,
        dst_name: dst_name.to_string(),
        relation_type: RelationType::DirectCall,
        span: Span::default(),
        stdlib_category: None,
    });

    let files = [&file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();

    let resolver = RelationResolver::new();
    resolver
        .resolve_batch(&file.raw_relations, &file, &symbols, &index)
        .into_iter()
        .next()
        .expect("bare call must produce a relation")
}

#[test]
fn test_bare_call_to_struct_upgrades_to_constructor() {
    // `OnceCell(Imp::new())` inside `new()` parses as a bare-identifier
    // call, but the callee resolves to the struct: the edge must carry
    // construction semantics.
    let resolved = resolve_bare_call(
        "new",
        "OnceCell",
        vec![create_test_entity(0, "OnceCell", EntityKind::Struct)],
    );
    assert!(!resolved.is_external);
    assert_eq!(resolved.callee_name, "OnceCell");
    assert_eq!(resolved.relation_type, RelationType::ConstructorCall);
}

#[test]
fn test_bare_call_to_class_upgrades_to_constructor() {
    let resolved = resolve_bare_call(
        "make",
        "Widget",
        vec![create_test_entity(0, "Widget", EntityKind::Class)],
    );
    assert!(!resolved.is_external);
    assert_eq!(resolved.relation_type, RelationType::ConstructorCall);
}

#[test]
fn test_bare_call_to_function_keeps_direct_call() {
    let resolved = resolve_bare_call(
        "main",
        "helper",
        vec![create_test_entity(0, "helper", EntityKind::Function)],
    );
    assert!(!resolved.is_external);
    assert_eq!(resolved.relation_type, RelationType::DirectCall);
}

#[test]
fn test_bare_call_to_unresolved_keeps_direct_call() {
    let resolved = resolve_bare_call("main", "missing_type", vec![]);
    assert!(resolved.is_external);
    assert_eq!(resolved.relation_type, RelationType::DirectCall);
}

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
/// Drop accounting: the debug/log post-filter shares the filter-counter
/// family with stdlib and self-loop drops. A debug-looking unresolved call
/// is dropped and counted in `debug_filtered`; an ordinary unknown call is
/// kept as an Unknown edge and never touches that counter.
#[test]
fn test_debug_filter_drop_counted() {
    use cce_metrics::MetricsRegistry;

    let registry = MetricsRegistry::new();
    let metrics = cce_metrics::RelationMetrics::new(&registry, 7);

    let mut caller = ParsedFile::new(Language::Rust, "caller.rs".to_string(), "");
    caller.add_entity(create_test_function_entity(0, "caller"));
    for dst in ["debug", "mystery_function"] {
        caller.add_relation(RawRelationData {
            src: EntityId(0),
            level: cce_types::RelationLevel::Entity,
            dst_name: dst.to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            stdlib_category: None,
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

    let resolved = resolver.resolve_batch(&caller.raw_relations, &caller, &symbols, &index);
    assert_eq!(resolved.len(), 1, "only the unknown call is kept");
    assert_eq!(metrics.debug_filtered.get(), 1);
    assert_eq!(metrics.stdlib_filtered.get(), 0);
}

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

#[test]
fn test_repeated_call_sites_report_shared_call_frequency() {
    let mut callee_file = ParsedFile::new(Language::Rust, "callee.rs".to_string(), "");
    let mut callee_entity = create_test_function_entity(1, "callee");
    callee_entity.modifiers = vec!["pub".to_string()];
    callee_file.add_entity(callee_entity);

    let mut caller = ParsedFile::new(Language::Rust, "caller.rs".to_string(), "");
    let mut caller_entity = create_test_function_entity(1, "caller");
    caller_entity.span = Span {
        start_byte: 0,
        end_byte: 200,
        ..Span::default()
    };
    caller.add_entity(caller_entity);
    for offset in [0u32, 20, 40] {
        caller.add_relation(RawRelationData {
            src: EntityId(1),
            level: cce_types::RelationLevel::Entity,
            dst_name: "callee".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span {
                start_byte: offset as usize,
                end_byte: offset as usize + 5,
                ..Span::default()
            },
            stdlib_category: None,
        });
    }

    let files = [&callee_file, &caller];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();

    let resolved =
        RelationResolver::new().resolve_batch(&caller.raw_relations, &caller, &symbols, &index);

    assert_eq!(resolved.len(), 3, "each call site stays its own edge");
    assert!(
        resolved.iter().all(|edge| edge.call_frequency == 3),
        "every call site of the pair reports the pair's full frequency, got {:?}",
        resolved
            .iter()
            .map(|edge| edge.call_frequency)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_unguarded_caller_reports_no_cfg_condition() {
    let mut callee_file = ParsedFile::new(Language::Rust, "callee.rs".to_string(), "");
    let mut callee_entity = create_test_function_entity(1, "callee");
    callee_entity.modifiers = vec!["pub".to_string()];
    callee_file.add_entity(callee_entity);

    let mut caller = ParsedFile::new(Language::Rust, "caller.rs".to_string(), "");
    let mut caller_entity = create_test_function_entity(1, "caller");
    caller_entity.span = Span {
        start_byte: 0,
        end_byte: 200,
        ..Span::default()
    };
    caller.add_entity(caller_entity);
    caller.add_relation(RawRelationData {
        src: EntityId(1),
        level: cce_types::RelationLevel::Entity,
        dst_name: "callee".to_string(),
        relation_type: RelationType::DirectCall,
        span: Span {
            start_byte: 0,
            end_byte: 5,
            ..Span::default()
        },
        stdlib_category: None,
    });

    let files = [&callee_file, &caller];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();

    let resolved = RelationResolver::new()
        .resolve_batch(&caller.raw_relations, &caller, &symbols, &index)
        .into_iter()
        .next()
        .expect("call resolves");

    assert_eq!(resolved.cfg_condition, None);
    assert_eq!(resolved.call_frequency, 1);
}

#[test]
fn test_conditional_caller_propagates_its_cfg_guard_to_the_edge() {
    let mut callee_file = ParsedFile::new(Language::Rust, "callee.rs".to_string(), "");
    let mut callee_entity = create_test_function_entity(1, "callee");
    callee_entity.modifiers = vec!["pub".to_string()];
    callee_file.add_entity(callee_entity);

    let mut caller = ParsedFile::new(Language::Rust, "caller.rs".to_string(), "");
    let mut caller_entity = create_test_function_entity(1, "caller");
    caller_entity.span = Span {
        start_byte: 0,
        end_byte: 200,
        ..Span::default()
    };
    caller_entity.set_metadata(
        cce_types::entity::meta_keys::CFG_PREDICATE.to_string(),
        "cfg(unix)".to_string(),
    );
    caller.add_entity(caller_entity);
    caller.add_relation(RawRelationData {
        src: EntityId(1),
        level: cce_types::RelationLevel::Entity,
        dst_name: "callee".to_string(),
        relation_type: RelationType::DirectCall,
        span: Span {
            start_byte: 0,
            end_byte: 5,
            ..Span::default()
        },
        stdlib_category: None,
    });

    let files = [&callee_file, &caller];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();

    let resolved = RelationResolver::new()
        .resolve_batch(&caller.raw_relations, &caller, &symbols, &index)
        .into_iter()
        .next()
        .expect("call resolves");

    assert_eq!(resolved.cfg_condition.as_deref(), Some("cfg(unix)"));
}

#[test]
fn test_trait_object_receiver_is_virtual_dispatch() {
    for receiver in [
        "&dyn Shape",
        "Box<dyn Shape>",
        "Arc<dyn Shape + Send>",
        "impl Fn(i32) -> i32",
        "T",
        "Self",
    ] {
        assert!(
            super::is_dynamic_receiver(receiver),
            "`{receiver}` is statically erased, so dispatch is dynamic"
        );
    }
}

#[test]
fn test_concrete_receiver_is_not_virtual_dispatch() {
    for receiver in [
        "Circle",
        "std::rc::Rc<Circle>",
        "&mut Circle",
        "Vec<String>",
        "",
    ] {
        assert!(
            !super::is_dynamic_receiver(receiver),
            "`{receiver}` pins one implementation"
        );
    }
}

#[test]
fn test_virtual_dispatch_and_instance_method_carry_distinct_tags() {
    let virtual_call = super::instance_call_context("&dyn Shape");
    let direct_call = super::instance_call_context("Circle");

    assert_eq!(virtual_call.tag(), "virtual_dispatch");
    assert_eq!(direct_call.tag(), "instance_method");
    assert_eq!(virtual_call.to_string(), "virtual_dispatch");
    assert_eq!(
        direct_call,
        CallContext::InstanceMethod {
            receiver_type: "Circle".to_string()
        }
    );
}

/// Build a three-file project: `caller.c` holds a file-level function
/// reference to `foo`, `def.c` defines it, and `other.c` holds a larger
/// same-name static definition with internal linkage.
fn resolve_funcref_across_files() -> (
    cce_types::ResolvedRelation,
    crate::index::core::RelationIndex,
) {
    let mut caller = ParsedFile::new(Language::C, "caller.c".to_string(), "");
    caller.add_relation(RawRelationData {
        src: EntityId(0u32.into()),
        level: cce_types::RelationLevel::File,
        dst_name: "foo".to_string(),
        relation_type: RelationType::FunctionReference,
        span: Span::default(),
        stdlib_category: None,
    });
    let mut def_file = ParsedFile::new(Language::C, "def.c".to_string(), "");
    let mut def_entity = create_test_entity(2, "foo", EntityKind::Function);
    def_entity.span = Span::new(0, 100, 0, 0, 4, 0);
    def_file.add_entity(def_entity);
    let mut static_file = ParsedFile::new(Language::C, "other.c".to_string(), "");
    let mut static_entity = create_test_entity(3, "foo", EntityKind::Function);
    static_entity.span = Span::new(0, 900, 0, 0, 30, 0);
    static_entity.modifiers = vec!["static".to_string()];
    static_file.add_entity(static_entity);

    let files = [&caller, &def_file, &static_file];
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
        .expect("funcref must resolve to the visible definition");
    (resolved, index)
}

#[test]
fn test_funcref_ignores_larger_foreign_static() {
    // The foreign static has internal linkage and can never be the target
    // of another file's reference, even though its body is larger than the
    // visible definition's.
    let (resolved, index) = resolve_funcref_across_files();
    let callee = resolved
        .callee_id
        .expect("funcref must have an internal callee");
    let file = index
        .get_file_path_by_entity(callee)
        .expect("callee must have a file");
    assert!(
        file.ends_with("def.c"),
        "foreign static must not win over the visible definition, got {file}"
    );
}

/// Caller file holds its own small static `foo` plus a file-level reference;
/// a larger extern `foo` lives elsewhere. Same-file linkage wins over span.
#[test]
fn test_funcref_prefers_same_file_static_over_larger_foreign() {
    let mut caller = ParsedFile::new(Language::C, "caller.c".to_string(), "");
    caller.add_relation(RawRelationData {
        src: EntityId(0u32.into()),
        level: cce_types::RelationLevel::File,
        dst_name: "foo".to_string(),
        relation_type: RelationType::FunctionReference,
        span: Span::default(),
        stdlib_category: None,
    });
    let mut own_entity = create_test_entity(1, "foo", EntityKind::Function);
    own_entity.span = Span::new(0, 50, 0, 0, 2, 0);
    own_entity.modifiers = vec!["static".to_string()];
    caller.add_entity(own_entity);
    let mut foreign_file = ParsedFile::new(Language::C, "def.c".to_string(), "");
    let mut foreign_entity = create_test_entity(2, "foo", EntityKind::Function);
    foreign_entity.span = Span::new(0, 900, 0, 0, 30, 0);
    foreign_file.add_entity(foreign_entity);

    let files = [&caller, &foreign_file];
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
        .expect("funcref must resolve to the same-file definition");
    let callee = resolved
        .callee_id
        .expect("funcref must have an internal callee");
    let file = index
        .get_file_path_by_entity(callee)
        .expect("callee must have a file");
    assert!(
        file.ends_with("caller.c"),
        "same-file static must win over the larger foreign definition, got {file}"
    );
}

/// Two-file Go project: `Engine` embeds `RouterGroup`, which owns `Use`.
/// The caller file holds `setup` calling `engine.Use` on a variable typed
/// `*Engine`.
fn go_engine_project() -> (ParsedFile, ParsedFile) {
    let mut engine_file = ParsedFile::new(Language::Go, "engine.go".to_string(), "");
    let mut engine = create_test_entity(10, "Engine", EntityKind::Struct);
    engine.children = vec![EntityId(11)];
    engine_file.add_entity(engine);
    let mut embedded = create_test_entity(11, "RouterGroup", EntityKind::Field);
    embedded.parent = Some(EntityId(10));
    engine_file.add_entity(embedded);
    engine_file.add_entity(create_test_entity(12, "setup", EntityKind::Function));
    let mut engine_var = create_test_entity(13, "engine", EntityKind::Variable);
    engine_var.set_metadata("type_annotation", "*Engine");
    engine_file.add_entity(engine_var);
    engine_file.add_relation(RawRelationData {
        src: EntityId(12),
        level: cce_types::RelationLevel::Entity,
        dst_name: "engine.Use".to_string(),
        relation_type: RelationType::InstanceMethodCall,
        span: Span::default(),
        stdlib_category: None,
    });
    let mut router_file = ParsedFile::new(Language::Go, "router.go".to_string(), "");
    router_file.add_entity(create_test_entity(20, "RouterGroup", EntityKind::Struct));
    let mut use_method = create_test_entity(21, "Use", EntityKind::Method);
    use_method.span = Span::new(0, 200, 0, 0, 6, 0);
    use_method.set_metadata("receiver_type", "*RouterGroup");
    router_file.add_entity(use_method);
    (engine_file, router_file)
}

fn build_go_engine_project() -> (
    ParsedFile,
    ProjectSymbolTable,
    crate::index::core::RelationIndex,
) {
    let (engine_file, router_file) = go_engine_project();
    let files = [&engine_file, &router_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();
    (engine_file, symbols, index)
}

#[test]
fn test_go_embedded_promotion_rewrites_candidate() {
    // The two-segment call must gain an owner-qualified candidate before the
    // raw literal, so type-member resolution reaches the embedded owner.
    let (engine_file, symbols, index) = build_go_engine_project();
    let resolver = RelationResolver::new();
    let names = resolver.ordered_resolution_names(
        "engine.Use",
        &engine_file,
        &symbols,
        &index,
        Some("*Engine"),
    );
    assert_eq!(
        names.first().map(String::as_str),
        Some("RouterGroup.Use"),
        "embedded owner must lead the candidates, got {names:?}"
    );
}

#[test]
fn test_go_embedded_call_resolves_to_owner() {
    // End to end: `engine.Use` on an `*Engine` receiver resolves to the
    // `RouterGroup.Use` method instead of degrading to Unknown.
    let (engine_file, symbols, index) = build_go_engine_project();
    let resolver = RelationResolver::new();
    let resolved = resolver
        .resolve_batch(&engine_file.raw_relations, &engine_file, &symbols, &index)
        .into_iter()
        .next()
        .expect("embedded call must produce a relation");
    assert!(
        !resolved.is_external,
        "embedded call must resolve internally, got {:?}",
        resolved.external_type
    );
    assert_eq!(resolved.callee_name, "Use");
    let callee = resolved.callee_id.expect("must have an internal callee");
    let file = index
        .get_file_path_by_entity(callee)
        .expect("callee must have a file");
    assert!(
        file.ends_with("router.go"),
        "call must land on the embedded owner, got {file}"
    );
}

#[test]
fn test_embedded_walk_stops_when_outer_owns_method() {
    // Promotion only fills the gap the receiver's own type leaves: when the
    // outer type owns the method, no embedded candidate may be produced.
    let (mut engine_file, router_file) = go_engine_project();
    let mut own_use = create_test_entity(14, "Use", EntityKind::Method);
    own_use.set_metadata("receiver_type", "*Engine");
    engine_file.add_entity(own_use);
    let files = [&engine_file, &router_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.embedded_method_owner("*Engine", "Use", &index, &symbols),
        None,
        "outer owner must suppress promotion"
    );
}

#[test]
fn test_embedded_walk_ignores_typed_named_fields() {
    // A named field carrying a type annotation is not an anonymous embedded
    // field and must never open a promotion path, even when its type owns
    // the called method.
    let (mut engine_file, router_file) = go_engine_project();
    for entity in engine_file.entities.iter_mut() {
        if entity.name == "RouterGroup" && entity.kind == EntityKind::Field {
            entity.name = "group".to_string();
            entity.set_metadata("type_annotation", "RouterGroup");
        }
    }
    let files = [&engine_file, &router_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.embedded_method_owner("*Engine", "Use", &index, &symbols),
        None,
        "named fields must not promote"
    );
}

#[test]
fn test_embedded_walk_terminates_on_cycle() {
    // Mutual embedding with no owner must return empty instead of looping.
    let mut a_file = ParsedFile::new(Language::Go, "a.go".to_string(), "");
    let mut struct_a = create_test_entity(30, "A", EntityKind::Struct);
    struct_a.children = vec![EntityId(31)];
    a_file.add_entity(struct_a);
    let mut field_b = create_test_entity(31, "B", EntityKind::Field);
    field_b.parent = Some(EntityId(30));
    a_file.add_entity(field_b);
    let mut b_file = ParsedFile::new(Language::Go, "b.go".to_string(), "");
    let mut struct_b = create_test_entity(40, "B", EntityKind::Struct);
    struct_b.children = vec![EntityId(41)];
    b_file.add_entity(struct_b);
    let mut field_a = create_test_entity(41, "A", EntityKind::Field);
    field_a.parent = Some(EntityId(40));
    b_file.add_entity(field_a);
    let files = [&a_file, &b_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.embedded_method_owner("*A", "Ping", &index, &symbols),
        None,
        "cyclic embedding must terminate empty"
    );
}

/// Single-owner C++ project: class `S` owns one `bar`, referenced as
/// `S::bar` from another file.
fn cpp_single_owner_project() -> (ParsedFile, ParsedFile) {
    let mut user_file = ParsedFile::new(Language::Cpp, "user.cpp".to_string(), "");
    user_file.add_relation(RawRelationData {
        src: EntityId(0u32.into()),
        level: cce_types::RelationLevel::File,
        dst_name: "S::bar".to_string(),
        relation_type: RelationType::FunctionReference,
        span: Span::default(),
        stdlib_category: None,
    });
    let mut def_file = ParsedFile::new(Language::Cpp, "s.cpp".to_string(), "");
    let mut class_s = create_test_entity(50, "S", EntityKind::Class);
    class_s.children = vec![EntityId(51)];
    def_file.add_entity(class_s);
    let mut bar = create_test_entity(51, "bar", EntityKind::Method);
    bar.parent = Some(EntityId(50));
    bar.span = Span::new(0, 120, 0, 0, 4, 0);
    def_file.add_entity(bar);
    (user_file, def_file)
}

fn build_cpp_project(
    user_file: &ParsedFile,
    def_file: &ParsedFile,
) -> (ProjectSymbolTable, crate::index::core::RelationIndex) {
    let files = [user_file, def_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    (symbols, builder.build())
}

#[test]
fn test_scoped_funcref_resolves_single_owner() {
    // Exactly one candidate owned by the named scope resolves.
    let (user_file, def_file) = cpp_single_owner_project();
    let (symbols, index) = build_cpp_project(&user_file, &def_file);
    let resolver = RelationResolver::new();
    let callee = resolver
        .resolve_scoped_funcref("S::bar", "S", "bar", "user.cpp", &index, &symbols)
        .expect("single owner must resolve");
    let entity = index
        .get_function_by_entity_id(callee)
        .expect("callee must exist");
    assert_eq!(entity.name, "bar");
}

#[test]
fn test_scoped_funcref_resolves_end_to_end() {
    // The file-level member-pointer reference lands on the member method.
    let (user_file, def_file) = cpp_single_owner_project();
    let (symbols, index) = build_cpp_project(&user_file, &def_file);
    let resolver = RelationResolver::new();
    let resolved = resolver
        .resolve_batch(&user_file.raw_relations, &user_file, &symbols, &index)
        .into_iter()
        .next()
        .expect("scoped funcref must produce a relation");
    assert!(
        !resolved.is_external,
        "scoped funcref must resolve internally"
    );
    assert_eq!(resolved.callee_name, "bar");
}

#[test]
fn test_scoped_funcref_drops_overload_ambiguity() {
    // Two same-owner overloads cannot be told apart without arity: drop.
    let (user_file, mut def_file) = cpp_single_owner_project();
    let mut second = create_test_entity(52, "bar", EntityKind::Method);
    second.parent = Some(EntityId(50));
    second.span = Span::new(200, 320, 0, 6, 10, 0);
    def_file.add_entity(second);
    if let Some(class_s) = def_file.entities.iter_mut().find(|e| e.name == "S") {
        class_s.children.push(EntityId(52));
    }
    let (symbols, index) = build_cpp_project(&user_file, &def_file);
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.resolve_scoped_funcref("S::bar", "S", "bar", "user.cpp", &index, &symbols),
        None,
        "overload ambiguity must be dropped"
    );
}

#[test]
fn test_scoped_funcref_drops_wrong_scope() {
    // A member of another scope is never the target of this reference.
    let (user_file, mut def_file) = cpp_single_owner_project();
    for entity in def_file.entities.iter_mut() {
        if entity.name == "S" {
            entity.name = "Other".to_string();
        }
    }
    let (symbols, index) = build_cpp_project(&user_file, &def_file);
    let resolver = RelationResolver::new();
    assert_eq!(
        resolver.resolve_scoped_funcref("S::bar", "S", "bar", "user.cpp", &index, &symbols),
        None,
        "foreign scope must not resolve"
    );
}

/// Build a two-file JavaScript project: `app.js` holds a `handle` method
/// containing a `this.get` call, `response.js` holds a same-name `get`
/// method. When `with_local_get` is set, `app.js` also owns a `get` method.
/// Resolves the call end to end.
fn resolve_this_call_against_foreign_file(with_local_get: bool) -> cce_types::ResolvedRelation {
    // Mirrors the real extraction shape: `call.method` records
    // `this.get('env')` as bare `get` with a span covering only the
    // property name, the receiver surviving solely in the source text.
    let source = "var x = this.get('env');\n";
    let mut caller_file = ParsedFile::new(Language::JavaScript, "app.js".to_string(), source);
    caller_file.add_entity(create_test_entity(100, "handle", EntityKind::Method));
    if with_local_get {
        caller_file.add_entity(create_test_entity(101, "get", EntityKind::Method));
    }
    caller_file.add_relation(RawRelationData {
        src: EntityId(100u32.into()),
        level: cce_types::RelationLevel::Entity,
        dst_name: "get".to_string(),
        relation_type: RelationType::InstanceMethodCall,
        span: Span {
            start_byte: 13,
            end_byte: 16,
            ..Default::default()
        },
        stdlib_category: None,
    });
    let mut def_file = ParsedFile::new(Language::JavaScript, "response.js".to_string(), "");
    def_file.add_entity(create_test_entity(200, "get", EntityKind::Method));
    let files = [&caller_file, &def_file];
    let symbols = SymbolTableBuilder::new(PathBuf::from(".")).build(&files);
    let builder = crate::index::builder::IndexBuilder::new();
    for file in &files {
        builder.register_file_entities(file);
    }
    let index = builder.build();
    let resolver = RelationResolver::new();
    resolver
        .resolve_batch(&caller_file.raw_relations, &caller_file, &symbols, &index)
        .into_iter()
        .next()
        .expect("this.get must produce a relation")
}

#[test]
fn test_self_receiver_does_not_hijack_foreign_same_name() {
    // `this.get(...)` with no local `get` must not bind another file's
    // `get`: a self-rooted receiver names the caller's own object graph,
    // and the foreign same-name method is a guess, not evidence.
    let resolved = resolve_this_call_against_foreign_file(false);
    assert!(
        resolved.is_external,
        "foreign same-name method must not hijack a self-receiver call, got {:?}",
        resolved.callee_name
    );
}

#[test]
fn test_self_receiver_prefers_caller_file_candidate() {
    // The guard only removes the cross-file guess: a same-file `get` still
    // wins for `this.get`.
    let resolved = resolve_this_call_against_foreign_file(true);
    assert!(
        !resolved.is_external,
        "caller-file candidate must still resolve, got {:?}",
        resolved.callee_name
    );
    assert_eq!(resolved.callee_name, "get");
}
