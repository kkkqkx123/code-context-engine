//! Shared imports/exports collection for file-level summaries
//!
//! Import-like entities (import/require/include/export) are separated from
//! retrieval chunks at the entity level: import-only groups are dropped
//! before conversion/chunking, and the small-fragment merger never mixes
//! them with other entities. This module is their single retrieval surface
//! besides the relation index — every summary generator collects ALL
//! imports, exports and import-associated comments through these helpers.

use cce_types::ParsedFile;
use cce_utils::normalize_whitespace;

/// Collect all standardized import sources of a file.
///
/// Uses the cached `import_table` on `ParsedFile` to avoid re-parsing the
/// AST; falls back to AST parsing only when no cached table is available.
pub fn collect_imports(parsed_file: &ParsedFile) -> Vec<String> {
    let import_table = if let Some(ref cached) = parsed_file.import_table {
        cached.clone()
    } else {
        use crate::parser::ast_parser::AstParser;
        let mut parser = AstParser::new();
        let tree = parser
            .parse_with_tree(&parsed_file.source, &parsed_file.language)
            .ok()
            .map(|(t, _)| t);
        if let Some(ref tree) = tree {
            crate::relation_helpers::extract_imports(
                tree,
                &parsed_file.source,
                &parsed_file.language,
                None,
            )
            .unwrap_or_default()
        } else {
            cce_types::ImportTable::default()
        }
    };

    let mut imports: Vec<String> = import_table
        .all_standardized_imports()
        .iter()
        .map(|i| i.source.clone())
        .collect();
    imports.sort();
    imports.dedup();
    imports
}

/// Collect all exported symbol names of a file.
pub fn collect_exports(parsed_file: &ParsedFile) -> Vec<String> {
    let mut exports: Vec<String> = crate::relation_helpers::extract_exports_from_entities(
        &parsed_file.entities,
        &parsed_file.language,
    )
    .iter()
    .map(|e| e.function_name.clone())
    .collect();
    exports.sort();
    exports.dedup();
    exports
}

/// Collect doc comments attached to import-like entities
/// (import/require/include/export).
///
/// Import-only groups are dropped before chunking, so these comments would
/// otherwise be lost from every retrieval surface; the file-level summary
/// carries them instead.
pub fn collect_import_comments(parsed_file: &ParsedFile) -> Vec<String> {
    let mut comments: Vec<String> = parsed_file
        .entities
        .iter()
        .filter(|e| e.kind.is_import_like())
        .filter_map(|e| e.doc_comment.clone())
        .map(|c| normalize_whitespace(&crate::ast_to_nl::clean_comment_content(&c)))
        .filter(|c| !c.trim().is_empty())
        .collect();
    comments.sort();
    comments.dedup();
    comments
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::entity::{Entity, EntityKind};
    use cce_types::{Span, StandardizedImport};

    #[test]
    fn test_collect_import_comments_empty() {
        let parsed = ParsedFile::new(
            cce_types::Language::Rust,
            "fn run() {}".to_string(),
            "src/lib.rs",
        );
        assert!(collect_import_comments(&parsed).is_empty());
    }

    #[test]
    fn test_collect_imports_dedup_sorted_from_cached_table() {
        // The summary collects ALL imports of a file (deduplicated
        // and sorted), using the cached ImportTable to avoid re-parsing.
        let mut parsed = ParsedFile::new(
            cce_types::Language::Rust,
            "use zeta; use alpha; use zeta;".to_string(),
            "src/lib.rs",
        );
        let mut table = cce_types::ImportTable::default();
        table.add_standardized_import(StandardizedImport::new(
            cce_types::ImportKind::ModuleImport,
            "zeta",
        ));
        table.add_standardized_import(StandardizedImport::new(
            cce_types::ImportKind::ModuleImport,
            "alpha",
        ));
        table.add_standardized_import(StandardizedImport::new(
            cce_types::ImportKind::ModuleImport,
            "zeta",
        ));
        parsed.import_table = Some(table);

        assert_eq!(
            collect_imports(&parsed),
            vec!["alpha".to_string(), "zeta".to_string()],
            "imports must be deduplicated and sorted"
        );
    }

    #[test]
    fn test_collect_exports_dedup_sorted() {
        // The summary collects ALL exported symbols of a file so
        // the file-level description carries every public surface.
        let mut parsed = ParsedFile::new(cce_types::Language::Rust, "".to_string(), "src/lib.rs");
        let mut public = Entity::new(
            cce_types::entity::EntityId(1),
            EntityKind::Function,
            "run".to_string(),
            Span::new(0, 10, 0, 0, 0, 10),
        );
        public
            .metadata
            .insert("visibility".to_string(), "pub".to_string());
        let mut private = Entity::new(
            cce_types::entity::EntityId(2),
            EntityKind::Function,
            "helper".to_string(),
            Span::new(11, 21, 1, 0, 1, 10),
        );
        private
            .metadata
            .insert("visibility".to_string(), "private".to_string());
        parsed.entities = vec![public.clone(), private.clone(), public];

        let exports = collect_exports(&parsed);
        assert_eq!(
            exports,
            vec!["run".to_string()],
            "only public top-level entities are exports, deduplicated"
        );
    }

    #[test]
    fn test_collect_import_comments_only_import_like() {
        let mut parsed = ParsedFile::new(
            cce_types::Language::Rust,
            "use std::fmt;".to_string(),
            "src/lib.rs",
        );
        parsed.entities = vec![
            Entity::new(
                cce_types::entity::EntityId(1),
                EntityKind::Import,
                "use std::fmt;".to_string(),
                Span::new(0, 14, 0, 0, 0, 14),
            ),
            Entity::new(
                cce_types::entity::EntityId(2),
                EntityKind::Function,
                "run".to_string(),
                Span::new(16, 30, 1, 0, 1, 14),
            ),
        ];
        parsed.entities[0].doc_comment = Some("Re-export fmt for tests".to_string());
        parsed.entities[1].doc_comment = Some("Not an import comment".to_string());

        let comments = collect_import_comments(&parsed);
        assert_eq!(comments, vec!["Re-export fmt for tests".to_string()]);
    }
}
