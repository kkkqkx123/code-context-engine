#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::ast_to_nl::{AstToNlConverter, ConversionRequest};
    use crate::grouper::types::{EntityGroup, GroupType, ProcessingResult};
    use cce_config::AstToNlConfig;
    use cce_types::entity::{EntityId, EntityKind, GroupedEntity};
    use cce_types::language::Language;
    use cce_types::{
        BehaviorFact, BehaviorFactKind, BehaviorStore, ControlFlowFact, ControlFlowFactKind,
        ControlFlowStore, OutputMode,
    };
    use compact_str::CompactString;

    fn create_test_header(
        id: u64,
        name: &str,
        kind: EntityKind,
        doc: Option<&str>,
    ) -> GroupedEntity {
        GroupedEntity {
            id: EntityId(id),
            kind,
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

    fn create_test_member(
        id: u64,
        name: &str,
        kind: EntityKind,
        doc: Option<&str>,
    ) -> GroupedEntity {
        GroupedEntity {
            id: EntityId(id),
            kind,
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

    fn create_test_group(id: u64, name: &str, language: Language) -> EntityGroup {
        let header = create_test_header(id, name, EntityKind::Function, Some("/// Function doc."));
        EntityGroup {
            group_id: CompactString::from(format!("group_{id}")),
            group_type: GroupType::Standalone,
            header: Some(header),
            header_id: Some(EntityId(id)),
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Function,
            name: CompactString::from(name),
            language,
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

    /// A mock plugin that emits a recognizable marker prefixed with its id.
    struct MarkerPlugin {
        meta: cce_plugin::PluginMetadata,
        marker: String,
    }

    impl cce_plugin::CodePlugin for MarkerPlugin {
        fn metadata(&self) -> &cce_plugin::PluginMetadata {
            &self.meta
        }
        fn supports_bm25(&self) -> bool {
            true
        }
        fn supports_embedding(&self) -> bool {
            true
        }
        fn generate_bm25(
            &self,
            _group: &EntityGroup,
        ) -> Result<Option<String>, cce_plugin::PluginError> {
            Ok(Some(format!("BM25:{}:{}", self.marker, self.meta.id)))
        }
        fn generate_embedding(
            &self,
            _group: &EntityGroup,
        ) -> Result<Option<String>, cce_plugin::PluginError> {
            Ok(Some(format!("EMB:{}:{}", self.marker, self.meta.id)))
        }
    }

    fn marker_plugin(id: &str, priority: i32, marker: &str) -> MarkerPlugin {
        MarkerPlugin {
            meta: cce_plugin::PluginMetadata {
                id: id.to_string(),
                name: id.to_string(),
                version: "0.1.0".to_string(),
                priority,
                capability_priorities: std::collections::HashMap::new(),
                description: None,
                capabilities: Vec::new(),
            },
            marker: marker.to_string(),
        }
    }

    #[test]
    fn test_convert_standalone_group_bm25() {
        let converter = AstToNlConverter::new();
        let header = create_test_header(
            1,
            "parse_json",
            EntityKind::Function,
            Some("/// Parses a JSON string."),
        );
        let group = EntityGroup {
            group_id: CompactString::from("group_1"),
            group_type: GroupType::Standalone,
            header: Some(header),
            header_id: Some(EntityId(1)),
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Function,
            name: CompactString::from("parse_json"),
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

        let groups = vec![group];
        let results = converter.convert_entity_groups(&groups, "src/parser.rs", None, None, None);

        assert_eq!(results.len(), 1);
        let group_conv = &results[0];
        assert!(group_conv.header_conversion.is_some());
        assert!(group_conv.member_conversions.is_empty());

        let conv = group_conv.header_conversion.as_ref().unwrap();
        assert_eq!(conv.name, "parse_json");
        assert!(conv.bm25_text.is_some());
    }

    #[test]
    fn test_convert_standalone_group_embedding() {
        let config = AstToNlConfig {
            default_mode: OutputMode::Embedding,
            ..Default::default()
        };
        let converter = AstToNlConverter::with_config(&config);
        let header = create_test_header(
            1,
            "validate_token",
            EntityKind::Function,
            Some("/// Validates an auth token."),
        );
        let group = EntityGroup {
            group_id: CompactString::from("group_1"),
            group_type: GroupType::Standalone,
            header: Some(header),
            header_id: Some(EntityId(1)),
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Function,
            name: CompactString::from("validate_token"),
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

        let groups = vec![group];
        let results = converter.convert_entity_groups(&groups, "src/auth.rs", None, None, None);

        assert_eq!(results.len(), 1);
        let conv = results[0].header_conversion.as_ref().unwrap();
        assert!(conv.embedding_text.is_some());
        assert!(conv.bm25_text.is_none());
    }

    #[test]
    fn test_convert_class_group_with_members() {
        let config = AstToNlConfig::both();
        let converter = AstToNlConverter::with_config(&config);
        let header = create_test_header(
            1,
            "UserService",
            EntityKind::Class,
            Some("/// Service for user management."),
        );
        let member = create_test_member(
            2,
            "create_user",
            EntityKind::Method,
            Some("/// Creates a new user."),
        );

        let group = EntityGroup {
            group_id: CompactString::from("group_1"),
            group_type: GroupType::ClassWithMethods,
            header: Some(header),
            header_id: Some(EntityId(1)),
            members: smallvec::smallvec![member],
            member_ids: smallvec::smallvec![EntityId(2)],
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Class,
            name: CompactString::from("UserService"),
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

        let groups = vec![group];
        let results = converter.convert_entity_groups(
            &groups,
            "src/services/user_service.rs",
            None,
            None,
            None,
        );

        assert_eq!(results.len(), 1);
        let group_conv = &results[0];
        assert!(group_conv.header_conversion.is_some());
        assert_eq!(group_conv.member_conversions.len(), 1);

        let header_text = group_conv
            .header_conversion
            .as_ref()
            .and_then(|conversion| conversion.embedding_text.as_deref())
            .expect("class group should produce embedding text");
        assert!(header_text.contains("Service for user management"));
        assert!(!header_text.contains("with 1 members"));
    }

    #[test]
    fn test_convert_multiple_groups() {
        let converter = AstToNlConverter::new();
        let header1 =
            create_test_header(1, "func_a", EntityKind::Function, Some("/// Function A."));
        let group1 = EntityGroup {
            group_id: CompactString::from("group_1"),
            group_type: GroupType::Standalone,
            header: Some(header1),
            header_id: Some(EntityId(1)),
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Function,
            name: CompactString::from("func_a"),
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
        let header2 =
            create_test_header(2, "func_b", EntityKind::Function, Some("/// Function B."));
        let group2 = EntityGroup {
            group_id: CompactString::from("group_2"),
            group_type: GroupType::Standalone,
            header: Some(header2),
            header_id: Some(EntityId(2)),
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Function,
            name: CompactString::from("func_b"),
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

        let groups = vec![group1, group2];
        let results = converter.convert_entity_groups(&groups, "src/lib.rs", None, None, None);

        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].header_conversion.as_ref().unwrap().name,
            "func_a"
        );
        assert_eq!(
            results[1].header_conversion.as_ref().unwrap().name,
            "func_b"
        );
    }

    #[test]
    fn test_convert_force_mode_override() {
        let converter = AstToNlConverter::new();
        let header = create_test_header(
            1,
            "compute_stats",
            EntityKind::Function,
            Some("/// Computes statistics."),
        );
        let group = EntityGroup {
            group_id: CompactString::from("group_1"),
            group_type: GroupType::Standalone,
            header: Some(header),
            header_id: Some(EntityId(1)),
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Function,
            name: CompactString::from("compute_stats"),
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

        let groups = vec![group];
        let request = ConversionRequest {
            force_mode: Some(OutputMode::Embedding),
        };
        let results =
            converter.convert_entity_groups(&groups, "src/stats.rs", Some(&request), None, None);

        assert_eq!(results.len(), 1);
        let conv = results[0].header_conversion.as_ref().unwrap();
        assert!(conv.embedding_text.is_some());
        assert!(conv.bm25_text.is_none());
    }

    #[test]
    fn test_convert_group_no_header() {
        let converter = AstToNlConverter::new();
        let group = EntityGroup {
            group_id: CompactString::from("group_empty"),
            group_type: GroupType::Standalone,
            header: None,
            header_id: None,
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Function,
            name: CompactString::from(""),
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

        let groups = vec![group];
        let results = converter.convert_entity_groups(&groups, "src/empty.rs", None, None, None);

        assert_eq!(results.len(), 1);
        assert!(results[0].header_conversion.is_none());
    }

    #[test]
    fn test_convert_entity_groups_for_index_uses_sidecars() {
        let converter = AstToNlConverter::with_config(&AstToNlConfig::both());
        let header =
            create_test_header(1, "demo", EntityKind::Function, Some("/// Demo function."));
        let member = create_test_member(2, "helper", EntityKind::Method, None);

        let group = EntityGroup {
            group_id: CompactString::from("group_1"),
            group_type: GroupType::ClassWithMethods,
            header: Some(header),
            header_id: Some(EntityId(1)),
            members: smallvec::smallvec![member],
            member_ids: smallvec::smallvec![EntityId(2)],
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Class,
            name: CompactString::from("Demo"),
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

        let source = "fn demo() { let x = 1; if value > 0 { return true; } }";
        let mut behavior = BehaviorStore::default();
        let bind_start = source.find("let x = 1;").unwrap();
        let bind_end = bind_start + "let x = 1;".len();
        behavior.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataBind,
            "let x = 1;",
            bind_start,
            bind_end,
        ));

        let mut control_flow = ControlFlowStore::default();
        let if_start = source.find("if value > 0").unwrap();
        let if_end = source.len() - 1;
        control_flow
            .entry_mut(EntityId(1))
            .push_fact(ControlFlowFact::new(
                ControlFlowFactKind::If,
                &source[if_start..if_end],
                if_start,
                if_end,
            ));

        let processing_result = ProcessingResult {
            groups: vec![group.clone()],
            entity_meta: HashMap::new(),
            behavior,
            control_flow,
            stats: Default::default(),
        };

        let pure_results =
            converter.convert_entity_groups(&[group], "src/demo.rs", None, None, None);
        let enriched_results = converter.convert_entity_groups(
            &processing_result.groups,
            "src/demo.rs",
            None,
            Some(&processing_result),
            Some(source),
        );

        let pure_text = pure_results[0]
            .header_conversion
            .as_ref()
            .and_then(|conv| conv.embedding_text.as_deref())
            .expect("pure conversion should produce embedding text");
        assert!(!pure_text.contains("Control flow:"));
        assert!(!pure_text.contains("Behavior:"));

        let enriched_text = enriched_results[0]
            .header_conversion
            .as_ref()
            .and_then(|conv| conv.embedding_text.as_deref())
            .expect("enriched conversion should produce embedding text");
        assert!(
            !enriched_text.contains("Control flow:"),
            "embedding text should not contain 'Control flow:' label, got: {}",
            enriched_text
        );
        assert!(
            !enriched_text.contains("Behavior:"),
            "embedding text should not contain 'Behavior:' label, got: {}",
            enriched_text
        );
        assert!(
            enriched_text.contains("if value > 0"),
            "expected raw source, got: {}",
            enriched_text
        );
        assert!(
            enriched_text.contains("return true"),
            "expected return true"
        );
        assert!(enriched_text.contains("let x = 1;"));
    }

    #[test]
    fn test_convert_entity_groups_for_index_empty_sidecars() {
        let converter = AstToNlConverter::with_config(&AstToNlConfig::both());
        let header = create_test_header(1, "empty", EntityKind::Function, Some("/// No sidecars."));

        let group = EntityGroup {
            group_id: CompactString::from("group_empty"),
            group_type: GroupType::Standalone,
            header: Some(header),
            header_id: Some(EntityId(1)),
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Function,
            name: CompactString::from("empty"),
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

        let source = "fn empty() {}";
        let processing_result = ProcessingResult {
            groups: vec![group.clone()],
            entity_meta: HashMap::new(),
            behavior: BehaviorStore::default(),
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };

        let enriched_results = converter.convert_entity_groups(
            &processing_result.groups,
            "src/empty.rs",
            None,
            Some(&processing_result),
            Some(source),
        );

        assert_eq!(enriched_results.len(), 1);
        let text = enriched_results[0]
            .header_conversion
            .as_ref()
            .and_then(|conv| conv.embedding_text.as_deref())
            .expect("should produce embedding text");
        assert!(
            !text.contains("Control flow:"),
            "empty control flow should not be appended"
        );
        assert!(
            !text.contains("Behavior:"),
            "empty behavior should not be appended"
        );
    }

    #[test]
    fn test_convert_entity_groups_for_index_only_behavior_no_control_flow() {
        let converter = AstToNlConverter::with_config(&AstToNlConfig::both());
        let header =
            create_test_header(1, "behave", EntityKind::Function, Some("/// Has behavior."));

        let group = EntityGroup {
            group_id: CompactString::from("group_behave"),
            group_type: GroupType::Standalone,
            header: Some(header),
            header_id: Some(EntityId(1)),
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Function,
            name: CompactString::from("behave"),
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

        let source = "fn behave() { let x = 1; }";
        let mut behavior = BehaviorStore::default();
        let bind_start = source.find("let x = 1;").unwrap();
        let bind_end = bind_start + "let x = 1;".len();
        behavior.entry_mut(EntityId(1)).push_fact(BehaviorFact::new(
            BehaviorFactKind::DataBind,
            "let x = 1;",
            bind_start,
            bind_end,
        ));

        let processing_result = ProcessingResult {
            groups: vec![group.clone()],
            entity_meta: HashMap::new(),
            behavior,
            control_flow: ControlFlowStore::default(),
            stats: Default::default(),
        };

        let enriched_results = converter.convert_entity_groups(
            &processing_result.groups,
            "src/behave.rs",
            None,
            Some(&processing_result),
            Some(source),
        );

        let text = enriched_results[0]
            .header_conversion
            .as_ref()
            .and_then(|conv| conv.embedding_text.as_deref())
            .expect("should produce embedding text");
        assert!(!text.contains("Control flow:"));
        assert!(!text.contains("Behavior:"));
        assert!(text.contains("let x = 1;"));
    }

    #[test]
    fn test_convert_entity_groups_for_index_only_control_flow_no_behavior() {
        let converter = AstToNlConverter::with_config(&AstToNlConfig::both());
        let header = create_test_header(
            1,
            "flow",
            EntityKind::Function,
            Some("/// Has control flow."),
        );

        let group = EntityGroup {
            group_id: CompactString::from("group_flow"),
            group_type: GroupType::Standalone,
            header: Some(header),
            header_id: Some(EntityId(1)),
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Function,
            name: CompactString::from("flow"),
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

        let source = "fn flow() { for i in 0..10 { process(i); } }";
        let mut control_flow = ControlFlowStore::default();
        let loop_start = source.find("for i in").unwrap();
        let loop_end = source.rfind('}').unwrap() + 1;
        control_flow
            .entry_mut(EntityId(1))
            .push_fact(ControlFlowFact::new(
                ControlFlowFactKind::Loop,
                &source[loop_start..loop_end],
                loop_start,
                loop_end,
            ));

        let processing_result = ProcessingResult {
            groups: vec![group.clone()],
            entity_meta: HashMap::new(),
            behavior: BehaviorStore::default(),
            control_flow,
            stats: Default::default(),
        };

        let enriched_results = converter.convert_entity_groups(
            &processing_result.groups,
            "src/flow.rs",
            None,
            Some(&processing_result),
            Some(source),
        );

        let text = enriched_results[0]
            .header_conversion
            .as_ref()
            .and_then(|conv| conv.embedding_text.as_deref())
            .expect("should produce embedding text");
        assert!(
            !text.contains("Control flow:"),
            "embedding text should not contain 'Control flow:' label"
        );
        assert!(text.contains("for i in"));
        assert!(
            !text.contains("Behavior:"),
            "empty behavior should not be appended"
        );
    }

    #[test]
    fn test_plugin_batch_partial_coverage_falls_back_per_group() {
        use cce_plugin::{PluginBundle, PluginRegistry};
        use std::sync::Arc;

        let mut registry = PluginRegistry::new();
        registry.register_bundle(
            PluginBundle::new(Arc::new(marker_plugin("py_plugin", 10, "P")))
                .with_languages(vec!["python".to_string()]),
        );

        let converter = AstToNlConverter::with_config(&AstToNlConfig::both())
            .with_plugin_registry(Arc::new(registry));

        // Python group: covered by the plugin.
        let py_group = create_test_group(1, "flask_route", Language::Python);
        // Rust group: no matching plugin — must fall back to built-in.
        let rs_group = create_test_group(2, "rust_fn", Language::Rust);

        let groups = vec![py_group, rs_group];
        let results = converter.convert_entity_groups(&groups, "src/mixed.py", None, None, None);

        assert_eq!(results.len(), 2);

        let py_conv = results[0].header_conversion.as_ref().unwrap();
        let py_bm25 = py_conv.bm25_text.as_deref().unwrap();
        assert!(
            py_bm25.contains("BM25:P:py_plugin"),
            "plugin should cover python group, got: {py_bm25}"
        );

        // The uncovered rust group must still produce built-in conversion
        // instead of aborting the whole batch.
        let rs_conv = results[1].header_conversion.as_ref().unwrap();
        let rs_bm25 = rs_conv.bm25_text.as_deref().unwrap();
        assert!(
            !rs_bm25.contains("BM25:"),
            "rust group should use built-in generator, got: {rs_bm25}"
        );
    }

    #[test]
    fn test_plugin_priority_is_deterministic() {
        use cce_plugin::{PluginBundle, PluginRegistry};
        use std::sync::Arc;

        let mut registry = PluginRegistry::new();
        // Higher priority must win regardless of registration order.
        registry.register_bundle(PluginBundle::new(Arc::new(marker_plugin("low", 1, "LOW"))));
        registry.register_bundle(PluginBundle::new(Arc::new(marker_plugin(
            "high", 100, "HIGH",
        ))));

        let converter = AstToNlConverter::with_config(&AstToNlConfig::both())
            .with_plugin_registry(Arc::new(registry));

        let group = create_test_group(1, "target", Language::Rust);
        let groups = vec![group];
        let results = converter.convert_entity_groups(&groups, "src/target.rs", None, None, None);

        let conv = results[0].header_conversion.as_ref().unwrap();
        let bm25 = conv.bm25_text.as_deref().unwrap();
        assert!(
            bm25.contains("BM25:HIGH:high"),
            "highest-priority plugin should win, got: {bm25}"
        );
        let emb = conv.embedding_text.as_deref().unwrap();
        assert!(
            emb.contains("EMB:HIGH:high"),
            "highest-priority plugin should win for embedding, got: {emb}"
        );
    }

    #[test]
    fn test_negative_priority_plugin_is_below_builtin_fallback() {
        use cce_plugin::{PluginBundle, PluginRegistry};
        use std::sync::Arc;

        // A below-builtin fallback plugin (negative priority, no language
        // filter). It must not run while the built-in produces conversions.
        let mut registry = PluginRegistry::new();
        registry.register_bundle(PluginBundle::new(Arc::new(marker_plugin(
            "below", -1, "BELOW",
        ))));

        let converter = AstToNlConverter::with_config(&AstToNlConfig::both())
            .with_plugin_registry(Arc::new(registry));

        // Ordinary function group: the built-in converter handles it, so the
        // fallback plugin stays silent.
        let normal = create_test_group(1, "target", Language::Rust);
        let results = converter.convert_entity_groups(&[normal], "src/target.rs", None, None, None);
        let conv = results[0].header_conversion.as_ref().unwrap();
        assert!(
            !conv
                .bm25_text
                .as_deref()
                .is_some_and(|t| t.contains("BELOW")),
            "fallback plugin must not run when the built-in produced text"
        );

        // Import-kind group: the built-in converter handles it too, so the
        // below-builtin tier still stays silent.
        let import_header =
            create_test_header(2, "use std::collections::HashMap", EntityKind::Import, None);
        let mut import_group =
            create_test_group(2, "use std::collections::HashMap", Language::Rust);
        import_group.header = Some(import_header);
        let results =
            converter.convert_entity_groups(&[import_group], "src/target.rs", None, None, None);
        let conv = results[0]
            .header_conversion
            .as_ref()
            .expect("the built-in converter should convert an import group");
        assert!(
            !conv
                .bm25_text
                .as_deref()
                .is_some_and(|t| t.contains("BELOW")),
            "fallback plugin must not run when the built-in converted the import group"
        );
        assert!(
            !conv
                .embedding_text
                .as_deref()
                .is_some_and(|t| t.contains("BELOW")),
            "fallback plugin must not run when the built-in converted the import group"
        );
    }

    #[test]
    fn test_positive_plugin_wins_over_below_builtin_plugin() {
        use cce_plugin::{PluginBundle, PluginRegistry};
        use std::sync::Arc;

        let mut registry = PluginRegistry::new();
        registry.register_bundle(PluginBundle::new(Arc::new(marker_plugin(
            "high", 10, "HIGH",
        ))));
        registry.register_bundle(PluginBundle::new(Arc::new(marker_plugin(
            "below", -1, "BELOW",
        ))));

        let converter = AstToNlConverter::with_config(&AstToNlConfig::both())
            .with_plugin_registry(Arc::new(registry));

        // Override-tier plugin wins by priority; the fallback tier is never
        // consulted even for groups the built-in cannot convert.
        let import_header =
            create_test_header(1, "use std::collections::HashMap", EntityKind::Import, None);
        let mut import_group =
            create_test_group(1, "use std::collections::HashMap", Language::Rust);
        import_group.header = Some(import_header);
        let results =
            converter.convert_entity_groups(&[import_group], "src/t.rs", None, None, None);
        let conv = results[0].header_conversion.as_ref().unwrap();
        let bm25 = conv.bm25_text.as_deref().unwrap();
        assert!(
            bm25.contains("BM25:HIGH:high"),
            "override tier must run before the built-in and the fallback tier, got: {bm25}"
        );
    }
}
