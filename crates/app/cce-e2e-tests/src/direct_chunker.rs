use std::sync::Arc;

use cce_config::modules::ast_to_nl::ChunkingConfig;
use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult, GroupChunker};
use cce_parser::ast_to_nl::converter::GroupConversions;
use cce_parser::grouper::PreprocessingPipeline;
use cce_parser::parser::ParseCoordinator;
use cce_types::entity::EntityId;
use cce_types::grouper::EntityGroup;
use cce_types::{ConversionResult, ParsedFile};

pub struct EntityChunker {
    parser: Arc<std::sync::Mutex<ParseCoordinator>>,
    grouper: PreprocessingPipeline,
    chunker: std::sync::Mutex<GroupChunker>,
}

impl Default for EntityChunker {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityChunker {
    pub fn new() -> Self {
        let config = ChunkingConfig::default();
        Self {
            parser: Arc::new(std::sync::Mutex::new(ParseCoordinator::new())),
            grouper: PreprocessingPipeline::new(),
            chunker: std::sync::Mutex::new(GroupChunker::new(config)),
        }
    }

    pub fn chunk_file(&self, file_path: &str, content: &str) -> Vec<ChunkedResult> {
        let parsed: ParsedFile = match self.parser.lock().unwrap().parse(file_path, content) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("ParseCoordinator failed for {}: {}", file_path, e);
                return vec![];
            }
        };

        let result = self.grouper.process(&parsed);
        let conversions = build_group_conversions(&result.groups, file_path, content);

        let chunker = self.chunker.lock().unwrap();
        match chunker.chunk_groups(&conversions, file_path) {
            Ok(output) => output.chunks,
            Err(e) => {
                tracing::warn!("chunking failed for {}: {}", file_path, e);
                vec![]
            }
        }
    }

    pub fn chunk_files(&self, files: &[(String, String)]) -> Vec<ChunkedResult> {
        let mut all = Vec::new();
        for (file_path, content) in files {
            let chunks = self.chunk_file(file_path, content);
            all.extend(chunks);
        }
        all
    }
}

fn build_group_conversions(
    groups: &[EntityGroup],
    file_path: &str,
    file_source: &str,
) -> Vec<GroupConversions> {
    groups
        .iter()
        .filter_map(|group| {
            let member_conversions = group
                .members
                .iter()
                .filter_map(|member| {
                    let span = group.entity_spans.get(&member.id)?;
                    let source = file_source.get(span.start_byte..span.end_byte)?;
                    // Blank slices (empty span, comment-only region) would flow into
                    // chunks and get rejected by embedding providers with an opaque
                    // 400; skip them at the source.
                    if source.trim().is_empty() {
                        return None;
                    }
                    Some(raw_source_conversion(
                        member.id,
                        member.kind,
                        &member.name,
                        file_path,
                        source,
                    ))
                })
                .collect::<Vec<_>>();

            let header_conversion = if member_conversions.is_empty() {
                let entity_id = group
                    .header_id
                    .or_else(|| group.member_ids.first().copied())
                    .or_else(|| group.entity_spans.keys().next().copied())?;
                let source = file_source.get(group.span.start_byte..group.span.end_byte)?;
                if source.trim().is_empty() {
                    return None;
                }
                Some(raw_source_conversion(
                    entity_id,
                    group.kind,
                    group.name.as_str(),
                    file_path,
                    source,
                ))
            } else {
                // No group source available here; the identity-only conversion
                // carries the group name as text, which cannot be blank when
                // the group has members (guaranteed by this branch).
                let name = group.name.as_str();
                if name.trim().is_empty() {
                    return None;
                }
                Some(raw_source_conversion(
                    group.header_id.unwrap_or(EntityId(0)),
                    group.kind,
                    name,
                    file_path,
                    name,
                ))
            };

            Some(GroupConversions {
                group: group.clone(),
                header_conversion,
                member_conversions,
            })
        })
        .collect()
}

fn raw_source_conversion(
    entity_id: EntityId,
    kind: cce_types::entity::EntityKind,
    name: &str,
    file_path: &str,
    source: &str,
) -> ConversionResult {
    ConversionResult::new(
        entity_id,
        kind,
        name.to_string(),
        file_path.to_string(),
        source.to_string(),
        source.to_string(),
        Vec::new(),
    )
}

pub fn get_chunk_text(chunk: &ChunkedResult) -> &str {
    assert_eq!(chunk.path, ChunkPath::Embedding, "Expected embedding chunk");
    &chunk.text
}

pub fn get_chunk_entity_name(chunk: &ChunkedResult) -> &str {
    chunk.bm25_title.as_deref().unwrap_or("unknown")
}
