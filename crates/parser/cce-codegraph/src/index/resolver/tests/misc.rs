use super::*;

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
