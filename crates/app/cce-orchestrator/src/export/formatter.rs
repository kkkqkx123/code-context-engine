//! Markdown formatter for export
//!
//! This module provides functionality to format file documents as Markdown.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use cce_parser::ast_to_nl::clean_comment_content;
use cce_types::entity::EntityKind;

use super::aggregator::{EntityNlDocument, FileNlDocument};
use super::direct_generator::DirectExportDocument;
use super::error::ExportError;

/// A node in the entity hierarchy tree
#[derive(Clone)]
struct EntityNode {
    /// The entity itself (for containers, the header entity)
    entity: EntityNlDocument,
    /// Child entities contained within this entity
    children: Vec<EntityNode>,
    /// Whether this entity is a container type
    is_container: bool,
}

/// File-level metadata for direct export formatting.
///
/// Provides the same file-level context as `FileNlDocument` but is decoupled
/// from the aggregation pipeline, allowing the direct exporter to supply
/// metadata independently.
#[derive(Debug, Clone, Default)]
pub struct FileExportMetadata {
    /// File-level documentation comment (from //! crate/module doc)
    pub file_doc_comment: Option<String>,
    /// Import list
    pub imports: Vec<String>,
    /// Export list
    pub exports: Vec<String>,
    /// Formatted summary line (e.g. "summary: <text> | entities: ... | N lines")
    pub summary_line: Option<String>,
}

/// Markdown formatter
///
/// Formats file documents as Markdown for export.
pub struct MarkdownFormatter {
    /// Project root directory (for computing relative paths)
    project_root: PathBuf,
}

impl MarkdownFormatter {
    /// Create a new markdown formatter
    pub fn new() -> Self {
        Self {
            project_root: PathBuf::new(),
        }
    }

    /// Create a new markdown formatter with project root
    pub fn with_project_root(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Format a file document as Markdown.
    ///
    /// When `doc.summary` is `None`, the metadata section (imports, exports,
    /// summary line) is omitted and only the entity hierarchy is rendered.
    pub fn format(&self, doc: &FileNlDocument) -> Result<String, ExportError> {
        let mut output = String::new();

        // Title: relative path only (language removed - implied by file extension)
        let display_path = self.make_relative(&doc.source_path);
        output.push_str(&format!("# {}\n\n", display_path));

        // File-level documentation comment (from //! crate/module doc)
        if let Some(ref summary) = doc.summary {
            if let Some(ref file_doc) = summary.file_doc_comment {
                let cleaned_doc = clean_comment_content(file_doc);
                if !cleaned_doc.is_empty() {
                    output.push_str(&cleaned_doc);
                    output.push_str("\n\n");
                }
            }
        }

        // Metadata section: imports, exports, summary
        let mut has_metadata = false;

        // Imports (compact, inline style)
        if !doc.imports.is_empty() {
            output.push_str(&format!("- imports: `{}`\n", doc.imports.join("`, `")));
            has_metadata = true;
        }

        // Exports (compact, inline style)
        if !doc.exports.is_empty() {
            output.push_str(&format!("- exports: `{}`\n", doc.exports.join("`, `")));
            has_metadata = true;
        }

        // Clean summary from FileSummary data
        if let Some(ref summary) = doc.summary {
            if let Some(summary_line) = self.format_summary(summary, &doc.entities) {
                output.push_str(&summary_line);
                output.push('\n');
                has_metadata = true;
            }
        }

        // Separator between metadata and entities
        if has_metadata {
            output.push_str("---\n");
        }

        // Build hierarchical entity tree from flat list
        let tree = self.build_entity_tree(&doc.entities);

        // Render tree with nested formatting and constant merging
        if !tree.is_empty() {
            for node in &tree {
                self.render_node(node, &mut output, 0);
            }
        }

        Ok(output)
    }

    /// Format a list of direct export documents as a complete file-level Markdown document.
    ///
    /// This provides a unified formatting path for the direct exporter, producing
    /// output consistent with the `format()` method for `FileNlDocument`:
    /// file-level title, metadata section, and `---` separated entity entries.
    pub fn format_file_export(
        &self,
        file_path: &str,
        exports: &[DirectExportDocument],
        metadata: &FileExportMetadata,
    ) -> Result<String, ExportError> {
        let mut output = String::new();

        // Title: relative path
        let display_path = self.make_relative(file_path);
        output.push_str(&format!("{}\n\n", display_path));

        // File-level documentation comment
        if let Some(ref file_doc) = metadata.file_doc_comment {
            let cleaned_doc = clean_comment_content(file_doc);
            if !cleaned_doc.is_empty() {
                output.push_str(&cleaned_doc);
                output.push_str("\n\n");
            }
        }

        // Metadata section
        let mut has_metadata = false;

        if !metadata.imports.is_empty() {
            output.push_str(&format!("- imports: `{}`\n", metadata.imports.join("`, `")));
            has_metadata = true;
        }

        if !metadata.exports.is_empty() {
            output.push_str(&format!("- exports: `{}`\n", metadata.exports.join("`, `")));
            has_metadata = true;
        }

        if let Some(ref summary_line) = metadata.summary_line {
            output.push_str(&format!("{}\n", summary_line));
            has_metadata = true;
        }

        if has_metadata {
            output.push_str("---\n");
        }

        // Render each export entity
        for (i, export) in exports.iter().enumerate() {
            output.push_str(&self.format_direct_export(export)?);
            if i < exports.len() - 1 {
                output.push_str("---\n\n");
            }
        }

        Ok(output)
    }

    /// Format a single DirectExportDocument entry
    fn format_direct_export(&self, doc: &DirectExportDocument) -> Result<String, ExportError> {
        let mut output = String::new();

        // Header: modifiers + kind + name
        let mut kind_str = format!("{}", doc.kind);
        if !doc.modifiers.is_empty() {
            kind_str = format!("{} {}", doc.modifiers.join(" "), kind_str);
        }
        output.push_str(&format!("{} {}\n", kind_str, doc.name));

        // Doc comment
        if let Some(comment) = &doc.doc_comment {
            let cleaned =
                super::direct_generator::DirectExportGenerator::clean_doc_comment_preserving_lines(
                    comment,
                );
            if !cleaned.is_empty() {
                output.push('\n');
                output.push_str(&cleaned);
            }
        }

        // NL description
        if let Some(embedding_text) = &doc.embedding_text {
            let cleaned = super::path_utils::strip_index_context(embedding_text);
            if !cleaned.is_empty() {
                output.push_str("\n\n");
                output.push_str(&cleaned);
            }
        }

        // Members
        if !doc.members.is_empty() {
            output.push_str("\n\nMembers:\n");
            for member in &doc.members {
                let line_range = if member.start_line == member.end_line {
                    format!("line {}", member.start_line)
                } else {
                    format!("lines {}-{}", member.start_line, member.end_line)
                };
                output.push_str(&format!(
                    "  {} {}: {}\n",
                    member.kind, member.name, line_range
                ));
                if let Some(doc_comment) = &member.doc_comment {
                    let cleaned = super::direct_generator::DirectExportGenerator::clean_doc_comment(
                        doc_comment,
                    );
                    if !cleaned.is_empty() {
                        output.push_str(&format!("    {}\n", cleaned));
                    }
                }
                if let Some(embedding_text) = &member.embedding_text {
                    let cleaned = super::path_utils::strip_index_context(embedding_text);
                    if !cleaned.is_empty() {
                        for line in cleaned.lines() {
                            output.push_str(&format!("    {}\n", line));
                        }
                    }
                }
            }
        }

        // Nested items
        if !doc.nested_items.is_empty() {
            output.push_str("\nNested Items:\n");
            for nested in &doc.nested_items {
                let line_range = if nested.start_line == nested.end_line {
                    format!("line {}", nested.start_line)
                } else {
                    format!("lines {}-{}", nested.start_line, nested.end_line)
                };
                output.push_str(&format!(
                    "  {} {}: {}\n",
                    nested.group_type, nested.name, line_range
                ));
            }
        }

        // Related entities (relation enhancement)
        if !doc.related_entities.is_empty() {
            output.push_str("\nrelated: ");
            output.push_str(&self.format_related(&doc.related_entities));
            output.push('\n');
        }

        output.push('\n');
        Ok(output)
    }

    /// Build a hierarchical entity tree from a flat entity list
    ///
    /// Container entities (Struct, Class, Module, etc.) become parent nodes.
    /// Entities whose span falls within a container's span become children.
    /// Adjacent constants at the same level are merged into aggregate nodes.
    /// Final dedup check: removes containers with identical descriptions.
    fn build_entity_tree(&self, entities: &[EntityNlDocument]) -> Vec<EntityNode> {
        // Final dedup pass: keep at most one InherentImpl/TraitImpl per (kind, name) pair globally
        let filtered_entities = {
            use std::collections::HashSet;
            let mut seen: HashSet<(EntityKind, String)> = HashSet::new();
            let mut result = Vec::new();

            for entity in entities {
                match entity.kind {
                    EntityKind::InherentImpl | EntityKind::TraitImpl => {
                        let key = (entity.kind, entity.name.clone());
                        if !seen.contains(&key) {
                            result.push(entity.clone());
                            seen.insert(key);
                        }
                        // Skip duplicate (kind, name) pair
                    }
                    _ => {
                        // All non-containers and other containers: always add
                        result.push(entity.clone());
                    }
                }
            }
            result
        };

        // Separate container entities from others
        let mut containers: Vec<(usize, &EntityNlDocument)> = Vec::new();
        let mut others: Vec<&EntityNlDocument> = Vec::new();

        for entity in &filtered_entities {
            if self.is_container_kind(entity.kind) {
                containers.push((entity.span.start_position.row, entity));
            } else {
                others.push(entity);
            }
        }

        // Sort containers by position
        containers.sort_by_key(|(row, _)| *row);

        // Assign each non-container to its containing parent
        let mut tree: Vec<EntityNode> = containers
            .into_iter()
            .map(|(_, entity)| EntityNode {
                entity: entity.clone(),
                children: Vec::new(),
                is_container: true,
            })
            .collect();

        for other in others {
            let parent = tree.iter_mut().rev().find(|node| {
                let ps = node.entity.span.start_position.row;
                let pe = node.entity.span.end_position.row;
                let cs = other.span.start_position.row;
                let ce = other.span.end_position.row;
                // Only parenthesize if child is strictly within parent's span
                // (exclude exact same span - that would be self-containment)
                cs >= ps && ce <= pe && !(cs == ps && ce == pe)
            });

            if let Some(parent_node) = parent {
                parent_node.children.push(EntityNode {
                    entity: other.clone(),
                    children: Vec::new(),
                    is_container: false,
                });
            } else {
                // Top-level non-container entity
                tree.push(EntityNode {
                    entity: other.clone(),
                    children: Vec::new(),
                    is_container: false,
                });
            }
        }

        // Merge adjacent constants within each node's children list
        for node in &mut tree {
            node.children = self.merge_adjacent_constants(&node.children);
        }

        // Also merge adjacent constants at the top level
        self.merge_adjacent_constants(&tree)
    }

    /// Merge adjacent constant nodes into aggregate entries
    ///
    /// Multiple consecutive Constant entities at the same level are merged into a single node:
    /// "Constants: INCOMPLETE, RUNNING, COMPLETE (in line 125-127)"
    fn merge_adjacent_constants(&self, nodes: &[EntityNode]) -> Vec<EntityNode> {
        if nodes.is_empty() {
            return Vec::new();
        }

        let mut result = Vec::new();
        let mut i = 0;

        while i < nodes.len() {
            if nodes[i].entity.kind == EntityKind::Constant {
                // Collect consecutive constants
                let mut consts = Vec::new();
                let mut j = i;
                while j < nodes.len() && nodes[j].entity.kind == EntityKind::Constant {
                    consts.push(&nodes[j].entity);
                    j += 1;
                }

                if consts.len() >= 2 {
                    // Merge into a single aggregate node
                    let merged = self.create_merged_constant_node(&consts);
                    result.push(merged);
                } else {
                    // Single constant, keep as-is
                    result.push(nodes[i].clone());
                }

                i = j;
            } else {
                result.push(nodes[i].clone());
                i += 1;
            }
        }

        result
    }

    /// Create a merged entity node from multiple adjacent constants
    fn create_merged_constant_node(&self, constants: &[&EntityNlDocument]) -> EntityNode {
        let names: Vec<&str> = constants.iter().map(|c| c.name.as_str()).collect();
        let first_span = constants[0].span;
        let last_span = constants[constants.len() - 1].span;

        let merged_span = cce_types::Span {
            start_byte: first_span.start_byte,
            end_byte: last_span.end_byte,
            start_position: first_span.start_position,
            end_position: last_span.end_position,
        };

        EntityNode {
            entity: EntityNlDocument::new(
                names.join(", "),
                EntityKind::Constant,
                Vec::new(),
                String::new(),
                merged_span,
                constants[0].group_type,
            ),
            children: Vec::new(),
            is_container: false,
        }
    }

    /// Render an entity node and its children with nesting
    fn render_node(&self, node: &EntityNode, output: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);

        if node.is_container {
            let line_info = self.format_line_info(&node.entity.span);
            let mods = Self::format_modifiers(&node.entity.modifiers);

            let description = node.entity.nl_description.trim();
            let has_desc = !description.is_empty()
                && !self.is_description_redundant(description, &node.entity.name);

            if has_desc {
                // Use description as the main text with modifiers and line info.
                // This avoids redundant header (kind + name) when description already
                // conveys the same information.
                // e.g., "unsafe sync trait trait_impl. Implements Sync for OnceCell<T> (line 20)."
                let desc_trimmed = description.trim_end_matches('.');
                output.push_str(&format!(
                    "{}{}{}{}.\n",
                    indent, mods, desc_trimmed, line_info
                ));
            } else {
                let header = format!(
                    "{}{}{} {}{}",
                    indent,
                    mods,
                    kind_label_lowercase(node.entity.kind),
                    node.entity.name,
                    line_info
                );
                output.push_str(&format!("{}\n", header));
            }

            // Related entities (compact)
            if !node.entity.related_entities.is_empty() {
                output.push_str(&format!(
                    "{}  related: {}\n",
                    indent,
                    self.format_related(&node.entity.related_entities)
                ));
            }

            // Partition children: simple (Field/Property) merged inline, complex rendered individually
            let (simple_children, complex_children): (Vec<&EntityNode>, Vec<&EntityNode>) =
                node.children.iter().partition(|c| Self::is_simple_child(c));

            if !simple_children.is_empty() {
                let child_kind = kind_label_lowercase(simple_children[0].entity.kind);
                let parts: Vec<String> = simple_children
                    .iter()
                    .map(|c| {
                        let cli = self.format_line_info(&c.entity.span);
                        format!("{}{}", c.entity.name, cli)
                    })
                    .collect();
                output.push_str(&format!(
                    "{}  with {} {}\n",
                    indent,
                    child_kind,
                    parts.join(", ")
                ));
            }

            // Render complex children individually
            for child in &complex_children {
                self.render_simple_entity(child, output, depth + 1);
            }
        } else {
            self.render_simple_entity(node, output, depth);
        }
    }

    /// Check if a child entity is simple enough to merge inline
    ///
    /// Simple children are leaf entities (no nested children) of kind Field or Property.
    /// They can be compactly represented as "with field name (line X), name (line Y)".
    fn is_simple_child(node: &EntityNode) -> bool {
        if !node.children.is_empty() {
            return false;
        }
        matches!(node.entity.kind, EntityKind::Field | EntityKind::Property)
    }

    /// Render a leaf/non-container entity (or merged constant group)
    fn render_simple_entity(&self, node: &EntityNode, output: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);
        let line_info = self.format_line_info(&node.entity.span);
        let description = node.entity.nl_description.trim();

        // Detect semantic descriptions: "initialize function. Returns Result<(), E>."
        // generated by semantic_entity_description(name, kind) -> "{name} {kind_text}."
        // Only detect if the description has ADDITIONAL content beyond just "{name} {kind_text}."
        // e.g., "wait function." has no extra info → skip; "wait function. Blocks..." has → merge.
        // Uses both raw name and normalized name for prefix matching so that
        // "with_value function" also matches "with value function." in descriptions.
        let name_lower = node.entity.name.to_lowercase();
        let semantic_name = normalize_name(&node.entity.name);
        let kind_lower = kind_label_lowercase(node.entity.kind);
        let raw_prefix = format!("{} {}", name_lower, kind_lower);
        let norm_prefix = format!("{} {}", semantic_name, kind_lower);

        // Also detect modifier-prefixed semantic descriptions:
        // "public into inner function. Returns Option<T>." generated when
        // entity has modifiers (e.g., pub) that semantic_entity_description
        // prepends as "public" before "{name} {kind_text}."
        let desc_lower = description.to_lowercase();
        let has_semantic_desc = !description.is_empty()
            && (desc_lower.starts_with(&raw_prefix)
                || desc_lower.starts_with(&norm_prefix)
                || Self::desc_starts_with_mod_prefix(&desc_lower, &raw_prefix)
                || Self::desc_starts_with_mod_prefix(&desc_lower, &norm_prefix))
            && description.len() > raw_prefix.len() + 2;

        // Handle merged constants specially (kind is "Constants")
        let is_merged_constants =
            node.entity.kind == EntityKind::Constant && node.entity.name.contains(", ");

        if has_semantic_desc {
            // "initialize function. Returns Result<(), E> (line 44-54)"
            output.push_str(&format!("{}{}{}\n", indent, description, line_info));
        } else if is_merged_constants {
            // Merged constants: "Constants name1, name2 (line X-Y)"
            output.push_str(&format!(
                "{}{} {}{}\n",
                indent, "constants", node.entity.name, line_info
            ));
        } else {
            // Standard: lowercase kind + name
            let mods = Self::format_modifiers(&node.entity.modifiers);
            output.push_str(&format!(
                "{}{}{} {}{}\n",
                indent, mods, kind_lower, node.entity.name, line_info
            ));

            // Description on separate line (skip if redundant)
            if !description.is_empty()
                && !self.is_description_redundant(description, &node.entity.name)
            {
                output.push_str(&format!("{}{}\n", indent, description));
            }
        }

        // Related entities
        if !node.entity.related_entities.is_empty() {
            output.push_str(&format!(
                "{}  related: {}\n",
                indent,
                self.format_related(&node.entity.related_entities)
            ));
        }
    }

    /// Check if an entity kind is a container type (can have nested children)
    fn is_container_kind(&self, kind: EntityKind) -> bool {
        matches!(
            kind,
            EntityKind::Struct
                | EntityKind::Class
                | EntityKind::Module
                | EntityKind::Trait
                | EntityKind::TraitImpl
                | EntityKind::InherentImpl
                | EntityKind::Interface
                | EntityKind::Enum
                | EntityKind::Namespace
        )
    }

    /// Check if a description starts with known modifier words followed by the given prefix.
    ///
    /// Semantic descriptions generated by `semantic_entity_description` may have a modifier
    /// prefix (e.g., "public", "public constant") before the "{name} {kind_text}" pattern.
    /// This method strips known modifier words and checks if the remainder starts with `prefix`.
    ///
    /// Example: "public into inner function. Returns Option<T>." with prefix "into inner function"
    /// → strips "public " → remainder "into inner function. Returns..." starts with "into inner function"
    fn desc_starts_with_mod_prefix(desc_lower: &str, prefix: &str) -> bool {
        const MODIFIER_WORDS: &[&str] = &[
            "public",
            "private",
            "protected",
            "static",
            "asynchronous",
            "unsafe",
            "abstract",
            "virtual",
            "override",
            "constant",
            "pub",
            "mut",
            "const",
            "async",
            "default",
        ];

        let mut rest = desc_lower;
        loop {
            let trimmed = rest.trim_start();
            let matched = MODIFIER_WORDS.iter().find_map(|w| trimmed.strip_prefix(w));
            match matched {
                Some(after) if !after.is_empty() => {
                    rest = after.trim_start();
                }
                _ => break,
            }
        }

        rest.starts_with(prefix)
    }

    /// Format related entities list
    fn format_related(&self, related: &[super::aggregator::RelatedEntity]) -> String {
        related
            .iter()
            .map(|r| {
                let loc = r
                    .location
                    .map(|l| format!(":{}", l.start_position.row + 1))
                    .unwrap_or_default();
                let file = r
                    .file_path
                    .as_ref()
                    .map(|f| format!(" in {}", f))
                    .unwrap_or_default();
                format!("`{}` ({}{}{})", r.name, r.relation_type, loc, file)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Convert a potentially absolute path to a relative path based on project_root
    ///
    /// Uses the shared component-safe relativization; the former CWD-based
    /// fallback was removed because it made the result depend on the runtime
    /// working directory.
    fn make_relative(&self, path: &str) -> String {
        if self.project_root.as_os_str().is_empty() {
            return path.replace('\\', "/");
        }
        super::path_utils::relative_source_path(path, &self.project_root)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// Check if a description is redundant (just restates entity name + kind words)
    ///
    /// Many BM25-template descriptions simply repeat the entity name and generic type
    /// indicators (e.g., "function", "entity", "variable"). These add no value when
    /// the entity header already displays "Kind: name".
    ///
    /// A description is considered redundant if every word in it is either:
    /// - Part of the entity name (accounting for camelCase, snake_case, etc.)
    /// - A generic entity type word (e.g., "function", "struct", "entity")
    fn is_description_redundant(&self, description: &str, entity_name: &str) -> bool {
        let desc_lower = description.to_lowercase();

        // Entity name decomposed into individual words (camelCase, snake_case, etc.)
        let name_words: HashSet<String> = self
            .decompose_name(entity_name)
            .into_iter()
            .map(|w| w.to_lowercase())
            .collect();

        // Generic type/entity words that add no semantic value beyond the header
        // Note: "module" is intentionally excluded because module descriptions
        // carry semantic meaning (e.g., "race module" vs "unsync module").
        let noise_words: HashSet<&str> = [
            "function", "method", "struct", "class", "enum", "trait", "entity", "variable",
            "constant", "field", "property", "type", "value", "name", "new",
        ]
        .into_iter()
        .collect();

        // Split description into words and check each one
        // Strip trailing punctuation so "field." matches noise word "field"
        let words: Vec<String> = desc_lower
            .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
            .filter(|s| !s.is_empty())
            .map(|w| {
                w.trim_end_matches(|c: char| c.is_ascii_punctuation())
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();

        if words.is_empty() {
            return true;
        }

        // If ALL words are either name words or noise words, it's redundant
        words
            .iter()
            .all(|w| name_words.contains(w.as_str()) || noise_words.contains(w.as_str()))
    }

    /// Decompose an identifier into individual words
    ///
    /// Handles camelCase, PascalCase, snake_case, SCREAMING_CASE:
    /// - "OnceBox" → ["once", "box"]
    /// - "INCOMPLETE" → ["incomplete"]
    /// - "get_or_init" → ["get", "or", "init"]
    /// - "OnceNonZeroUsize" → ["once", "non", "zero", "usize"]
    /// - "XMLParser" → ["xml", "parser"]
    /// - "my-variable" → ["my", "variable"]
    fn decompose_name(&self, name: &str) -> Vec<String> {
        if name.is_empty() {
            return vec![];
        }

        // Step 1: Replace all separators with spaces (snake_case, kebab-case)
        let normalized: String = name.replace(['_', '-'], " ");

        // Step 2: Insert spaces at camelCase boundaries
        let mut with_boundaries = String::new();
        let chars: Vec<char> = normalized.chars().collect();

        for i in 0..chars.len() {
            let c = chars[i];

            if c.is_uppercase() && i > 0 {
                let prev = chars[i - 1];
                if prev != ' ' {
                    let insert_space = if prev.is_lowercase() {
                        // camelCase boundary: "getUser" → get|User
                        true
                    } else if prev.is_uppercase()
                        && i + 1 < chars.len()
                        && chars[i + 1].is_lowercase()
                    {
                        // End of acronym: "XMLParser" → XML|Parser
                        true
                    } else if prev.is_ascii_digit() {
                        // Digit boundary: "user2Name" → user2|Name
                        true
                    } else {
                        false
                    };

                    if insert_space {
                        with_boundaries.push(' ');
                    }
                }
            }

            with_boundaries.push(c);
        }

        // Step 3: Split on whitespace and normalize to lowercase
        with_boundaries
            .split_whitespace()
            .map(|s| s.to_lowercase())
            .collect()
    }

    /// Format line information from a span
    ///
    /// Returns ` (in line N)` for single-line spans,
    /// or ` (in line N-M)` for multi-line spans.
    /// Returns empty string if span has no meaningful line info (row == 0).
    fn format_line_info(&self, span: &cce_types::Span) -> String {
        // Skip if span is unset (both bytes are 0)
        if span.start_byte == 0 && span.end_byte == 0 {
            return String::new();
        }

        let start = span.start_position.row;
        let end = span.end_position.row;

        // Convert from 0-based to 1-based
        let start_line = start + 1;
        let end_line = end + 1;

        if start_line == end_line {
            format!(" (line {})", start_line)
        } else {
            format!(" (line {}-{})", start_line, end_line)
        }
    }

    /// Format entity modifiers as a space-separated prefix string
    pub(crate) fn format_modifiers(modifiers: &[String]) -> String {
        let filtered: Vec<&str> = modifiers
            .iter()
            .map(|modifier| modifier.trim())
            .filter(|modifier| Self::is_significant_modifier(modifier))
            .collect();

        if filtered.is_empty() {
            String::new()
        } else {
            format!("{} ", filtered.join(" "))
        }
    }

    /// Format the file summary as a compact metadata line.
    ///
    /// Includes the natural language summary text, entity list with kinds,
    /// and line count to provide a complete file overview in export.
    fn format_summary(
        &self,
        summary: &super::summary_view::ExportSummaryView,
        entities: &[EntityNlDocument],
    ) -> Option<String> {
        let mut parts = Vec::new();

        if !summary.summary_text.is_empty() {
            parts.push(summary.summary_text.clone());
        }

        if !summary.main_entities.is_empty() {
            let mut entity_lookup: HashMap<&str, &EntityNlDocument> = HashMap::new();
            for entity in entities {
                entity_lookup.entry(entity.name.as_str()).or_insert(entity);
            }
            let labels = format_summary_entity_labels(&summary.main_entities, &entity_lookup);
            if !labels.is_empty() {
                parts.push(format!("entities: {}", labels.join(", ")));
            }
        }

        if summary.line_count > 0 {
            parts.push(format!("{} lines", summary.line_count));
        }

        if parts.is_empty() {
            None
        } else {
            Some(format!("summary: {}", parts.join(" | ")))
        }
    }

    /// Check whether a modifier carries enough semantic value to keep in export output.
    fn is_significant_modifier(modifier: &str) -> bool {
        matches!(
            modifier,
            "async" | "unsafe" | "const" | "static" | "extern" | "mut"
        )
    }
}

/// Build `FileExportMetadata` from an `ExportSummaryView` for the direct export path.
///
/// Provides the same file-level context (imports, exports, summary line) that
/// the aggregated path derives from `FileNlDocument.summary`, without requiring
/// the aggregation pipeline.
pub fn metadata_from_summary_view(
    summary: &super::summary_view::ExportSummaryView,
) -> FileExportMetadata {
    let labels = format_summary_entity_labels(&summary.main_entities, &HashMap::new());
    let mut parts = Vec::new();

    if !summary.summary_text.is_empty() {
        parts.push(summary.summary_text.clone());
    }

    if !labels.is_empty() {
        parts.push(format!("entities: {}", labels.join(", ")));
    }

    if summary.line_count > 0 {
        parts.push(format!("{} lines", summary.line_count));
    }

    FileExportMetadata {
        file_doc_comment: summary.file_doc_comment.clone(),
        imports: summary.imports.clone(),
        exports: summary.exports.clone(),
        summary_line: if parts.is_empty() {
            None
        } else {
            Some(format!("summary: {}", parts.join(" | ")))
        },
    }
}

/// Build labeled entity strings for summary metadata.
///
/// When `entity_lookup` is empty, entities render as bare names.
pub fn format_summary_entity_labels(
    main_entities: &[String],
    entity_lookup: &HashMap<&str, &EntityNlDocument>,
) -> Vec<String> {
    main_entities
        .iter()
        .map(|name| {
            if let Some(entity) = entity_lookup.get(name.as_str()) {
                format!(
                    "`{}{} {}`",
                    MarkdownFormatter::format_modifiers(&entity.modifiers),
                    kind_label_lowercase(entity.kind),
                    entity.name
                )
            } else if looks_like_type_parameter(name) {
                format!("`type parameter {}`", name)
            } else {
                format!("`{}`", name)
            }
        })
        .collect()
}

impl Default for MarkdownFormatter {
    fn default() -> Self {
        Self::new()
    }
}

/// Lowercase entity kind label for inline descriptions
///
/// Used in compact container descriptions like "struct OnceCell (line 8-13)".
pub(crate) fn kind_label_lowercase(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Function => "function",
        EntityKind::Method => "method",
        EntityKind::Class => "class",
        EntityKind::Struct => "struct",
        EntityKind::Enum => "enum",
        EntityKind::Interface => "interface",
        EntityKind::Trait => "trait",
        EntityKind::TraitImpl => "trait_impl",
        EntityKind::InherentImpl => "inherent_impl",
        EntityKind::Module => "module",
        EntityKind::Variable => "variable",
        EntityKind::Constant => "constant",
        EntityKind::Field => "field",
        EntityKind::Property => "property",
        EntityKind::TypeAlias => "typealias",
        EntityKind::Namespace => "namespace",
        EntityKind::Constructor => "constructor",
        EntityKind::Unknown => "unknown",
        _ => "other",
    }
}

/// Heuristically identify generic placeholders that should not be reduced to bare names.
pub(crate) fn looks_like_type_parameter(name: &str) -> bool {
    name.len() == 1 && name.chars().all(|c| c.is_ascii_uppercase())
}

/// Normalize a name by splitting snake_case/camelCase into space-separated words.
/// This mirrors NameNormalizer from cce_parser without requiring pub access.
fn normalize_name(name: &str) -> String {
    if name.contains('_') || name.contains('-') {
        name.replace('-', "_")
            .split('_')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ")
    } else if name.chars().any(|c| c.is_uppercase()) {
        let mut result = String::new();
        let chars: Vec<char> = name.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            if c.is_uppercase() {
                if i > 0 {
                    let prev_is_lower = chars[i - 1].is_lowercase();
                    let next_is_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();
                    if prev_is_lower || next_is_lower {
                        result.push(' ');
                    }
                }
                result.push(c.to_ascii_lowercase());
            } else {
                result.push(c);
            }
        }
        result
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_parser::grouper::GroupType;
    use cce_parser::summary::FileSummary;
    use cce_types::Span;
    use cce_types::entity::EntityKind;
    use cce_types::language::Language;

    use super::super::summary_view::ExportSummaryView;

    fn make_entity(name: &str, kind: EntityKind, modifiers: Vec<String>) -> EntityNlDocument {
        EntityNlDocument::new(
            name.to_string(),
            kind,
            modifiers,
            String::new(),
            Span::from_lines(1, 2),
            GroupType::Standalone,
        )
    }

    #[test]
    fn test_format_summary_keeps_all_entities_and_kinds() {
        let formatter = MarkdownFormatter::new();
        let mut doc = FileNlDocument::new("src/lib.rs".into(), Language::Rust);
        doc.summary = Some(ExportSummaryView::from(
            FileSummary::new("src/lib.rs")
                .with_entities(vec![
                    "new".into(),
                    "with_value".into(),
                    "is_initialized".into(),
                    "initialize".into(),
                    "wait".into(),
                ])
                .with_line_count(415),
        ));
        doc.entities = vec![
            make_entity(
                "new",
                EntityKind::Function,
                vec!["pub".into(), "const".into()],
            ),
            make_entity(
                "with_value",
                EntityKind::Function,
                vec!["pub".into(), "const".into()],
            ),
            make_entity("is_initialized", EntityKind::Function, vec!["pub".into()]),
            make_entity("initialize", EntityKind::Function, vec!["pub".into()]),
            make_entity("wait", EntityKind::Function, vec!["pub".into()]),
        ];

        let output = formatter.format(&doc).expect("formatter should succeed");

        assert!(output.contains("summary: entities:"));
        assert!(output.contains("`const function new`"));
        assert!(output.contains("`const function with_value`"));
        assert!(output.contains("`function is_initialized`"));
        assert!(output.contains("415 lines"));
        assert!(!output.contains("... ("));
    }

    #[test]
    fn test_format_summary_falls_back_to_type_parameter_label() {
        let formatter = MarkdownFormatter::new();
        let mut doc = FileNlDocument::new("src/lib.rs".into(), Language::Rust);
        doc.summary = Some(ExportSummaryView::from(
            FileSummary::new("src/lib.rs")
                .with_entities(vec!["T".into()])
                .with_line_count(12),
        ));

        let output = formatter.format(&doc).expect("formatter should succeed");

        assert!(output.contains("`type parameter T`"));
        assert!(!output.contains("`T`"));
    }

    #[test]
    fn test_format_modifiers_filters_low_signal_entries() {
        let modifiers = vec![
            "pub".to_string(),
            "inline".to_string(),
            "async".to_string(),
            "unsafe".to_string(),
        ];

        assert_eq!(
            MarkdownFormatter::format_modifiers(&modifiers),
            "async unsafe "
        );
    }
}
