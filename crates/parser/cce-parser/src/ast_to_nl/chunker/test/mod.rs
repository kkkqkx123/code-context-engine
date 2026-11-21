// Chunking integration tests grouped by scenario.
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

use cce_plugin::{CodePlugin, PluginBundle, PluginError, PluginMetadata, PluginRegistry};

type ChunkFn = fn(Vec<GroupConversions>, &str) -> Result<Option<Vec<ChunkedResult>>, PluginError>;

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
        bundle = bundle.with_file_patterns(patterns.into_iter().map(|p| p.to_string()).collect());
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
    GroupChunker::new(ChunkingConfig::default()).with_plugin_registry(std::sync::Arc::new(registry))
}

mod chunking;
mod cross_group;
mod misc;
mod plugin;
