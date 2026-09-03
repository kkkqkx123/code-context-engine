use cce_parser::grouper::types::EntityGroup;
use cce_types::Span;
use cce_types::entity::EntityKind;
use compact_str::CompactString;
use smallvec::smallvec;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;

#[test]
fn test_direct_export_generator_basic() {
    use cce_orchestrator::export::DirectExportGenerator;
    use cce_types::entity::GroupedEntity;

    let code = r#"pub fn add(a: i32, b: i32) -> i32 {
    a + b
}"#;

    let entity = GroupedEntity {
        id: Default::default(),
        name: "add".to_string(),
        kind: EntityKind::Function,
        signature: "pub fn add(a: i32, b: i32) -> i32".to_string(),
        parameters: smallvec![
            (CompactString::new("a"), Some(CompactString::new("i32"))),
            (CompactString::new("b"), Some(CompactString::new("i32"))),
        ],
        return_type: Some("i32".to_string()),
        doc_comment: Some("/// Adds two numbers.".to_string()),
        modifiers: vec!["pub".to_string()],
        attributes: HashMap::new(),
        subtype: None,
        is_stdlib: false,
        stdlib_category: None,
        metadata: HashMap::new(),
    };

    let group = EntityGroup {
        group_id: "test_group".into(),
        group_type: cce_parser::grouper::types::GroupType::Standalone,
        header: Some(entity),
        header_id: None,
        members: smallvec![],
        member_ids: smallvec![],
        entity_spans: HashMap::new(),
        combined_source: Some(Arc::from(code)),
        combined_source_lazy: OnceLock::new(),
        span: Span::from_lines(1, 3),
        kind: EntityKind::Function,
        name: "add".into(),
        language: cce_types::language::Language::Rust,
        pattern_info: Default::default(),
        member_roles: smallvec![],
        nested_groups: Box::new([]),
        nesting_level: 0,
        parent_group_id: None,
        has_significant_nested: false,
        metadata: HashMap::new(),
        test_info: cce_types::TestInfo::unknown(),
    };

    let conversions = cce_parser::ast_to_nl::converter::group_converter::GroupConversions {
        group,
        header_conversion: None,
        member_conversions: vec![],
    };
    let export = DirectExportGenerator::generate(&conversions).expect("export failed");

    assert_eq!(export.name, "add");
    assert_eq!(export.kind, EntityKind::Function);
    assert_eq!(export.modifiers, vec!["pub"]);
    assert!(export.source_code.contains("a + b"));
    assert!(export.doc_comment.is_some());
}

#[test]
fn test_clean_doc_comment_variations() {
    use cce_orchestrator::export::DirectExportGenerator;

    // Triple slash
    let result = DirectExportGenerator::clean_doc_comment("/// This is a doc");
    assert_eq!(result, "This is a doc");

    // Double slash (regular comment)
    let result = DirectExportGenerator::clean_doc_comment("// Regular comment");
    assert_eq!(result, "Regular comment");

    // Block comment style
    let result = DirectExportGenerator::clean_doc_comment("/** This is a block\n* comment\n*/");
    assert!(result.contains("This is a block"));
    assert!(result.contains("comment"));

    // Empty doc
    let result = DirectExportGenerator::clean_doc_comment("///");
    assert_eq!(result, "");
}
