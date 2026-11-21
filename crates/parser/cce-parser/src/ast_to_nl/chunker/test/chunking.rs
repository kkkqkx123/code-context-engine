use super::*;

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
    let mut member_text = (0..600)
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
        bm25_text: Some("TestMethod body here".to_string()),
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
