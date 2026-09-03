//! File category enumeration (cross-layer contract)
//!
//! Moved from `cce_parser::summary::strategy::categorization` so the plugin
//! chunk contract (`cce_core::types::ast_to_nl::ChunkedResult`) can reference
//! it without depending on the parser crate. The string-based detection
//! helpers moved with it; the `ParsedFile`-centric helpers that require
//! parser-internal logic remain in the parser crate.

use serde::{Deserialize, Serialize};

use crate::types::{FileType, LanguageInfo, ParsedFile, TestInfo};

/// File category: mutually exclusive content type (business-layer classification).
///
/// This is the business-layer category used for summary generation, importance
/// scoring and query filtering. It is distinct from the routing-layer
/// [`crate::types::FileType`] (which decides the processing pipeline) and the
/// chunk-payload layer
/// [`crate::types::ast_to_nl::ChunkContentType`](crate::types::ast_to_nl::ChunkContentType).
///
/// Orthogonal properties (test files, auto-generated files) are tracked
/// separately: test files via `TestInfo` markers, generated files via
/// [`FileCategory::is_generated_file`] (routing only, never a category value).
///
/// The variant discriminant IS the storage encoding: the u8 value is what
/// gets persisted in Qdrant payloads, BM25 numeric fields and SQLite columns.
/// Only append new variants; never reorder or reuse existing codes.
/// Any change to the variant set must bump
/// [`crate::INDEX_FORMAT_VERSION`] and trigger a full rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum FileCategory {
    /// Source code files (AST-parsed implementation/header sources)
    #[default]
    Code = 0,
    /// Configuration files
    Config = 1,
    /// Documentation files
    Documentation = 2,
    /// Schema definition files (proto, graphql, etc.)
    Schema = 3,
    /// Generic text content without dedicated semantics: logs, `.txt`,
    /// unknown extensions. Never reported as `Code`.
    Other = 4,
}

impl Serialize for FileCategory {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.as_u8())
    }
}

impl<'de> Deserialize<'de> for FileCategory {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = u8::deserialize(deserializer)?;
        Self::from_u8(code)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown FileCategory code: {code}")))
    }
}

impl FileCategory {
    /// Storage encoding of this category.
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Decode a storage-encoded category. Returns `None` for unknown codes.
    #[inline]
    pub const fn from_u8(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Code),
            1 => Some(Self::Config),
            2 => Some(Self::Documentation),
            3 => Some(Self::Schema),
            4 => Some(Self::Other),
            _ => None,
        }
    }

    /// Map a routing-layer [`FileType`] to a business-layer [`FileCategory`].
    ///
    /// This is the single source of truth for the `FileType` → `FileCategory`
    /// conversion, ensuring routing and business classifications stay
    /// consistent on the routing decision. `Source`/`Header` map to `Code`;
    /// generic text and unknown extensions map to [`FileCategory::Other`]
    /// so unclassified text never pollutes the code category.
    pub fn from_file_type(file_type: FileType) -> Self {
        match file_type {
            FileType::Source | FileType::Header => FileCategory::Code,
            FileType::Schema => FileCategory::Schema,
            FileType::Config => FileCategory::Config,
            FileType::Documentation => FileCategory::Documentation,
            FileType::Text => FileCategory::Other,
        }
    }

    /// Determine file category from parsed file.
    ///
    /// Pure delegation to the unified detection chain:
    /// [`LanguageInfo::detect_from_path`] → [`Self::from_file_type`].
    pub fn determine(parsed_file: &ParsedFile) -> Self {
        Self::determine_from_path(&parsed_file.path)
    }

    /// Determine file category from the path alone.
    ///
    /// The only non-delegate classification entry: everything funnels through
    /// [`LanguageInfo::detect_from_path`] (routing layer) and then converts via
    /// [`Self::from_file_type`]. Schema files carry their own
    /// [`FileType::Schema`] variant at the routing layer, so no override is
    /// needed here and every derivation entry agrees by construction.
    pub fn determine_from_path(path: &str) -> Self {
        Self::from_file_type(LanguageInfo::detect_from_path(path).file_type)
    }

    /// Decode a category name. Returns `None` for unknown names.
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "code" => Some(Self::Code),
            "config" => Some(Self::Config),
            "documentation" => Some(Self::Documentation),
            "schema" => Some(Self::Schema),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    /// Get category name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            FileCategory::Code => "code",
            FileCategory::Config => "config",
            FileCategory::Documentation => "documentation",
            FileCategory::Schema => "schema",
            FileCategory::Other => "other",
        }
    }

    /// Whether the file should be handled by the specialized summary
    /// generators (test/config/documentation/schema/generated files).
    ///
    /// Test and generated markers are orthogonal to the content type, so
    /// they are detected independently here.
    pub fn is_specialized_file(parsed_file: &ParsedFile) -> bool {
        Self::is_test_file(&parsed_file.path)
            || Self::is_generated_file(&parsed_file.path, &parsed_file.source)
            || Self::determine(parsed_file) != Self::Code
    }

    /// Whether the file should skip model-enhanced summary generation
    /// (test/config/documentation/generated files). Logs and other generic
    /// text ([`FileCategory::Other`]) carry no summary-enhancement value and
    /// are skipped as well. Schema files remain eligible for model
    /// enhancement.
    pub fn should_skip_model_enhancement(parsed_file: &ParsedFile) -> bool {
        Self::is_test_file(&parsed_file.path)
            || Self::is_generated_file(&parsed_file.path, &parsed_file.source)
            || matches!(
                Self::determine(parsed_file),
                Self::Config | Self::Documentation | Self::Other
            )
    }

    /// Check if file is a test file.
    ///
    /// Delegates to the per-language file-path rules of
    /// [`TestInfo::from_path`] (single source of truth for path-based test
    /// determination). The language is inferred from the file extension;
    /// unrecognized extensions fall back to the generic `tests/` segment
    /// rule.
    pub fn is_test_file(path: &str) -> bool {
        let language = LanguageInfo::detect_from_path(path).language;
        TestInfo::from_path(Some(&language), path).is_test()
    }

    /// Display-tag heuristic: whether a path looks like a config file.
    ///
    /// Uses directory segments, exact file names, and extension rules
    /// instead of substring matching to avoid false positives
    /// (e.g. `settings_page.rs`, `config_backup.rs`). All string parsing
    /// goes through `cce_core::utils::path` so `\` and `/` separators are
    /// handled identically.
    ///
    /// This is a *display-only* heuristic used for summary tags and
    /// presentation; it never decides the stored category or the processing
    /// route (those come from [`FileCategory::from_file_type`] on the
    /// routing layer's decision).
    pub fn looks_like_config(path: &str) -> bool {
        use crate::path::{file_name_str, is_build_config_name_lower, segments};

        let path_lower = path.to_lowercase();
        let file_name = file_name_str(&path_lower);

        // Directory segment: files under a config/ directory
        let in_config_dir = segments(&path_lower)
            .iter()
            .any(|seg| *seg == "config" || *seg == ".config");

        // Exact file names (with optional extension variants), plus the
        // canonical build config file name rule set
        let named_config = file_name == "config"
            || file_name.starts_with("config.")
            || file_name.starts_with(".config.")
            || file_name == "settings"
            || file_name.starts_with("settings.")
            || file_name == ".env"
            || file_name == "dockerfile"
            || file_name.starts_with("dockerfile.")
            || file_name == "makefile"
            || file_name == "gnumakefile"
            || file_name.starts_with("tsconfig.")
            || is_build_config_name_lower(file_name);

        // Config-ish extensions
        let config_ext = path_lower.ends_with(".yaml")
            || path_lower.ends_with(".yml")
            || path_lower.ends_with(".toml");

        in_config_dir || named_config || config_ext
    }

    /// Display-tag heuristic: whether a path looks like documentation.
    ///
    /// Directory segments (`docs/`), extension rules and the shared
    /// well-known extensionless doc-name list
    /// ([`crate::path::EXTENSIONLESS_DOC_NAMES`]) feed this tag.
    ///
    /// Like [`Self::looks_like_config`], this is display-only: summary tags
    /// and importance hints, never the stored category or pipeline choice.
    pub fn looks_like_documentation(path: &str) -> bool {
        use crate::path::{file_name_str, is_extensionless_doc_name, segments};

        let path_lower = path.to_lowercase();
        let file_name = file_name_str(&path_lower);

        // Exact doc file names (README.md, LICENSE, CHANGELOG, COPYING,
        // AUTHORS, NOTICE, ...) share the canonical extensionless list.
        let named_doc = crate::path::EXTENSIONLESS_DOC_NAMES
            .iter()
            .any(|&name| file_name.starts_with(&format!("{name}.")))
            || is_extensionless_doc_name(file_name);

        // Directory segment: files under a docs/ directory
        let in_docs_dir = segments(&path_lower).contains(&"docs");

        let doc_exts = [".md", ".rst", ".txt", ".adoc"];
        named_doc || in_docs_dir || doc_exts.iter().any(|&ext| path_lower.ends_with(ext))
    }

    /// Check if file is a core module (main, lib, mod, index)
    ///
    /// Matches on the exact file stem (lowercased) instead of substring
    /// matching, so `library.rs`, `cmd_indexer.rs` or `imodal.rs` are never
    /// misclassified as core modules.
    pub fn is_core_module(path: &str) -> bool {
        const CORE_MODULE_STEMS: [&str; 6] =
            ["main", "lib", "mod", "index", "__init__", "__main__"];
        let stem = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        CORE_MODULE_STEMS.contains(&stem.to_lowercase().as_str())
    }

    /// Check if file is auto-generated by scanning the first 10 source lines
    /// for common generation markers.
    ///
    /// Heuristic: recognizes `//` comments, `@generated` and the `#` /
    /// `<!--`-style markers (Python, Shell, XML, HTML). A markdown heading
    /// like `# generated by ...` may therefore match; this is acceptable
    /// because the result is used for routing decisions only (rule-based
    /// summaries, importance), never as a category value.
    pub fn is_generated_file(_path: &str, source: &str) -> bool {
        let generation_markers = [
            "// code generated by",
            "// generated by",
            "// auto-generated",
            "// do not edit",
            "@generated",
            "# code generated by",
            "# generated by",
            "# auto-generated",
            "# do not edit",
            "<!-- generated by",
            "<!-- @generated",
            "<!-- auto-generated",
            "<!-- do not edit",
        ];
        source.lines().take(10).any(|line| {
            generation_markers
                .iter()
                .any(|m| line.to_lowercase().contains(m))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Path-based classification must agree across representative shapes:
    /// code, documentation, config (including build files without a config
    /// extension) and unknown extensions falling back to `Other`.
    #[test]
    fn test_determine_from_path_classification() {
        assert_eq!(
            FileCategory::determine_from_path("src/main.rs"),
            FileCategory::Code
        );
        assert_eq!(
            FileCategory::determine_from_path("README.md"),
            FileCategory::Documentation
        );
        assert_eq!(
            FileCategory::determine_from_path("Cargo.toml"),
            FileCategory::Config
        );
        assert_eq!(
            FileCategory::determine_from_path("Makefile"),
            FileCategory::Config
        );
        // Unknown extensions and plain text stay out of the code category.
        for path in ["image.png", "notes.txt", "run.log"] {
            assert_eq!(
                FileCategory::determine_from_path(path),
                FileCategory::Other,
                "{path} must classify as Other"
            );
        }
    }

    /// Schema files are classified through the routing layer's dedicated
    /// `FileType::Schema` variant — no override needed.
    #[test]
    fn test_determine_from_path_schema_via_routing_layer() {
        for path in [
            "api.proto",
            "schema.graphql",
            "service.thrift",
            "types.avsc",
        ] {
            assert_eq!(
                FileCategory::determine_from_path(path),
                FileCategory::Schema,
                "{path} must classify as Schema"
            );
        }
    }

    /// Every category round-trips through its storage encoding, and both
    /// derivation entries (`from_file_type` / `chunk_content_type`) agree.
    #[test]
    fn test_category_encoding_roundtrip() {
        for code in 0u8..=4 {
            let category = FileCategory::from_u8(code).expect("known encoding");
            assert_eq!(category.as_u8(), code);
        }
        assert!(FileCategory::from_u8(5).is_none());
        assert_eq!(FileCategory::from_name("other"), Some(FileCategory::Other));
    }
}
