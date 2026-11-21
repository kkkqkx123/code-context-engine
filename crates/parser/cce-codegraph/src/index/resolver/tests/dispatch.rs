use super::*;

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
