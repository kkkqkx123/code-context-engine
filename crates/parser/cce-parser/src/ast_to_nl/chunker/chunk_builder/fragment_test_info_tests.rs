use super::*;
use crate::grouper::types::GroupType;
use cce_types::entity::GroupedEntity;
use cce_types::language::Language;
use compact_str::CompactString;
use smallvec::SmallVec;
use std::collections::HashMap;

fn mixed_group() -> EntityGroup {
    let mut header = GroupedEntity::new(
        EntityId(10),
        EntityKind::Function,
        "impl".to_string(),
        "impl".to_string(),
    );
    header.test_info = cce_types::TestInfo::unknown();
    let mut prod = GroupedEntity::new(
        EntityId(11),
        EntityKind::Function,
        "new".to_string(),
        "new".to_string(),
    );
    prod.test_info = cce_types::TestInfo::unknown();
    let mut helper = GroupedEntity::new(
        EntityId(12),
        EntityKind::Function,
        "add_str".to_string(),
        "add_str".to_string(),
    );
    helper.test_info = cce_types::TestInfo::test_ast();
    EntityGroup {
        group_id: CompactString::from("group_10"),
        group_type: GroupType::ClassWithMethods,
        header: Some(header),
        header_id: Some(EntityId(10)),
        members: smallvec::smallvec![prod, helper],
        member_ids: smallvec::smallvec![EntityId(11), EntityId(12)],
        entity_spans: HashMap::new(),
        combined_source: None,
        combined_source_lazy: std::sync::OnceLock::new(),
        span: cce_types::Span::default(),
        kind: EntityKind::Function,
        name: CompactString::from("Builder"),
        language: Language::Rust,
        pattern_info: crate::grouper::types::PatternInfo::None,
        member_roles: SmallVec::new(),
        nested_groups: Box::new([]),
        nesting_level: 0,
        parent_group_id: None,
        has_significant_nested: false,
        metadata: Default::default(),
        test_info: cce_types::TestInfo::unknown(),
    }
}

#[test]
fn test_fragment_info_separates_prod_and_test() {
    let group = mixed_group();
    let prod = ChunkBuilder::fragment_test_info(&group, &[EntityId(11)]);
    assert!(!prod.is_test());
    let helper = ChunkBuilder::fragment_test_info(&group, &[EntityId(12)]);
    assert!(helper.is_test());
    let mixed = ChunkBuilder::fragment_test_info(&group, &[EntityId(11), EntityId(12)]);
    assert!(mixed.is_test());
    let empty = ChunkBuilder::fragment_test_info(&group, &[]);
    assert_eq!(empty, group.test_info);
}
