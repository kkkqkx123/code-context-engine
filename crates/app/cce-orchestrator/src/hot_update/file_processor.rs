use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::change::compute::compute_entity_changes;
use super::change::{EntityChange, EntityChangeType, FileChangeType, ParseResultWithChanges};
use super::error::{HotUpdateError, Result};
use super::watcher::{FileEvent, FileEventType};
use cce_parser::parser::ParseCoordinator;
use cce_storage_sqlite::SqliteClient;
use cce_storage_sqlite::repo::entity_repo::EntityRepository;
use cce_storage_sqlite::repo::file_repo::FileRepository;
use cce_types::Span;
use cce_types::entity::{Entity, EntityId, EntityKind};
use cce_types::{LanguageInfo, ParsedFile};

/// File processor for reading files, routing them by content type and
/// recording entity changes.
///
/// This is the hot-update parse stage only: it reads file content, routes
/// document-like paths away from tree-sitter and diffs entities against the
/// stored state. Grouping and chunk generation live in the shared index
/// `FileProcessor` (chunk pipelines) and the export processor (render-input
/// groups), so no chunk-pipeline work happens here.
pub struct FileProcessor {
    parser: ParseCoordinator,
    entity_id_seed: u64,
    /// Test-only parse counter. When set, every tree-sitter parse triggered by
    /// this processor increments it, giving recovery tests a durable proof of
    /// whether a file was re-parsed or reused from its checkpoint envelope.
    parse_counter: Option<Arc<AtomicUsize>>,
}

impl FileProcessor {
    pub fn new() -> Self {
        Self::with_entity_id_seed(0)
    }

    /// Create a file processor whose parser seeds the raw entity ID counter at
    /// `entity_id_seed`.
    ///
    /// Hot-update parses reuse the `EntityId` space of the previously indexed
    /// epoch. Seeding the counter above the existing maximum prevents freshly
    /// parsed entities from colliding with unchanged ones cloned into the
    /// candidate epoch.
    pub fn with_entity_id_seed(entity_id_seed: u64) -> Self {
        Self {
            parser: ParseCoordinator::with_entity_id_seed(entity_id_seed),
            entity_id_seed,
            parse_counter: None,
        }
    }

    /// Attach a test-only counter that is incremented on every parse.
    pub fn with_parse_counter(mut self, counter: Arc<AtomicUsize>) -> Self {
        self.parse_counter = Some(counter);
        self
    }

    fn count_parse(&self) {
        if let Some(counter) = &self.parse_counter {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub async fn process_file_from_event(
        &mut self,
        event: &FileEvent,
        metadata_store: &Option<Arc<SqliteClient>>,
        project_id: i64,
    ) -> Result<ParseResultWithChanges> {
        let change_type = match event.event_type {
            FileEventType::Created => FileChangeType::Added,
            _ => FileChangeType::Modified,
        };
        self.process_file_change_at(
            &event.path,
            &event.path.to_string_lossy(),
            change_type,
            metadata_store,
            project_id,
        )
        .await
    }

    pub async fn process_file_change(
        &mut self,
        path: &Path,
        change_type: FileChangeType,
        metadata_store: &Option<Arc<SqliteClient>>,
        project_id: i64,
    ) -> Result<ParseResultWithChanges> {
        self.process_file_change_at(
            path,
            &path.to_string_lossy(),
            change_type,
            metadata_store,
            project_id,
        )
        .await
    }

    /// Parse a file read from `read_path` while recording `parse_path` (the
    /// project-relative path) in the parse result and storage rows.
    ///
    /// Change detection keys the hash cache on project-relative paths, so the
    /// coordinator reads the file from its absolute on-disk location but must
    /// keep the relative path as the identity used by chunks, summaries and
    /// the `files` table.
    pub async fn process_file_change_at(
        &mut self,
        read_path: &Path,
        parse_path: &str,
        change_type: FileChangeType,
        metadata_store: &Option<Arc<SqliteClient>>,
        project_id: i64,
    ) -> Result<ParseResultWithChanges> {
        // Event-driven read: no scan-phase hash baseline exists here, so the
        // verification inside `read_verified_utf8` is skipped by passing
        // `None`; the entry point stays shared with the full-index path.
        let content = crate::index::read_verified_utf8(read_path, None)
            .await
            .map_err(|e| {
                HotUpdateError::file(format!(
                    "Failed to read file {}: {}",
                    read_path.display(),
                    e
                ))
            })?;

        // Route like the full-index path: documentation/config/text files
        // carry no AST semantics and must not enter the tree-sitter pipeline
        // (their language is unsupported for AST parsing and would fail).
        if LanguageInfo::detect_from_path(parse_path).is_document_like() {
            return Ok(self.process_document_content(parse_path, &content, change_type));
        }

        let parsed_file = self
            .parser
            .parse(parse_path, &content)
            .map_err(|e| HotUpdateError::parse(parse_path.to_string(), e.to_string()))?;
        self.count_parse();

        let mut result = ParseResultWithChanges::new(
            parse_path.into(),
            parsed_file.clone(),
            change_type,
            change_type == FileChangeType::Added,
        );
        result.content_hash = parsed_file.file_hash.clone();

        populate_entity_changes(
            &mut result,
            &parsed_file,
            metadata_store,
            parse_path,
            project_id,
            change_type,
        );

        Ok(result)
    }

    /// Build a parse result for a non-code file (documentation, config, text).
    ///
    /// No tree-sitter parse is attempted: these files have no entities or
    /// relations. The returned `ParsedFile` keeps path/source/hash so
    /// downstream processors can identify and chunk it; chunk generation
    /// happens in the shared index `FileProcessor`, routed by the explicit
    /// document route marker on the result.
    fn process_document_content(
        &self,
        parse_path: &str,
        content: &str,
        change_type: FileChangeType,
    ) -> ParseResultWithChanges {
        let language_info = LanguageInfo::detect_from_path(parse_path);
        let content_hash = cce_utils::hash::calculate_hash(content.as_bytes());
        let mut parsed_file =
            ParsedFile::new(language_info.language, parse_path.to_string(), content);
        parsed_file.file_hash = Some(content_hash.clone());

        let mut result = ParseResultWithChanges::new(
            parse_path.into(),
            parsed_file,
            change_type,
            change_type == FileChangeType::Added,
        );
        result.content_hash = Some(content_hash);
        result
    }

    /// Re-read a file from disk and parse it, producing a fresh parse result.
    ///
    /// Used by relation rebuilding to refresh dependent files after a
    /// dependency changed. Like [`Self::process_file_change_at`], the on-disk
    /// location (`read_path`) and the storage identity (`parse_path`, the
    /// project-relative path) are supplied separately, so chunks, summaries
    /// and `files` rows always key on the relative form even when the file is
    /// read through an absolute path.
    ///
    /// Document-like files are routed to the same non-AST placeholder path as
    /// `process_file_change_at`, so passing e.g. a `.md` path succeeds with
    /// an empty-entity result instead of failing tree-sitter parsing.
    pub async fn reparse_file(
        &self,
        read_path: &Path,
        parse_path: &str,
    ) -> Result<ParseResultWithChanges> {
        // Event-driven read: no scan-phase hash baseline exists here, so the
        // verification inside `read_verified_utf8` is skipped by passing
        // `None`; the entry point stays shared with the full-index path.
        let content = crate::index::read_verified_utf8(read_path, None)
            .await
            .map_err(|e| {
                HotUpdateError::file(format!(
                    "Failed to read file {}: {}",
                    read_path.display(),
                    e
                ))
            })?;

        // Route like `process_file_change_at`: documentation/config/text files
        // carry no AST semantics and must not enter the tree-sitter pipeline
        // (their language is unsupported for AST parsing and would fail).
        if LanguageInfo::detect_from_path(parse_path).is_document_like() {
            return Ok(self.process_document_content(
                parse_path,
                &content,
                FileChangeType::Modified,
            ));
        }

        let mut parser = ParseCoordinator::with_entity_id_seed(self.entity_id_seed);
        let parsed_file = parser
            .parse(parse_path, &content)
            .map_err(|e| HotUpdateError::parse(parse_path.to_string(), e.to_string()))?;
        self.count_parse();

        let mut result = ParseResultWithChanges::new(
            parse_path.into(),
            parsed_file,
            FileChangeType::Modified,
            false,
        );
        result.content_hash = result.parsed_file.file_hash.clone();

        Ok(result)
    }
}

impl Default for FileProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Diff a freshly obtained parse output against the previously stored
/// entities (or mark everything as added) and record the changes on
/// `result`. Shared by the fresh-parse path and the snapshot-cache resume
/// path so both produce identical change records.
pub(crate) fn populate_entity_changes(
    result: &mut ParseResultWithChanges,
    parsed_file: &cce_types::entity::ParsedFile,
    metadata_store: &Option<Arc<SqliteClient>>,
    lookup_path: &str,
    project_id: i64,
    change_type: FileChangeType,
) {
    let new_entities = &parsed_file.entities;

    let old_entities = if change_type == FileChangeType::Added {
        None
    } else if let Some(store) = metadata_store {
        match store.read_connection() {
            Ok(conn) => {
                let path_str = lookup_path;
                match FileRepository::get_by_path_and_project(&conn, path_str, project_id) {
                    Ok(Some(file_record)) => {
                        match EntityRepository::get_by_file_and_project(
                            &conn,
                            file_record.id,
                            project_id,
                        ) {
                            Ok(records) if !records.is_empty() => {
                                Some(entity_records_to_entities(&records))
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "No database connection for entity diff");
                None
            }
        }
    } else {
        tracing::trace!("No metadata store — cannot query old entities for diff");
        None
    };

    match old_entities {
        Some(ref old) => {
            let changes = compute_entity_changes(old, new_entities);
            for change in changes {
                result.add_entity_change(change);
            }
        }
        None => {
            // No previous entities (first parse or metadata store unavailable):
            // treat all entities as Added
            for entity in new_entities {
                result.add_entity_change(
                    EntityChange::new(entity.id, entity.name.clone(), EntityChangeType::Added)
                        .with_entity(entity.clone()),
                );
            }
        }
    }
}

fn parse_entity_kind(s: &str) -> EntityKind {
    match s {
        "Unknown" => EntityKind::Unknown,
        "Class" => EntityKind::Class,
        "Struct" => EntityKind::Struct,
        "Enum" => EntityKind::Enum,
        "Interface" => EntityKind::Interface,
        "Trait" => EntityKind::Trait,
        "TraitImpl" => EntityKind::TraitImpl,
        "InherentImpl" => EntityKind::InherentImpl,
        "TypeAlias" => EntityKind::TypeAlias,
        "Union" => EntityKind::Union,
        "EnumVariant" => EntityKind::EnumVariant,
        "Annotation" => EntityKind::Annotation,
        "Macro" => EntityKind::Macro,
        "Function" => EntityKind::Function,
        "Method" => EntityKind::Method,
        "Constructor" => EntityKind::Constructor,
        "Destructor" => EntityKind::Destructor,
        "Operator" => EntityKind::Operator,
        "Field" => EntityKind::Field,
        "Property" => EntityKind::Property,
        "Variable" => EntityKind::Variable,
        "Constant" => EntityKind::Constant,
        "Module" => EntityKind::Module,
        "Namespace" => EntityKind::Namespace,
        "Package" => EntityKind::Package,
        "StyleRule" => EntityKind::StyleRule,
        "StyleSelector" => EntityKind::StyleSelector,
        "StyleProperty" => EntityKind::StyleProperty,
        "Keyframe" => EntityKind::Keyframe,
        "Element" => EntityKind::Element,
        "Attribute" => EntityKind::Attribute,
        "Expression" => EntityKind::Expression,
        "Component" => EntityKind::Component,
        "Template" => EntityKind::Template,
        "Directive" => EntityKind::Directive,
        "ControlFlow" => EntityKind::ControlFlow,
        "Animation" => EntityKind::Animation,
        "Binding" => EntityKind::Binding,
        "Action" => EntityKind::Action,
        "AtRule" => EntityKind::AtRule,
        "EventHandler" => EntityKind::EventHandler,
        "ScriptContent" => EntityKind::ScriptContent,
        "StyleContent" => EntityKind::StyleContent,
        "EmbeddedBlock" => EntityKind::EmbeddedBlock,
        "TestSuite" => EntityKind::TestSuite,
        "TestCase" => EntityKind::TestCase,
        "TestHook" => EntityKind::TestHook,
        "Assertion" => EntityKind::Assertion,
        "Mock" => EntityKind::Mock,
        _ => EntityKind::Unknown,
    }
}

fn entity_records_to_entities(records: &[cce_storage_sqlite::EntityRecord]) -> Vec<Entity> {
    records
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let kind = parse_entity_kind(&r.kind);
            let span = Span::new(
                r.span_start_byte.unwrap_or(0) as usize,
                r.span_end_byte.unwrap_or(0) as usize,
                r.span_start_row.unwrap_or(0) as usize,
                r.span_start_column.unwrap_or(0) as usize,
                r.span_end_row.unwrap_or(0) as usize,
                r.span_end_column.unwrap_or(0) as usize,
            );
            Entity {
                id: EntityId(i as u64),
                kind,
                name: r.name.clone(),
                signature: r.signature.clone().unwrap_or_default(),
                parameters: Vec::new(),
                return_type: r.return_type.clone(),
                span,
                depth: r.depth.unwrap_or(0) as usize,
                parent: None,
                children: Vec::new(),
                doc_comment: r.doc_comment.clone(),
                modifiers: Vec::new(),
                attributes: HashMap::new(),
                metadata: HashMap::new(),
                is_stdlib: false,
                stdlib_category: None,
                subtype: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_processor_creation() {
        let processor = FileProcessor::new();
        let _ = processor.parser;
    }

    #[test]
    fn test_file_processor_default() {
        let processor = FileProcessor::default();
        let _ = processor.parser;
    }

    #[tokio::test]
    async fn file_processor_process_file_from_event_sets_content_hash() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("sample.rs");
        std::fs::write(&path, "pub fn f() {}").expect("write file");

        let mut processor = FileProcessor::new();
        let result = processor
            .process_file_from_event(&FileEvent::created(path), &None, 1)
            .await
            .expect("process file event");
        assert_eq!(
            result.content_hash, result.parsed_file.file_hash,
            "process_file_from_event must record the parsed file hash"
        );
    }

    #[test]
    fn is_document_like_classifies_by_extension() {
        let is_document_like = |path: &str| LanguageInfo::detect_from_path(path).is_document_like();
        assert!(is_document_like("docs/README.md"));
        assert!(is_document_like("Cargo.toml"));
        assert!(is_document_like("logs/app.log"));
        assert!(is_document_like("config.yaml"));
        assert!(!is_document_like("src/main.rs"));
        assert!(!is_document_like("lib.py"));
    }

    /// Documentation files must bypass tree-sitter entirely: their language
    /// is unsupported for AST parsing, so the parse would fail. The produced
    /// result keeps source and hash for downstream document-pipeline
    /// processing.
    #[tokio::test]
    async fn document_change_bypasses_tree_sitter() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("README.md");
        let content = "# guide\n\nsome text\n";
        std::fs::write(&path, content).expect("write file");

        let counter = Arc::new(AtomicUsize::new(0));
        let mut processor = FileProcessor::new().with_parse_counter(Arc::clone(&counter));
        let result = processor
            .process_file_change_at(&path, "README.md", FileChangeType::Modified, &None, 1)
            .await
            .expect("document change must not fail");

        assert_eq!(
            counter.load(Ordering::Relaxed),
            0,
            "document files must not trigger a tree-sitter parse"
        );
        assert!(
            result.parsed_file.entities.is_empty(),
            "documents carry no entities"
        );
        assert!(
            result.content_route.is_document(),
            "document placeholders must carry a document marker"
        );
        assert_eq!(
            result.content_route,
            cce_types::ContentRoute::Documentation,
            "README.md should route to Documentation"
        );
        assert_eq!(
            result.content_hash,
            Some(cce_utils::hash::calculate_hash(content.as_bytes()))
        );
        assert_eq!(result.parsed_file.path, "README.md");
        assert_eq!(&*result.parsed_file.source, content);
    }

    /// Config/text files follow the same non-AST route as documentation.
    #[tokio::test]
    async fn config_and_text_changes_bypass_tree_sitter() {
        for name in ["settings.toml", "notes.log"] {
            let dir = tempfile::TempDir::new().expect("temp dir");
            let path = dir.path().join(name);
            std::fs::write(&path, "key = value\n").expect("write file");

            let counter = Arc::new(AtomicUsize::new(0));
            let mut processor = FileProcessor::new().with_parse_counter(Arc::clone(&counter));
            let result = processor
                .process_file_change_at(&path, name, FileChangeType::Added, &None, 1)
                .await
                .expect("non-code change must not fail");
            assert_eq!(
                counter.load(Ordering::Relaxed),
                0,
                "{name} must not trigger a tree-sitter parse"
            );
            assert!(result.parsed_file.entities.is_empty());
            assert!(result.is_new_file, "added files must be flagged new");
        }
    }

    /// Code files keep going through tree-sitter with entity diffing.
    #[tokio::test]
    async fn code_change_still_uses_tree_sitter() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("sample.rs");
        // Single-char names and empty bodies are filtered as low-value
        // entities, so use a realistic function.
        std::fs::write(&path, "fn sample_main() {\n    println!(\"hi\");\n}\n")
            .expect("write file");

        let counter = Arc::new(AtomicUsize::new(0));
        let mut processor = FileProcessor::new().with_parse_counter(Arc::clone(&counter));
        let result = processor
            .process_file_change_at(&path, "sample.rs", FileChangeType::Modified, &None, 1)
            .await
            .expect("code change must parse");

        assert_eq!(
            counter.load(Ordering::Relaxed),
            1,
            "code files must be parsed by tree-sitter"
        );
        assert!(
            !result.parsed_file.entities.is_empty(),
            "rust function must yield an entity"
        );
    }

    /// `reparse_file` must route document-like files through the non-AST
    /// placeholder path instead of failing with "Language not supported".
    #[tokio::test]
    async fn reparse_file_handles_document_paths() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let content = "# guide\n\nreparse me\n";
        let path = dir.path().join("README.md");
        std::fs::write(&path, content).expect("write file");

        let counter = Arc::new(AtomicUsize::new(0));
        let processor = FileProcessor::new().with_parse_counter(Arc::clone(&counter));
        let result = processor
            .reparse_file(&path, "README.md")
            .await
            .expect("document paths must not fail in reparse_file");

        assert_eq!(
            counter.load(Ordering::Relaxed),
            0,
            "document files must not trigger a tree-sitter parse"
        );
        assert!(
            result.parsed_file.entities.is_empty(),
            "documents carry no entities"
        );
        assert!(
            result.content_route.is_document(),
            "reparse of a document must carry a document marker"
        );
        assert_eq!(result.content_route, cce_types::ContentRoute::Documentation);
        assert_eq!(
            result.content_hash,
            Some(cce_utils::hash::calculate_hash(content.as_bytes()))
        );
        assert!(result.parsed_file.source.contains("reparse me"));
    }

    /// `ParseResultWithChanges::new` derives the content route from the path
    /// once, so every construction site produces an explicit marker.
    #[test]
    fn parse_result_derives_content_route_from_path() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let code_path = dir.path().join("sample.rs");
        std::fs::write(&code_path, "pub fn f() {}").expect("write file");

        let doc_parsed = ParsedFile::new(
            cce_types::Language::Unknown,
            "docs/README.md".to_string(),
            "# guide",
        );
        let doc_result = ParseResultWithChanges::new(
            "docs/README.md".into(),
            doc_parsed,
            FileChangeType::Added,
            true,
        );
        assert_eq!(
            doc_result.content_route,
            cce_types::ContentRoute::Documentation
        );

        let mut coordinator = ParseCoordinator::new();
        let code_parsed = coordinator
            .parse("sample.rs", "pub fn sample_main() {}\n")
            .expect("rust should parse");
        let code_result = ParseResultWithChanges::new(
            code_path.clone(),
            code_parsed,
            FileChangeType::Modified,
            false,
        );
        assert_eq!(code_result.content_route, cce_types::ContentRoute::Ast);
    }
}
