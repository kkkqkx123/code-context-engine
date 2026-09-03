//! Rule-based summary generator
//!
//! Generates file summaries using heuristics and pattern matching
//! without requiring LLM calls.

use std::{cmp::Ordering, sync::Arc};

use super::specialized;
use crate::ast_to_nl::CodeFormGroup;
use crate::grouper::{GroupType, ProcessingResult};
use crate::summary::SummaryConfig;
use crate::summary::strategy::{FileCategory, ImportanceDecision};
use crate::summary::types::FileSummary;
use cce_metrics::SummaryMetrics;
use cce_types::ParsedFile;

mod import_export_extractor;
mod summary_text_builder;
mod tag_generator;

/// Rule-based summary generator
///
/// Generates file summaries using deterministic rules based on
/// file structure, imports, exports, and entity analysis.
pub struct RuleBasedGenerator {
    config: SummaryConfig,
    /// Monitoring metrics (optional)
    metrics: Option<Arc<SummaryMetrics>>,
}

impl RuleBasedGenerator {
    /// Create a new rule-based generator
    pub fn new() -> Self {
        Self {
            config: SummaryConfig::default(),
            metrics: None,
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: SummaryConfig) -> Self {
        Self {
            config,
            metrics: None,
        }
    }

    /// Set monitoring metrics
    pub fn with_metrics(mut self, metrics: Arc<SummaryMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Generate summary for a parsed file
    ///
    /// Runs PreprocessingPipeline to obtain entity group information for
    /// accurate importance determination and richer summary generation.
    pub fn generate_sync(&self, parsed_file: &ParsedFile) -> FileSummary {
        let start = std::time::Instant::now();

        // Check if this is a specialized file type that needs different handling
        if FileCategory::is_specialized_file(parsed_file) {
            return specialized::generate_specialized_summary(parsed_file);
        }

        // Run preprocessing for accurate group-aware analysis
        let processor = crate::grouper::PreprocessingPipeline::new();
        let processing_result = processor.process(parsed_file);

        let summary = self.generate_with_groups_sync(parsed_file, &processing_result);

        let duration = start.elapsed().as_millis() as u64;

        if let Some(ref metrics) = self.metrics {
            let summary_length = summary.summary_text.len();
            metrics.record_generation(duration as f64, summary_length);
        }

        summary
    }

    /// Generate summary for a parsed file with pre-computed processing result
    ///
    /// This method allows callers to provide a pre-computed `ProcessingResult`
    /// to avoid redundant preprocessing when the same file is processed multiple times.
    /// If `processing_result` is `None`, preprocessing will be performed internally.
    pub fn generate_sync_with_result(
        &self,
        parsed_file: &ParsedFile,
        processing_result: Option<&ProcessingResult>,
    ) -> FileSummary {
        let start = std::time::Instant::now();

        // Check if this is a specialized file type that needs different handling
        if FileCategory::is_specialized_file(parsed_file) {
            return specialized::generate_specialized_summary(parsed_file);
        }

        // Run preprocessing if not provided
        let default_result;
        let processing_result = match processing_result {
            Some(result) => result,
            None => {
                let processor = crate::grouper::PreprocessingPipeline::new();
                default_result = processor.process(parsed_file);
                &default_result
            }
        };

        let summary = self.generate_with_groups_sync(parsed_file, processing_result);

        let duration = start.elapsed().as_millis() as u64;

        if let Some(ref metrics) = self.metrics {
            let summary_length = summary.summary_text.len();
            metrics.record_generation(duration as f64, summary_length);
        }

        summary
    }

    /// Generate summary for a parsed file (async wrapper)
    pub async fn generate(&self, parsed_file: &ParsedFile) -> FileSummary {
        self.generate_sync(parsed_file)
    }

    /// Generate summaries for multiple files
    pub async fn generate_batch(&self, parsed_files: &[ParsedFile]) -> Vec<FileSummary> {
        parsed_files.iter().map(|f| self.generate_sync(f)).collect()
    }

    /// Generate summaries for multiple files with pre-computed processing results
    ///
    /// This method allows callers to provide pre-computed `ProcessingResult`s
    /// to avoid redundant preprocessing when the same files are processed multiple times.
    /// If `processing_results` is `None`, preprocessing will be performed internally for each file.
    pub fn generate_batch_with_results(
        &self,
        parsed_files: &[ParsedFile],
        processing_results: Option<&[ProcessingResult]>,
    ) -> Vec<FileSummary> {
        match processing_results {
            Some(results) => {
                assert_eq!(
                    parsed_files.len(),
                    results.len(),
                    "Number of processing results must match number of parsed files"
                );
                parsed_files
                    .iter()
                    .zip(results.iter())
                    .map(|(f, r)| self.generate_sync_with_result(f, Some(r)))
                    .collect()
            }
            None => parsed_files.iter().map(|f| self.generate_sync(f)).collect(),
        }
    }

    /// Generate summary using pre-processor results
    ///
    /// Leverages `ProcessingResult` from `PreprocessingPipeline` to generate
    /// richer summaries with class-method associations, utility identification,
    /// and merged call patterns.
    pub fn generate_with_groups_sync(
        &self,
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
    ) -> FileSummary {
        // Check if this is a specialized file type that needs different handling
        if FileCategory::is_specialized_file(parsed_file) {
            return specialized::generate_specialized_summary(parsed_file);
        }

        let mut summary = FileSummary::new(&parsed_file.path)
            .with_category(FileCategory::Code)
            .with_file_level_test_info(
                &parsed_file.language,
                &parsed_file.path,
                Some(&processing_result.groups),
            );

        // Basic file info
        summary.language = parsed_file.language.to_string();
        summary.line_count = parsed_file.source.lines().count() as u32;
        summary.entity_count = processing_result.stats.input_entities as u32;

        // Calculate importance level using simplified decision logic
        summary.importance_level =
            ImportanceDecision::determine_importance(parsed_file, processing_result);

        // Extract main entities using group information
        summary.main_entities = self.extract_main_entities_from_groups(processing_result);

        // Extract imports
        summary.imports = self.extract_imports(parsed_file);

        // Extract exports
        summary.exports = self.extract_exports(parsed_file);

        // Generate tags using group information
        summary.tags = self.generate_tags_with_groups(parsed_file, processing_result);

        // Preserve file-level documentation for grouped export paths.
        summary.file_doc_comment = parsed_file.file_doc_comment.clone();

        // Generate enriched summary text
        summary.summary_text =
            self.generate_summary_text_with_groups(&summary, parsed_file, processing_result);

        summary
    }

    /// Generate summary using pre-processor results (async wrapper)
    pub async fn generate_with_groups(
        &self,
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
    ) -> FileSummary {
        self.generate_with_groups_sync(parsed_file, processing_result)
    }

    /// Generate summary using CodeForm objects (avoiding re-parsing)
    ///
    /// This method allows downstream modules to use pre-converted CodeForm
    /// objects instead of repeatedly re-parsing the AST for imports/exports.
    ///
    /// # Advantages over `generate_sync()`:
    /// - Avoids re-parsing imports with AstParser (already done during grouping)
    /// - Imports are extracted from CodeForm modifiers and type annotations
    /// - More efficient for pipelines that already have grouped results
    pub fn generate_with_code_forms_sync(
        &self,
        parsed_file: &ParsedFile,
        code_forms: &[CodeFormGroup],
        processing_result: &ProcessingResult,
    ) -> FileSummary {
        // Check if this is a specialized file type
        if FileCategory::is_specialized_file(parsed_file) {
            return specialized::generate_specialized_summary(parsed_file);
        }

        let mut summary = FileSummary::new(&parsed_file.path)
            .with_category(FileCategory::Code)
            .with_file_level_test_info(
                &parsed_file.language,
                &parsed_file.path,
                Some(&processing_result.groups),
            );

        // Basic file info
        summary.language = parsed_file.language.to_string();
        summary.line_count = parsed_file.source.lines().count() as u32;
        summary.entity_count = parsed_file.entities.len() as u32;

        // Set importance level using the complete group analysis.
        summary.importance_level =
            ImportanceDecision::determine_importance(parsed_file, processing_result);

        // Extract main entities from code forms
        summary.main_entities = self.extract_main_entities_from_code_forms(code_forms);

        // Extract imports from code forms (avoids re-parsing)
        summary.imports = self.extract_imports_from_code_forms(code_forms, parsed_file);

        // Extract exports
        summary.exports = self.extract_exports(parsed_file);

        // Generate tags
        summary.tags = self.generate_tags(parsed_file);

        // Set file-level documentation
        summary.file_doc_comment = parsed_file.file_doc_comment.clone();

        // Generate summary text
        summary.summary_text = self.generate_summary_text(&summary, parsed_file);

        summary
    }

    /// Generate summary using CodeForm objects (async wrapper)
    pub async fn generate_with_code_forms(
        &self,
        parsed_file: &ParsedFile,
        code_forms: &[CodeFormGroup],
        processing_result: &ProcessingResult,
    ) -> FileSummary {
        self.generate_with_code_forms_sync(parsed_file, code_forms, processing_result)
    }

    /// Extract main entities from code forms
    fn extract_main_entities_from_code_forms(&self, code_forms: &[CodeFormGroup]) -> Vec<String> {
        let mut entities = Vec::new();

        for code_form in code_forms {
            // Add header
            entities.push(code_form.header.name.clone());

            // Add members
            for member in &code_form.members {
                entities.push(member.name.clone());
            }
        }

        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        entities.retain(|e| seen.insert(e.clone()));

        // Limit to max_entities
        entities.truncate(self.config.max_entities);
        entities
    }

    fn extract_main_entities_from_groups(
        &self,
        processing_result: &ProcessingResult,
    ) -> Vec<String> {
        let mut entities_with_score: Vec<(String, f32)> = Vec::new();

        for group in &processing_result.groups {
            if let Some(ref header) = group.header {
                // Base score
                let mut score = 1.0;

                // Get call information from entity_meta
                if let Some(meta) = processing_result.entity_meta.get(&header.name) {
                    // Higher call count indicates higher importance
                    score += (meta.call_count as f32).min(5.0) * 0.1;

                    // Being called by multiple callers indicates it's a public interface
                    if meta.callers.len() > 2 {
                        score += 0.2;
                    }

                    // Having many callees indicates it's a complex function
                    if meta.callees.len() > 3 {
                        score += 0.15;
                    }

                    // Merged entities might be more important
                    if meta.is_merged {
                        score += 0.1;
                    }
                }

                // Adjust score based on group type
                match group.group_type {
                    GroupType::ClassWithMethods => {
                        score += 0.3;
                        entities_with_score.push((header.name.clone(), score));
                        for member in &group.members {
                            let member_score = score * 0.8;
                            entities_with_score.push((member.name.clone(), member_score));
                        }
                    }
                    GroupType::InterfaceWithImpls | GroupType::TraitWithImpls => {
                        score += 0.2;
                        entities_with_score.push((header.name.clone(), score));
                        for member in &group.members {
                            let member_score = score * 0.8;
                            entities_with_score.push((member.name.clone(), member_score));
                        }
                    }
                    GroupType::RelatedFunctions => {
                        // Include all related function names
                        entities_with_score.push((header.name.clone(), score));
                        for member in &group.members {
                            let mut member_score = score * 0.8; // Members are less important than header
                            if let Some(meta) = processing_result.entity_meta.get(&member.name) {
                                member_score += (meta.call_count as f32).min(5.0) * 0.1;
                            }
                            entities_with_score.push((member.name.clone(), member_score));
                        }
                    }
                    GroupType::ModuleWithContents => {
                        score += 0.1;
                        entities_with_score.push((header.name.clone(), score));
                        for member in &group.members {
                            let member_score = score * 0.7;
                            entities_with_score.push((member.name.clone(), member_score));
                        }
                    }
                    GroupType::Standalone => {
                        entities_with_score.push((header.name.clone(), score));
                    }
                    _ => {
                        entities_with_score.push((header.name.clone(), score));
                    }
                }
            }
        }

        // Sort by score (descending)
        entities_with_score.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        let mut entities = Vec::new();
        for (entity, _) in entities_with_score {
            if seen.insert(entity.clone()) {
                entities.push(entity);
            }
        }

        // Limit to max_entities
        entities.truncate(self.config.max_entities);
        entities
    }
}

impl Default for RuleBasedGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl crate::summary::types::SummaryGenerator for RuleBasedGenerator {
    async fn generate(&self, parsed_file: &ParsedFile) -> crate::summary::types::FileSummary {
        self.generate_sync(parsed_file)
    }

    async fn generate_with_groups(
        &self,
        parsed_file: &ParsedFile,
        processing_result: &crate::grouper::ProcessingResult,
    ) -> crate::summary::types::FileSummary {
        self.generate_with_groups_sync(parsed_file, processing_result)
    }

    async fn generate_batch(
        &self,
        parsed_files: &[ParsedFile],
    ) -> Vec<crate::summary::types::FileSummary> {
        parsed_files.iter().map(|f| self.generate_sync(f)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grouper::{ProcessingResult, ProcessingStats};
    use crate::summary::FileSummary;
    use crate::summary::types::ImportanceLevel;
    use cce_types::{Entity, EntityId, EntityKind, Language, StandardizedImport};
    fn create_test_parsed_file() -> ParsedFile {
        let mut file = ParsedFile::new(Language::Rust, "src/main.rs".to_string(), "fn main() {}");

        // Add some entities
        let entity = Entity {
            id: EntityId(0),
            kind: EntityKind::Function,
            name: "main".to_string(),
            signature: "fn main()".to_string(),
            parameters: Vec::new(),
            return_type: None,
            span: cce_types::Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: std::collections::HashMap::new(),
            metadata: std::collections::HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        };
        file.add_entity(entity);

        file
    }

    #[test]
    fn test_generate_with_groups_preserves_file_doc_comment() {
        let mut parsed_file = create_test_parsed_file();
        parsed_file.file_doc_comment = Some("Module overview".to_string());

        let generator = RuleBasedGenerator::new();
        let processing_result = ProcessingResult {
            groups: Vec::new(),
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: ProcessingStats::default(),
        };

        let summary = generator.generate_with_groups_sync(&parsed_file, &processing_result);

        assert_eq!(summary.file_doc_comment.as_deref(), Some("Module overview"));
    }

    #[test]
    fn test_rule_based_generator() {
        let generator = RuleBasedGenerator::new();
        let parsed_file = create_test_parsed_file();

        let summary = generator.generate_sync(&parsed_file);

        assert_eq!(summary.file_path, "src/main.rs");
        assert_eq!(summary.language, "Rust");
        assert_eq!(summary.importance_level, ImportanceLevel::High); // main.rs is a core module
    }

    #[test]
    fn test_rule_based_generator_with_doc_comment() {
        let mut parsed_file = create_test_parsed_file();
        parsed_file.file_doc_comment = Some("Main entry point.".to_string());

        let generator = RuleBasedGenerator::new();
        let summary = generator.generate_sync(&parsed_file);

        assert_eq!(summary.file_path, "src/main.rs");
        assert!(!summary.summary_text.is_empty());
        assert!(summary.summary_text.contains("Main entry point."));
    }

    #[test]
    fn test_importance_level() {
        let mut file = ParsedFile::new(Language::Rust, "src/lib.rs".to_string(), "");

        // Add multiple entities (>= 10 makes it important)
        for i in 0..10 {
            let entity = Entity {
                id: EntityId(i as u64),
                kind: if i % 2 == 0 {
                    EntityKind::Function
                } else {
                    EntityKind::Struct
                },
                name: format!("item{}", i),
                signature: format!("fn item{}()", i),
                parameters: Vec::new(),
                return_type: None,
                span: cce_types::Span::default(),
                depth: 0,
                parent: None,
                children: Vec::new(),
                doc_comment: None,
                modifiers: Vec::new(),
                attributes: std::collections::HashMap::new(),
                metadata: std::collections::HashMap::new(),
                is_stdlib: false,
                subtype: None,
                stdlib_category: None,
            };
            file.add_entity(entity);
        }

        // Test importance level is set based on entity count
        let generator = RuleBasedGenerator::new();
        let summary = generator.generate_sync(&file);
        assert_eq!(summary.importance_level, ImportanceLevel::High); // 10+ entities = High
    }

    #[test]
    fn test_summary_text_keeps_full_entity_list() {
        let mut file = ParsedFile::new(
            Language::Rust,
            "src/lib.rs".to_string(),
            "fn new() {}\nfn with_value() {}\nfn is_initialized() {}\nfn initialize() {}",
        );

        for (id, (name, modifiers)) in [
            ("new", vec!["pub".to_string(), "const".to_string()]),
            ("with_value", vec!["pub".to_string()]),
            ("is_initialized", vec!["pub".to_string()]),
            ("initialize", vec!["pub".to_string()]),
        ]
        .into_iter()
        .enumerate()
        {
            let entity = Entity {
                id: EntityId(id as u64),
                kind: EntityKind::Function,
                name: name.to_string(),
                signature: format!("fn {}()", name),
                parameters: Vec::new(),
                return_type: None,
                span: cce_types::Span::default(),
                depth: 0,
                parent: None,
                children: Vec::new(),
                doc_comment: None,
                modifiers,
                attributes: std::collections::HashMap::new(),
                metadata: std::collections::HashMap::new(),
                is_stdlib: false,
                subtype: None,
                stdlib_category: None,
            };
            file.add_entity(entity);
        }

        let generator = RuleBasedGenerator::new();
        let summary = FileSummary::new("src/lib.rs")
            .with_entities(vec![
                "new".into(),
                "with_value".into(),
                "is_initialized".into(),
                "initialize".into(),
            ])
            .with_line_count(10);
        let text = generator.generate_summary_text(&summary, &file);

        // format_entity_overview groups functions together
        assert!(text.contains("new, with_value, is_initialized, initialize"));
    }

    #[test]
    fn test_summary_text_keeps_full_import_list() {
        let file = ParsedFile::new(Language::Rust, "src/lib.rs".to_string(), "");
        let summary = FileSummary::new("src/lib.rs")
            .with_imports(vec![
                "std::cell::Cell".into(),
                "std::cell::UnsafeCell".into(),
                "std::panic::RefUnwindSafe".into(),
                "std::panic::UnwindSafe".into(),
                "std::sync::atomic::AtomicBool".into(),
            ])
            .with_line_count(10);

        let generator = RuleBasedGenerator::new();
        let text = generator.generate_summary_text(&summary, &file);

        assert!(text.contains("Uses: std::cell::Cell, std::cell::UnsafeCell"));
        assert!(text.contains("std::sync::atomic::AtomicBool"));
        assert!(!text.contains("and 1 more"));
    }

    #[test]
    fn test_summary_text_includes_exports_and_import_notes() {
        // Imports and exports are collected at the file level only.
        // The summary text must carry the full "Uses:" list, the "Exports:"
        // list, and doc comments attached to import-like entities (which are
        // otherwise lost once import-only groups are dropped before chunking).
        let mut file = ParsedFile::new(Language::Rust, "src/lib.rs".to_string(), "");
        let import = Entity {
            id: EntityId(1),
            kind: EntityKind::Import,
            name: "use std::fmt;".to_string(),
            signature: String::new(),
            parameters: Vec::new(),
            return_type: None,
            span: cce_types::Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: Some("Re-export fmt for tests".to_string()),
            modifiers: Vec::new(),
            attributes: std::collections::HashMap::new(),
            metadata: std::collections::HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        };
        file.add_entity(import);
        let mut exported = Entity {
            id: EntityId(2),
            kind: EntityKind::Function,
            name: "run".to_string(),
            signature: "pub fn run()".to_string(),
            parameters: Vec::new(),
            return_type: None,
            span: cce_types::Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: vec!["pub".to_string()],
            attributes: std::collections::HashMap::new(),
            metadata: std::collections::HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        };
        exported
            .metadata
            .insert("visibility".to_string(), "pub".to_string());
        file.add_entity(exported);
        let mut table = cce_types::ImportTable::default();
        table.add_standardized_import(StandardizedImport::new(
            cce_types::ImportKind::ModuleImport,
            "std::fmt",
        ));
        file.import_table = Some(table);

        let generator = RuleBasedGenerator::new();
        let summary = generator.generate_sync(&file);

        assert_eq!(
            summary.imports,
            vec!["std::fmt".to_string()],
            "the file-level summary must collect all imports"
        );
        assert_eq!(
            summary.exports,
            vec!["run".to_string()],
            "the file-level summary must collect all exports"
        );
        assert!(
            summary.summary_text.contains("Uses: std::fmt"),
            "summary text must list imports, got: {}",
            summary.summary_text
        );
        assert!(
            summary.summary_text.contains("Exports: run"),
            "summary text must list exports, got: {}",
            summary.summary_text
        );
        assert!(
            summary
                .summary_text
                .contains("Import notes: Re-export fmt for tests"),
            "summary text must carry import-associated doc comments, got: {}",
            summary.summary_text
        );
    }

    #[test]
    fn test_group_main_entities_use_deterministic_tie_break() {
        let mut parsed_file = ParsedFile::new(
            Language::Rust,
            "src/lib.rs".to_string(),
            "fn alpha() {}\nfn beta() {}",
        );
        parsed_file.add_entity(cce_types::Entity::new(
            EntityId(1),
            EntityKind::Function,
            "Beta".to_string(),
            cce_types::Span::from_lines(1, 1),
        ));
        parsed_file.add_entity(cce_types::Entity::new(
            EntityId(2),
            EntityKind::Function,
            "Alpha".to_string(),
            cce_types::Span::from_lines(2, 2),
        ));
        let generator = RuleBasedGenerator::new();

        let processing_result = ProcessingResult {
            groups: vec![
                crate::grouper::EntityGroup::from_entity(
                    parsed_file.entities[0].clone(),
                    Language::Rust,
                ),
                crate::grouper::EntityGroup::from_entity(
                    parsed_file.entities[1].clone(),
                    Language::Rust,
                ),
            ],
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: ProcessingStats {
                input_entities: 2,
                ..ProcessingStats::default()
            },
        };

        let summary = generator.generate_with_groups_sync(&parsed_file, &processing_result);

        assert_eq!(
            summary.main_entities,
            vec!["Alpha".to_string(), "Beta".to_string()]
        );
    }

    #[test]
    fn test_summary_test_info_from_groups() {
        use cce_types::test_info::{TestInfo, TestSource};

        let generator = RuleBasedGenerator::new();
        let summary_for = |path: &str, groups: Vec<crate::grouper::EntityGroup>| {
            let mut parsed_file = ParsedFile::new(Language::Rust, path.to_string(), "");
            parsed_file.add_entity(Entity::new(
                EntityId(0),
                EntityKind::Function,
                "adder".to_string(),
                cce_types::Span::from_lines(1, 1),
            ));
            let processing_result = ProcessingResult {
                groups,
                entity_meta: std::collections::HashMap::new(),
                behavior: Default::default(),
                control_flow: Default::default(),
                stats: ProcessingStats::default(),
            };
            generator.generate_with_groups_sync(&parsed_file, &processing_result)
        };

        // AST-level test group in a normal-path file marks the file as test.
        let mut ast_test_group = crate::grouper::EntityGroup::from_entity(
            parsed_entity("adder", "src/lib.rs"),
            Language::Rust,
        );
        ast_test_group.test_info = TestInfo::test_ast();
        let summary = summary_for("src/lib.rs", vec![ast_test_group]);
        assert!(summary.test_info.is_test());
        assert_eq!(summary.test_info.source, TestSource::Ast);

        // Path-level test file without test groups stays test (path baseline).
        let summary = summary_for("tests/foo.rs", vec![]);
        assert!(summary.test_info.is_test());
        assert_eq!(summary.test_info.source, TestSource::Path);

        // Ordinary file with no test signal stays unknown.
        let summary = summary_for("src/lib.rs", vec![]);
        assert!(summary.test_info.is_unknown());
    }

    fn parsed_entity(name: &str, path: &str) -> Entity {
        let mut file = ParsedFile::new(Language::Rust, path.to_string(), "");
        file.add_entity(Entity::new(
            EntityId(1),
            EntityKind::Function,
            name.to_string(),
            cce_types::Span::from_lines(1, 1),
        ));
        file.entities.remove(0)
    }

    #[test]
    fn test_class_with_methods_includes_member_entities() {
        let mut parsed_file = ParsedFile::new(
            Language::Rust,
            "src/once_cell.rs".to_string(),
            "pub struct OnceCell<T> {\n    inner: UnsafeCell<MaybeUninit<T>>,\n}\n\nimpl<T> OnceCell<T> {\n    pub fn get_mut(&mut self) -> &mut T { ... }\n    pub fn get_or_init<F>(&self, f: F) -> &T { ... }\n}",
        );
        // Add a top-level entity for group-based analysis
        parsed_file.add_entity(cce_types::Entity::new(
            EntityId(100),
            EntityKind::Function,
            "once_cell_init".to_string(),
            cce_types::Span::from_lines(1, 1),
        ));

        let header = cce_types::entity::GroupedEntity::new(
            EntityId(1),
            EntityKind::Struct,
            "OnceCell".to_string(),
            "pub struct OnceCell<T>".to_string(),
        );

        let member1 = cce_types::entity::GroupedEntity::new(
            EntityId(2),
            EntityKind::Method,
            "get_mut".to_string(),
            "pub fn get_mut(&mut self) -> &mut T".to_string(),
        );

        let member2 = cce_types::entity::GroupedEntity::new(
            EntityId(3),
            EntityKind::Method,
            "get_or_init".to_string(),
            "pub fn get_or_init<F>(&self, f: F) -> &T".to_string(),
        );

        let group = crate::grouper::EntityGroup {
            group_id: compact_str::CompactString::new("once_cell_group"),
            name: compact_str::CompactString::new("OnceCell"),
            kind: EntityKind::Struct,
            group_type: GroupType::ClassWithMethods,
            header: Some(header),
            members: vec![member1, member2].into(),
            language: Language::Rust,
            ..Default::default()
        };

        let generator = RuleBasedGenerator::new();
        let processing_result = ProcessingResult {
            groups: vec![group],
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: ProcessingStats {
                input_entities: 3,
                ..ProcessingStats::default()
            },
        };

        let summary = generator.generate_with_groups_sync(&parsed_file, &processing_result);

        assert!(
            summary.main_entities.contains(&"OnceCell".to_string()),
            "summary should contain header entity 'OnceCell', got: {:?}",
            summary.main_entities
        );
        assert!(
            summary.main_entities.contains(&"get_mut".to_string()),
            "summary should contain member entity 'get_mut', got: {:?}",
            summary.main_entities
        );
        assert!(
            summary.main_entities.contains(&"get_or_init".to_string()),
            "summary should contain member entity 'get_or_init', got: {:?}",
            summary.main_entities
        );
    }
}

#[cfg(test)]
mod improvement_tests {
    use super::*;
    use crate::grouper::ProcessingStats;
    use crate::summary::strategy::ImportanceLevel;
    use cce_types::entity::GroupedEntity;
    use cce_types::{Entity, EntityId, EntityKind, Language, ParsedFile};

    fn group_with_header(
        kind: EntityKind,
        name: &str,
        signature: &str,
        doc: Option<&str>,
    ) -> crate::grouper::EntityGroup {
        let mut header =
            GroupedEntity::new(EntityId(0), kind, name.to_string(), signature.to_string());
        header.doc_comment = doc.map(|d| d.to_string());
        crate::grouper::EntityGroup {
            group_id: compact_str::CompactString::new("g"),
            name: compact_str::CompactString::new(name),
            kind,
            group_type: GroupType::Standalone,
            header: Some(header),
            members: Default::default(),
            language: Language::Rust,
            ..Default::default()
        }
    }

    fn processing_result(groups: Vec<crate::grouper::EntityGroup>) -> ProcessingResult {
        ProcessingResult {
            groups,
            entity_meta: std::collections::HashMap::new(),
            behavior: Default::default(),
            control_flow: Default::default(),
            stats: ProcessingStats::default(),
        }
    }

    #[test]
    fn test_structure_overview_uses_compact_signature() {
        let generator = RuleBasedGenerator::new();
        let pr = processing_result(vec![group_with_header(
            EntityKind::Struct,
            "OnceCell",
            "pub struct OnceCell<T>",
            None,
        )]);

        let overview = generator.generate_structure_overview(&pr);

        assert!(overview.contains("struct OnceCell<T>."), "got: {overview}");
        assert!(
            !overview.contains("pub"),
            "modifiers must be stripped: {overview}"
        );
    }

    #[test]
    fn test_structure_overview_falls_back_to_kind_name_without_signature() {
        let generator = RuleBasedGenerator::new();
        let pr = processing_result(vec![group_with_header(
            EntityKind::Enum,
            "Status",
            "",
            None,
        )]);

        let overview = generator.generate_structure_overview(&pr);

        assert!(overview.contains("enum Status."), "got: {overview}");
    }

    #[test]
    fn test_structure_overview_appends_header_doc_first_line() {
        let generator = RuleBasedGenerator::new();
        let pr = processing_result(vec![group_with_header(
            EntityKind::Struct,
            "OnceCell",
            "pub struct OnceCell<T>",
            Some("Thread-safe cell with lazy initialization.\n\nSee docs."),
        )]);

        let overview = generator.generate_structure_overview(&pr);

        assert!(
            overview.contains("— Thread-safe cell with lazy initialization."),
            "header doc first line must be attached, got: {overview}"
        );
    }

    #[test]
    fn test_structure_overview_limits_documented_headers() {
        let mut config = SummaryConfig::rule_based();
        config.max_entities = 1;
        let generator = RuleBasedGenerator::with_config(config);
        let pr = processing_result(vec![
            group_with_header(EntityKind::Struct, "First", "", Some("First doc.")),
            group_with_header(EntityKind::Struct, "Second", "", Some("Second doc.")),
        ]);

        let overview = generator.generate_structure_overview(&pr);

        assert!(overview.contains("First doc"), "got: {overview}");
        assert!(
            !overview.contains("Second doc"),
            "only max_entities headers may carry doc, got: {overview}"
        );
    }

    #[test]
    fn test_doc_preview_respects_importance_budget() {
        let generator = RuleBasedGenerator::new();
        // Paragraphs (blank-line separated) survive clean_comment_content,
        // hard line breaks inside a paragraph are collapsed.
        let doc = Some("line1\n\nline2\n\nline3\n\nline4\n\nline5\n\nline6");

        let high = generator.doc_preview(doc, ImportanceLevel::High);
        assert_eq!(high, "line1 line2 line3 line4 line5");

        let low = generator.doc_preview(doc, ImportanceLevel::Low);
        assert_eq!(low, "line1");

        assert_eq!(generator.doc_preview(None, ImportanceLevel::High), "");
    }

    #[test]
    fn test_truncated_list_shows_remainder() {
        let items: Vec<String> = (0..12).map(|i| format!("mod_{i}")).collect();
        let text = RuleBasedGenerator::truncated_list("Uses", &items, 10);

        assert!(
            text.contains("mod_0, mod_1, mod_2, mod_3, mod_4, mod_5, mod_6, mod_7, mod_8, mod_9")
        );
        assert!(text.contains("(and 2 more)"), "got: {text}");
    }

    #[test]
    fn test_summary_text_keeps_doc_when_list_is_long() {
        let mut config = SummaryConfig::rule_based();
        config.max_summary_length = 60;
        let generator = RuleBasedGenerator::with_config(config);
        let mut file = ParsedFile::new(Language::Rust, "src/lib.rs".to_string(), "fn main() {}");
        file.add_entity(Entity::new(
            EntityId(0),
            EntityKind::Function,
            "main".to_string(),
            cce_types::Span::from_lines(1, 1),
        ));
        file.file_doc_comment = Some("Module overview.".to_string());
        let summary = FileSummary::new("src/lib.rs")
            .with_file_doc_comment(Some("Module overview.".to_string()))
            .with_imports((0..100).map(|i| format!("mod_{i}")).collect())
            .with_line_count(10)
            .with_importance_level(ImportanceLevel::Medium);

        let text = generator.generate_summary_text(&summary, &file);

        assert!(
            text.starts_with("Module overview."),
            "doc must lead the summary under budget pressure, got: {text}"
        );
        assert!(
            text.contains("(and"),
            "long dependency lists must be truncated, got: {text}"
        );
    }
}
