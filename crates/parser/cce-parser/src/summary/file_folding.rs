//! File folding module for code compression before LLM processing
//!
//! This module provides functionality to fold code files into minimal representations
//! by extracting only essential definitions (classes, functions, types) and discarding
//! implementation details. This significantly reduces token usage for LLM calls while
//! preserving semantic structure for file summary generation.
//!
//! # Architecture
//!
//! ```text
//! ParsedFile → FileFolder → FoldedContent
//!     ↓                           ↓
//!   Entities              Compressed Text
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use cce_parser::summary::FileFolder;
//! use cce_types::ParsedFile;
//!
//! # fn example(parsed_file: &ParsedFile) {
//! let folder = FileFolder::new()
//!     .with_max_tokens(2000)
//!     .with_merge_functions(true);
//!
//! let folded = folder.fold(parsed_file);
//! println!("Folded content ({} tokens): {}", folded.estimated_tokens, folded.content);
//! # }
//! ```

use cce_types::{Entity, EntityKind, ParsedFile};
use cce_utils::token_estimation::estimate_tokens;

/// Represents a folded section of code
#[derive(Debug, Clone)]
pub struct FoldedSection {
    /// Type of this section
    pub section_type: SectionType,
    /// Names in this section
    pub names: Vec<String>,
    /// Start line number
    pub start_line: usize,
    /// End line number (for single-item sections)
    pub end_line: Option<usize>,
    /// Full signature (for detailed mode)
    pub signature: Option<String>,
}

/// Section type for folded content
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionType {
    /// Class definition
    Class,
    /// Struct definition
    Struct,
    /// Interface/trait definition
    Interface,
    /// Enum definition
    Enum,
    /// Type alias definition
    TypeAlias,
    /// Function group (merged functions)
    Functions,
    /// Module/namespace
    Module,
}

impl SectionType {
    /// Get display name for the section type
    pub fn as_str(&self) -> &'static str {
        match self {
            SectionType::Class => "class",
            SectionType::Struct => "struct",
            SectionType::Interface => "interface",
            SectionType::Enum => "enum",
            SectionType::TypeAlias => "type",
            SectionType::Functions => "functions",
            SectionType::Module => "module",
        }
    }
}

/// Folded content result
#[derive(Debug, Clone)]
pub struct FoldedContent {
    /// The folded text content
    pub content: String,
    /// Sections included in the folded content
    pub sections: Vec<FoldedSection>,
    /// Estimated token count
    pub estimated_tokens: usize,
    /// Number of sections dropped due to token limit
    pub sections_dropped: usize,
}

impl FoldedContent {
    /// Create new folded content
    pub fn new(content: String, sections: Vec<FoldedSection>) -> Self {
        // Use precise token estimation supporting mixed-language text
        let estimated_tokens = estimate_tokens(&content);

        Self {
            content,
            sections,
            estimated_tokens,
            sections_dropped: 0,
        }
    }

    /// Create empty folded content
    pub fn empty() -> Self {
        Self {
            content: String::new(),
            sections: Vec::new(),
            estimated_tokens: 0,
            sections_dropped: 0,
        }
    }

    /// Check if content is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
}

/// Configuration for file folding
#[derive(Debug, Clone)]
pub struct FileFoldingConfig {
    /// Maximum tokens allowed (default: 2000)
    pub max_tokens: usize,
    /// Whether to merge adjacent functions (default: true)
    pub merge_functions: bool,
    /// Maximum line span for function merging (default: 100)
    pub max_line_span: usize,
    /// Format mode: detailed (with signatures) or minimal (names only)
    pub mode: FoldMode,
    /// Whether to include imports in folded content
    pub include_imports: bool,
}

impl Default for FileFoldingConfig {
    fn default() -> Self {
        Self {
            max_tokens: 2000,
            merge_functions: true,
            max_line_span: 100,
            mode: FoldMode::Detailed,
            include_imports: true,
        }
    }
}

/// Folding mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldMode {
    /// Detailed mode: include full signatures
    Detailed,
    /// Minimal mode: names only
    Minimal,
}

/// File folder for compressing code files
#[derive(Debug, Clone)]
pub struct FileFolder {
    config: FileFoldingConfig,
}

impl FileFolder {
    /// Create a new file folder with default config
    pub fn new() -> Self {
        Self {
            config: FileFoldingConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: FileFoldingConfig) -> Self {
        Self { config }
    }

    /// Set maximum tokens
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.config.max_tokens = max_tokens;
        self
    }

    /// Set function merging
    pub fn with_merge_functions(mut self, merge: bool) -> Self {
        self.config.merge_functions = merge;
        self
    }

    /// Set fold mode
    pub fn with_mode(mut self, mode: FoldMode) -> Self {
        self.config.mode = mode;
        self
    }

    /// Fold a parsed file into minimal representation
    pub fn fold(&self, parsed_file: &ParsedFile) -> FoldedContent {
        // Extract foldable entities (top-level only, skip variables/fields)
        let entities = self.extract_foldable_entities(parsed_file);

        if entities.is_empty() {
            return FoldedContent::empty();
        }

        // Build sections from entities
        let sections = self.build_sections(&entities);

        // Format content with the original file prefix so token limiting can rebuild it
        let prefix_lines = self.build_prefix_lines(parsed_file);
        let section_lines = self.format_section_lines(&sections);
        let content = Self::assemble_content(&prefix_lines, &section_lines);

        let mut folded = FoldedContent::new(content, sections);

        // Apply token limit if exceeded
        if folded.estimated_tokens > self.config.max_tokens {
            folded = self.apply_token_limit(folded, &prefix_lines);
        }

        folded
    }

    /// Extract entities that should be included in folding
    fn extract_foldable_entities<'a>(&self, parsed_file: &'a ParsedFile) -> Vec<&'a Entity> {
        parsed_file
            .entities
            .iter()
            .filter(|e| {
                // Only top-level entities
                e.is_top_level()
                    && match e.kind {
                        // Include type definitions
                        EntityKind::Class
                        | EntityKind::Struct
                        | EntityKind::Enum
                        | EntityKind::Interface
                        | EntityKind::Trait
                        | EntityKind::TraitImpl
                        | EntityKind::InherentImpl
                        | EntityKind::TypeAlias
                        // Include functions
                        | EntityKind::Function
                        | EntityKind::Method
                        | EntityKind::Constructor
                        // Include modules
                        | EntityKind::Module
                        | EntityKind::Namespace => true,
                        // Skip variables, fields, parameters
                        _ => false,
                    }
            })
            .collect()
    }

    /// Build folded sections from entities
    fn build_sections(&self, entities: &[&Entity]) -> Vec<FoldedSection> {
        let mut sections: Vec<FoldedSection> = Vec::new();

        if self.config.merge_functions {
            // Group consecutive functions
            let mut current_functions: Vec<&Entity> = Vec::new();
            let mut function_start_line: Option<usize> = None;

            for entity in entities {
                let line = entity.span.start_position.row;

                if entity.kind.is_function_like() {
                    // Check if we should merge with previous functions
                    if let Some(start_line) = function_start_line {
                        let span = line.saturating_sub(start_line);
                        if span > self.config.max_line_span && !current_functions.is_empty() {
                            // Flush current function group
                            sections.push(self.create_function_section(&current_functions));
                            current_functions.clear();
                        }
                    }

                    current_functions.push(entity);
                    if function_start_line.is_none() {
                        function_start_line = Some(line);
                    }
                } else {
                    // Flush pending functions first
                    if !current_functions.is_empty() {
                        sections.push(self.create_function_section(&current_functions));
                        current_functions.clear();
                        function_start_line = None;
                    }

                    // Add non-function section
                    sections.push(self.create_entity_section(entity));
                }
            }

            // Flush remaining functions
            if !current_functions.is_empty() {
                sections.push(self.create_function_section(&current_functions));
            }
        } else {
            // No merging: each entity is its own section
            for entity in entities {
                if entity.kind.is_function_like() {
                    sections.push(FoldedSection {
                        section_type: SectionType::Functions,
                        names: vec![entity.name.clone()],
                        start_line: entity.span.start_position.row,
                        end_line: Some(entity.span.end_position.row),
                        signature: if self.config.mode == FoldMode::Detailed {
                            Some(entity.signature.clone())
                        } else {
                            None
                        },
                    });
                } else {
                    sections.push(self.create_entity_section(entity));
                }
            }
        }

        sections
    }

    /// Create a section for a merged function group
    fn create_function_section(&self, functions: &[&Entity]) -> FoldedSection {
        let names: Vec<String> = functions.iter().map(|f| f.name.clone()).collect();
        let start_line = functions
            .first()
            .map(|f| f.span.start_position.row)
            .unwrap_or(0);

        // Collect signatures if in detailed mode
        let signature = if self.config.mode == FoldMode::Detailed {
            let sigs: Vec<String> = functions
                .iter()
                .map(|f| f.signature.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if sigs.is_empty() {
                None
            } else {
                Some(sigs.join(", "))
            }
        } else {
            None
        };

        FoldedSection {
            section_type: SectionType::Functions,
            names,
            start_line,
            end_line: functions.last().map(|f| f.span.end_position.row),
            signature,
        }
    }

    /// Create a section for a single entity
    fn create_entity_section(&self, entity: &Entity) -> FoldedSection {
        let section_type = match entity.kind {
            EntityKind::Class => SectionType::Class,
            EntityKind::Struct | EntityKind::InherentImpl => SectionType::Struct,
            EntityKind::Enum => SectionType::Enum,
            EntityKind::Interface | EntityKind::Trait | EntityKind::TraitImpl => {
                SectionType::Interface
            }
            EntityKind::TypeAlias => SectionType::TypeAlias,
            EntityKind::Module | EntityKind::Namespace => SectionType::Module,
            _ => SectionType::Functions,
        };

        let signature = if self.config.mode == FoldMode::Detailed {
            Some(entity.signature.trim().to_string())
        } else {
            None
        };

        FoldedSection {
            section_type,
            names: vec![entity.name.clone()],
            start_line: entity.span.start_position.row,
            end_line: Some(entity.span.end_position.row),
            signature,
        }
    }

    /// Build the non-section prefix reused by token limiting.
    fn build_prefix_lines(&self, parsed_file: &ParsedFile) -> Vec<String> {
        let mut lines = vec![
            format!("// File: {}", parsed_file.path),
            format!("// Language: {}", parsed_file.language),
        ];

        if self.config.include_imports {
            let imports = self.format_imports(parsed_file);
            if !imports.is_empty() {
                lines.push(String::new());
                lines.push("// Imports:".to_string());
                lines.extend(imports);
            }
        }

        lines
    }

    /// Format section lines for the current fold mode.
    fn format_section_lines(&self, sections: &[FoldedSection]) -> Vec<String> {
        sections
            .iter()
            .map(|section| self.format_section(section))
            .collect()
    }

    /// Assemble final folded text from the reusable prefix and kept sections.
    fn assemble_content(prefix_lines: &[String], section_lines: &[String]) -> String {
        let mut lines = prefix_lines.to_vec();
        lines.push(String::new());
        lines.push("// Definitions:".to_string());
        lines.extend(section_lines.iter().cloned());
        lines.join("\n")
    }

    /// Format imports from parsed file
    fn format_imports(&self, parsed_file: &ParsedFile) -> Vec<String> {
        let mut imports: Vec<String> = Vec::new();

        // Parse AST to extract imports
        use crate::parser::ast_parser::AstParser;
        let mut parser = AstParser::new();
        let tree = parser
            .parse_with_tree(&parsed_file.source, &parsed_file.language)
            .ok()
            .map(|(t, _)| t);

        let import_table = if let Some(ref tree) = tree {
            crate::relation_helpers::extract_imports(
                tree,
                &parsed_file.source,
                &parsed_file.language,
                None,
            )
            .unwrap_or_default()
        } else {
            cce_types::ImportTable::default()
        };

        // Use standardized imports
        use crate::parser::extractor::symbol_extractor::ImportKind;

        for imp in import_table.all_standardized_imports() {
            match imp.kind {
                ImportKind::SymbolImport => {
                    let name = imp.effective_name();
                    if !name.is_empty() {
                        imports.push(format!("//   use {}::{{{}}}", imp.source, name));
                    }
                }
                ImportKind::DefaultImport => {
                    let name = imp.effective_name();
                    if !name.is_empty() {
                        imports.push(format!("//   import {} from '{}'", name, imp.source));
                    }
                }
                ImportKind::NamespaceImport => {
                    imports.push(format!("//   use {}::*", imp.source));
                }
                _ => {}
            }
        }

        imports
    }

    /// Format a single section
    fn format_section(&self, section: &FoldedSection) -> String {
        match self.config.mode {
            FoldMode::Detailed => self.format_section_detailed(section),
            FoldMode::Minimal => self.format_section_minimal(section),
        }
    }

    /// Format section in detailed mode
    fn format_section_detailed(&self, section: &FoldedSection) -> String {
        if section.section_type == SectionType::Functions {
            // For function groups
            if let Some(ref sig) = section.signature {
                if section.names.len() == 1 {
                    format!("{} | {}", section.start_line, sig)
                } else {
                    format!(
                        "{} | [{}] {}",
                        section.start_line,
                        section.names.join(", "),
                        sig
                    )
                }
            } else {
                format!("{} | fn {}", section.start_line, section.names.join(", "))
            }
        } else {
            // For single-item sections
            let type_name = section.section_type.as_str();
            if let Some(ref sig) = section.signature {
                format!("{} | {}", section.start_line, sig)
            } else {
                format!(
                    "{} | {} {}",
                    section.start_line,
                    type_name,
                    section.names.join(", ")
                )
            }
        }
    }

    /// Format section in minimal mode
    fn format_section_minimal(&self, section: &FoldedSection) -> String {
        if section.section_type == SectionType::Functions {
            format!("{} | {}", section.start_line, section.names.join(", "))
        } else {
            format!(
                "{} | {} {}",
                section.start_line,
                section.section_type.as_str(),
                section.names.join(", ")
            )
        }
    }

    /// Apply token limit by intelligently dropping sections
    ///
    /// Strategy:
    /// 1. Keep imports and module sections first (usually most important for understanding structure)
    /// 2. Sort remaining sections by importance (types > functions, earlier in file = more important)
    /// 3. Drop from lowest importance first until the rebuilt content fits, or report no usable fold
    fn apply_token_limit(&self, folded: FoldedContent, prefix_lines: &[String]) -> FoldedContent {
        let target_tokens = self.config.max_tokens;

        if folded.estimated_tokens <= target_tokens {
            return folded;
        }

        let (import_sections, mut other_sections): (Vec<_>, Vec<_>) = folded
            .sections
            .into_iter()
            .partition(|section| matches!(section.section_type, SectionType::Module));

        fn section_importance(section: &FoldedSection) -> usize {
            let type_priority = match section.section_type {
                SectionType::Class | SectionType::Struct | SectionType::Interface => 100,
                SectionType::Enum | SectionType::TypeAlias => 90,
                SectionType::Module => 200,
                SectionType::Functions => 50,
            };

            let position_score = 10000_usize.saturating_sub(section.start_line);
            type_priority + position_score
        }

        other_sections.sort_by(|a, b| {
            section_importance(b)
                .cmp(&section_importance(a))
                .then_with(|| a.start_line.cmp(&b.start_line))
        });

        let mut kept_sections = import_sections;
        kept_sections.append(&mut other_sections);
        let original_count = kept_sections.len();

        loop {
            if kept_sections.is_empty() {
                return FoldedContent::empty();
            }

            let mut candidate = kept_sections.clone();
            candidate.sort_by_key(|section| section.start_line);
            let section_lines = self.format_section_lines(&candidate);
            let content = Self::assemble_content(prefix_lines, &section_lines);
            let estimated_tokens = estimate_tokens(&content);

            if estimated_tokens <= target_tokens {
                let dropped_sections = original_count - candidate.len();

                return FoldedContent {
                    content,
                    sections: candidate,
                    estimated_tokens,
                    sections_dropped: dropped_sections,
                };
            }

            kept_sections.pop();
        }
    }
}

impl Default for FileFolder {
    fn default() -> Self {
        Self::new()
    }
}

/// Quick fold function for convenience
pub fn fold_file(parsed_file: &ParsedFile, max_tokens: usize) -> FoldedContent {
    FileFolder::new()
        .with_max_tokens(max_tokens)
        .with_mode(FoldMode::Detailed)
        .fold(parsed_file)
}

/// Fold file with minimal mode (names only)
pub fn fold_file_minimal(parsed_file: &ParsedFile, max_tokens: usize) -> FoldedContent {
    FileFolder::new()
        .with_max_tokens(max_tokens)
        .with_mode(FoldMode::Minimal)
        .fold(parsed_file)
}

/// Check if folded content is short enough to include fully
///
/// Creates a `FileFolder` with minimal mode and `merge_functions=true`,
/// folds the file, and returns `true` if the result is compact enough
/// (low token count or very few sections).
pub fn is_folded_content_short(parsed_file: &ParsedFile, threshold_tokens: usize) -> bool {
    let folded = FileFolder::new()
        .with_max_tokens(threshold_tokens)
        .with_mode(FoldMode::Minimal)
        .with_merge_functions(true)
        .fold(parsed_file);

    !folded.is_empty() && (folded.estimated_tokens < threshold_tokens || folded.sections.len() <= 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{EntityId, Language};
    use cce_types::{Position, Span};

    fn create_test_entity(id: u32, kind: EntityKind, name: &str, line: u32) -> Entity {
        let line_usize = line as usize;
        Entity {
            id: EntityId(id.into()),
            kind,
            name: name.to_string(),
            signature: format!("fn {}()", name),
            parameters: Vec::new(),
            return_type: None,
            span: Span {
                start_byte: 0,
                end_byte: 0,
                start_position: Position {
                    row: line_usize,
                    column: 0,
                },
                end_position: Position {
                    row: line_usize + 5,
                    column: 0,
                },
            },
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
        }
    }

    fn create_test_parsed_file() -> ParsedFile {
        let mut file = ParsedFile::new(Language::Rust, "src/main.rs".to_string(), "fn main() {}");

        // Add some entities
        file.add_entity(create_test_entity(0, EntityKind::Function, "main", 1));
        file.add_entity(create_test_entity(1, EntityKind::Function, "helper", 10));
        file.add_entity(create_test_entity(2, EntityKind::Struct, "User", 25));

        file
    }

    #[test]
    fn test_file_folder_basic() {
        let folder = FileFolder::new();
        let file = create_test_parsed_file();

        let folded = folder.fold(&file);

        assert!(!folded.is_empty());
        assert!(!folded.content.is_empty());
        // With function merging enabled (default), main and helper are merged into 1 section
        // Plus User struct = 2 total sections
        assert_eq!(folded.sections.len(), 2);
        assert!(folded.content.contains("main"));
        assert!(folded.content.contains("helper"));
        assert!(folded.content.contains("User"));
    }

    #[test]
    fn test_fold_file_convenience() {
        let file = create_test_parsed_file();
        let folded = fold_file(&file, 2000);

        assert!(!folded.is_empty());
        assert!(folded.content.contains("main"));
    }

    #[test]
    fn test_section_type_display() {
        assert_eq!(SectionType::Class.as_str(), "class");
        assert_eq!(SectionType::Functions.as_str(), "functions");
        assert_eq!(SectionType::Struct.as_str(), "struct");
    }

    #[test]
    fn test_folded_content_empty() {
        let empty = FoldedContent::empty();
        assert!(empty.is_empty());
        assert_eq!(empty.estimated_tokens, 0);
    }

    #[test]
    fn test_minimal_mode() {
        let folder = FileFolder::new().with_mode(FoldMode::Minimal);
        let file = create_test_parsed_file();

        let folded = folder.fold(&file);

        assert!(!folded.is_empty());
        // Minimal mode should still include names
        assert!(folded.content.contains("main"));
    }

    #[test]
    fn token_limit_keeps_original_header_and_fits_budget() {
        let mut file = create_test_parsed_file();
        for idx in 0..40 {
            let entity_id = 100 + idx;
            let line = 50 + idx;
            let name = format!("function_{idx:02}_alpha_beta");
            file.add_entity(create_test_entity(
                entity_id,
                EntityKind::Function,
                &name,
                line,
            ));
        }

        let folded = FileFolder::new().with_max_tokens(40).fold(&file);

        assert!(!folded.is_empty());
        assert!(folded.sections_dropped > 0);
        assert!(folded.estimated_tokens <= 40);
        assert!(folded.content.starts_with("// File: src/main.rs"));
        assert!(folded.content.contains("// Language: Rust"));
    }

    #[test]
    fn token_limit_reports_empty_when_budget_is_too_tight() {
        let file = create_test_parsed_file();

        let folded = FileFolder::new().with_max_tokens(1).fold(&file);

        assert!(folded.is_empty());
    }
}
