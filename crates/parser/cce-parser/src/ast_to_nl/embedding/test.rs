#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::ast_to_nl::embedding::EmbeddingGenerator;
    use crate::grouper::types::{EntityGroup, GroupType};
    use cce_config::EmbeddingGeneratorConfig;
    use cce_types::entity::{EntityId, EntityKind, GroupedEntity};
    use cce_types::language::Language;
    use compact_str::CompactString;

    fn create_func_entity(id: u64, name: &str, doc: Option<&str>) -> GroupedEntity {
        GroupedEntity {
            id: EntityId(id),
            kind: EntityKind::Function,
            name: name.to_string(),
            signature: String::new(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: doc.map(|s| s.to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        }
    }

    fn create_standalone_group(entity: GroupedEntity) -> EntityGroup {
        let name = entity.name.clone();
        EntityGroup {
            group_id: CompactString::from(format!("group_{}", entity.id.0)),
            group_type: GroupType::Standalone,
            header: Some(entity.clone()),
            header_id: Some(entity.id),
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: entity.kind,
            name: CompactString::from(name),
            language: Language::Rust,
            pattern_info: Default::default(),
            member_roles: Default::default(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        }
    }

    #[test]
    fn test_generate_function_with_doc() {
        let generator = EmbeddingGenerator::new();
        let entity = create_func_entity(1, "calculate_total", Some("/// Calculates total price."));
        let group = create_standalone_group(entity);
        let descriptions = generator.generate_for_group(&group);

        assert!(!descriptions.is_empty());
        let text = &descriptions[0];
        assert!(!text.is_empty());
    }

    #[test]
    fn test_generate_function_no_doc() {
        let generator = EmbeddingGenerator::new();
        let entity = create_func_entity(2, "process_user_data", None);
        let group = create_standalone_group(entity);
        let descriptions = generator.generate_for_group(&group);

        assert!(!descriptions.is_empty());
        let text = &descriptions[0];
        assert!(!text.is_empty());
    }

    #[test]
    fn test_generate_struct_entity() {
        let generator = EmbeddingGenerator::new();
        let entity = GroupedEntity {
            id: EntityId(3),
            kind: EntityKind::Struct,
            name: "UserConfig".to_string(),
            signature: String::new(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: Some("/// Holds user configuration.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        };
        let group = create_standalone_group(entity);
        let descriptions = generator.generate_for_group(&group);

        assert!(!descriptions.is_empty());
    }

    #[test]
    fn test_generate_class_group() {
        let generator = EmbeddingGenerator::new();
        let header = GroupedEntity {
            id: EntityId(10),
            kind: EntityKind::Class,
            name: "DatabasePool".to_string(),
            signature: String::new(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: Some("/// Manages a pool of database connections.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        };
        let member1 = GroupedEntity {
            id: EntityId(11),
            kind: EntityKind::Method,
            name: "connect".to_string(),
            signature: String::new(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: Some("/// Establishes a new connection.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        };
        let member2 = GroupedEntity {
            id: EntityId(12),
            kind: EntityKind::Method,
            name: "disconnect".to_string(),
            signature: String::new(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: Some("/// Closes an existing connection.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        };

        let group = EntityGroup {
            group_id: CompactString::from("group_10"),
            group_type: GroupType::ClassWithMethods,
            header: Some(header),
            header_id: Some(EntityId(10)),
            members: smallvec::smallvec![member1, member2],
            member_ids: smallvec::smallvec![EntityId(11), EntityId(12)],
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Class,
            name: CompactString::from("DatabasePool"),
            language: Language::Rust,
            pattern_info: Default::default(),
            member_roles: Default::default(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        };

        let descriptions = generator.generate_for_group(&group);
        assert!(!descriptions.is_empty());
    }

    #[test]
    fn test_word_limit_truncation() {
        let config = EmbeddingGeneratorConfig {
            max_summary_words: 5,
            ..Default::default()
        };
        let generator = EmbeddingGenerator::with_config(&config);
        let entity = create_func_entity(4, "long_name", Some("/// A B C D E F G H I J K L M N."));
        let group = create_standalone_group(entity);
        let descriptions = generator.generate_for_group(&group);

        assert!(!descriptions.is_empty());
        for desc in &descriptions {
            let word_count = desc.split_whitespace().count();
            assert!(word_count <= 5, "Description exceeds word limit: {}", desc);
        }
    }

    #[test]
    fn test_standalone_entity_compat() {
        let generator = EmbeddingGenerator::new();
        let entity = create_func_entity(
            5,
            "validate_email",
            Some("/// Validates an email address format."),
        );
        let text = generator.generate(&entity);

        assert!(!text.is_empty());
        assert!(text.contains("email") || text.contains("valid"));
    }

    #[test]
    fn test_multiple_members_generate_separate_descriptions() {
        let generator = EmbeddingGenerator::new();
        let header = GroupedEntity {
            id: EntityId(20),
            kind: EntityKind::Trait,
            name: "Serializable".to_string(),
            signature: String::new(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: Some("/// Interface for serialization.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        };
        let member = GroupedEntity {
            id: EntityId(21),
            kind: EntityKind::Method,
            name: "serialize".to_string(),
            signature: String::new(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: Some("/// Converts data to bytes.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        };

        let group = EntityGroup {
            group_id: CompactString::from("group_20"),
            group_type: GroupType::TraitWithImpls,
            header: Some(header),
            header_id: Some(EntityId(20)),
            members: smallvec::smallvec![member],
            member_ids: smallvec::smallvec![EntityId(21)],
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Trait,
            name: CompactString::from("Serializable"),
            language: Language::Rust,
            pattern_info: Default::default(),
            member_roles: Default::default(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        };

        let descriptions = generator.generate_for_group(&group);
        assert!(!descriptions.is_empty());
    }

    #[test]
    fn test_default_config_creation() {
        let generator = EmbeddingGenerator::default();
        let entity = create_func_entity(6, "test_default", Some("/// Default test."));
        let text = generator.generate(&entity);
        assert!(!text.is_empty());
    }
}
