#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::ast_to_nl::bm25::Bm25Generator;
    use crate::grouper::types::{EntityGroup, GroupType};
    use cce_types::entity::{EntityId, EntityKind, GroupedEntity};
    use cce_types::language::Language;
    use compact_str::CompactString;

    fn create_func_entity(id: u64, name: &str, doc: Option<&str>) -> GroupedEntity {
        GroupedEntity {
            id: EntityId(id),
            kind: EntityKind::Function,
            name: name.to_string(),
            signature: format!("fn {}()", name),
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
    fn test_generate_function_bm25() {
        let generator = Bm25Generator::new();
        let entity = create_func_entity(
            1,
            "calculate_total_price",
            Some("/// Calculates the total price."),
        );
        let text = generator.generate(&entity);

        assert!(!text.is_empty());
        assert!(text.contains("calculate_total_price") || text.contains("calculate"));
    }

    #[test]
    fn test_generate_function_no_doc() {
        let generator = Bm25Generator::new();
        let entity = create_func_entity(2, "processUserData", None);
        let text = generator.generate(&entity);

        assert!(!text.is_empty());
    }

    #[test]
    fn test_generate_struct_bm25() {
        let generator = Bm25Generator::new();
        let entity = GroupedEntity {
            id: EntityId(3),
            kind: EntityKind::Struct,
            name: "AppConfig".to_string(),
            signature: "struct AppConfig".to_string(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: Some("/// Holds application configuration.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        };
        let text = generator.generate(&entity);

        assert!(!text.is_empty());
    }

    #[test]
    fn test_generate_for_group() {
        let generator = Bm25Generator::new();
        let entity =
            create_func_entity(4, "query_database", Some("/// Executes a database query."));
        let group = create_standalone_group(entity);
        let text = generator.generate_for_group(&group);

        assert!(!text.is_empty());
    }

    #[test]
    fn test_extract_keywords() {
        let generator = Bm25Generator::new();
        let entity = create_func_entity(
            5,
            "find_user_by_email",
            Some("/// Finds a user by their email address."),
        );
        let keywords = generator.extract_keywords(&entity);

        assert!(!keywords.is_empty());
        assert!(keywords.contains(&"find_user_by_email".to_string()));
    }

    #[test]
    fn test_extract_keywords_no_doc() {
        let generator = Bm25Generator::new();
        let entity = create_func_entity(6, "validate_input", None);
        let keywords = generator.extract_keywords(&entity);

        assert!(!keywords.is_empty());
    }

    #[test]
    fn test_class_group_with_members() {
        let generator = Bm25Generator::new();
        let header = GroupedEntity {
            id: EntityId(10),
            kind: EntityKind::Class,
            name: "HttpClient".to_string(),
            signature: "class HttpClient".to_string(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: Some("/// HTTP client for API requests.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        };
        let member = GroupedEntity {
            id: EntityId(11),
            kind: EntityKind::Method,
            name: "get".to_string(),
            signature: "fn get(&self, key: &str) -> Option<Response>".to_string(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: Some("/// Sends a GET request.".to_string()),
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
            members: smallvec::smallvec![member],
            member_ids: smallvec::smallvec![EntityId(11)],
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Class,
            name: CompactString::from("HttpClient"),
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

        let text = generator.generate_for_group(&group);
        assert!(!text.is_empty());
        assert!(!text.contains("&self"));
        assert!(!text.contains("->"));
        assert!(!text.contains("key:"));
    }

    #[test]
    fn test_keyword_count_limit() {
        use cce_config::Bm25GeneratorConfig;
        let config = Bm25GeneratorConfig { max_keywords: 3 };
        let generator = Bm25Generator::with_config(&config);
        let entity = create_func_entity(
            7,
            "find_all_active_users",
            Some("/// Finds all active users in the system."),
        );
        let keywords = generator.extract_keywords(&entity);

        assert!(keywords.len() <= 3);
    }
}
