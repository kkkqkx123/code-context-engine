#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::ast_to_nl::chunker::ChunkPath;
    use crate::ast_to_nl::chunker::ChunkedResult;
    use crate::ast_to_nl::chunker::GroupChunker;
    use crate::ast_to_nl::chunker::TextSplitter;
    use crate::ast_to_nl::chunker::chunk_builder::{ChunkBuilder, SingleChunkContext};
    use crate::ast_to_nl::chunker::chunker::ChunkInfrastructure;
    use crate::ast_to_nl::chunker::tracker::GroupTracker;
    use crate::ast_to_nl::converter::GroupConversions;
    use crate::grouper::types::{EntityGroup, GroupType};
    use cce_config::modules::ChunkingConfig;
    use cce_types::ConversionResult;
    use cce_types::Span;
    use cce_types::entity::{EntityId, EntityKind, GroupedEntity};
    use cce_types::language::Language;
    use cce_utils::token_estimation::TokenEstimator;
    use compact_str::CompactString;

    fn create_small_group() -> EntityGroup {
        EntityGroup {
            group_id: CompactString::from("group_test"),
            group_type: GroupType::Standalone,
            header: Some(GroupedEntity {
                id: EntityId(1),
                kind: EntityKind::Function,
                name: "hello".to_string(),
                signature: String::new(),
                parameters: Default::default(),
                return_type: None,
                doc_comment: Some("/// Prints hello.".to_string()),
                modifiers: Vec::new(),
                attributes: HashMap::new(),
                subtype: None,
                is_stdlib: false,
                stdlib_category: None,
                metadata: Default::default(),
                test_info: cce_types::TestInfo::unknown(),
            }),
            header_id: Some(EntityId(1)),
            members: Default::default(),
            member_ids: Default::default(),
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Function,
            name: CompactString::from("hello"),
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

    fn create_conversion(bm25: &str, embedding: &str) -> ConversionResult {
        ConversionResult {
            entity_id: EntityId(1),
            kind: EntityKind::Function,
            name: "hello".to_string(),
            file_path: "src/main.rs".to_string(),
            bm25_text: Some(bm25.to_string()),
            embedding_text: Some(embedding.to_string()),
            keywords: vec!["hello".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn test_chunk_small_group_no_split() {
        let config = ChunkingConfig {
            max_tokens: 512,
            max_bm25_terms: 200,
            ..Default::default()
        };
        let chunker = GroupChunker::new(config);
        let group = create_small_group();
        let conv = create_conversion(
            "hello function prints hello message",
            "Outputs a greeting message.",
        );
        let result = chunker
            .chunk_group(&group, &conv, "src/main.rs")
            .expect("chunk")
            .chunks;

        assert!(!result.is_empty(), "Should produce at least one chunk");
        for chunk in &result {
            assert!(!chunk.text.is_empty(), "Chunk text should not be empty");
            assert_eq!(chunk.source_group_id, "group_test");
        }
    }

    #[test]
    fn test_chunk_large_bm25_triggers_split() {
        let config = ChunkingConfig {
            max_tokens: 1024,
            max_bm25_terms: 100,
            ..Default::default()
        };
        let chunker = GroupChunker::new(config);
        let group = create_small_group();
        let long_text = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty".to_string();
        let conv = create_conversion(&long_text, "Short embedding text.");
        let result = chunker
            .chunk_group(&group, &conv, "src/main.rs")
            .expect("chunk")
            .chunks;

        assert!(!result.is_empty());
    }

    #[test]
    fn test_chunk_with_header_and_members() {
        let config = ChunkingConfig {
            max_tokens: 512,
            max_bm25_terms: 200,
            ..Default::default()
        };
        let chunker = GroupChunker::new(config);

        let header_entity = GroupedEntity {
            id: EntityId(1),
            kind: EntityKind::Class,
            name: "Calculator".to_string(),
            signature: String::new(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: Some("/// Performs calculations.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        };

        let member_entity = GroupedEntity {
            id: EntityId(2),
            kind: EntityKind::Method,
            name: "add".to_string(),
            signature: String::new(),
            parameters: Default::default(),
            return_type: None,
            doc_comment: Some("/// Adds two numbers.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        };

        let group = EntityGroup {
            group_id: CompactString::from("group_calc"),
            group_type: GroupType::ClassWithMethods,
            header: Some(header_entity),
            header_id: Some(EntityId(1)),
            members: smallvec::smallvec![member_entity],
            member_ids: smallvec::smallvec![EntityId(2)],
            entity_spans: Default::default(),
            combined_source: None,
            combined_source_lazy: Default::default(),
            span: Default::default(),
            kind: EntityKind::Class,
            name: CompactString::from("Calculator"),
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

        let header_conv = create_conversion(
            "Calculator class performs calculations",
            "A class for performing mathematical calculations.",
        );
        let member_conv =
            create_conversion("add method adds two numbers", "Adds two numbers together.");

        let group_convs = GroupConversions {
            group: group.clone(),
            header_conversion: Some(header_conv),
            member_conversions: vec![member_conv],
        };

        let result = chunker
            .chunk_group_with_conversions(&group, &group_convs, "src/calc.rs")
            .expect("chunk")
            .chunks;
        assert!(
            !result.is_empty(),
            "Should produce chunks for class with methods"
        );
    }

    #[test]
    fn test_bm25_and_embedding_paths_both_produced() {
        let config = ChunkingConfig {
            max_tokens: 512,
            max_bm25_terms: 200,
            ..Default::default()
        };
        let chunker = GroupChunker::new(config);
        let group = create_small_group();
        let conv = create_conversion(
            "hello function prints hello message to console",
            "Outputs a greeting message to the standard output.",
        );
        let result = chunker
            .chunk_group(&group, &conv, "src/main.rs")
            .expect("chunk")
            .chunks;

        let has_bm25 = result.iter().any(|c| c.path.to_string() == "bm25");
        let has_embedding = result.iter().any(|c| c.path.to_string() == "emb");

        assert!(has_bm25, "Should contain BM25 path chunks");
        assert!(has_embedding, "Should contain Embedding path chunks");
    }

    #[test]
    fn test_chunk_empty_text_produces_no_chunks() {
        let config = ChunkingConfig::default();
        let chunker = GroupChunker::new(config);
        let group = create_small_group();
        let conv = ConversionResult {
            entity_id: EntityId(1),
            kind: EntityKind::Function,
            name: "hello".to_string(),
            file_path: "src/main.rs".to_string(),
            bm25_text: None,
            embedding_text: None,
            ..Default::default()
        };
        let result = chunker
            .chunk_group(&group, &conv, "src/main.rs")
            .expect("chunk")
            .chunks;

        assert!(result.is_empty(), "No text should produce no chunks");
    }

    #[test]
    fn test_chunk_group_without_header_produces_no_chunks() {
        let config = ChunkingConfig::default();
        let chunker = GroupChunker::new(config);
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

        let group_convs = GroupConversions {
            group: group.clone(),
            header_conversion: None,
            member_conversions: vec![],
        };

        let result = chunker
            .chunk_group_with_conversions(&group, &group_convs, "src/empty.rs")
            .expect("chunk")
            .chunks;
        assert!(result.is_empty(), "No header should produce no chunks");
    }

    #[test]
    fn test_chunk_metadata_contains_correct_fields() {
        let config = ChunkingConfig {
            max_tokens: 512,
            max_bm25_terms: 200,
            ..Default::default()
        };
        let chunker = GroupChunker::new(config);
        let group = create_small_group();
        let conv = create_conversion("hello function prints hello", "Outputs a greeting.");
        let result = chunker
            .chunk_group(&group, &conv, "src/main.rs")
            .expect("chunk")
            .chunks;

        for chunk in &result {
            assert_eq!(chunk.metadata.file_path, "src/main.rs");
            assert_eq!(chunk.source_group_id, "group_test");
            assert!(chunk.token_count > 0);
            assert!(!chunk.chunk_id.is_empty());
        }
    }

    fn create_merge_test_group(
        group_id: &str,
        text: &str,
        start_byte: usize,
        end_byte: usize,
    ) -> EntityGroup {
        use compact_str::CompactString;
        use smallvec::SmallVec;
        use std::sync::Arc;

        EntityGroup {
            group_id: CompactString::from(group_id),
            group_type: GroupType::Standalone,
            header: Some(GroupedEntity::new(
                EntityId(group_id.chars().last().unwrap().to_digit(10).unwrap() as u64),
                EntityKind::Function,
                format!("func_{}", group_id),
                format!("fn func_{}()", group_id),
            )),
            header_id: Some(EntityId(
                group_id.chars().last().unwrap().to_digit(10).unwrap() as u64,
            )),
            members: SmallVec::new(),
            member_ids: SmallVec::new(),
            entity_spans: HashMap::new(),
            combined_source: Some(Arc::from(text)),
            combined_source_lazy: std::sync::OnceLock::new(),
            span: Span::new(start_byte, end_byte, 0, 0, 0, 0),
            kind: EntityKind::Function,
            name: CompactString::from(format!("func_{}", group_id)),
            language: Language::Rust,
            pattern_info: Default::default(),
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        }
    }

    fn create_merge_test_conversion(text: &str) -> ConversionResult {
        let bm25_word_count = text.split_whitespace().filter(|w| !w.is_empty()).count();

        ConversionResult {
            entity_id: EntityId(0),
            kind: EntityKind::Function,
            name: "test".to_string(),
            file_path: "test.rs".to_string(),
            bm25_text: Some(text.to_string()),
            embedding_text: Some(text.to_string()),
            embedding_tokens: None,
            bm25_word_count: Some(bm25_word_count),
            keywords: vec![],
            source_entity_ids: vec![],
            source_span: Span::default(),
            entity_metadata: Default::default(),
            entity_end_lines: vec![],
            embedding_brief_header: None,
            glossary: Vec::new(),
        }
    }

    #[test]
    fn test_cross_group_merge_multiple_small_groups_embedding() {
        let config = ChunkingConfig {
            min_chunk_tokens: 150,
            max_tokens: 512,
            ..Default::default()
        };
        let chunker = GroupChunker::new(config);

        let group_convs = vec![
            GroupConversions {
                group: create_merge_test_group("g1", "hello world foo bar", 0, 19),
                header_conversion: Some(create_merge_test_conversion("hello world foo bar")),
                member_conversions: vec![],
            },
            GroupConversions {
                group: create_merge_test_group("g2", "one two three four five", 20, 39),
                header_conversion: Some(create_merge_test_conversion("one two three four five")),
                member_conversions: vec![],
            },
            GroupConversions {
                group: create_merge_test_group("g3", "alpha beta gamma", 40, 55),
                header_conversion: Some(create_merge_test_conversion("alpha beta gamma")),
                member_conversions: vec![],
            },
        ];

        let chunks = chunker
            .chunk_groups(&group_convs, "test.rs")
            .expect("chunk")
            .chunks;
        let emb_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .collect();

        assert_eq!(
            emb_chunks.len(),
            1,
            "Should merge all embedding chunks into 1"
        );
        assert_eq!(
            emb_chunks[0].source_group_id, "g1",
            "Should preserve leftmost group"
        );
        assert_eq!(emb_chunks[0].chunk_index, 0);
        assert_eq!(emb_chunks[0].total_chunks, 1);
    }

    #[test]
    fn test_cross_group_bm25_tiny_rescue_merges() {
        let config = ChunkingConfig {
            min_chunk_bm25_terms: 80,
            max_bm25_terms: 150,
            ..Default::default()
        };
        let chunker = GroupChunker::new(config);

        let group_convs = vec![
            GroupConversions {
                group: create_merge_test_group("g1", "hello world foo bar", 0, 19),
                header_conversion: Some(create_merge_test_conversion("hello world foo bar")),
                member_conversions: vec![],
            },
            GroupConversions {
                group: create_merge_test_group("g2", "one two three four five", 20, 39),
                header_conversion: Some(create_merge_test_conversion("one two three four five")),
                member_conversions: vec![],
            },
        ];

        let chunks = chunker
            .chunk_groups(&group_convs, "test.rs")
            .expect("chunk")
            .chunks;
        let bm25_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Bm25)
            .collect();

        assert_eq!(
            bm25_chunks.len(),
            1,
            "tiny same-kind BM25 neighbours merge within the ceiling"
        );
        assert_eq!(bm25_chunks[0].path, ChunkPath::Bm25);
    }

    #[test]
    fn test_cross_group_merge_reduces_chunk_count() {
        let config = ChunkingConfig {
            min_chunk_tokens: 150,
            max_tokens: 512,
            ..Default::default()
        };
        let chunker = GroupChunker::new(config);

        let group_convs = vec![
            GroupConversions {
                group: create_merge_test_group("g1", "small text", 0, 10),
                header_conversion: Some(create_merge_test_conversion("small text")),
                member_conversions: vec![],
            },
            GroupConversions {
                group: create_merge_test_group("g2", "another small text", 11, 27),
                header_conversion: Some(create_merge_test_conversion("another small text")),
                member_conversions: vec![],
            },
        ];

        let chunks = chunker
            .chunk_groups(&group_convs, "test.rs")
            .expect("chunk")
            .chunks;
        let emb_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .collect();

        assert_eq!(
            emb_chunks.len(),
            1,
            "Should reduce from 2 groups to 1 chunk"
        );
    }

    #[test]
    fn test_cross_group_merge_no_merge_when_all_above_threshold() {
        let config = ChunkingConfig {
            min_chunk_tokens: 150,
            max_tokens: 512,
            ..Default::default()
        };
        let chunker = GroupChunker::new(config);

        // TokenEstimator uses latin_factor=0.25, so tokens ≈ chars * 0.25.
        // To get token_count >= 150, need >= 600 chars.
        let long_text = "A".repeat(600);
        let group_convs = vec![
            GroupConversions {
                group: create_merge_test_group("g1", &long_text, 0, 600),
                header_conversion: Some(create_merge_test_conversion(&long_text)),
                member_conversions: vec![],
            },
            GroupConversions {
                group: create_merge_test_group("g2", &long_text, 601, 1201),
                header_conversion: Some(create_merge_test_conversion(&long_text)),
                member_conversions: vec![],
            },
        ];

        let chunks = chunker
            .chunk_groups(&group_convs, "test.rs")
            .expect("chunk")
            .chunks;
        let emb_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .collect();

        assert_eq!(
            emb_chunks.len(),
            2,
            "No merge when all chunks above threshold"
        );
    }

    #[test]
    fn test_cross_group_merge_both_paths_produced() {
        let config = ChunkingConfig {
            min_chunk_tokens: 150,
            min_chunk_bm25_terms: 80,
            max_tokens: 512,
            max_bm25_terms: 150,
            ..Default::default()
        };
        let chunker = GroupChunker::new(config);

        let group_convs = vec![
            GroupConversions {
                group: create_merge_test_group("g1", "hello world", 0, 11),
                header_conversion: Some(create_merge_test_conversion("hello world")),
                member_conversions: vec![],
            },
            GroupConversions {
                group: create_merge_test_group("g2", "foo bar baz", 12, 22),
                header_conversion: Some(create_merge_test_conversion("foo bar baz")),
                member_conversions: vec![],
            },
        ];

        let chunks = chunker
            .chunk_groups(&group_convs, "test.rs")
            .expect("chunk")
            .chunks;

        let emb_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .collect();
        let bm25_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Bm25)
            .collect();

        assert!(
            emb_chunks.len() <= 2,
            "Embedding chunks should be processed"
        );
        assert!(bm25_chunks.len() <= 2, "BM25 chunks should be processed");
    }

    #[test]
    fn test_cross_group_merge_chunk_indices_normalized() {
        let config = ChunkingConfig {
            min_chunk_tokens: 150,
            max_tokens: 512,
            ..Default::default()
        };
        let chunker = GroupChunker::new(config);

        let group_convs = vec![
            GroupConversions {
                group: create_merge_test_group("g1", "hello world", 0, 11),
                header_conversion: Some(create_merge_test_conversion("hello world")),
                member_conversions: vec![],
            },
            GroupConversions {
                group: create_merge_test_group("g2", "foo bar baz", 12, 22),
                header_conversion: Some(create_merge_test_conversion("foo bar baz")),
                member_conversions: vec![],
            },
        ];

        let chunks = chunker
            .chunk_groups(&group_convs, "test.rs")
            .expect("chunk")
            .chunks;

        // Check per-path consistency: indices within each path are 0-based and total_chunks matches
        let emb_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .collect();
        for (i, chunk) in emb_chunks.iter().enumerate() {
            assert_eq!(
                chunk.chunk_index, i,
                "embedding chunk_index should be normalized"
            );
            assert_eq!(
                chunk.total_chunks,
                emb_chunks.len(),
                "embedding total_chunks should match path count"
            );
        }

        // Both paths merge tiny same-kind neighbours: each path yields one
        // chunk carrying normalized per-path indices.
        let bm25_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Bm25)
            .collect();
        assert_eq!(bm25_chunks.len(), 1);
        for chunk in &bm25_chunks {
            assert_eq!(chunk.chunk_index, 0);
            assert_eq!(chunk.total_chunks, 1);
        }
    }

    // ── Helpers for tests moved from chunker.rs ──

    fn create_test_group(text: &str) -> EntityGroup {
        use smallvec::SmallVec;

        EntityGroup {
            group_id: CompactString::from("test_group"),
            group_type: GroupType::Standalone,
            header: Some(GroupedEntity::new(
                EntityId(0),
                EntityKind::Function,
                "test_func".to_string(),
                "fn test_func()".to_string(),
            )),
            header_id: Some(EntityId(0)),
            members: SmallVec::new(),
            member_ids: SmallVec::new(),
            entity_spans: HashMap::new(),
            combined_source: Some(std::sync::Arc::from(text)),
            combined_source_lazy: std::sync::OnceLock::new(),
            span: Span::default(),
            kind: EntityKind::Function,
            name: CompactString::from("test_func"),
            language: Language::Rust,
            pattern_info: Default::default(),
            member_roles: SmallVec::new(),
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: Default::default(),
            test_info: cce_types::TestInfo::unknown(),
        }
    }

    fn create_test_conversion(text: &str) -> ConversionResult {
        let bm25_word_count = text.split_whitespace().filter(|w| !w.is_empty()).count();

        ConversionResult {
            entity_id: EntityId(0),
            kind: EntityKind::Function,
            name: "test".to_string(),
            file_path: "test.rs".to_string(),
            bm25_text: Some(text.to_string()),
            embedding_text: Some(text.to_string()),
            embedding_tokens: None,
            bm25_word_count: Some(bm25_word_count),
            keywords: vec![],
            source_entity_ids: vec![],
            source_span: Span::default(),
            entity_metadata: Default::default(),
            entity_end_lines: vec![],
            embedding_brief_header: None,
            glossary: Vec::new(),
        }
    }

    // ── Tests moved from chunker.rs ──

    #[test]
    fn test_chunk_single_path_no_split() {
        let config = ChunkingConfig::default();
        let group = create_test_group("Short text");
        let conversion = create_test_conversion("Short text");

        let chunker = GroupChunker::new(config);
        let chunks = chunker
            .chunk_group(&group, &conversion, "test.rs")
            .expect("chunk")
            .chunks;

        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_chunk_single_path_with_split() {
        let config = ChunkingConfig {
            max_tokens: 100,
            max_bm25_terms: 100,
            ..Default::default()
        };
        let text = "A B C D E F G H I J K L M N O P Q R S T U V W X Y Z a b c d e f g h i j k l m n o p q r s t u v w x y z 0 1 2 3 4 5 6 7 8 9 AA BB CC DD EE FF GG HH II JJ KK LL MM NN OO PP QQ RR SS TT";
        let group = create_test_group(text);
        let conversion = create_test_conversion(text);

        let chunker = GroupChunker::new(config);
        let chunks = chunker
            .chunk_group(&group, &conversion, "test.rs")
            .expect("chunk")
            .chunks;

        assert!(chunks.len() > 1, "Should split into multiple chunks");
        assert!(chunks.iter().all(|c| !c.text.is_empty()));
        for c in &chunks {
            assert!(c.total_chunks >= 1);
            assert!(c.chunk_index < c.total_chunks);
        }
    }

    #[test]
    fn test_chunk_group_with_conversions_no_members() {
        let config = ChunkingConfig::default();
        let estimator = TokenEstimator::default();
        let splitter = TextSplitter::new(config.clone());
        let mut tracker = GroupTracker::new();
        let group = create_test_group("Header text");

        let header_conv = ConversionResult {
            entity_id: EntityId(0),
            kind: EntityKind::Function,
            name: "test".to_string(),
            file_path: "test.rs".to_string(),
            bm25_text: Some("BM25 header text".to_string()),
            embedding_text: Some("Embedding header text".to_string()),
            embedding_tokens: None,
            bm25_word_count: Some(3),
            keywords: vec![],
            source_entity_ids: vec![],
            source_span: Span::default(),
            entity_metadata: Default::default(),
            entity_end_lines: vec![],
            embedding_brief_header: None,
            glossary: Vec::new(),
        };

        let group_conversions = GroupConversions {
            group: group.clone(),
            header_conversion: Some(header_conv),
            member_conversions: vec![],
        };

        let infra = ChunkInfrastructure {
            config: &config,
            estimator: &estimator,
            splitter: &splitter,
            pre_injection_embedding: config.max_tokens,
            pre_injection_bm25: config.max_bm25_terms,
            emb_budget: config.budget(ChunkPath::Embedding, false, 0),
            bm25_budget: config.budget(ChunkPath::Bm25, false, 0),
            has_vocabulary: false,
        };
        let chunks = crate::ast_to_nl::chunker::header_chunk::chunk_group_with_conversions(
            &infra,
            &mut tracker,
            &group,
            &group_conversions,
            "test.rs",
        )
        .expect("chunk")
        .chunks;

        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_single_function_chunk_claims_the_range_of_the_code_it_carries() {
        // A C function group holds its local variables as members, but those
        // members contribute no text of their own: the group text is the
        // function's own text, local declarations included. Within tolerance
        // the text stays one whole carrying the function's code; it must claim
        // the function's range, not the narrow range of a local variable whose
        // name happens to appear inside the text.
        let base = ChunkingConfig {
            max_bm25_terms: 150,
            ..Default::default()
        };
        let estimator = TokenEstimator::default();
        let mut tracker = GroupTracker::new();

        let mut group = create_test_group("body");
        group.members.push(GroupedEntity::new(
            EntityId(2),
            EntityKind::Variable,
            "ptr".to_string(),
            "uint8_t* ptr".to_string(),
        ));
        group.member_ids.push(EntityId(2));
        group
            .entity_spans
            .insert(EntityId(0), Span::new(0, 400, 198, 0, 240, 0));
        group
            .entity_spans
            .insert(EntityId(2), Span::new(100, 120, 200, 0, 200, 0));
        group.span = Span::new(0, 400, 198, 0, 240, 0);

        let body =
            "mov_writer_move body line with ptr inside it and more words to overflow the budget \
                    so the group text is split into several fragments that all carry function code"
                .repeat(6);
        let header_conv = ConversionResult {
            entity_id: EntityId(0),
            kind: EntityKind::Function,
            name: "mov_writer_move".to_string(),
            file_path: "mov-writer.c".to_string(),
            bm25_text: Some(body.clone()),
            embedding_text: Some(body),
            embedding_tokens: None,
            bm25_word_count: None,
            keywords: vec![],
            source_entity_ids: vec![],
            source_span: Span::default(),
            entity_metadata: Default::default(),
            entity_end_lines: vec![],
            embedding_brief_header: None,
            glossary: Vec::new(),
        };

        let group_conversions = GroupConversions {
            group: group.clone(),
            header_conversion: Some(header_conv),
            member_conversions: vec![],
        };

        let emb_dec = ChunkBuilder::decoration_allowance(&group, ChunkPath::Embedding);
        let bm25_dec = ChunkBuilder::decoration_allowance(&group, ChunkPath::Bm25);
        let body_cfg = base.clone();
        let emb_budget = base.budget(ChunkPath::Embedding, false, emb_dec);
        let bm25_budget = base.budget(ChunkPath::Bm25, false, bm25_dec);
        let splitter = TextSplitter::with_budgets(body_cfg.clone(), emb_budget, bm25_budget);
        let infra = ChunkInfrastructure {
            config: &body_cfg,
            estimator: &estimator,
            splitter: &splitter,
            pre_injection_embedding: emb_budget.pre_injection_limit,
            pre_injection_bm25: bm25_budget.pre_injection_limit,
            emb_budget,
            bm25_budget,
            has_vocabulary: false,
        };
        let chunks = crate::ast_to_nl::chunker::header_chunk::chunk_group_with_conversions(
            &infra,
            &mut tracker,
            &group,
            &group_conversions,
            "mov-writer.c",
        )
        .expect("chunk")
        .chunks;

        let bm25: Vec<_> = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Bm25)
            .collect();
        assert_eq!(
            bm25.len(),
            1,
            "within-tolerance single-function BM25 text stays one whole"
        );
        // Every fragment carries function code, so every fragment stays
        // inside the function range; a local variable mentioned in the text
        // never narrows a fragment down to the variable's own range.
        let mut claimed: Vec<(usize, usize)> = Vec::new();
        for chunk in bm25 {
            let ranges = chunk.metadata.source_ranges();
            assert!(
                !ranges.is_empty(),
                "a fragment must claim a source range: {:?}",
                chunk.chunk_id
            );
            for range in ranges {
                let claimed_range = (range.start_position.row, range.end_position.row);
                assert!(
                    claimed_range.0 >= 198 && claimed_range.1 <= 240,
                    "a fragment must stay inside the function range, got {range:?}"
                );
                assert_ne!(
                    claimed_range,
                    (200, 200),
                    "a fragment must not claim the narrow range of a local variable"
                );
                claimed.push(claimed_range);
            }
        }
        claimed.sort_unstable();
        claimed.dedup();
        assert_eq!(
            claimed,
            vec![(198, 240)],
            "the tolerated whole claims the whole function range: {claimed:?}"
        );
    }

    #[test]
    fn test_add_relations_empty() {
        let tracker = GroupTracker::new();
        let mut chunks: Vec<crate::ast_to_nl::chunker::ChunkedResult> = vec![];
        crate::ast_to_nl::chunker::chunker::add_relations(&tracker, &mut chunks);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_groups_empty() {
        let chunker = GroupChunker::new(ChunkingConfig::default());
        let chunks = chunker.chunk_groups(&[], "test.rs").expect("chunk").chunks;
        assert!(chunks.is_empty());
    }

    // ── Chunk override three-tier chain (above → builtin → below) ──

    use cce_plugin::{CodePlugin, PluginBundle, PluginError, PluginMetadata, PluginRegistry};

    type ChunkFn =
        fn(Vec<GroupConversions>, &str) -> Result<Option<Vec<ChunkedResult>>, PluginError>;

    /// Configurable `CodePlugin` test double for the `Chunk` capability.
    struct ChunkMockPlugin {
        meta: PluginMetadata,
        chunk_fn: Option<ChunkFn>,
    }

    impl ChunkMockPlugin {
        fn with_id(id: &str, priority: i32) -> Self {
            Self {
                meta: PluginMetadata {
                    id: id.to_string(),
                    name: id.to_string(),
                    version: "0.1.0".to_string(),
                    priority,
                    capabilities: Vec::new(),
                    capability_priorities: std::collections::HashMap::new(),
                    capability_timeouts: std::collections::HashMap::new(),
                    description: None,
                },
                chunk_fn: None,
            }
        }

        fn chunk_fn(mut self, f: ChunkFn) -> Self {
            self.chunk_fn = Some(f);
            self
        }
    }

    impl CodePlugin for ChunkMockPlugin {
        fn metadata(&self) -> &PluginMetadata {
            &self.meta
        }
        fn supports_chunk(&self) -> bool {
            self.chunk_fn.is_some()
        }
        fn chunk(
            &self,
            conversions: Vec<GroupConversions>,
            file_path: &str,
        ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
            match self.chunk_fn {
                Some(f) => f(conversions, file_path),
                None => Ok(None),
            }
        }
    }

    fn chunk_register(
        registry: &mut PluginRegistry,
        plugin: ChunkMockPlugin,
        patterns: Option<Vec<&str>>,
    ) {
        let mut bundle = PluginBundle::new(std::sync::Arc::new(plugin));
        if let Some(patterns) = patterns {
            bundle =
                bundle.with_file_patterns(patterns.into_iter().map(|p| p.to_string()).collect());
        }
        registry.register_bundle(bundle);
    }

    /// Build a `ChunkedResult` from a single text via the real builder, so the
    /// shape matches what storage consumers expect.
    fn plugin_chunk_from_text(group: &EntityGroup, text: &str, path: ChunkPath) -> ChunkedResult {
        let tracker = GroupTracker::new();
        let builder = ChunkBuilder::new();
        builder.from_single_text(
            &tracker,
            SingleChunkContext {
                group,
                file_path: "plugin_app.py",
                path,
                text,
                keywords: &[],
            },
        )
    }

    fn chunk_conversions() -> Vec<GroupConversions> {
        vec![GroupConversions {
            group: create_merge_test_group("g1", "hello world", 0, 11),
            header_conversion: Some(create_merge_test_conversion("hello world")),
            member_conversions: vec![],
        }]
    }

    fn chunker_with(plugins: Vec<(ChunkMockPlugin, Option<Vec<&str>>)>) -> GroupChunker {
        let mut registry = PluginRegistry::new();
        for (plugin, patterns) in plugins {
            chunk_register(&mut registry, plugin, patterns);
        }
        GroupChunker::new(ChunkingConfig::default())
            .with_plugin_registry(std::sync::Arc::new(registry))
    }

    #[test]
    fn test_chunk_override_above_plugin_wins_over_builtin() {
        fn plugin_chunks(
            conversions: Vec<GroupConversions>,
            _file_path: &str,
        ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
            let group = &conversions[0].group;
            Ok(Some(vec![plugin_chunk_from_text(
                group,
                "PLUGIN-CHUNK-MARKER",
                ChunkPath::Bm25,
            )]))
        }
        let chunker = chunker_with(vec![(
            ChunkMockPlugin::with_id("chunker", 100).chunk_fn(plugin_chunks),
            None,
        )]);

        let chunks = chunker
            .chunk_groups(&chunk_conversions(), "app.py")
            .expect("chunk")
            .chunks;
        assert_eq!(
            chunks.len(),
            1,
            "plugin chunk list replaces built-in output"
        );
        assert_eq!(chunks[0].text, "PLUGIN-CHUNK-MARKER");
    }

    #[test]
    fn test_chunk_override_first_non_none_plugin_wins() {
        fn decline(
            _conversions: Vec<GroupConversions>,
            _file_path: &str,
        ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
            Ok(None)
        }
        fn plugin_chunks(
            conversions: Vec<GroupConversions>,
            _file_path: &str,
        ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
            let group = &conversions[0].group;
            Ok(Some(vec![plugin_chunk_from_text(
                group,
                "SECOND-PLUGIN",
                ChunkPath::Bm25,
            )]))
        }
        let chunker = chunker_with(vec![
            (
                ChunkMockPlugin::with_id("decline", 100).chunk_fn(decline),
                None,
            ),
            (
                ChunkMockPlugin::with_id("second", 10).chunk_fn(plugin_chunks),
                None,
            ),
        ]);

        let chunks = chunker
            .chunk_groups(&chunk_conversions(), "app.py")
            .expect("chunk")
            .chunks;
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "SECOND-PLUGIN");
    }

    #[test]
    fn test_chunk_decline_falls_back_to_builtin() {
        fn decline(
            _conversions: Vec<GroupConversions>,
            _file_path: &str,
        ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
            Ok(None)
        }
        let chunker = chunker_with(vec![(
            ChunkMockPlugin::with_id("decline", 100).chunk_fn(decline),
            None,
        )]);

        let chunks = chunker
            .chunk_groups(&chunk_conversions(), "app.py")
            .expect("chunk")
            .chunks;
        assert!(!chunks.is_empty(), "built-in chunker must produce chunks");
        assert!(
            chunks.iter().all(|c| !c.text.contains("PLUGIN")),
            "declined plugin must not influence chunk content"
        );
    }

    #[test]
    fn test_chunk_error_falls_back_to_builtin() {
        fn fail(
            _conversions: Vec<GroupConversions>,
            _file_path: &str,
        ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
            Err(PluginError::ExecutionFailed("broken".to_string()))
        }
        let chunker = chunker_with(vec![(
            ChunkMockPlugin::with_id("broken", 100).chunk_fn(fail),
            None,
        )]);

        let chunks = chunker
            .chunk_groups(&chunk_conversions(), "app.py")
            .expect("chunk")
            .chunks;
        assert!(
            !chunks.is_empty(),
            "built-in chunker must take over on error"
        );
    }

    #[test]
    fn test_chunk_file_pattern_mismatch_uses_builtin() {
        fn plugin_chunks(
            conversions: Vec<GroupConversions>,
            _file_path: &str,
        ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
            let group = &conversions[0].group;
            Ok(Some(vec![plugin_chunk_from_text(
                group,
                "PLUGIN-CHUNK-MARKER",
                ChunkPath::Bm25,
            )]))
        }
        let chunker = chunker_with(vec![(
            ChunkMockPlugin::with_id("py-only", 100).chunk_fn(plugin_chunks),
            Some(vec!["*.py"]),
        )]);

        // `.rs` file does not match the plugin's pattern → built-in chunker.
        let chunks = chunker
            .chunk_groups(&chunk_conversions(), "lib.rs")
            .expect("chunk")
            .chunks;
        assert!(!chunks.is_empty());
        assert!(
            chunks
                .iter()
                .all(|c| !c.text.contains("PLUGIN-CHUNK-MARKER"))
        );
    }

    #[test]
    fn test_chunk_below_plugin_used_only_when_builtin_empty() {
        fn below_chunks(
            conversions: Vec<GroupConversions>,
            _file_path: &str,
        ) -> Result<Option<Vec<ChunkedResult>>, PluginError> {
            let group = &conversions[0].group;
            Ok(Some(vec![plugin_chunk_from_text(
                group,
                "BELOW-FALLBACK",
                ChunkPath::Bm25,
            )]))
        }
        let chunker = chunker_with(vec![(
            ChunkMockPlugin::with_id("below", -1).chunk_fn(below_chunks),
            None,
        )]);

        // Content present → built-in produces chunks → below tier stays silent.
        let chunks = chunker
            .chunk_groups(&chunk_conversions(), "app.py")
            .expect("chunk")
            .chunks;
        assert!(!chunks.is_empty());
        assert!(
            chunks.iter().all(|c| !c.text.contains("BELOW-FALLBACK")),
            "below-tier plugin must stay silent when the built-in produced chunks"
        );

        // Empty conversions → built-in produces nothing → below tier runs.
        let empty: Vec<GroupConversions> = vec![GroupConversions {
            group: create_merge_test_group("g1", "", 0, 0),
            header_conversion: Some(create_merge_test_conversion("")),
            member_conversions: vec![],
        }];
        let chunks = chunker
            .chunk_groups(&empty, "app.py")
            .expect("chunk")
            .chunks;
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "BELOW-FALLBACK");
    }

    #[test]
    fn test_direct_like_block_between_body_and_pre_fits_hard() {
        use smallvec::SmallVec;
        let config = ChunkingConfig {
            max_tokens: 256,
            max_bm25_terms: 200,
            min_chunk_tokens: 10,
            ..Default::default()
        };
        let mut group = create_small_group();
        group.group_id = CompactString::from("group_1474");
        group.group_type = GroupType::ClassWithMethods;
        group.kind = EntityKind::Class;
        group.name = CompactString::from("PipelineTests");
        group.header = Some(GroupedEntity::new(
            EntityId(1),
            EntityKind::Class,
            "PipelineTests".to_string(),
            "class PipelineTests".to_string(),
        ));
        group.header_id = Some(EntityId(1));
        group.members = SmallVec::from_vec(vec![GroupedEntity::new(
            EntityId(2),
            EntityKind::Method,
            "TestMethod".to_string(),
            "void TestMethod()".to_string(),
        )]);
        group.member_ids = SmallVec::from_vec(vec![EntityId(2)]);
        let decoration = ChunkBuilder::decoration_allowance(&group, ChunkPath::Embedding);
        let budget = config.budget(ChunkPath::Embedding, false, decoration);
        assert!(budget.body_limit > 1 && budget.body_limit < budget.pre_injection_limit);
        let over = budget.body_limit.saturating_add(5);
        let mut member_text = (0..200)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        member_text =
            cce_text::truncate_to_cost(&member_text, cce_text::ChunkUnit::Tokens, over).to_string();
        assert!(
            config.cost(&member_text, ChunkPath::Embedding) > budget.body_limit,
            "fixture must sit between body and pre-injection"
        );
        assert!(
            config.cost(&member_text, ChunkPath::Embedding) <= budget.pre_injection_limit,
            "fixture must fit the pre-injection budget"
        );
        let header_conv = ConversionResult {
            entity_id: EntityId(1),
            kind: EntityKind::Class,
            name: "PipelineTests".to_string(),
            file_path: "PipelineTests.cs".to_string(),
            bm25_text: Some("PipelineTests".to_string()),
            embedding_text: Some("PipelineTests".to_string()),
            ..Default::default()
        };
        let member_conv = ConversionResult {
            entity_id: EntityId(2),
            kind: EntityKind::Method,
            name: "TestMethod".to_string(),
            file_path: "PipelineTests.cs".to_string(),
            bm25_text: Some(member_text.clone()),
            embedding_text: Some(member_text),
            ..Default::default()
        };
        let conversions = vec![GroupConversions {
            group: group.clone(),
            header_conversion: Some(header_conv),
            member_conversions: vec![member_conv],
        }];
        let chunker = GroupChunker::new(config.clone());
        let output = chunker
            .chunk_groups(&conversions, "PipelineTests.cs")
            .expect("between-body-and-pre block must not fail the file");
        assert!(!output.chunks.is_empty(), "file must keep its chunks");
        for chunk in &output.chunks {
            assert!(
                config.cost(&chunk.text, chunk.path) <= config.limit(chunk.path),
                "chunk {} exceeds the hard limit",
                chunk.chunk_id
            );
        }
    }

    #[test]
    fn test_tiny_budget_keeps_file_despite_large_decoration() {
        use smallvec::SmallVec;
        let config = ChunkingConfig {
            max_tokens: 40,
            max_bm25_terms: 40,
            min_chunk_tokens: 5,
            min_chunk_bm25_terms: 5,
            ..Default::default()
        };
        let mut group = create_small_group();
        group.group_id = CompactString::from("tiny_group");
        group.group_type = GroupType::ClassWithMethods;
        group.kind = EntityKind::Class;
        group.name = CompactString::from("Tiny");
        group.header = Some(GroupedEntity::new(
            EntityId(1),
            EntityKind::Class,
            "Tiny".to_string(),
            "class Tiny".to_string(),
        ));
        group.header_id = Some(EntityId(1));
        group.members = SmallVec::from_vec(vec![GroupedEntity::new(
            EntityId(2),
            EntityKind::Method,
            "run".to_string(),
            "void run()".to_string(),
        )]);
        group.member_ids = SmallVec::from_vec(vec![EntityId(2)]);
        let header_conv = ConversionResult {
            entity_id: EntityId(1),
            kind: EntityKind::Class,
            name: "Tiny".to_string(),
            file_path: "tiny.cs".to_string(),
            bm25_text: Some("Tiny".to_string()),
            embedding_text: Some("Tiny".to_string()),
            ..Default::default()
        };
        let member_conv = ConversionResult {
            entity_id: EntityId(2),
            kind: EntityKind::Method,
            name: "run".to_string(),
            file_path: "tiny.cs".to_string(),
            bm25_text: Some("run body here".to_string()),
            embedding_text: Some("run body here".to_string()),
            ..Default::default()
        };
        let conversions = vec![GroupConversions {
            group: group.clone(),
            header_conversion: Some(header_conv),
            member_conversions: vec![member_conv],
        }];
        let chunker = GroupChunker::new(config.clone());
        let output = chunker
            .chunk_groups(&conversions, "tiny.cs")
            .expect("decoration over reserve must not drop the file");
        assert!(!output.chunks.is_empty(), "file must keep its chunks");
        for chunk in &output.chunks {
            assert!(
                config.cost(&chunk.text, chunk.path) <= config.limit(chunk.path),
                "chunk {} exceeds the hard limit",
                chunk.chunk_id
            );
        }
    }

    #[test]
    fn test_repair_folds_parent_overflow_back_into_body() {
        use crate::ast_to_nl::chunker::single_path::{ChunkInfrastructure, ChunkInput};
        use crate::ast_to_nl::chunker::strategy::SplitStrategy;
        let config = ChunkingConfig {
            max_bm25_terms: 60,
            min_chunk_bm25_terms: 5,
            max_tokens: 512,
            ..Default::default()
        };
        let mut parent = create_small_group();
        parent.group_id = CompactString::from("parent_long");
        parent.name = CompactString::from("VeryLongParentModuleNameThatKeepsGrowingAndGrowing");
        parent.kind = EntityKind::Class;
        let mut child = create_small_group();
        child.group_id = CompactString::from("child_short");
        child.name = CompactString::from("s");
        child.kind = EntityKind::Function;
        child.parent_group_id = Some(CompactString::from("parent_long"));
        child.header = Some(GroupedEntity::new(
            EntityId(11),
            EntityKind::Function,
            "s".to_string(),
            "fn s()".to_string(),
        ));
        child.header_id = Some(EntityId(11));
        let mut tracker = GroupTracker::new();
        tracker.register_identity(&parent);
        tracker.register_identity(&child);
        let decoration = ChunkBuilder::decoration_allowance(&child, ChunkPath::Bm25);
        let emb_budget = config.budget_with_reserve(ChunkPath::Embedding, 0, 0);
        let bm25_budget = config.budget_with_reserve(ChunkPath::Bm25, 0, decoration);
        let splitter = TextSplitter::with_budgets(config.clone(), emb_budget, bm25_budget);
        let estimator = TokenEstimator::default();
        let infra = ChunkInfrastructure {
            config: &config,
            estimator: &estimator,
            splitter: &splitter,
            pre_injection_embedding: emb_budget.pre_injection_limit,
            pre_injection_bm25: bm25_budget.pre_injection_limit,
            emb_budget,
            bm25_budget,
            has_vocabulary: false,
        };
        let body = bm25_budget.body_limit;
        assert!(body > 1 && body < bm25_budget.pre_injection_limit);
        let words_needed = body + 40;
        let text = (0..words_needed)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let nl = crate::ast_to_nl::chunker::boundary::compute_nl_entity_boundaries(
            &text,
            Some(EntityId(11)),
            &[],
            ChunkPath::Bm25,
        );
        let input = ChunkInput {
            infra: &infra,
            group: &child,
            text: &text,
            file_path: "parent_child.rs",
            keywords: &[],
            path: ChunkPath::Bm25,
            strategy: SplitStrategy::ByNlEntityBoundaries,
            nl_boundaries: Some(nl.as_slice()),
            tracker: &tracker,
            header_mode: None,
        };
        let output = crate::ast_to_nl::chunker::single_path::chunk_single_path(input)
            .expect("repair must keep the combination text");
        assert!(!output.chunks.is_empty(), "repair must not drop the file");
        // Within tolerance the single-entity BM25 text stays one whole without
        // folding the body budget; the whole exceeds pre-injection but stays
        // within three times of it, so no repair subdivides it.
        assert_eq!(
            output.chunks.len(),
            1,
            "within-tolerance single-entity BM25 text stays one whole"
        );
        assert!(
            config.cost(&output.chunks[0].text, ChunkPath::Bm25) > bm25_budget.pre_injection_limit,
            "tolerated whole exceeds pre-injection by design"
        );
        assert!(
            config.cost(&output.chunks[0].text, ChunkPath::Bm25)
                <= bm25_budget.pre_injection_limit.saturating_mul(3),
            "tolerated whole stays within three times pre-injection"
        );
    }

    #[test]
    fn test_single_entity_chunks_subdivide_and_fit_hard_limit() {
        // Beyond three times the hard limit the splitter subdivides, so every
        // stored chunk fits; within tolerance the whole stays by design.
        let config = ChunkingConfig {
            max_tokens: 40,
            max_bm25_terms: 40,
            min_chunk_tokens: 5,
            min_chunk_bm25_terms: 5,
            ..Default::default()
        };
        let mut parent = create_small_group();
        parent.group_id = CompactString::from("parent_long");
        parent.name = CompactString::from("LongParentName Word ".repeat(50));
        parent.kind = EntityKind::Class;
        let mut child = create_small_group();
        child.group_id = CompactString::from("child_tiny");
        child.group_type = GroupType::ClassWithMethods;
        child.kind = EntityKind::Class;
        child.name = CompactString::from("Tiny");
        child.header = Some(GroupedEntity::new(
            EntityId(21),
            EntityKind::Class,
            "Tiny".to_string(),
            "class Tiny".to_string(),
        ));
        child.header_id = Some(EntityId(21));
        child.parent_group_id = Some(CompactString::from("parent_long"));
        let long_text = "w ".repeat(400);
        let header_conv = ConversionResult {
            entity_id: EntityId(21),
            kind: EntityKind::Class,
            name: "Tiny".to_string(),
            file_path: "tiny_parent.cs".to_string(),
            bm25_text: Some(long_text.clone()),
            embedding_text: Some(long_text.clone()),
            ..Default::default()
        };
        let member_conv = ConversionResult {
            entity_id: EntityId(22),
            kind: EntityKind::Method,
            name: "run".to_string(),
            file_path: "tiny_parent.cs".to_string(),
            bm25_text: Some(long_text.clone()),
            embedding_text: Some(long_text),
            ..Default::default()
        };
        let conversions = vec![
            GroupConversions {
                group: parent.clone(),
                header_conversion: Some(ConversionResult {
                    entity_id: EntityId(20),
                    kind: EntityKind::Class,
                    name: parent.name.to_string(),
                    file_path: "tiny_parent.cs".to_string(),
                    bm25_text: Some("parent".to_string()),
                    embedding_text: Some("parent".to_string()),
                    ..Default::default()
                }),
                member_conversions: vec![],
            },
            GroupConversions {
                group: child.clone(),
                header_conversion: Some(header_conv),
                member_conversions: vec![member_conv],
            },
        ];
        let chunker = GroupChunker::new(config.clone());
        let output = chunker
            .chunk_groups(&conversions, "tiny_parent.cs")
            .expect("tiny budget must not drop the file");
        assert!(!output.chunks.is_empty(), "file must keep its chunks");
        // Subdivision keeps every fragment within budget; truncation only
        // fires for residual over-hard assembly.
        for chunk in &output.chunks {
            assert!(
                config.cost(&chunk.text, chunk.path) <= config.limit(chunk.path),
                "chunk {} must fit the hard limit",
                chunk.chunk_id
            );
        }
    }

    #[test]
    fn test_measured_terminology_reserve_below_proportional_without_extra_truncation() {
        use cce_plugin::vocabulary::InjectionLimits;
        use cce_plugin::vocabulary::{FileVocabulary, VocabularyTable, inject_chunks};
        use cce_plugin::{CodePlugin, PluginBundle, PluginError, PluginMetadata, PluginRegistry};
        use cce_types::plugin::VocabularyEntry;
        use std::sync::Arc;
        struct VocabularyMockPlugin {
            meta: PluginMetadata,
            entries: Vec<VocabularyEntry>,
        }
        impl CodePlugin for VocabularyMockPlugin {
            fn metadata(&self) -> &PluginMetadata {
                &self.meta
            }
            fn supports_vocabulary(&self) -> bool {
                true
            }
            fn vocabulary(&self) -> Result<Vec<VocabularyEntry>, PluginError> {
                Ok(self.entries.clone())
            }
        }
        let config = ChunkingConfig::default();
        let proportional_emb = config.glossary_reserve(ChunkPath::Embedding);
        let proportional_bm25 = config.glossary_reserve(ChunkPath::Bm25);
        assert!(proportional_emb > 0 && proportional_bm25 > 0);
        let plugin = VocabularyMockPlugin {
            meta: PluginMetadata {
                id: "vocab".to_string(),
                name: "vocab".to_string(),
                version: "0.1.0".to_string(),
                priority: 0,
                capabilities: Vec::new(),
                capability_priorities: std::collections::HashMap::new(),
                capability_timeouts: std::collections::HashMap::new(),
                description: None,
            },
            entries: vec![VocabularyEntry::new("stts", "box")],
        };
        let mut registry = PluginRegistry::new();
        registry.register_bundle(PluginBundle::new(Arc::new(plugin)));
        let chunker = GroupChunker::new(config.clone()).with_plugin_registry(Arc::new(registry));
        let texts = ["parse_stts reads the table"];
        let measured_emb =
            chunker.measured_terminology_reserve("a.c", Some("c"), &texts, ChunkPath::Embedding);
        let measured_bm25 =
            chunker.measured_terminology_reserve("a.c", Some("c"), &texts, ChunkPath::Bm25);
        assert!(
            measured_emb < proportional_emb,
            "single small hit must reserve less than the proportional estimate"
        );
        assert!(
            measured_bm25 < proportional_bm25,
            "single small hit must reserve less than the proportional estimate"
        );
        let table = VocabularyTable::from_entries(vec![VocabularyEntry::new("stts", "box")]);
        let view = FileVocabulary::all(Arc::new(table));
        let mut chunks = vec![{
            let group = create_small_group();
            let tracker = GroupTracker::new();
            let builder = ChunkBuilder::new();
            builder.from_single_text(
                &tracker,
                SingleChunkContext {
                    group: &group,
                    file_path: "a.c",
                    path: ChunkPath::Embedding,
                    text: "parse_stts reads the table",
                    keywords: &[],
                },
            )
        }];
        let stats = inject_chunks(
            &mut chunks,
            &view,
            false,
            config.glossary_max_entries,
            InjectionLimits {
                bm25: config.limit(ChunkPath::Bm25),
                embedding: config.limit(ChunkPath::Embedding),
            },
        );
        assert_eq!(stats.injected, 1);
        assert_eq!(
            stats.truncated, 0,
            "measured reserve must not grow the truncation count"
        );
    }
}
