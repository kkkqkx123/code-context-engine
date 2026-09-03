//! File aggregator for export
//!
//! This module provides functionality to aggregate chunks into file-level documents.

use std::collections::HashMap;

use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult};
use cce_parser::grouper::GroupType;
use cce_types::Span;
use cce_types::entity::EntityKind;
use cce_types::language::Language;
use regex::Regex;

use super::error::ExportError;
use super::summary_view::ExportSummaryView;

/// Related entity information (for relation enhancement)
#[derive(Debug, Clone)]
pub struct RelatedEntity {
    /// Entity name
    pub name: String,
    /// Relation type (for display)
    pub relation_type: String,
    /// File path (if cross-file)
    pub file_path: Option<String>,
    /// Location of the relation (call site or reference point)
    pub location: Option<Span>,
}

/// Entity-level natural language document
#[derive(Debug, Clone)]
pub struct EntityNlDocument {
    /// Entity name
    pub name: String,
    /// Entity kind
    pub kind: EntityKind,
    /// Entity modifiers (e.g., "pub", "static", "unsafe", "mut")
    pub modifiers: Vec<String>,
    /// Natural language description (from Embedding-path chunks)
    pub nl_description: String,
    /// Source code span
    pub span: Span,
    /// Group type
    pub group_type: GroupType,
    /// Related entities (optional, for relation enhancement)
    pub related_entities: Vec<RelatedEntity>,
}

impl EntityNlDocument {
    /// Create a new entity document
    pub fn new(
        name: String,
        kind: EntityKind,
        modifiers: Vec<String>,
        nl_description: String,
        span: Span,
        group_type: GroupType,
    ) -> Self {
        Self {
            name,
            kind,
            modifiers,
            nl_description,
            span,
            group_type,
            related_entities: Vec::new(),
        }
    }

    /// Add a related entity
    pub fn add_related(&mut self, related: RelatedEntity) {
        self.related_entities.push(related);
    }
}

/// File-level aggregated document
#[derive(Debug, Clone)]
pub struct FileNlDocument {
    /// Source file path
    pub source_path: String,
    /// Programming language
    pub language: Language,
    /// Export-oriented summary view (optional)
    pub summary: Option<ExportSummaryView>,
    /// Entity documents
    pub entities: Vec<EntityNlDocument>,
    /// Import list
    pub imports: Vec<String>,
    /// Export list
    pub exports: Vec<String>,
    /// Total token count
    pub total_tokens: usize,
}

impl FileNlDocument {
    /// Create a new file document
    pub fn new(source_path: String, language: Language) -> Self {
        Self {
            source_path,
            language,
            summary: None,
            entities: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            total_tokens: 0,
        }
    }

    /// Set summary
    pub fn with_summary(mut self, summary: ExportSummaryView) -> Self {
        self.summary = Some(summary);
        self
    }

    /// Add an entity
    pub fn add_entity(&mut self, entity: EntityNlDocument) {
        self.entities.push(entity);
    }

    /// Set imports
    pub fn set_imports(&mut self, imports: Vec<String>) {
        self.imports = imports;
    }

    /// Set exports
    pub fn set_exports(&mut self, exports: Vec<String>) {
        self.exports = exports;
    }

    /// Get entity count
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }
}

/// File aggregator
///
/// Aggregates chunks into file-level documents for export.
pub struct FileAggregator;

/// Category-based priority for entity deduplication.
///
/// Variants are ordered from lowest to highest priority.
/// Using derived `Ord` ensures type-safe comparison without arbitrary numeric values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EntityCategory {
    /// Noise entities (variables, parameters, unknown)
    Noise,
    /// Test entities (test suite, test case, test hook)
    Test,
    /// Module-level containers (module, namespace, package)
    ModuleLike,
    /// Data members (field, property, constant)
    DataMember,
    /// Callable entities (function, method, constructor, destructor, operator)
    Callable,
    /// Type definitions (class, struct, enum, interface, trait, type alias, union)
    TypeDef,
}

impl EntityCategory {
    fn from_kind(kind: EntityKind) -> Self {
        match kind {
            EntityKind::Class
            | EntityKind::Struct
            | EntityKind::Enum
            | EntityKind::Interface
            | EntityKind::Trait
            | EntityKind::TraitImpl
            | EntityKind::InherentImpl
            | EntityKind::TypeAlias
            | EntityKind::Union => Self::TypeDef,

            EntityKind::Function
            | EntityKind::Method
            | EntityKind::Constructor
            | EntityKind::Destructor
            | EntityKind::Operator => Self::Callable,

            EntityKind::Field | EntityKind::Property | EntityKind::Constant => Self::DataMember,

            EntityKind::Module | EntityKind::Namespace | EntityKind::Package => Self::ModuleLike,

            EntityKind::TestSuite | EntityKind::TestCase | EntityKind::TestHook => Self::Test,

            _ => Self::Noise,
        }
    }
}

impl FileAggregator {
    /// Create a new file aggregator
    pub fn new() -> Self {
        Self
    }

    /// Aggregate chunks into a file-level document
    ///
    /// # Arguments
    ///
    /// * `chunks` - Chunks to aggregate (should all be from the same file)
    /// * `summary` - Optional export summary view
    ///
    /// # Returns
    ///
    /// Aggregated file document
    pub fn aggregate(
        &self,
        chunks: &[ChunkedResult],
        summary: Option<ExportSummaryView>,
    ) -> Result<FileNlDocument, ExportError> {
        if chunks.is_empty() {
            return Err(ExportError::NoChunks);
        }

        // Extract file path from first chunk
        let file_path = chunks
            .first()
            .map(|c| c.metadata.file_path.clone())
            .unwrap_or_default();

        // Extract language from first chunk
        let language = chunks
            .first()
            .and_then(|c| c.metadata.language())
            .unwrap_or(Language::Unknown);

        // Group chunks by source_group_id
        let groups = self.group_by_source(chunks);

        // Build entity documents for each group
        let raw_entities: Vec<EntityNlDocument> = groups
            .into_iter()
            .map(|(group_id, group_chunks)| self.build_entity_doc(group_id, group_chunks))
            .collect();

        // Deduplicate entities by (start_row, name),
        // keeping the one with the highest-priority EntityKind
        let deduped = Self::deduplicate_entities(raw_entities);

        // Filter out entities that are fully contained within a higher-priority entity.
        // This ensures inner elements (local variables, parameters) that slipped through
        // are removed, while keeping structural top-level entities.
        let entities = Self::filter_contained_entities(deduped);

        // Calculate total tokens (only count Embedding-path chunks to avoid double-counting)
        let total_tokens: usize = chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .map(|c| c.token_count)
            .sum();

        // Build file document
        let mut doc = FileNlDocument::new(file_path, language);

        // Set summary if provided
        if let Some(view) = summary {
            doc.imports = view.imports.clone();
            doc.exports = view.exports.clone();
            doc.summary = Some(view);
        }

        doc.entities = entities;
        doc.total_tokens = total_tokens;

        Ok(doc)
    }

    /// Check if an entity is semantically significant enough to appear in the output.
    /// Filters out noise entities like generic type parameters, local variables, etc.
    fn is_entity_significant(entity: &EntityNlDocument) -> bool {
        // Always keep structural entities (type definitions, functions, fields, etc.)
        if matches!(
            entity.kind,
            EntityKind::Class
                | EntityKind::Struct
                | EntityKind::Enum
                | EntityKind::Interface
                | EntityKind::Trait
                | EntityKind::TraitImpl
                | EntityKind::InherentImpl
                | EntityKind::TypeAlias
                | EntityKind::Union
                | EntityKind::Function
                | EntityKind::Method
                | EntityKind::Constructor
                | EntityKind::Destructor
                | EntityKind::Operator
                | EntityKind::Module
                | EntityKind::Namespace
                | EntityKind::Package
                | EntityKind::Field
                | EntityKind::Property
                | EntityKind::Constant
                | EntityKind::TestSuite
                | EntityKind::TestCase
                | EntityKind::TestHook
        ) {
            return true;
        }

        // Filter out generic type parameters (single uppercase letters like T, F, E)
        if entity.kind == EntityKind::Variable
            && entity.name.len() == 1
            && entity.name.chars().all(|c| c.is_ascii_uppercase())
        {
            return false;
        }

        // Everything else (multi-char Variable, Unknown, etc.) is noise
        false
    }

    /// Deduplicate entities by precise span position, keeping the highest-priority kind.
    ///
    /// Uses complete span information (start_byte, end_byte, column, name) to accurately
    /// identify unique entities. This prevents same-line, same-name entities from incorrectly
    /// deduplicating each other (e.g., definition vs. reference on the same line).
    ///
    /// Also performs container-based deduplication: if multiple container-type entities
    /// (InherentImpl, TraitImpl) share the same name and kind, only the one with the largest
    /// span is kept (representing the full container definition).
    ///
    /// Also filters out insignificant entities.
    fn deduplicate_entities(entities: Vec<EntityNlDocument>) -> Vec<EntityNlDocument> {
        // First pass: exact deduplication by (start_byte, end_byte, column, name)
        let mut seen: HashMap<(usize, usize, u32, String), EntityNlDocument> = HashMap::new();

        for entity in entities {
            // Skip insignificant entities first
            if !Self::is_entity_significant(&entity) {
                continue;
            }

            // Use precise byte range and column position instead of just row
            let key = (
                entity.span.start_byte,
                entity.span.end_byte,
                entity.span.start_position.column as u32,
                entity.name.clone(),
            );

            match seen.get(&key) {
                Some(existing) => {
                    // Keep the entity with higher category priority
                    // (definitions take priority over references, etc.)
                    if EntityCategory::from_kind(entity.kind)
                        > EntityCategory::from_kind(existing.kind)
                    {
                        seen.insert(key, entity);
                    }
                }
                None => {
                    seen.insert(key, entity);
                }
            }
        }

        let mut result: Vec<EntityNlDocument> = seen.into_values().collect();

        // Second pass: container-based deduplication
        // Merge container entities (InherentImpl, TraitImpl) with same name and kind
        // Keep the one with the largest span (typically the header that covers all methods)
        result = Self::deduplicate_containers(result);

        // Preserve a deterministic order by sorting by span position and name.
        result.sort_by(|a, b| {
            a.span
                .start_position
                .row
                .cmp(&b.span.start_position.row)
                .then_with(|| {
                    a.span
                        .start_position
                        .column
                        .cmp(&b.span.start_position.column)
                })
                .then_with(|| a.span.start_byte.cmp(&b.span.start_byte))
                .then_with(|| a.span.end_byte.cmp(&b.span.end_byte))
                .then_with(|| {
                    EntityCategory::from_kind(a.kind).cmp(&EntityCategory::from_kind(b.kind))
                })
                .then_with(|| a.kind.to_string().cmp(&b.kind.to_string()))
                .then_with(|| a.name.cmp(&b.name))
        });
        result
    }

    /// Deduplicate container entities (InherentImpl, TraitImpl).
    ///
    /// When the same impl block appears in multiple groups (different source_group_ids),
    /// multiple EntityNlDocuments may be generated. This method identifies such duplicates by:
    /// 1. Exact match: same (kind, nl_description)
    /// 2. Prefix match: same kind, same description prefix (first sentence)
    ///
    /// And keeps only the largest span.
    fn deduplicate_containers(entities: Vec<EntityNlDocument>) -> Vec<EntityNlDocument> {
        let containers: Vec<EntityNlDocument> = entities
            .iter()
            .filter(|e| matches!(e.kind, EntityKind::InherentImpl | EntityKind::TraitImpl))
            .cloned()
            .collect();

        let non_containers: Vec<EntityNlDocument> = entities
            .iter()
            .filter(|e| !matches!(e.kind, EntityKind::InherentImpl | EntityKind::TraitImpl))
            .cloned()
            .collect();

        if containers.is_empty() {
            return entities;
        }

        // Group containers: (kind, description_prefix) is the dedup key
        // Prefix = first 100 chars of nl_description or up to first period
        let mut groups: HashMap<(EntityKind, String), Vec<EntityNlDocument>> = HashMap::new();

        for container in containers {
            // Extract prefix: take first 100 chars or up to first sentence
            let desc_prefix = if let Some(period_idx) = container.nl_description.find('.') {
                container.nl_description[..period_idx.min(100)].to_string()
            } else {
                container.nl_description[..container.nl_description.len().min(100)].to_string()
            };

            let key = (container.kind, desc_prefix);
            groups.entry(key).or_default().push(container);
        }

        // For each group, keep only the largest span
        let mut deduplicated = Vec::new();
        for (_, mut group) in groups {
            if group.len() == 1 {
                deduplicated.push(
                    group
                        .into_iter()
                        .next()
                        .expect("group should have exactly one element"),
                );
            } else {
                // Sort by span size descending
                group.sort_by(|a, b| {
                    let a_size = (a.span.end_byte as i64) - (a.span.start_byte as i64);
                    let b_size = (b.span.end_byte as i64) - (b.span.start_byte as i64);
                    b_size.cmp(&a_size)
                });
                deduplicated.push(
                    group
                        .into_iter()
                        .next()
                        .expect("group should have at least one element"),
                );
            }
        }

        // Rebuild result preserving order: non-containers first, then deduplicated containers
        let mut result = non_containers;
        result.extend(deduplicated);
        result
    }

    /// Filter out entities that are fully contained within a higher-priority entity.
    ///
    /// This prevents inner elements (local variables, parameters, etc.) from appearing
    /// as top-level entities when they are semantically part of a parent entity.
    ///
    /// The containment rule:
    /// - A non-structural entity (Variable, Parameter, etc.) is filtered out if its span
    ///   is fully contained within any structural entity's span.
    /// - Structural entities (Class, Function, Struct, Field, etc.) are always kept.
    fn filter_contained_entities(entities: Vec<EntityNlDocument>) -> Vec<EntityNlDocument> {
        // Entities with these kinds should NOT be filtered out by containment,
        // even if they are inside another entity (e.g., fields inside a struct).
        const STRUCTURAL_KINDS: [EntityKind; 20] = [
            EntityKind::Class,
            EntityKind::Struct,
            EntityKind::Enum,
            EntityKind::Interface,
            EntityKind::Trait,
            EntityKind::TraitImpl,
            EntityKind::InherentImpl,
            EntityKind::TypeAlias,
            EntityKind::Union,
            EntityKind::Function,
            EntityKind::Method,
            EntityKind::Constructor,
            EntityKind::Destructor,
            EntityKind::Operator,
            EntityKind::Module,
            EntityKind::Namespace,
            EntityKind::Package,
            EntityKind::Field,
            EntityKind::Property,
            EntityKind::Constant,
        ];

        // Collect all structural entities and their spans as containment boundaries
        let structural_spans: Vec<(u32, u32)> = entities
            .iter()
            .filter(|e| STRUCTURAL_KINDS.contains(&e.kind))
            .map(|e| {
                (
                    e.span.start_position.row as u32,
                    e.span.end_position.row as u32,
                )
            })
            .collect();

        entities
            .into_iter()
            .filter(|entity| {
                // Always keep structural entities regardless of containment
                if STRUCTURAL_KINDS.contains(&entity.kind) {
                    return true;
                }

                // For non-structural entities, check if contained within any structural entity
                let entity_start = entity.span.start_position.row as u32;
                let entity_end = entity.span.end_position.row as u32;
                let _entity_name = &entity.name;

                let is_contained = structural_spans.iter().any(|(parent_start, parent_end)| {
                    // Entity must be strictly within the parent span (same start row is OK
                    // if name differs, but we exclude self-containment)
                    entity_start >= *parent_start && entity_end <= *parent_end
                        // Must NOT be the same entity (same name + same start row)
                        && !(entity_start == *parent_start && entity_end == *parent_end)
                });

                !is_contained
            })
            .collect()
    }

    /// Group chunks by their source group ID
    fn group_by_source<'a>(
        &self,
        chunks: &'a [ChunkedResult],
    ) -> Vec<(String, Vec<&'a ChunkedResult>)> {
        // First pass: collect all Embedding-path chunks and their group mapping
        let mut group_map: HashMap<String, Vec<&ChunkedResult>> = HashMap::new();

        for chunk in chunks {
            let group_id = &chunk.source_group_id;
            group_map.entry(group_id.clone()).or_default().push(chunk);
        }

        // Convert to Vec and sort by the earliest occurrence
        let mut groups: Vec<(String, Vec<&ChunkedResult>)> = group_map.into_iter().collect();
        for (_, chunks) in &mut groups {
            chunks.sort_by(|a, b| {
                a.metadata
                    .source_span
                    .start_position
                    .row
                    .cmp(&b.metadata.source_span.start_position.row)
                    .then_with(|| {
                        a.metadata
                            .source_span
                            .start_position
                            .column
                            .cmp(&b.metadata.source_span.start_position.column)
                    })
                    .then_with(|| {
                        a.metadata
                            .source_span
                            .start_byte
                            .cmp(&b.metadata.source_span.start_byte)
                    })
                    .then_with(|| {
                        a.metadata
                            .source_span
                            .end_byte
                            .cmp(&b.metadata.source_span.end_byte)
                    })
                    .then_with(|| a.path.as_str().cmp(b.path.as_str()))
                    .then_with(|| a.chunk_id.cmp(&b.chunk_id))
            });
        }
        groups.sort_by(|a, b| {
            let a_order =
                a.1.iter()
                    .map(|c| c.metadata.source_span.start_position.row)
                    .min();
            let a_col =
                a.1.iter()
                    .map(|c| c.metadata.source_span.start_position.column)
                    .min();
            let b_order =
                b.1.iter()
                    .map(|c| c.metadata.source_span.start_position.row)
                    .min();
            let b_col =
                b.1.iter()
                    .map(|c| c.metadata.source_span.start_position.column)
                    .min();
            a_order
                .cmp(&b_order)
                .then_with(|| a_col.cmp(&b_col))
                .then_with(|| a.0.cmp(&b.0))
        });

        groups
    }

    /// Build an entity document from a group of chunks
    fn build_entity_doc(&self, _group_id: String, chunks: Vec<&ChunkedResult>) -> EntityNlDocument {
        let mut chunks = chunks;
        chunks.sort_by(|a, b| {
            a.metadata
                .source_span
                .start_position
                .row
                .cmp(&b.metadata.source_span.start_position.row)
                .then_with(|| {
                    a.metadata
                        .source_span
                        .start_position
                        .column
                        .cmp(&b.metadata.source_span.start_position.column)
                })
                .then_with(|| {
                    a.metadata
                        .source_span
                        .start_byte
                        .cmp(&b.metadata.source_span.start_byte)
                })
                .then_with(|| {
                    a.metadata
                        .source_span
                        .end_byte
                        .cmp(&b.metadata.source_span.end_byte)
                })
                .then_with(|| a.path.as_str().cmp(b.path.as_str()))
                .then_with(|| a.chunk_id.cmp(&b.chunk_id))
        });

        // Use the first chunk for basic info
        let first = chunks[0];

        // Deduplicate chunks by source span to prevent duplicate descriptions
        // when the same impl block is chunked multiple times.
        let mut seen_spans: std::collections::HashSet<(usize, usize)> =
            std::collections::HashSet::new();
        let mut unique_chunks = Vec::new();
        for chunk in chunks {
            let span_key = (
                chunk.metadata.source_span.start_byte,
                chunk.metadata.source_span.end_byte,
            );
            if seen_spans.insert(span_key) {
                unique_chunks.push(chunk);
            }
        }

        // Collect all Embedding-path descriptions (pure semantic text for export).
        // BM25 text is NEVER used for export - it is keyword-optimized for search indexing only.
        // Export needs only the NL text, not vector storage; text generation is always available.
        let nl_description = unique_chunks
            .iter()
            .filter(|c| c.path == ChunkPath::Embedding)
            .map(|c| c.text.as_str())
            .filter(|t| !t.is_empty())
            .map(Self::clean_placeholder_and_temp_vars)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");

        // Extract entity name from chunk metadata
        let name = Self::extract_name_from_chunk(first);
        let kind = Self::extract_kind_from_chunk(first);
        let modifiers = Self::extract_modifiers_from_chunk(first);

        EntityNlDocument::new(
            name,
            kind,
            modifiers,
            nl_description,
            first.metadata.source_span,
            first.group_type,
        )
    }

    /// Extract entity name from chunk metadata with proper fallback
    ///
    /// Ensures the name is never empty by trying multiple sources and logging warnings.
    /// Prevents Silent Failure of empty entity names.
    fn extract_name_from_chunk(chunk: &ChunkedResult) -> String {
        // First choice: bm25_title (primary source)
        if let Some(name) = &chunk.bm25_title {
            if !name.is_empty() {
                return name.clone();
            }
        }

        // Second choice: entity kind from metadata
        if let Some(meta) = &chunk.metadata.code_metadata {
            let kind_name = format!("{:?}", meta.entity_kind);
            if !kind_name.is_empty() {
                tracing::warn!(
                    chunk_id = %chunk.chunk_id,
                    entity_kind = %kind_name,
                    "Entity name missing, using entity kind as fallback"
                );
                return kind_name;
            }
        }

        // Last resort: use chunk ID as placeholder
        tracing::error!(
            chunk_id = %chunk.chunk_id,
            "Entity name completely missing, using chunk ID"
        );
        format!("entity_{}", chunk.chunk_id)
    }

    fn clean_placeholder_and_temp_vars(text: &str) -> String {
        static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(r"\b(?:placeholder|temporary variable|temp variable)\b\s*").unwrap()
        });
        let result = re.replace_all(text, "");
        result.trim().to_string()
    }

    /// Extract entity kind from chunk metadata
    fn extract_kind_from_chunk(chunk: &ChunkedResult) -> EntityKind {
        chunk
            .metadata
            .code_metadata
            .as_ref()
            .map(|m| m.entity_kind)
            .unwrap_or_default()
    }

    /// Extract entity modifiers from chunk metadata
    fn extract_modifiers_from_chunk(chunk: &ChunkedResult) -> Vec<String> {
        chunk
            .metadata
            .code_metadata
            .as_ref()
            .map(|m| m.modifiers.clone())
            .unwrap_or_default()
    }
}

impl Default for FileAggregator {
    fn default() -> Self {
        Self::new()
    }
}
