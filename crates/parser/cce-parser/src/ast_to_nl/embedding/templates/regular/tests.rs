use super::*;
use cce_types::entity::EntityKind;

#[test]
fn test_regular_template_class() {
    let group = EntityGroup {
        name: "UserService".into(),
        kind: EntityKind::Class,
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);

    assert!(!results.is_empty());
    assert!(results[0].contains("UserService"));
    assert!(results[0].contains("class"));
}

#[test]
fn test_regular_template_enum() {
    let group = EntityGroup {
        name: "Status".into(),
        kind: EntityKind::Enum,
        members: [
            cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(1),
                name: "Active".to_string(),
                ..Default::default()
            },
            cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(2),
                name: "Inactive".to_string(),
                ..Default::default()
            },
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);

    assert!(!results.is_empty());
    assert!(results[0].contains("Status"));
    assert!(results[0].contains("enum"));
    assert!(
        results.len() >= 2,
        "expected at least 2 results, got {}: {:?}",
        results.len(),
        results
    );
    assert!(
        results[1].contains("Active"),
        "results[1] expected 'Active', got {:?}",
        results[1]
    );
    assert!(
        results[1].contains("Status.Active"),
        "results[1] expected a compact path, got {:?}",
        results[1]
    );
}

#[test]
fn test_regular_template_function() {
    let group = EntityGroup {
        name: "calculate_total".into(),
        kind: EntityKind::Function,
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);

    assert!(!results.is_empty());
    assert!(results[0].contains("calculate_total"));
}

#[test]
fn test_member_with_doc_comment_preserves_entity_name_and_kind() {
    let group = EntityGroup {
        name: "OnceCell".into(),
        kind: EntityKind::Struct,
        members: vec![cce_types::entity::GroupedEntity {
            id: cce_types::entity::EntityId(1),
            name: "get_mut".to_string(),
            kind: EntityKind::Method,
            doc_comment: Some("Returns a mutable reference to the underlying value.".to_string()),
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);

    assert!(
        results.len() >= 2,
        "expected group + member, got {:?}",
        results
    );
    let member_desc = &results[1];
    assert!(
        member_desc.contains("get_mut"),
        "member description should contain original identifier, got: {}",
        member_desc
    );
    assert!(
        member_desc.contains("method"),
        "member description should contain kind label, got: {}",
        member_desc
    );
    assert!(
        member_desc.contains("Returns a mutable reference"),
        "member description should contain doc comment, got: {}",
        member_desc
    );
}

#[test]
fn test_semantic_entity_description_preserves_original_name() {
    let field = cce_types::entity::GroupedEntity {
        name: "state".to_string(),
        kind: EntityKind::Field,
        modifiers: vec!["pub".to_string()],
        ..Default::default()
    };
    let constant = cce_types::entity::GroupedEntity {
        name: "INCOMPLETE".to_string(),
        kind: EntityKind::Constant,
        ..Default::default()
    };
    let method = cce_types::entity::GroupedEntity {
        name: "get_mut".to_string(),
        kind: EntityKind::Method,
        ..Default::default()
    };

    assert_eq!(
        RegularGroupTemplate::semantic_entity_description(&field.name, field.kind, Some(&field)),
        "field state."
    );
    assert_eq!(
        RegularGroupTemplate::semantic_entity_description(
            &constant.name,
            constant.kind,
            Some(&constant)
        ),
        "constant INCOMPLETE."
    );
    assert_eq!(
        RegularGroupTemplate::semantic_entity_description(&method.name, method.kind, Some(&method)),
        "method get_mut."
    );
}

#[test]
fn test_modifier_text_drops_visibility_keeps_behavior() {
    assert_eq!(
        RegularGroupTemplate::modifier_text(&["pub".to_string()]),
        None,
        "visibility alone leaves no modifier"
    );
    assert_eq!(
        RegularGroupTemplate::modifier_text(&["pub(crate)".to_string(), "async".to_string()]),
        Some("async".to_string()),
        "behavioral modifiers survive visibility filtering"
    );
    assert_eq!(
        RegularGroupTemplate::modifier_text(&["unsafe".to_string()]),
        Some("unsafe".to_string()),
        "standalone behavioral modifier is kept"
    );
}

#[test]
fn test_call_targets_line_caps_and_dedupes() {
    use cce_types::entity::meta_keys;
    let mut member = cce_types::entity::GroupedEntity {
        id: cce_types::entity::EntityId(1),
        name: "demo".to_string(),
        kind: EntityKind::Function,
        ..Default::default()
    };
    member.metadata.insert(
        meta_keys::CALL_PATHS.to_string(),
        "self, self.buf.flush, ReadByLine::new, ReadByLine::new, a, b, c, d, e, f, g, h, i"
            .to_string(),
    );
    let line = RegularGroupTemplate::call_targets_line(Some(&member))
        .expect("distinctive callees should survive");
    assert!(
        !line.contains("self"),
        "self plumbing must go, got: {}",
        line
    );
    assert_eq!(
        line.matches("ReadByLine::new").count(),
        1,
        "duplicates must collapse, got: {}",
        line
    );
    assert!(
        line.split(',').count() <= MAX_CALL_TARGETS,
        "line must respect the cap, got: {}",
        line
    );
    assert!(line.contains("ReadByLine::new"), "got: {}", line);
}

#[test]
fn test_loop_variable_with_signature_emits_single_line() {
    let group = EntityGroup {
        name: "Context".into(),
        kind: EntityKind::Struct,
        members: vec![cce_types::entity::GroupedEntity {
            id: cce_types::entity::EntityId(1),
            name: "v".to_string(),
            kind: EntityKind::Variable,
            subtype: Some("loop".to_string()),
            signature: "for _, v := range c.Params.".to_string(),
            metadata: [("source_type".to_string(), "c.Params".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);
    let member_desc = results
        .iter()
        .find(|r| r.contains("Context.v"))
        .expect("member description should be emitted");
    assert!(
        member_desc.contains("loop variable"),
        "identity keeps the production mode, got: {}",
        member_desc
    );
    assert!(
        !member_desc.contains("(loop iteration)"),
        "relation line is skipped when the signature carries the source, got: {}",
        member_desc
    );
    assert!(
        member_desc.contains("c.Params"),
        "source survives via the signature, got: {}",
        member_desc
    );
}

#[test]
fn test_semantic_entity_description_field_with_type() {
    use compact_str::CompactString;

    let field = cce_types::entity::GroupedEntity {
        name: "value".to_string(),
        kind: EntityKind::Field,
        parameters: smallvec::smallvec![("value".into(), Some(CompactString::new("Option<u32>")))],
        ..Default::default()
    };

    let desc =
        RegularGroupTemplate::semantic_entity_description(&field.name, field.kind, Some(&field));
    assert!(
        desc.contains("Option<u32>"),
        "field description should include type, got: {}",
        desc
    );
}

#[test]
fn test_kind_label_drops_redundant_subtype() {
    assert_eq!(
        RegularGroupTemplate::kind_label_with_subtype(EntityKind::TraitImpl, Some("trait")),
        "trait_impl",
        "a subtype restating the kind adds no signal"
    );
    assert_eq!(
        RegularGroupTemplate::kind_label_with_subtype(EntityKind::Method, Some("async")),
        "async method",
        "a distinctive subtype is kept"
    );
    assert_eq!(
        RegularGroupTemplate::kind_label_with_subtype(EntityKind::Method, None),
        "method",
        "no subtype keeps the kind display"
    );
}

#[test]
fn test_build_signature_from_fields() {
    use compact_str::CompactString;

    assert_eq!(
        TemplateHelpers::build_signature_from_fields::<CompactString, CompactString>(&[], None,),
        "()"
    );
    let params = vec![
        (CompactString::new("x"), Some(CompactString::new("i32"))),
        (CompactString::new("y"), Some(CompactString::new("String"))),
    ];
    assert_eq!(
        TemplateHelpers::build_signature_from_fields(&params, Some("bool")),
        "(x: i32, y: String) -> bool"
    );
    // Single param, no return type
    let params = vec![(
        CompactString::new("self"),
        Some(CompactString::new("&mut Self")),
    )];
    assert_eq!(
        TemplateHelpers::build_signature_from_fields::<CompactString, CompactString>(&params, None),
        "(self: &mut Self)"
    );
    // Empty param name, no return
    assert_eq!(
        TemplateHelpers::build_signature_from_fields::<CompactString, CompactString>(&[], None),
        "()"
    );
}

#[test]
fn test_member_with_duplicate_doc_comment_is_deduplicated() {
    let group = EntityGroup {
        name: "OnceCell".into(),
        kind: EntityKind::Struct,
        members: vec![cce_types::entity::GroupedEntity {
            id: cce_types::entity::EntityId(1),
            name: "get_mut".to_string(),
            kind: EntityKind::Method,
            doc_comment: Some("get_mut method.".to_string()),
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);

    assert!(
        results.len() >= 2,
        "expected group + member, got {:?}",
        results
    );
    let member_desc = &results[1];
    assert!(
        member_desc.contains("OnceCell.get_mut"),
        "qualified name should be inlined into the kind line, got: {}",
        member_desc
    );
    let get_mut_count = member_desc.matches("get_mut").count();
    // get_mut appears in: inlined qualified name and doc_comment
    assert_eq!(
        get_mut_count, 2,
        "expected 'get_mut' in qualified name and doc_comment, got: {}",
        member_desc
    );
}

#[test]
fn test_group_with_doc_comment_does_not_emit_relationship_metadata() {
    let mut group = EntityGroup {
        name: "OnceCell".into(),
        kind: EntityKind::Struct,
        header: Some(cce_types::entity::GroupedEntity {
            id: cce_types::entity::EntityId(0),
            name: "OnceCell".to_string(),
            kind: EntityKind::Struct,
            doc_comment: Some("A cell that can be written to only once.".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };
    group.metadata.insert(
        "trait_impls".to_string(),
        "Debug, RefUnwindSafe".to_string(),
    );

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);

    assert!(!results.is_empty());
    let group_desc = &results[0];
    assert!(
        !group_desc.contains("Trait implementations"),
        "group description should not emit trait_impls metadata, got: {}",
        group_desc
    );
    assert!(
        !group_desc.contains("Debug"),
        "group description should not contain Debug trait metadata, got: {}",
        group_desc
    );
    assert!(
        !group_desc.contains("RefUnwindSafe"),
        "group description should not contain RefUnwindSafe trait metadata, got: {}",
        group_desc
    );
}

#[test]
fn test_variable_relation_skipped_when_signature_covers_call() {
    let group = EntityGroup {
        name: "HandlersChain".into(),
        kind: EntityKind::Struct,
        members: vec![cce_types::entity::GroupedEntity {
            id: cce_types::entity::EntityId(1),
            name: "length".to_string(),
            kind: EntityKind::Variable,
            signature: "length := len(c).".to_string(),
            metadata: [("call_target".to_string(), "len(c)".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);
    let member_desc = results
        .iter()
        .find(|r| r.contains("HandlersChain.length"))
        .expect("member description should be emitted");
    assert!(
        !member_desc.contains("assigned from"),
        "relation restating the signature should be skipped, got: {}",
        member_desc
    );
    assert!(
        member_desc.contains("len(c)"),
        "call identifier must survive via the signature, got: {}",
        member_desc
    );
}

#[test]
fn test_variable_loop_iteration_marker_preserved() {
    let group = EntityGroup {
        name: "Engine".into(),
        kind: EntityKind::Struct,
        members: vec![cce_types::entity::GroupedEntity {
            id: cce_types::entity::EntityId(1),
            name: "node".to_string(),
            kind: EntityKind::Variable,
            subtype: Some("loop variable".to_string()),
            metadata: [("source_type".to_string(), "Params".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);
    let member_desc = results
        .iter()
        .find(|r| r.contains("Engine.node"))
        .expect("member description should be emitted");
    assert!(
        member_desc.contains("(loop iteration)"),
        "loop iteration marker carries behavioral signal, got: {}",
        member_desc
    );
}

#[test]
fn test_variable_relation_kept_when_signature_not_emitted() {
    let group = EntityGroup {
        name: "Engine".into(),
        kind: EntityKind::Struct,
        members: vec![cce_types::entity::GroupedEntity {
            id: cce_types::entity::EntityId(1),
            name: "count".to_string(),
            kind: EntityKind::Variable,
            signature: "count: i32".to_string(),
            metadata: [("literal_type".to_string(), "i32".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);
    let member_desc = results
        .iter()
        .find(|r| r.contains("Engine.count"))
        .expect("member description should be emitted");
    assert!(
        member_desc.contains("of type i32."),
        "type info must be kept when the signature is not emitted, got: {}",
        member_desc
    );
}

#[test]
fn test_variable_loop_source_summarized_in_relation() {
    let head = "[]struct {\nname string\nvalue any\n}";
    let rows = "{\n{\"base type\", 1},\n{\"zero value\", 0},\n}".repeat(20);
    let group = EntityGroup {
        name: "Engine".into(),
        kind: EntityKind::Struct,
        members: vec![cce_types::entity::GroupedEntity {
            id: cce_types::entity::EntityId(1),
            name: "tt".to_string(),
            kind: EntityKind::Variable,
            subtype: Some("loop variable".to_string()),
            signature: String::new(),
            metadata: [("source_type".to_string(), format!("{head}{rows}"))]
                .into_iter()
                .collect(),
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);
    let member_desc = results
        .iter()
        .find(|r| r.contains("Engine.tt"))
        .expect("member description should be emitted");
    assert!(
        member_desc.contains("[]struct"),
        "collection head must survive, got: {}",
        member_desc
    );
    assert!(
        member_desc.contains("(loop iteration)"),
        "loop iteration marker must survive, got: {}",
        member_desc
    );
    assert!(
        !member_desc.contains("zero value"),
        "literal rows must not leak into text, got: {}",
        member_desc
    );
}

fn dummy_member() -> cce_types::entity::GroupedEntity {
    cce_types::entity::GroupedEntity {
        id: cce_types::entity::EntityId(1),
        name: "member".to_string(),
        ..Default::default()
    }
}

#[test]
fn test_duplicate_member_suppresses_exact_duplicate() {
    assert!(RegularGroupTemplate::is_duplicate_member(
        "function foo.\nreturns x.",
        "function foo.\nreturns x.",
        &dummy_member(),
    ));
}

#[test]
fn test_duplicate_member_keeps_read_write_siblings() {
    // Shared boilerplate (asserts, size checks) must not suppress the
    // sibling carrying the distinctive decode branch.
    let group = "struct codec.\nassert size.\ncheck bounds.\nMethods: decode(); encode().";
    let member = "function codec.decode.\nassert size.\ncheck bounds.\ndecode tag type.";
    assert!(!RegularGroupTemplate::is_duplicate_member(
        group,
        member,
        &dummy_member(),
    ));
}

#[test]
fn test_duplicate_member_keeps_member_with_novel_segment() {
    // A member that extends the group by even one short line (calls
    // info) is never redundant, however small the extension is.
    assert!(!RegularGroupTemplate::is_duplicate_member(
        "function foo.\nreturns x.",
        "function foo.\nreturns x.\ncalls: bar.",
        &dummy_member(),
    ));
}

#[test]
fn test_duplicate_member_suppresses_member_fully_covered_by_large_group() {
    // Redundancy ignores group size: every member segment appears whole
    // in some group segment, so the member adds nothing.
    let group = "class c.\nfunction foo.\nreturns x.\nMethods: a(); b(); c(); d(); e(); f().";
    assert!(RegularGroupTemplate::is_duplicate_member(
        group,
        "function foo.\nreturns x.",
        &dummy_member(),
    ));
}

#[test]
fn test_duplicate_member_methods_mention_does_not_subsume() {
    // A bare name mention in the Methods summary must not subsume the
    // full member description with kind label and qualified name.
    assert!(!RegularGroupTemplate::is_duplicate_member(
        "struct oncecell.\nmethods: get_mut().",
        "method oncecell.get_mut.\nreturns a mutable reference.",
        &dummy_member(),
    ));
}

#[test]
fn test_duplicate_member_rejects_cross_segment_stitching() {
    // Member segments must each fit inside a single group segment;
    // sentences scattered across different group segments do not count.
    assert!(!RegularGroupTemplate::is_duplicate_member(
        "class c.\nreturns a.\nreturns b.",
        "returns a. returns b.",
        &dummy_member(),
    ));
}

#[test]
fn test_trait_impl_member_qualified_with_self_type() {
    // `impl Matcher for RegexMatcher`: members carry the concrete type,
    // the trait name stays in the group description.
    let mut header = cce_types::entity::GroupedEntity {
        id: cce_types::entity::EntityId(1),
        name: "Matcher".to_string(),
        kind: EntityKind::TraitImpl,
        ..Default::default()
    };
    header
        .metadata
        .insert("impl_for_type".to_string(), "RegexMatcher".to_string());
    let group = EntityGroup {
        name: "Matcher".into(),
        kind: EntityKind::TraitImpl,
        header: Some(header),
        members: vec![cce_types::entity::GroupedEntity {
            id: cce_types::entity::EntityId(2),
            name: "find_at".to_string(),
            kind: EntityKind::Method,
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);
    assert!(
        results[0].contains("For RegexMatcher."),
        "group description names the self type: {results:?}"
    );
    assert!(
        results.iter().any(|r| r.contains("RegexMatcher.find_at")),
        "trait impl members keep their self type qualifier: {results:?}"
    );
    assert!(
        !results.iter().any(|r| r.contains("method Matcher.find_at")),
        "trait name must not pose as the member owner: {results:?}"
    );
}

#[test]
fn test_trait_impl_without_self_type_falls_back_to_trait_name() {
    let group = EntityGroup {
        name: "Matcher".into(),
        kind: EntityKind::TraitImpl,
        members: vec![cce_types::entity::GroupedEntity {
            id: cce_types::entity::EntityId(2),
            name: "find_at".to_string(),
            kind: EntityKind::Method,
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);
    assert!(
        results.iter().any(|r| r.contains("Matcher.find_at")),
        "without recorded self type the trait name still qualifies: {results:?}"
    );
}

#[test]
fn test_impl_block_member_carries_owner_qualifier() {
    let group = EntityGroup {
        name: "Searcher".into(),
        kind: EntityKind::InherentImpl,
        members: vec![cce_types::entity::GroupedEntity {
            id: cce_types::entity::EntityId(2),
            name: "search_path".to_string(),
            kind: EntityKind::Method,
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };

    let template = RegularGroupTemplate::new();
    let results = template.generate(&group);
    assert!(
        results.iter().any(|r| r.contains("Searcher.search_path")),
        "impl methods keep their self type qualifier: {results:?}"
    );
}
