use cce_types::entity::GroupedEntity;
use cce_types::{EntityKind, ParsedFile};
use cce_utils::normalize_whitespace;
use cce_utils::token_estimation::{TokenEstimator, estimate_tokens};

use crate::ast_to_nl::clean_comment_content;
use crate::grouper::ProcessingResult;
use crate::summary::generator::entity_overview::{format_entity_overview, format_entity_stats};
use crate::summary::strategy::ImportanceLevel;
use crate::summary::types::FileSummary;

impl super::RuleBasedGenerator {
    /// Generate enriched summary text using group information
    ///
    /// Produces a structured summary with:
    /// - File-level doc comment (natural language, importance-based line budget)
    /// - Entity-kind statistics (file scale overview)
    /// - Structure overview (code-form hierarchy with signature and doc hints)
    /// - Imports, exports, import notes and line count (budgeted tail section)
    pub(crate) fn generate_summary_text_with_groups(
        &self,
        summary: &FileSummary,
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
    ) -> String {
        // File-level doc comment as leading description (natural language)
        let doc = self.doc_preview(
            summary.file_doc_comment.as_deref(),
            summary.importance_level,
        );

        // Entity-kind statistics then structure overview in code form
        let mut structure_parts = Vec::new();
        if let Some(stats) = format_entity_stats(&parsed_file.entities) {
            structure_parts.push(stats);
        }
        let structure = self.generate_structure_overview(processing_result);
        if !structure.is_empty() {
            structure_parts.push(structure);
        }
        let structure = structure_parts.join("\n");

        // Append imports, exports, import comments and line count
        let mut tail = Vec::new();
        Self::append_import_comments(parsed_file, &mut tail);
        self.append_imports_and_lines(summary, &mut tail);

        self.assemble_summary_text(doc, structure, tail)
    }

    /// Generate a hierarchical structure overview in code form.
    ///
    /// Uses only `group.kind` and `group_type` (universal across languages).
    /// Each group produces one line: `kind Name { member1, member2 }`, with
    /// the header signature replacing the bare name when available, and the
    /// header doc-comment first line appended for up to `max_entities` headers.
    /// ```text
    /// struct OnceCell<T> { get_mut, set, get_or_init } — thread-safe lazy init
    /// trait Default { default }
    /// module unsync { OnceCell<T>, Lazy<T, F> }
    /// ```
    pub(crate) fn generate_structure_overview(
        &self,
        processing_result: &ProcessingResult,
    ) -> String {
        let mut lines = Vec::new();
        let mut documented = 0usize;

        for group in &processing_result.groups {
            if let Some(ref header) = group.header {
                let kind_label = Self::kind_label(group.kind);
                let header_text = self.compact_header_text(kind_label, header);
                let member_names: Vec<String> = group
                    .members
                    .iter()
                    .filter(|m| !m.is_stdlib)
                    .map(|m| m.name.clone())
                    .collect();

                let mut line = if member_names.is_empty() {
                    format!("{header_text}.")
                } else {
                    format!("{header_text} {{ {} }}.", member_names.join(", "))
                };

                if documented < self.config.max_entities {
                    if let Some(doc_line) = Self::header_doc_line(header) {
                        line = format!("{} — {doc_line}", line.trim_end_matches('.'));
                        documented += 1;
                    }
                }

                lines.push(line);
            }
        }

        lines.join("\n")
    }

    /// Header text for a structure line: the compact signature when available,
    /// otherwise `kind_label + name`.
    fn compact_header_text(&self, kind_label: &str, header: &GroupedEntity) -> String {
        Self::compact_signature(&header.signature)
            .unwrap_or_else(|| format!("{kind_label} {}", header.name))
    }

    /// Strip leading modifiers from a signature, rejecting over-long signatures.
    ///
    /// `pub struct OnceCell<T>` becomes `struct OnceCell<T>`. Returns `None`
    /// when the signature is empty or too long, so the caller falls back to
    /// the `kind + name` form.
    pub(crate) fn compact_signature(signature: &str) -> Option<String> {
        const MAX_SIGNATURE_CHARS: usize = 64;

        let sig = signature.trim();
        if sig.is_empty() {
            return None;
        }

        let words: Vec<&str> = sig.split_whitespace().collect();
        let start = words
            .iter()
            .take_while(|w| Self::is_signature_modifier(w))
            .count();
        let cleaned = if start > 0 {
            words[start..].join(" ")
        } else {
            sig.to_string()
        };

        if cleaned.is_empty() || cleaned.len() > MAX_SIGNATURE_CHARS {
            None
        } else {
            Some(cleaned)
        }
    }

    fn is_signature_modifier(word: &str) -> bool {
        matches!(
            word,
            "pub"
                | "private"
                | "protected"
                | "public"
                | "static"
                | "async"
                | "const"
                | "final"
                | "sealed"
                | "export"
                | "default"
                | "abstract"
                | "override"
                | "internal"
                | "open"
                | "inline"
                | "unsafe"
                | "virtual"
                | "partial"
                | "readonly"
                | "required"
                | "extern"
                | "mut"
                | "ref"
        )
    }

    /// First cleaned, non-empty line of a header doc comment, if any.
    fn header_doc_line(header: &GroupedEntity) -> Option<String> {
        let doc = header.doc_comment.as_deref()?;
        let cleaned = clean_comment_content(doc);
        let first = cleaned.lines().map(str::trim).find(|l| !l.is_empty())?;
        Some(normalize_whitespace(first))
    }

    /// Leading preview of the file doc comment with an importance-based line
    /// budget (High: 5, Medium: 3, Low: 1). Empty lines are skipped.
    pub(crate) fn doc_preview(&self, doc: Option<&str>, importance: ImportanceLevel) -> String {
        let Some(doc) = doc else {
            return String::new();
        };
        let cleaned = clean_comment_content(doc);
        if cleaned.trim().is_empty() {
            return String::new();
        }

        let max_lines = match importance {
            ImportanceLevel::High => 5,
            ImportanceLevel::Medium => 3,
            ImportanceLevel::Low => 1,
        };

        let mut lines = Vec::new();
        for line in cleaned.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            lines.push(trimmed);
            if lines.len() >= max_lines {
                break;
            }
        }

        normalize_whitespace(&lines.join(" "))
    }

    /// Assemble the final summary text with per-section token budgets:
    /// doc (30%), structure (50%), tail (remainder). Each section is
    /// truncated independently so long lists cannot crowd out the semantic
    /// description sections.
    pub(crate) fn assemble_summary_text(
        &self,
        doc: String,
        structure: String,
        tail: Vec<String>,
    ) -> String {
        let mut remaining = self.config.max_summary_length;

        let doc_text = Self::truncate_to_max_length(&doc, remaining * 3 / 10);
        remaining = remaining.saturating_sub(estimate_tokens(&doc_text));

        let structure_text = Self::truncate_to_max_length(&structure, remaining * 5 / 10);
        remaining = remaining.saturating_sub(estimate_tokens(&structure_text));

        let tail_text = Self::truncate_to_max_length(&tail.join("\n"), remaining);

        let mut parts = Vec::new();
        if !doc_text.is_empty() {
            parts.push(doc_text);
        }
        if !structure_text.is_empty() {
            parts.push(structure_text);
        }
        if !tail_text.is_empty() {
            parts.push(tail_text);
        }
        parts.join("\n")
    }

    pub(crate) fn kind_label(kind: EntityKind) -> &'static str {
        match kind {
            EntityKind::Module => "module",
            EntityKind::Struct => "struct",
            EntityKind::Class => "class",
            EntityKind::Enum => "enum",
            EntityKind::Trait => "trait",
            EntityKind::Interface => "interface",
            EntityKind::Function => "function",
            EntityKind::Method => "method",
            _ => "entity",
        }
    }

    /// Append imports, exports and line count to summary parts (shared helper).
    ///
    /// Text-layer lists are truncated to `max_imports` entries so long
    /// dependency lists cannot crowd out the description sections; the full
    /// lists stay in the structured `FileSummary` fields.
    fn append_imports_and_lines(&self, summary: &FileSummary, parts: &mut Vec<String>) {
        if !summary.imports.is_empty() {
            parts.push(Self::truncated_list(
                "Uses",
                &summary.imports,
                self.config.max_imports,
            ));
        }
        if !summary.exports.is_empty() {
            parts.push(Self::truncated_list(
                "Exports",
                &summary.exports,
                self.config.max_imports,
            ));
        }
        if summary.line_count > 0 {
            parts.push(format!("Lines: {}", summary.line_count));
        }
    }

    /// Format a labeled list, truncating to `max` items with a remainder note.
    pub(crate) fn truncated_list(label: &str, items: &[String], max: usize) -> String {
        if items.len() > max {
            let shown: Vec<&str> = items.iter().take(max).map(String::as_str).collect();
            format!(
                "{label}: {} (and {} more)",
                shown.join(", "),
                items.len() - max
            )
        } else {
            format!("{label}: {}", items.join(", "))
        }
    }

    /// Append doc comments attached to import-like entities.
    ///
    /// Import-only groups are dropped before chunking, so these comments
    /// would otherwise be lost from every retrieval surface.
    fn append_import_comments(parsed_file: &ParsedFile, parts: &mut Vec<String>) {
        let comments = crate::summary::dependencies::collect_import_comments(parsed_file);
        if !comments.is_empty() {
            parts.push(format!("Import notes: {}", comments.join(" | ")));
        }
    }

    /// Truncate text to max token length
    pub(crate) fn truncate_to_max_length(text: &str, max_summary_length: usize) -> String {
        let estimated_tokens = estimate_tokens(text);
        if estimated_tokens > max_summary_length {
            let split_point = TokenEstimator::default().find_split_point(text, max_summary_length);
            if split_point > 3 {
                let truncate_at = split_point.saturating_sub(3);
                let safe_point = text
                    .char_indices()
                    .take_while(|(idx, _)| *idx < truncate_at)
                    .last()
                    .map(|(idx, ch)| idx + ch.len_utf8())
                    .unwrap_or(truncate_at);
                format!("{}...", &text[..safe_point])
            } else {
                text.to_string()
            }
        } else {
            text.to_string()
        }
    }

    /// Generate summary text
    pub(crate) fn generate_summary_text(
        &self,
        summary: &FileSummary,
        parsed_file: &ParsedFile,
    ) -> String {
        // File-level doc comment as leading description (natural language)
        let doc = self.doc_preview(
            summary.file_doc_comment.as_deref(),
            summary.importance_level,
        );

        // Entity-kind statistics then entity overview grouped by kind
        let mut structure_parts = Vec::new();
        if let Some(stats) = format_entity_stats(&parsed_file.entities) {
            structure_parts.push(stats);
        }
        if !summary.main_entities.is_empty() {
            if let Some(overview) = format_entity_overview(&parsed_file.entities) {
                structure_parts.push(overview);
            }
        }
        let structure = structure_parts.join("\n");

        // Append imports, exports, import comments and line count
        let mut tail = Vec::new();
        Self::append_import_comments(parsed_file, &mut tail);
        self.append_imports_and_lines(summary, &mut tail);

        self.assemble_summary_text(doc, structure, tail)
    }
}
