//! Chunking result types (cross-layer contract)
//!
//! Moved from `cce_parser::ast_to_nl::chunker::result` so the plugin chunk
//! contract (`cce_core::plugin::CodePlugin::chunk`) can reference it without
//! depending on the parser crate. The parser crate re-exports these types
//! from its original module path.

use serde::{Deserialize, Serialize};

use crate::types::Span;
use crate::types::entity::{EntityId, EntityKind};
use crate::types::grouper::GroupType;
use crate::types::language::Language;

use super::file_category::FileCategory;
use super::split_reason::SplitReason;

/// Content path discriminator for chunks.
///
/// Defined in `cce_core` so config-level logic (e.g. `ChunkingConfig`
/// limit checks) can reference it without depending on the parser crate.
pub use crate::types::ChunkPath;

/// Content type identifier for chunks (chunk-payload layer).
///
/// This type describes the payload shape of one chunk (code language /
/// document / config format / plain text), while the business-layer
/// [`FileCategory`](super::file_category::FileCategory) carried next to it in
/// [`ChunkMetadata`] is the single filter key for storage and queries. The
/// category is always derivable from this type (see
/// [`ChunkContentType::file_category`]), except for schema files whose chunks
/// use the `Document` payload with `FileCategory::Schema`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkContentType {
    /// Source code file
    Code { language: Language },
    /// Document file (Markdown, XML, etc.) - unified document type
    Document,
    /// Configuration file (JSON, YAML, TOML, etc.)
    Config { format: String },
    /// Plain text file
    PlainText,
}

impl ChunkContentType {
    /// Check if this is a code chunk
    pub fn is_code(&self) -> bool {
        matches!(self, ChunkContentType::Code { .. })
    }

    /// Check if this is a document chunk
    pub fn is_document(&self) -> bool {
        matches!(self, ChunkContentType::Document)
    }

    /// Check if this is a config chunk
    pub fn is_config(&self) -> bool {
        matches!(self, ChunkContentType::Config { .. })
    }

    /// Get language if this is a code chunk
    pub fn language(&self) -> Option<Language> {
        match self {
            ChunkContentType::Code { language } => Some(*language),
            _ => None,
        }
    }

    /// Business-layer category carried by this chunk payload type.
    ///
    /// Single source of truth for the payload → category direction;
    /// `ChunkMetadata` derives its `file_category` from here so both stored
    /// labels can never disagree. Schema chunks are the one payload that
    /// carries an explicit category override (`Document` + `Schema`, built
    /// via [`ChunkMetadata::for_schema`]).
    pub fn file_category(&self) -> FileCategory {
        match self {
            ChunkContentType::Code { .. } => FileCategory::Code,
            ChunkContentType::Document => FileCategory::Documentation,
            ChunkContentType::Config { .. } => FileCategory::Config,
            // Generic text (logs, `.txt`, unknown extensions) is never
            // reported as code.
            ChunkContentType::PlainText => FileCategory::Other,
        }
    }

    /// Derive the document-pipeline payload type from a business category.
    ///
    /// Mapping: `Documentation | Schema` → `Document`, `Config` → `Config`,
    /// `Other` → `PlainText`. AST code chunks never go through this
    /// constructor; they carry their language explicitly via
    /// [`ChunkMetadata::for_code`]. `format` is consumed only by the
    /// `Config` variant.
    pub fn from_file_category(category: FileCategory, format: String) -> Self {
        match category {
            FileCategory::Schema | FileCategory::Documentation => Self::Document,
            FileCategory::Config => Self::Config { format },
            FileCategory::Other => Self::PlainText,
            FileCategory::Code => {
                debug_assert!(
                    false,
                    "code categories must not derive a payload type; \
                     build code chunks through ChunkMetadata::for_code"
                );
                Self::PlainText
            }
        }
    }

    /// Whether this payload type is consistent with the given category.
    ///
    /// The `(Document, Schema)` pair is legal: schema files reuse the
    /// document payload while keeping the dedicated `Schema` category.
    /// `(Code, Code)` covers AST code payloads; generic text payloads always
    /// pair with [`FileCategory::Other`].
    pub fn matches_category(&self, category: FileCategory) -> bool {
        matches!(
            (self, category),
            (ChunkContentType::Code { .. }, FileCategory::Code)
                | (ChunkContentType::Document, FileCategory::Documentation)
                | (ChunkContentType::Document, FileCategory::Schema)
                | (ChunkContentType::Config { .. }, FileCategory::Config)
                | (ChunkContentType::PlainText, FileCategory::Other)
        )
    }
}

impl std::fmt::Display for ChunkContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunkContentType::Code { language } => write!(f, "code({:?})", language),
            ChunkContentType::Document => write!(f, "document"),
            ChunkContentType::Config { format } => write!(f, "config({})", format),
            ChunkContentType::PlainText => write!(f, "plaintext"),
        }
    }
}

/// Code-specific metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodeSpecificMetadata {
    /// Entities whose source is represented by this chunk's body.
    ///
    /// This is the authoritative entity set for source coverage, raw-code
    /// extraction, result deduplication, and evaluation. It deliberately
    /// excludes repeated module, type, and impl headers.
    #[serde(default)]
    pub content_entity_ids: Vec<EntityId>,
    /// Display names positionally aligned with `content_entity_ids`.
    ///
    /// Populated at chunk build time from the group header/members so the
    /// storage layer can name each covered entity without re-reading the
    /// group. May be shorter or empty on legacy/plugin chunks; consumers must
    /// treat a missing entry as "name unknown".
    #[serde(default)]
    pub content_entity_names: Vec<String>,
    /// Entities repeated only to make the chunk understandable.
    ///
    /// Context must never expand source coverage or make two member chunks
    /// appear to represent the same source body.
    #[serde(default)]
    pub context_entity_ids: Vec<EntityId>,
    /// Entity kind
    pub entity_kind: EntityKind,
    /// Entity modifiers (e.g., "pub", "static", "unsafe", "mut")
    #[serde(default)]
    pub modifiers: Vec<String>,
    /// Split reason
    pub split_reason: SplitReason,
    /// Entity IDs in overlap region (for deduplication tracking)
    #[serde(default)]
    pub overlap_entities: Vec<EntityId>,
    /// Whether this chunk contains overlap content
    #[serde(default)]
    pub has_overlap: bool,
    /// Whether this chunk is a fragment of a large entity
    #[serde(default)]
    pub is_fragment: bool,
    /// Fragment index (0-based) if this is a fragment
    #[serde(default)]
    pub fragment_index: Option<usize>,
    /// Total fragments if this is a fragment
    #[serde(default)]
    pub total_fragments: Option<usize>,
    /// Original entity ID before fragmentation (if this is a fragment)
    #[serde(default)]
    pub original_entity_id: Option<EntityId>,
    /// Serialized pattern detection information (JSON) from EntityGroup
    #[serde(default)]
    pub pattern_info: Option<String>,
}

/// How accurately a chunk's source ranges describe its text.
///
/// A range is useful only when its precision is explicit. In particular, a
/// fragment produced by a hard text limit can identify its enclosing entity,
/// but cannot honestly claim an exact sub-entity source range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SourceSpanKind {
    /// One or more complete source entities are represented by the chunk.
    ExactEntities,
    /// The chunk is a hard-limit fragment within one known entity.
    EnclosingEntity,
    /// The splitter could not recover entity ownership and used the group span.
    GroupFallback,
    /// The range belongs to source documentation rather than a code entity.
    DocumentRange,
    /// No trustworthy source range is available.
    #[default]
    Unavailable,
}

impl std::fmt::Display for SourceSpanKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceSpanKind::ExactEntities => write!(f, "exact_entities"),
            SourceSpanKind::EnclosingEntity => write!(f, "enclosing_entity"),
            SourceSpanKind::GroupFallback => write!(f, "group_fallback"),
            SourceSpanKind::DocumentRange => write!(f, "document_range"),
            SourceSpanKind::Unavailable => write!(f, "unavailable"),
        }
    }
}

/// Document-specific metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentSpecificMetadata {
    /// Document structure info (e.g., heading level for markdown)
    #[serde(default)]
    pub doc_structure: Option<String>,
    /// Document node IDs included in this chunk
    #[serde(default)]
    pub doc_node_ids: Vec<String>,
}

/// Chunked result unit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkedResult {
    /// Chunk ID (unique identifier, format: {source_group_id}_{path}_{index})
    pub chunk_id: String,
    /// Source group ID (same across both BM25 and Embedding chunks for the same group)
    pub source_group_id: String,
    /// Chunk path discriminator (Bm25 or Embedding)
    pub path: ChunkPath,
    /// Group type
    pub group_type: GroupType,
    /// Chunk index within group (per-path)
    pub chunk_index: usize,
    /// Total chunks in group (per-path)
    pub total_chunks: usize,

    /// Text content (BM25 NL description or Embedding NL description depending on path)
    pub text: String,

    /// Title for BM25 high-weight field (entity name, derived from EntityGroup.name)
    #[serde(default)]
    pub bm25_title: Option<String>,
    /// Keywords for BM25 keyword field (aggregated from ConversionResult.keywords)
    #[serde(default)]
    pub bm25_keywords: Vec<String>,

    /// Token count (estimated tokens for Embedding path, word count for BM25 path)
    pub token_count: usize,
    /// Byte range
    pub start_byte: usize,
    pub end_byte: usize,

    /// Previous overlap region
    #[serde(default)]
    pub prev_overlap: Option<OverlapRegion>,
    /// Next overlap region
    #[serde(default)]
    pub next_overlap: Option<OverlapRegion>,

    /// Related groups for cross-group tracking
    #[serde(default)]
    pub related_groups: Vec<GroupRelation>,

    /// Whether this chunk represents a member (or members) that carry their
    /// own docstring/behavior description (Embedding path only).
    ///
    /// Self-contained member chunks are exempt from cross-group merging:
    /// their topic stays pure instead of being diluted by adjacent members.
    /// Always `false` on the BM25 path.
    #[serde(default)]
    pub self_contained: bool,

    /// Metadata
    pub metadata: ChunkMetadata,
}

/// Overlap region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlapRegion {
    /// Overlap text content
    pub text: String,
    /// Token count in overlap
    pub token_count: usize,
    /// Source chunk ID
    pub source_chunk_id: String,
    /// Overlap type
    pub overlap_type: OverlapType,
    /// Start byte position in source chunk
    pub start_byte: usize,
    /// End byte position in source chunk
    pub end_byte: usize,
}

/// Overlap type
///
/// Note: Current implementation uses single-direction overlap (Previous only)
/// to avoid content duplication between adjacent chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlapType {
    /// Content from previous chunk's end (primary overlap type)
    /// Each chunk stores overlap from the end of its predecessor
    Previous,
    /// Content from next chunk's start (reserved, not currently used)
    /// Kept for backward compatibility and potential future use
    Next,
}

/// Cross-group relation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupRelation {
    /// Related group ID
    pub group_id: String,
    /// Relation type
    pub relation_type: GroupRelationType,
    /// Relation strength (0.0 - 1.0)
    pub strength: f32,
}

/// Group relation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupRelationType {
    /// Predecessor group in file
    Predecessor,
    /// Successor group in file
    Successor,
    /// Caller relationship
    Caller,
    /// Callee relationship
    Callee,
    /// Same hierarchy
    SameHierarchy,
}

impl std::fmt::Display for GroupRelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupRelationType::Predecessor => write!(f, "predecessor"),
            GroupRelationType::Successor => write!(f, "successor"),
            GroupRelationType::Caller => write!(f, "caller"),
            GroupRelationType::Callee => write!(f, "callee"),
            GroupRelationType::SameHierarchy => write!(f, "same_hierarchy"),
        }
    }
}

/// Chunk metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    // === Content type identifier ===
    /// Chunk payload type. `file_category` below is derived from it (see
    /// [`ChunkContentType::file_category`]); schema chunks are the only
    /// override (`Document` payload + `FileCategory::Schema`).
    pub content_type: ChunkContentType,

    // === Common fields (all chunk types) ===
    /// File path
    pub file_path: String,
    /// Source code/document span
    pub source_span: Span,
    /// Exact source coverage. `source_span` is retained as an enclosing
    /// navigation span for callers that can display only one range.
    #[serde(default)]
    pub source_ranges: Vec<Span>,
    /// Precision of `source_ranges`.
    pub source_span_kind: SourceSpanKind,
    /// Word count for BM25 length normalization (actual words, not tokens)
    #[serde(default)]
    pub bm25_word_count: Option<usize>,
    /// Alignment key for hybrid fusion. Always populated.
    /// For code chunks: equals source_group_id (e.g. "group_9").
    /// For document chunks: source_group_id (e.g. "src_services_user_service_rs_0_group_1").
    /// Two chunks from the same logical segment share the same segment_id,
    /// enabling BM25 ↔ vector alignment even when entity_id is absent.
    pub segment_id: String,
    /// Group IDs merged into this chunk by the cross-group merge pass.
    /// The chunk's `source_group_id` keeps the first group; every additional
    /// group contributing content is recorded here so relation graphs and
    /// source attribution retain the merged groups.
    #[serde(default)]
    pub merged_group_ids: Vec<String>,
    /// Test-code marker inherited from the source group (AST detection +
    /// file-path rules). For document/config/plain-text chunks it comes from
    /// the file-path rule applied at the document pipeline entry.
    pub test_info: crate::types::TestInfo,
    /// File-level content category, computed at parse time. Code chunks are
    /// annotated by the orchestrator after chunking; document/config chunks
    /// are set at the document pipeline entry. Orthogonal to `test_info`.
    pub file_category: FileCategory,

    // === Conditional metadata (based on content_type) ===
    /// Code-specific metadata (only for code chunks)
    #[serde(default)]
    pub code_metadata: Option<CodeSpecificMetadata>,
    /// Document-specific metadata (only for document/config chunks)
    #[serde(default)]
    pub doc_metadata: Option<DocumentSpecificMetadata>,
}

impl ChunkMetadata {
    // === Content type helpers ===

    /// Check if this is a code chunk
    pub fn is_code(&self) -> bool {
        self.content_type.is_code()
    }

    /// Check if this is a document chunk
    pub fn is_document(&self) -> bool {
        self.content_type.is_document()
    }

    /// Check if this is a config chunk
    pub fn is_config(&self) -> bool {
        self.content_type.is_config()
    }

    /// Get language if this is a code chunk
    pub fn language(&self) -> Option<Language> {
        self.content_type.language()
    }

    // === Code metadata accessors ===

    /// Get code-specific metadata
    pub fn as_code(&self) -> Option<&CodeSpecificMetadata> {
        self.code_metadata.as_ref()
    }

    /// Get mutable reference to code-specific metadata
    pub fn as_code_mut(&mut self) -> Option<&mut CodeSpecificMetadata> {
        self.code_metadata.as_mut()
    }

    /// Get source-body entity IDs (only for code chunks).
    pub fn content_entity_ids(&self) -> &[EntityId] {
        self.code_metadata
            .as_ref()
            .map(|m| m.content_entity_ids.as_slice())
            .unwrap_or(&[])
    }

    /// Get context-only entity IDs (only for code chunks).
    pub fn context_entity_ids(&self) -> &[EntityId] {
        self.code_metadata
            .as_ref()
            .map(|m| m.context_entity_ids.as_slice())
            .unwrap_or(&[])
    }

    /// Return precise source coverage.
    ///
    /// Returns `source_ranges` when available. Falls back to `source_span` only
    /// when the span kind indicates reliable entity boundaries (`ExactEntities`,
    /// `EnclosingEntity`, or `DocumentRange`). Returns an empty slice when
    /// source ranges are unavailable or unreliable (`Unavailable`, `GroupFallback`).
    pub fn source_ranges(&self) -> &[Span] {
        if !self.source_ranges.is_empty() {
            &self.source_ranges
        } else {
            // Only fall back to source_span when it's reliable
            match self.source_span_kind {
                SourceSpanKind::ExactEntities
                | SourceSpanKind::EnclosingEntity
                | SourceSpanKind::DocumentRange => std::slice::from_ref(&self.source_span),
                SourceSpanKind::Unavailable | SourceSpanKind::GroupFallback => &[],
            }
        }
    }

    /// Get entity kind (only for code chunks)
    pub fn entity_kind(&self) -> Option<EntityKind> {
        self.code_metadata.as_ref().map(|m| m.entity_kind)
    }

    /// Get split reason (only for code chunks)
    pub fn split_reason(&self) -> Option<SplitReason> {
        self.code_metadata.as_ref().map(|m| m.split_reason)
    }

    /// Check if chunk has overlap (only for code chunks)
    pub fn has_overlap(&self) -> bool {
        self.code_metadata
            .as_ref()
            .map(|m| m.has_overlap)
            .unwrap_or(false)
    }

    /// Check if chunk is a fragment (only for code chunks)
    pub fn is_fragment(&self) -> bool {
        self.code_metadata
            .as_ref()
            .map(|m| m.is_fragment)
            .unwrap_or(false)
    }

    // === Document metadata accessors ===

    /// Get document-specific metadata
    pub fn as_document(&self) -> Option<&DocumentSpecificMetadata> {
        self.doc_metadata.as_ref()
    }

    /// Get mutable reference to document-specific metadata
    pub fn as_document_mut(&mut self) -> Option<&mut DocumentSpecificMetadata> {
        self.doc_metadata.as_mut()
    }

    /// Get document structure info
    pub fn doc_structure(&self) -> Option<&str> {
        self.doc_metadata
            .as_ref()
            .and_then(|m| m.doc_structure.as_deref())
    }

    /// Get document node IDs
    pub fn doc_node_ids(&self) -> &[String] {
        self.doc_metadata
            .as_ref()
            .map(|m| m.doc_node_ids.as_slice())
            .unwrap_or(&[])
    }

    // === Factory methods ===

    /// Create metadata by deriving the category from the payload type.
    ///
    /// Single-source constructor for document-pipeline chunks: the
    /// `file_category` is always [`ChunkContentType::file_category`] of the
    /// given payload type, so the two stored labels cannot disagree. Code
    /// chunks carry code-specific metadata and are built via [`Self::for_code`].
    pub fn from_content_type(
        content_type: ChunkContentType,
        file_path: String,
        source_span: Span,
        doc_metadata: Option<DocumentSpecificMetadata>,
    ) -> Self {
        let file_category = content_type.file_category();
        debug_assert!(
            content_type.matches_category(file_category),
            "payload type {content_type:?} must match its derived category"
        );
        let source_span_kind = match content_type {
            ChunkContentType::PlainText => SourceSpanKind::Unavailable,
            _ => SourceSpanKind::DocumentRange,
        };
        Self {
            content_type,
            file_path: crate::path::normalize_project_path(&file_path),
            source_span,
            source_ranges: vec![source_span],
            source_span_kind,
            bm25_word_count: None,
            segment_id: String::new(),
            merged_group_ids: Vec::new(),
            test_info: crate::types::TestInfo::unknown(),
            file_category,
            code_metadata: None,
            doc_metadata,
        }
    }

    /// Create metadata from an explicitly paired payload type + category.
    ///
    /// Single-source constructor for document-pipeline chunks whose
    /// classification was derived once at the pipeline entry: both stored
    /// labels are assigned together and their consistency is asserted, so a
    /// `(Document, Schema)` pair survives just like every other legal
    /// combination. Code chunks carry code-specific metadata and are built
    /// via [`Self::for_code`].
    pub fn with_classification(
        content_type: ChunkContentType,
        file_category: FileCategory,
        file_path: String,
        source_span: Span,
        doc_metadata: Option<DocumentSpecificMetadata>,
    ) -> Self {
        let source_span_kind = match content_type {
            ChunkContentType::PlainText => SourceSpanKind::Unavailable,
            _ => SourceSpanKind::DocumentRange,
        };
        Self::from_parts(content_type, file_category).build(
            file_path,
            source_span,
            source_span_kind,
            None,
            doc_metadata,
        )
    }

    /// Create metadata for a code chunk
    pub fn for_code(
        file_path: String,
        source_span: Span,
        language: Language,
        code_metadata: CodeSpecificMetadata,
    ) -> Self {
        Self::from_parts(ChunkContentType::Code { language }, FileCategory::Code).build(
            file_path,
            source_span,
            SourceSpanKind::ExactEntities,
            Some(code_metadata),
            None,
        )
    }

    /// Create metadata for a document chunk
    pub fn for_document(
        file_path: String,
        source_span: Span,
        doc_metadata: DocumentSpecificMetadata,
    ) -> Self {
        Self::from_content_type(
            ChunkContentType::Document,
            file_path,
            source_span,
            Some(doc_metadata),
        )
    }

    /// Create metadata for a config chunk
    pub fn for_config(
        file_path: String,
        source_span: Span,
        format: String,
        doc_metadata: DocumentSpecificMetadata,
    ) -> Self {
        Self::from_content_type(
            ChunkContentType::Config { format },
            file_path,
            source_span,
            Some(doc_metadata),
        )
    }

    /// Create metadata for plain text
    pub fn for_plain_text(file_path: String, source_span: Span) -> Self {
        Self::from_content_type(ChunkContentType::PlainText, file_path, source_span, None)
    }

    /// Create metadata for a schema-definition chunk (`.proto`, `.graphql`, ...).
    ///
    /// Schema files reuse the document payload but keep the dedicated
    /// [`FileCategory::Schema`] so the category survives storage and queries.
    pub fn for_schema(
        file_path: String,
        source_span: Span,
        doc_metadata: DocumentSpecificMetadata,
    ) -> Self {
        debug_assert!(
            ChunkContentType::Document.matches_category(FileCategory::Schema),
            "document payload must accept the Schema category"
        );
        Self {
            content_type: ChunkContentType::Document,
            file_path: crate::path::normalize_project_path(&file_path),
            source_span,
            source_ranges: vec![source_span],
            source_span_kind: SourceSpanKind::DocumentRange,
            bm25_word_count: None,
            segment_id: String::new(),
            merged_group_ids: Vec::new(),
            test_info: crate::types::TestInfo::unknown(),
            file_category: FileCategory::Schema,
            code_metadata: None,
            doc_metadata: Some(doc_metadata),
        }
    }

    /// Intermediate builder pairing a payload type with an explicit category.
    fn from_parts(content_type: ChunkContentType, file_category: FileCategory) -> Self {
        debug_assert!(
            content_type.matches_category(file_category),
            "payload type {content_type:?} must match category {file_category:?}"
        );
        Self {
            content_type,
            file_category,
            ..Self::default()
        }
    }

    /// Complete a partially built metadata record.
    fn build(
        self,
        file_path: String,
        source_span: Span,
        source_span_kind: SourceSpanKind,
        code_metadata: Option<CodeSpecificMetadata>,
        doc_metadata: Option<DocumentSpecificMetadata>,
    ) -> Self {
        Self {
            file_path: crate::path::normalize_project_path(&file_path),
            source_span,
            source_ranges: vec![source_span],
            source_span_kind,
            code_metadata,
            doc_metadata,
            ..self
        }
    }

    /// Whether the stored payload type and category are consistent.
    pub fn category_consistent(&self) -> bool {
        self.content_type.matches_category(self.file_category)
    }
}

impl ChunkedResult {
    /// Create new ChunkedResult
    pub fn new(
        chunk_id: String,
        source_group_id: String,
        path: ChunkPath,
        chunk_index: usize,
        total_chunks: usize,
    ) -> Self {
        Self {
            chunk_id,
            source_group_id,
            path,
            group_type: GroupType::Standalone,
            chunk_index,
            total_chunks,
            text: String::new(),
            token_count: 0,
            start_byte: 0,
            end_byte: 0,
            prev_overlap: None,
            next_overlap: None,
            related_groups: Vec::new(),
            self_contained: false,
            bm25_title: None,
            bm25_keywords: Vec::new(),
            metadata: ChunkMetadata::default(),
        }
    }

    /// Get full text with overlap
    pub fn full_text_with_overlap(&self) -> String {
        let mut result = String::new();

        // Add previous overlap
        if let Some(ref overlap) = self.prev_overlap {
            result.push_str(&overlap.text);
            result.push(' ');
        }

        // Add main text
        if !self.text.is_empty() {
            result.push_str(&self.text);
        }

        // Add next overlap
        if let Some(ref overlap) = self.next_overlap {
            result.push(' ');
            result.push_str(&overlap.text);
        }

        result
    }

    /// Get pure text (without overlap)
    pub fn pure_text(&self) -> &str {
        &self.text
    }

    /// Check if first chunk
    pub fn is_first(&self) -> bool {
        self.chunk_index == 0
    }

    /// Check if last chunk
    pub fn is_last(&self) -> bool {
        self.chunk_index == self.total_chunks.saturating_sub(1)
    }
}

impl Default for ChunkMetadata {
    fn default() -> Self {
        let content_type = ChunkContentType::PlainText;
        Self {
            file_category: content_type.file_category(),
            content_type,
            file_path: String::new(),
            source_span: Span::default(),
            source_ranges: Vec::new(),
            source_span_kind: SourceSpanKind::Unavailable,
            bm25_word_count: None,
            segment_id: String::new(),
            merged_group_ids: Vec::new(),
            test_info: crate::types::TestInfo::unknown(),
            code_metadata: None,
            doc_metadata: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_content_type_file_category_mapping() {
        assert_eq!(
            ChunkContentType::Code {
                language: Language::Rust
            }
            .file_category(),
            FileCategory::Code
        );
        // Generic text payloads carry `Other`, never `Code`.
        assert_eq!(
            ChunkContentType::PlainText.file_category(),
            FileCategory::Other
        );
        assert_eq!(
            ChunkContentType::Document.file_category(),
            FileCategory::Documentation
        );
        assert_eq!(
            ChunkContentType::Config {
                format: "json".to_string()
            }
            .file_category(),
            FileCategory::Config
        );
    }

    #[test]
    fn test_from_file_category_roundtrip() {
        let cases = [
            (FileCategory::Other, ChunkContentType::PlainText),
            (
                FileCategory::Config,
                ChunkContentType::Config {
                    format: "toml".to_string(),
                },
            ),
            (FileCategory::Documentation, ChunkContentType::Document),
        ];
        for (category, expected) in cases {
            let derived = ChunkContentType::from_file_category(category, "toml".to_string());
            assert_eq!(derived, expected);
            assert!(derived.matches_category(category));
        }
    }

    #[test]
    fn test_metadata_factories_derive_consistent_category() {
        let span = Span::default();
        let code = ChunkMetadata::for_code(
            "a.rs".to_string(),
            span,
            Language::Rust,
            CodeSpecificMetadata::default(),
        );
        assert!(code.category_consistent());
        assert_eq!(code.file_category, FileCategory::Code);

        let doc = ChunkMetadata::for_document("a.md".to_string(), span, Default::default());
        assert!(doc.category_consistent());

        let config = ChunkMetadata::for_config(
            "a.json".to_string(),
            span,
            "json".to_string(),
            Default::default(),
        );
        assert!(config.category_consistent());
        assert_eq!(config.file_category, FileCategory::Config);

        // Plain-text chunks keep the generic text category.
        let plain = ChunkMetadata::for_plain_text("a.txt".to_string(), span);
        assert!(plain.category_consistent());
        assert_eq!(plain.file_category, FileCategory::Other);
    }

    #[test]
    fn test_schema_chunk_keeps_dedicated_category() {
        let schema =
            ChunkMetadata::for_schema("api.proto".to_string(), Span::default(), Default::default());
        assert!(schema.category_consistent());
        assert_eq!(schema.content_type, ChunkContentType::Document);
        assert_eq!(schema.file_category, FileCategory::Schema);
    }

    #[test]
    fn test_from_content_type_single_source() {
        let meta = ChunkMetadata::from_content_type(
            ChunkContentType::Config {
                format: "yaml".to_string(),
            },
            "conf/app.yaml".to_string(),
            Span::default(),
            None,
        );
        assert!(meta.category_consistent());
        assert_eq!(meta.file_category, FileCategory::Config);
        // Path normalization goes through the canonical helper
        assert_eq!(meta.file_path, "conf/app.yaml");
        // Plain text payload keeps the unavailable-span contract
        let plain = ChunkMetadata::from_content_type(
            ChunkContentType::PlainText,
            "log/run.log".to_string(),
            Span::default(),
            None,
        );
        assert_eq!(plain.source_span_kind, SourceSpanKind::Unavailable);
        let doc = ChunkMetadata::from_content_type(
            ChunkContentType::Document,
            "docs/x.md".to_string(),
            Span::default(),
            None,
        );
        assert_eq!(doc.source_span_kind, SourceSpanKind::DocumentRange);
    }
}
