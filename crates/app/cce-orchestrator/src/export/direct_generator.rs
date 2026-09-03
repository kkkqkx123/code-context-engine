//! Direct export generator using enriched GroupConversions.
//!
//! Generates export documents from GroupConversions (which already includes
//! IndexTextEnricher enrichment), ensuring all exports use the complete
//! processing pipeline.

use cce_parser::ast_to_nl::converter::group_converter::GroupConversions;
use cce_types::entity::EntityKind;
use cce_utils::comment_cleaner::strip_comment_markers;

/// Export document generated from enriched GroupConversions
#[derive(Debug, Clone)]
pub struct DirectExportDocument {
    /// Entity name
    pub name: String,

    /// Entity kind (function, struct, etc)
    pub kind: EntityKind,

    /// Entity modifiers (pub, async, unsafe, etc)
    pub modifiers: Vec<String>,

    /// Original source signature
    pub signature: String,

    /// Original doc comment
    pub doc_comment: Option<String>,

    /// Complete source code (original formatting preserved)
    pub source_code: String,

    /// Embedding text (NL description, includes enrichment)
    pub embedding_text: Option<String>,

    /// Members (for container types)
    pub members: Vec<MemberInfo>,

    /// Nested items
    pub nested_items: Vec<NestedInfo>,

    /// Related entities (from relation enhancement, optional)
    pub related_entities: Vec<super::aggregator::RelatedEntity>,
}

/// Member information
#[derive(Debug, Clone)]
pub struct MemberInfo {
    pub name: String,
    pub kind: EntityKind,
    pub signature: String,
    pub doc_comment: Option<String>,
    pub embedding_text: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
}

/// Nested group information
#[derive(Debug, Clone)]
pub struct NestedInfo {
    pub name: String,
    pub kind: EntityKind,
    pub group_type: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// Direct export generator
pub struct DirectExportGenerator;

impl DirectExportGenerator {
    /// Generate export from enriched GroupConversions
    pub fn generate(conversions: &GroupConversions) -> Result<DirectExportDocument, String> {
        let group = &conversions.group;
        let header = group
            .header
            .as_ref()
            .ok_or_else(|| "No header entity found".to_string())?;

        let source_code = group
            .combined_source
            .as_deref()
            .ok_or_else(|| "No source code available".to_string())?
            .to_string();

        let embedding_text = conversions
            .header_conversion
            .as_ref()
            .and_then(|c| c.embedding_text.clone());

        Ok(DirectExportDocument {
            name: header.name.clone(),
            kind: header.kind,
            modifiers: header.modifiers.clone(),
            signature: header.signature.clone(),
            doc_comment: header.doc_comment.clone(),
            source_code,
            embedding_text,
            members: Self::extract_members(conversions),
            nested_items: Self::extract_nested(group),
            related_entities: Vec::new(),
        })
    }

    /// Extract member information from enriched conversions
    fn extract_members(conversions: &GroupConversions) -> Vec<MemberInfo> {
        let group = &conversions.group;
        group
            .members
            .iter()
            .map(|member| {
                let span = group
                    .entity_spans
                    .get(&member.id)
                    .copied()
                    .unwrap_or(group.span);

                let embedding_text = conversions
                    .member_conversions
                    .iter()
                    .find(|c| c.entity_id == member.id)
                    .and_then(|c| c.embedding_text.clone());

                MemberInfo {
                    name: member.name.clone(),
                    kind: member.kind,
                    signature: member.signature.clone(),
                    doc_comment: member.doc_comment.clone(),
                    embedding_text,
                    start_line: span.start_position.row + 1,
                    end_line: span.end_position.row + 1,
                }
            })
            .collect()
    }

    /// Extract nested group information
    fn extract_nested(group: &cce_parser::grouper::types::EntityGroup) -> Vec<NestedInfo> {
        group
            .nested_groups
            .iter()
            .map(|nested| NestedInfo {
                name: nested.name.to_string(),
                kind: nested.kind,
                group_type: nested.group_type.to_string(),
                start_line: nested.span.start_position.row + 1,
                end_line: nested.span.end_position.row + 1,
            })
            .collect()
    }

    pub fn clean_doc_comment(comment: &str) -> String {
        strip_comment_markers(comment, false)
    }

    pub fn clean_doc_comment_preserving_lines(comment: &str) -> String {
        strip_comment_markers(comment, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_doc_comment_triple_slash() {
        let input = "/// This is a doc\n/// comment";
        let output = DirectExportGenerator::clean_doc_comment(input);
        assert_eq!(output, "This is a doc comment");
    }

    #[test]
    fn test_clean_doc_comment_triple_slash_preserving_lines() {
        let input = "/// This is a doc\n/// comment";
        let output = DirectExportGenerator::clean_doc_comment_preserving_lines(input);
        assert_eq!(output, "This is a doc\ncomment");
    }

    #[test]
    fn test_clean_doc_comment_block() {
        let input = "/** This is a block\n* comment\n*/";
        let output = DirectExportGenerator::clean_doc_comment(input);
        assert!(output.contains("This is a block"));
        assert!(output.contains("comment"));
    }

    #[test]
    fn test_clean_doc_comment_empty() {
        let input = "///";
        let output = DirectExportGenerator::clean_doc_comment(input);
        assert_eq!(output, "");
    }

    #[test]
    fn test_clean_doc_comment_already_cleaned() {
        let input = "This is a doc comment\n\nWith multiple paragraphs";
        let output = DirectExportGenerator::clean_doc_comment_preserving_lines(input);
        assert_eq!(output, input);
    }
}
