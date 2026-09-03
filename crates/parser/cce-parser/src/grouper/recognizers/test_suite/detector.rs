//! Core test suite detector
//!
//! Provides the main entry point for test detection, delegating to
//! language-specific implementations. Produces `TestInfo` markers (not
//! synthetic `EntityKind::TestSuite`/`TestCase` entities), keeping the marker
//! orthogonal to grouping and conversion logic.

use std::collections::HashMap;

use cce_types::entity::{Entity, EntityId};
use cce_types::language::Language;
use cce_types::test_info::TestInfo;

use crate::grouper::context::FileProcessingContext;

use super::languages;

/// Index of annotation-like entities (`attribute_item` in Rust, marker
/// annotations in Java/Kotlin, decorators in Python) sorted by source
/// position, enabling O(log n) adjacency lookup for entity spans.
pub struct AnnotationIndex {
    /// (start_byte, end_byte, name) sorted by start_byte.
    entries: Vec<(usize, usize, String)>,
}

impl AnnotationIndex {
    /// Build the index from all entities of the file.
    pub fn build(entities: &[Entity]) -> Self {
        let mut entries: Vec<(usize, usize, String)> = entities
            .iter()
            .filter(|e| e.kind.is_annotation_like())
            .map(|e| (e.span.start_byte, e.span.end_byte, e.name.clone()))
            .collect();
        entries.sort_by_key(|(start, _, _)| *start);
        Self { entries }
    }

    /// Collect the names of annotation nodes that directly precede `entity`
    /// in source, in source order.
    ///
    /// An annotation is "adjacent" when the gap between it (or the previously
    /// collected annotation) and the entity contains only whitespace and
    /// comments — no structural boundaries (`{`, `}`, `;`). This prevents
    /// parent-scope annotations (e.g. a struct's `#[derive]` leaking onto a
    /// field) from being attributed to the entity.
    pub fn adjacent_annotation_names(&self, entity: &Entity, source: &str) -> Vec<String> {
        const MAX_ANNOTATIONS: usize = 8;
        const MAX_GAP_BYTES: usize = 512;

        let mut names = Vec::new();
        let mut boundary = entity.span.start_byte;
        let mut cursor = self.last_before(boundary);

        while let Some((start, end, name)) = cursor {
            let end = *end;
            let start = *start;
            if end > boundary || boundary - end > MAX_GAP_BYTES {
                break;
            }
            let gap = &source[end..boundary.min(source.len())];
            if !is_clean_gap(gap) {
                break;
            }
            names.push(name.clone());
            boundary = start;
            if names.len() >= MAX_ANNOTATIONS {
                break;
            }
            cursor = self.last_before(boundary);
        }

        names.reverse();
        names
    }

    /// Find the annotation with the largest `start_byte < before`.
    fn last_before(&self, before: usize) -> Option<&(usize, usize, String)> {
        let idx = self
            .entries
            .partition_point(|(start, _, _)| *start < before);
        idx.checked_sub(1).map(|i| &self.entries[i])
    }
}

/// A gap between an annotation and the entity it precedes is clean when it
/// contains only whitespace and comment lines (line comments, doc comments,
/// block comments) with no structural boundaries or statements.
fn is_clean_gap(gap: &str) -> bool {
    let mut in_block_comment = false;
    for line in gap.lines() {
        let trimmed = line.trim_start();
        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with('*') {
            continue;
        }
        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }
        return false;
    }
    !in_block_comment
}

/// Test suite detector
///
/// Detects test entities across multiple languages using AST attribute
/// adjacency (highest priority) and constrained naming conventions.
pub struct TestSuiteDetector;

impl Default for TestSuiteDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl TestSuiteDetector {
    /// Create a new detector.
    pub fn new() -> Self {
        Self
    }

    /// Detect the `TestInfo` for every non-annotation entity of the file.
    ///
    /// This is the primary entry point used by the preprocessing pipeline.
    /// The returned map is keyed by `EntityId`; entities without any signal
    /// are omitted (callers treat a missing entry as `Unknown`).
    pub fn detect_test_info(&self, ctx: &FileProcessingContext) -> HashMap<EntityId, TestInfo> {
        let annotations = AnnotationIndex::build(ctx.entities);
        let source: &str = ctx.parsed_file.source.as_ref();
        let file_path = ctx.parsed_file.path.as_str();

        ctx.entities
            .iter()
            .filter(|e| !e.kind.is_annotation_like())
            .filter_map(|e| {
                let info = self.detect_entity(e, ctx.language(), file_path, &annotations, source);
                if info.is_unknown() {
                    None
                } else {
                    Some((e.id, info))
                }
            })
            .collect()
    }

    /// Detect the `TestInfo` for a single entity.
    ///
    /// Priority: AST attribute adjacency → constrained naming conventions.
    /// Returns `Unknown` when neither level produces a signal.
    pub fn detect_entity(
        &self,
        entity: &Entity,
        language: &Language,
        file_path: &str,
        annotations: &AnnotationIndex,
        source: &str,
    ) -> TestInfo {
        // Entities already classified as test kinds by the AST extractor
        // (e.g. Rust `#[test]` functions promoted to `TestCase`).
        if entity.kind.is_test_entity() {
            return TestInfo::test_ast();
        }
        // Attributes preserved by the parser as entity metadata (e.g. Rust
        // `#[cfg(test)]` before `mod tests`). Consumed here with the same
        // annotation semantics as Level-1 detection.
        if let Some(attrs) = entity.metadata.get("test_annotations") {
            let names: Vec<String> = attrs
                .split(", ")
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            if let Some(info) = languages::detect_from_annotations(entity, language, &names, source)
            {
                return info;
            }
        }
        let adjacent = annotations.adjacent_annotation_names(entity, source);
        if let Some(info) = languages::detect_from_annotations(entity, language, &adjacent, source)
        {
            return info;
        }
        if let Some(info) = languages::detect_conventional(entity, language, file_path, source) {
            return info;
        }
        TestInfo::unknown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::EntityKind;

    fn annotation(name: &str, start: usize, end: usize) -> Entity {
        let mut e = Entity::new(
            EntityId(0),
            EntityKind::Annotation,
            name.to_string(),
            Span::default(),
        );
        e.span = Span {
            start_byte: start,
            end_byte: end,
            ..Span::default()
        };
        e
    }

    fn fn_entity(id: u64, name: &str, start: usize) -> Entity {
        let mut e = Entity::new(
            EntityId(id),
            EntityKind::Function,
            name.to_string(),
            Span::default(),
        );
        e.span = Span {
            start_byte: start,
            end_byte: start + 10,
            ..Span::default()
        };
        e
    }

    #[test]
    fn test_adjacent_annotations_basic() {
        let source = "#[test]\nfn test_login() {}";
        let entities = vec![annotation("test", 0, 7), fn_entity(1, "test_login", 8)];
        let index = AnnotationIndex::build(&entities);
        let names = index.adjacent_annotation_names(&entities[1], source);
        assert_eq!(names, vec!["test".to_string()]);
    }

    #[test]
    fn test_adjacent_annotations_multiple() {
        let source = "#[cfg(test)]\n#[allow(dead_code)]\nmod tests {}";
        let entities = vec![
            annotation("cfg(test)", 0, 12),
            annotation("allow(dead_code)", 13, 32),
            fn_entity(1, "tests", 33),
        ];
        let index = AnnotationIndex::build(&entities);
        let names = index.adjacent_annotation_names(&entities[2], source);
        assert_eq!(
            names,
            vec!["cfg(test)".to_string(), "allow(dead_code)".to_string()]
        );
    }

    #[test]
    fn test_annotations_not_adjacent_across_scope_boundary() {
        // `#[derive]` on the struct must never be attributed to a field: the
        // gap contains `struct Foo {` (a structural boundary).
        let source = "#[derive(Debug)]\nstruct Foo {\n    x: i32,\n}";
        let derive_end = source.find("]").unwrap() + 1;
        let field_start = source.find("x: i32").unwrap();
        let entities = vec![
            annotation("derive(Debug)", 0, derive_end),
            fn_entity(1, "x", field_start),
        ];
        let index = AnnotationIndex::build(&entities);
        let names = index.adjacent_annotation_names(&entities[1], source);
        assert!(names.is_empty());
    }

    #[test]
    fn test_annotations_across_statement_boundary_rejected() {
        // `;` in the gap means the annotation is not adjacent.
        let source = "#[test]\nfn a() {}\n#[test]\nfn b() {}";
        let entities = vec![
            annotation("test", 0, 7),
            fn_entity(1, "a", 8),
            annotation("test", 18, 25),
            fn_entity(2, "b", 26),
        ];
        let index = AnnotationIndex::build(&entities);
        let names = index.adjacent_annotation_names(&entities[3], source);
        assert_eq!(names, vec!["test".to_string()]);
    }

    #[test]
    fn test_adjacent_annotations_with_doc_comment() {
        let source = "#[test]\n/// doc comment\nfn test_login() {}";
        let entities = vec![annotation("test", 0, 7), fn_entity(1, "test_login", 24)];
        let index = AnnotationIndex::build(&entities);
        let names = index.adjacent_annotation_names(&entities[1], source);
        assert_eq!(names, vec!["test".to_string()]);
    }

    #[test]
    fn test_detect_test_info_rust() {
        use crate::grouper::context::FileProcessingContext;
        use cce_config::NestProcessorConfig;
        use cce_types::entity::ParsedFile;

        let source =
            "#[cfg(test)]\nmod tests {\n    #[test]\n    fn test_a() {}\n}\nfn latest() {}";
        let parsed = ParsedFile::new(Language::Rust, "src/lib.rs".to_string(), source);
        let cfg_end = source.find("]").unwrap() + 1;
        let mod_start = source.find("mod tests").unwrap();
        let test_attr_start = source.rfind("#[test]").unwrap();
        let test_attr_end = test_attr_start + 7;
        let test_fn_start = source.find("fn test_a").unwrap();
        let latest_start = source.find("fn latest").unwrap();
        let mut entities = vec![
            annotation("cfg(test)", 0, cfg_end),
            fn_entity(1, "tests", mod_start),
            annotation("test", test_attr_start, test_attr_end),
            fn_entity(2, "test_a", test_fn_start),
            fn_entity(3, "latest", latest_start),
        ];
        entities[1].kind = EntityKind::Module;
        let parsed = ParsedFile {
            entities: entities.clone(),
            ..parsed
        };
        let config = NestProcessorConfig::default();
        let ctx = FileProcessingContext::new(&entities, &parsed, &config);
        let detector = TestSuiteDetector::new();
        let infos = detector.detect_test_info(&ctx);

        let module = infos.get(&EntityId(1)).expect("module should be detected");
        assert!(module.is_test());
        assert_eq!(module.source, cce_types::test_info::TestSource::Ast);

        let test_fn = infos.get(&EntityId(2)).expect("test fn should be detected");
        assert!(test_fn.is_test());

        // `latest` must never be misjudged by name matching
        assert!(!infos.contains_key(&EntityId(3)));
    }

    #[test]
    fn test_detect_from_metadata_annotations() {
        use crate::grouper::context::FileProcessingContext;
        use cce_config::NestProcessorConfig;
        use cce_types::entity::ParsedFile;

        // `#[cfg(test)]` attributes are preserved on the module entity as
        // metadata by the AST extractor; the detector must consume them.
        let source = "#[cfg(test)]\nmod tests {\n    fn helper() {}\n}";
        let mut entity = fn_entity(1, "tests", source.find("mod tests").unwrap());
        entity.kind = EntityKind::Module;
        entity.set_metadata("test_annotations", "cfg(test)");
        let parsed = ParsedFile::new(Language::Rust, "src/lib.rs".to_string(), source);
        let entities = [entity.clone()];
        let config = NestProcessorConfig::default();
        let ctx = FileProcessingContext::new(&entities, &parsed, &config);
        let detector = TestSuiteDetector::new();
        let infos = detector.detect_test_info(&ctx);

        let module = infos.get(&EntityId(1)).expect("module should be detected");
        assert!(module.is_test());
        assert_eq!(module.source, cce_types::test_info::TestSource::Ast);
    }

    #[test]
    fn test_test_case_kind_detected_without_annotations() {
        use crate::grouper::context::FileProcessingContext;
        use cce_config::NestProcessorConfig;
        use cce_types::entity::ParsedFile;

        // Entities promoted to `TestCase` by the extractor (from `#[test]`)
        // are test even when no annotation metadata is attached.
        let source = "fn smoke_once() {}";
        let mut entity = fn_entity(1, "smoke_once", 0);
        entity.kind = EntityKind::TestCase;
        let parsed = ParsedFile::new(Language::Rust, "src/lib.rs".to_string(), source);
        let entities = [entity.clone()];
        let config = NestProcessorConfig::default();
        let ctx = FileProcessingContext::new(&entities, &parsed, &config);
        let detector = TestSuiteDetector::new();
        let infos = detector.detect_test_info(&ctx);

        let case = infos
            .get(&EntityId(1))
            .expect("test case should be detected");
        assert!(case.is_test());
        assert_eq!(case.source, cce_types::test_info::TestSource::Ast);
    }

    #[test]
    fn test_detect_conventional_go() {
        use crate::grouper::context::FileProcessingContext;
        use cce_config::NestProcessorConfig;
        use cce_types::entity::ParsedFile;

        let source = "func TestUser() {}\nfunc helper() {}";
        let parsed = ParsedFile::new(Language::Go, "user_test.go".to_string(), source);
        let entities = vec![fn_entity(1, "TestUser", 0), fn_entity(2, "helper", 18)];
        let parsed = ParsedFile {
            entities: entities.clone(),
            ..parsed
        };
        let config = NestProcessorConfig::default();
        let ctx = FileProcessingContext::new(&entities, &parsed, &config);
        let detector = TestSuiteDetector::new();
        let infos = detector.detect_test_info(&ctx);

        assert!(infos.get(&EntityId(1)).is_some_and(|i| i.is_test()));
        assert!(!infos.contains_key(&EntityId(2)));
    }
}
