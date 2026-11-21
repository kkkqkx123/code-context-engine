use super::*;

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
