//! Parse context that flows through the parsing pipeline
//!
//! Contains all data accumulated during parsing, allowing stages to
//! read previous results and add new data.

use cce_types::language::{Language, LanguageInfo};
use cce_types::{BehaviorStore, ControlFlowStore, ImportTable, Span};
use tree_sitter::Tree;

/// Parse context that flows through the pipeline
///
/// Contains all data accumulated during parsing, allowing stages to
/// read previous results and add new data.
#[derive(Debug)]
pub struct ParseContext {
    /// File path being parsed
    pub file_path: String,
    /// Source code content
    pub source: String,

    // ── Language Detection + AST Parsing ──
    /// Detected language information
    pub language_info: Option<LanguageInfo>,
    /// Parsed AST tree
    pub tree: Option<Tree>,

    // ── Entity Extraction ──
    /// Extracted entities
    pub entities: Vec<cce_types::Entity>,
    /// Extracted behavior sidecar
    pub behavior: BehaviorStore,
    /// Extracted control-flow sidecar
    pub control_flow: ControlFlowStore,
    // ── Doc Comment Processing ──
    /// File-level doc comment
    pub file_doc_comment: Option<String>,
    /// Source range of the file-level doc comment.
    pub file_doc_span: Option<Span>,

    // ── Relation Extraction ──
    /// Extracted relations
    pub relations: Vec<cce_types::Relation>,

    // ── Post-Processing ──
    /// Embedded blocks (for Vue/Svelte)
    pub embedded_blocks: Vec<crate::parser::embedded_types::EmbeddedBlock>,
    /// Block entities from embedded code
    pub block_entities: Vec<cce_types::Entity>,
    /// Block relations from embedded code
    pub block_relations: Vec<cce_types::RawRelationData>,
    /// Local symbol table
    pub local_symbols: std::collections::HashMap<String, Vec<cce_types::EntityId>>,
    /// Import table extracted from AST
    pub import_table: Option<ImportTable>,
}

impl ParseContext {
    /// Create a new parse context
    pub fn new(file_path: String, source: String) -> Self {
        Self {
            file_path: cce_types::path::normalize_project_path(&file_path),
            source,
            language_info: None,
            tree: None,
            entities: Vec::new(),
            behavior: BehaviorStore::default(),
            control_flow: ControlFlowStore::default(),
            relations: Vec::new(),
            file_doc_comment: None,
            file_doc_span: None,
            embedded_blocks: Vec::new(),
            block_entities: Vec::new(),
            block_relations: Vec::new(),
            local_symbols: std::collections::HashMap::new(),
            import_table: None,
        }
    }

    /// Get the language
    pub fn language(&self) -> Option<&Language> {
        self.language_info.as_ref().map(|info| &info.language)
    }
}
