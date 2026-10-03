//! Parse context that flows through the parsing pipeline
//!
//! Contains all data accumulated during parsing, allowing stages to
//! read previous results and add new data.

use cce_types::language::{Language, LanguageInfo};
use cce_types::{BehaviorStore, ControlFlowStore, ImportTable, Span};
use tree_sitter::Tree;

/// Immutable input for the parsing pipeline.
///
/// Contains the file path, source content, and optionally pre-detected
/// language information. Created once at pipeline entry and never mutated.
#[derive(Debug)]
pub struct ParseInput {
    /// File path being parsed
    pub file_path: String,
    /// Source code content
    pub source: String,
    /// Pre-detected language information (None = auto-detect)
    pub language_info: Option<LanguageInfo>,
}

impl ParseInput {
    /// Create a new parse input
    pub fn new(file_path: String, source: String) -> Self {
        Self {
            file_path: cce_types::path::normalize_project_path(&file_path),
            source,
            language_info: None,
        }
    }

    /// Create a new parse input with pre-detected language
    pub fn with_language(file_path: String, source: String, language_info: LanguageInfo) -> Self {
        Self {
            file_path: cce_types::path::normalize_project_path(&file_path),
            source,
            language_info: Some(language_info),
        }
    }

    /// Get the language
    pub fn language(&self) -> Option<&Language> {
        self.language_info.as_ref().map(|info| &info.language)
    }
}

/// Output from the AST parsing stage.
#[derive(Debug)]
pub struct AstStageOutput {
    /// Parsed AST tree
    pub tree: Tree,
    /// Whether the tree contains syntax errors
    pub has_syntax_errors: bool,
}

/// Output from the entity extraction stage.
#[derive(Debug)]
pub struct EntityStageOutput {
    /// Extracted entities
    pub entities: Vec<cce_types::Entity>,
    /// Extracted behavior sidecar
    pub behavior: BehaviorStore,
    /// Extracted control-flow sidecar
    pub control_flow: ControlFlowStore,
}

/// Output from the doc comment processing stage.
#[derive(Debug)]
pub struct DocCommentStageOutput {
    /// File-level doc comment
    pub file_doc_comment: Option<String>,
    /// Source range of the file-level doc comment
    pub file_doc_span: Option<Span>,
}

/// Output from the relation extraction stage.
#[derive(Debug)]
pub struct RelationStageOutput {
    /// Extracted relations
    pub relations: Vec<cce_types::Relation>,
}

/// Output from the structural extraction stage.
#[derive(Debug)]
pub struct StructuralStageOutput {
    /// Embedded blocks (for Vue/Svelte)
    pub embedded_blocks: Vec<crate::parser::embedded_types::EmbeddedBlock>,
    /// Block entities from embedded code
    pub block_entities: Vec<cce_types::Entity>,
    /// Block relations from embedded code
    pub block_relations: Vec<cce_types::RawRelationData>,
    /// Local symbol table
    pub local_symbols: std::collections::HashMap<String, Vec<cce_types::EntityId>>,
}

/// Output from the post-processing stage.
#[derive(Debug)]
pub struct PostProcessStageOutput {
    /// Import table extracted from AST
    pub import_table: Option<ImportTable>,
}

/// Parse context that flows through the pipeline
///
/// Contains all data accumulated during parsing, allowing stages to
/// read previous results and add new data.
#[derive(Debug)]
pub struct ParseContext {
    /// Immutable input
    pub input: ParseInput,
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
    /// Tree-sitter reported syntax errors while parsing.
    /// Entities inside error regions may be missing, so index results
    /// for this file are partial even though parsing succeeded.
    pub has_syntax_errors: bool,
}

impl ParseContext {
    /// Create a new parse context
    pub fn new(file_path: String, source: String) -> Self {
        Self {
            input: ParseInput::new(file_path, source),
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
            has_syntax_errors: false,
        }
    }

    /// Get the file path
    pub fn file_path(&self) -> &str {
        &self.input.file_path
    }

    /// Get the source
    pub fn source(&self) -> &str {
        &self.input.source
    }

    /// Get the language
    pub fn language(&self) -> Option<&Language> {
        self.language_info.as_ref().map(|info| &info.language)
    }
}
