//! Embedding semantic summary generator
//!
//! This module generates pure semantic summaries for embedding-based search,
//! using the new group-oriented template architecture.

use crate::ast_to_nl::common::{NameNormalizer, create_standalone_group};
use crate::ast_to_nl::embedding::templates::GroupTemplateDispatcher;
use crate::ast_to_nl::embedding::text_cleaner::EmbeddingTextCleaner;
use crate::ast_to_nl::embedding::text_cleaner::find_trim_point;
use crate::ast_to_nl::noise::NoiseProfile;
use crate::grouper::types::{EntityGroup, GroupType};
use cce_config::EmbeddingGeneratorConfig;
use cce_types::{EntityKind, GroupedEntity};
use cce_utils::token_estimation::{TokenEstimator, estimate_tokens};

/// Minimum token count for a docstring to be split into a separate description segment.
const LONG_DOC_THRESHOLD: usize = 500;

/// Embedding semantic summary generator
///
/// Uses the new group-oriented template architecture for generating
/// semantic summaries optimized for vector embedding.
pub struct EmbeddingGenerator {
    config: EmbeddingGeneratorConfig,
    template_dispatcher: GroupTemplateDispatcher,
    text_cleaner: EmbeddingTextCleaner,
}

impl EmbeddingGenerator {
    /// Create a new embedding generator with configuration
    pub fn with_config(config: &EmbeddingGeneratorConfig) -> Self {
        Self {
            config: config.clone(),
            template_dispatcher: GroupTemplateDispatcher::new(),
            text_cleaner: EmbeddingTextCleaner::new(),
        }
    }

    /// Create a new embedding generator with default configuration
    pub fn new() -> Self {
        Self::with_config(&EmbeddingGeneratorConfig::default())
    }

    /// Generate embedding semantic summary for an entity group
    ///
    /// Returns a vector of descriptions where:
    /// - First element: Group overall description
    /// - Subsequent elements: Independent member descriptions
    /// - If nested groups exist, their descriptions are appended at the end
    /// - If module-level documentation exists, it's prepended as a separate chunk
    ///
    /// Each description is normalized by `EmbeddingTextCleaner` without
    /// rewriting Rust symbols or qualified paths.
    pub fn generate_for_group(&self, group: &EntityGroup) -> Vec<String> {
        // File documentation groups (README-style docs) are compressed to a
        // single summary chunk. Emitting the full doc causes the chunker to
        // split it into many near-identical chunks that dilute retrieval.
        if group.group_type == GroupType::FileDocumentation {
            if let Some(ref header) = group.header {
                if let Some(ref doc) = header.doc_comment {
                    let cleaned = self.text_cleaner.clean(doc);
                    let summary = self.file_doc_summary(&cleaned);
                    if !summary.is_empty() {
                        let profile = NoiseProfile::for_language(group.language);
                        let summary =
                            crate::ast_to_nl::embedding::filter_embedding_noise(&summary, profile);
                        return vec![self.truncate_to_word_limit(&summary)];
                    }
                }
            }
            return Vec::new();
        }

        let mut descriptions = Vec::new();

        // Generate regular group and member descriptions
        let mut group_descs = self.template_dispatcher.dispatch(group);
        descriptions.append(&mut group_descs);

        // If the group has a very long doc_comment (> 500 tokens), split into
        // a brief summary and a full documentation segment.
        if let Some(ref header) = group.header {
            if let Some(ref doc) = header.doc_comment {
                let doc_tokens = estimate_tokens(doc);
                if doc_tokens > LONG_DOC_THRESHOLD {
                    let byte_pos = TokenEstimator::default().find_split_point(doc, 150);
                    let split_point = find_trim_point(doc, byte_pos);
                    let doc_summary = &doc[..split_point];
                    descriptions.push(doc_summary.to_string());
                    descriptions.push(format!("Documentation of {}:\n{}", group.name, doc));
                }
            }
        }

        // Recursively process nested groups (e.g., inner classes, nested structs)
        for nested in group.nested_groups.iter() {
            let nested_descs = self.generate_for_group(nested);
            descriptions.extend(nested_descs);
        }

        // Clean and truncate each description
        descriptions
            .into_iter()
            .map(|desc| {
                let cleaned = self.text_cleaner.clean(&desc);
                self.truncate_to_word_limit(&cleaned)
            })
            .collect()
    }

    /// Generate a brief header for continuation chunks when a group is split.
    ///
    /// Returns a condensed description containing only:
    /// - Group name/type (e.g., "once cell inherent_impl")
    ///
    /// This provides group-level context without the redundancy of
    /// repeating the full header or listing all member names in every chunk.
    ///
    /// Example output:
    /// "once cell inherent_impl."
    ///
    /// For merged fragment groups, returns a non-content fallback brief
    /// to avoid repeating entity descriptions in every continuation chunk.
    pub fn generate_brief_for_group(&self, group: &EntityGroup) -> String {
        if matches!(group.group_type, GroupType::MergedFragments) {
            return self.fallback_brief(group);
        }
        let descriptions = self.template_dispatcher.dispatch(group);
        match descriptions.first() {
            Some(desc) => {
                let normalized = desc.trim_end_matches('.');
                format!("{}.", normalized)
            }
            None => self.fallback_brief(group),
        }
    }

    /// Fallback brief description when template dispatcher doesn't produce output.
    ///
    /// Includes the header's doc comment when available, so that merged fragment
    /// groups retain their documentation even when the full header is too long
    /// for the chunker's header budget.
    fn fallback_brief(&self, group: &EntityGroup) -> String {
        let kind_name = format!("{:?}", group.kind).to_lowercase();
        let group_name = group.name.as_str();
        let base = format!("{} {}", group_name, kind_name);

        if let Some(header) = &group.header {
            if let Some(ref doc) = header.doc_comment {
                let cleaned = doc
                    .lines()
                    .map(|l| l.trim().trim_start_matches("///").trim())
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if !cleaned.is_empty() {
                    return format!("{}. {}", base, cleaned);
                }
            }
        }
        base
    }

    /// Generate embedding semantic summary for a single entity
    ///
    /// This is a compatibility method for standalone entities.
    /// For grouped entities, prefer `generate_for_group`.
    pub fn generate(&self, entity: &GroupedEntity) -> String {
        // Suppress pure module declarations (no doc comment, no annotations)
        // These forward declarations have no informational content — the module's
        // actual content lives in its own file.
        if entity.kind == EntityKind::Module
            && entity.doc_comment.is_none()
            && !entity.metadata.contains_key("annotations")
        {
            return String::new();
        }

        // Create a standalone group from the entity
        let group = create_standalone_group(entity);
        let descriptions = self.generate_for_group(&group);

        // Return the first description (group description)
        descriptions
            .into_iter()
            .next()
            .unwrap_or_else(|| format!("{}.", NameNormalizer::normalize(&entity.name)))
    }

    /// Compress a file-level documentation comment into a single summary
    /// segment capped at ~300 tokens.
    fn file_doc_summary(&self, doc: &str) -> String {
        const FILE_DOC_SUMMARY_TOKENS: usize = 300;
        if estimate_tokens(doc) <= FILE_DOC_SUMMARY_TOKENS {
            return doc.to_string();
        }
        let byte_pos = TokenEstimator::default().find_split_point(doc, FILE_DOC_SUMMARY_TOKENS);
        let split_point = find_trim_point(doc, byte_pos);
        doc[..split_point].trim_end().to_string()
    }

    /// Truncate summary to word limit
    fn truncate_to_word_limit(&self, text: &str) -> String {
        let max_words = self.config.max_summary_words;
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() <= max_words {
            text.to_string()
        } else {
            let truncated: Vec<&str> = words.into_iter().take(max_words).collect();
            let mut result = truncated.join(" ");
            if !result.ends_with('.') {
                result.push('.');
            }
            result
        }
    }
}

impl Default for EmbeddingGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use cce_types::{EntityId, EntityKind};

    fn create_test_function() -> GroupedEntity {
        GroupedEntity {
            id: EntityId(1),
            kind: EntityKind::Function,
            name: "await_ready_for_timeout".to_string(),
            signature: "fn await_ready_for_timeout(timeout: Duration) -> bool".to_string(),
            parameters: smallvec::smallvec![("timeout".into(), Some("Duration".into()))],
            return_type: Some("bool".to_string()),
            doc_comment: Some(
                "/// Returns true if the device is ready, false if timed out.".to_string(),
            ),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        }
    }

    #[test]
    fn test_generate_function_summary() {
        let generator = EmbeddingGenerator::new();
        let entity = create_test_function();
        let text = generator.generate(&entity);

        // Should not be empty
        assert!(!text.is_empty());
    }

    #[test]
    fn test_generate_function_no_docstring() {
        let generator = EmbeddingGenerator::new();
        let entity = GroupedEntity {
            id: EntityId(1),
            kind: EntityKind::Function,
            name: "calculate_total_price".to_string(),
            signature: String::new(),
            parameters: smallvec::smallvec![
                ("price".into(), Some("f64".into())),
                ("quantity".into(), Some("i32".into())),
            ],
            return_type: Some("f64".to_string()),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        };
        let text = generator.generate(&entity);

        // Should not be empty
        assert!(!text.is_empty());
    }

    #[test]
    fn test_generate_struct_summary() {
        let generator = EmbeddingGenerator::new();
        let entity = GroupedEntity {
            id: EntityId(1),
            kind: EntityKind::Struct,
            name: "User".to_string(),
            signature: String::new(),
            parameters: smallvec::smallvec![
                ("id".into(), Some("i32".into())),
                ("name".into(), Some("String".into())),
            ],
            return_type: None,
            doc_comment: Some("/// Represents a user account.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        };
        let text = generator.generate(&entity);

        // Should not be empty
        assert!(!text.is_empty());
    }

    #[test]
    fn test_truncate_to_word_limit() {
        let config = EmbeddingGeneratorConfig {
            max_summary_words: 5,
            ..Default::default()
        };
        let generator = EmbeddingGenerator::with_config(&config);
        let long_text = "This is a very long summary that should be truncated to five words.";
        let truncated = generator.truncate_to_word_limit(long_text);

        let word_count = truncated.split_whitespace().count();
        assert!(word_count <= 5);
    }

    #[test]
    fn test_generate_for_group() {
        let generator = EmbeddingGenerator::new();
        let group = EntityGroup::default();
        let descriptions = generator.generate_for_group(&group);

        // Should return at least one description
        assert!(!descriptions.is_empty());
    }
}
