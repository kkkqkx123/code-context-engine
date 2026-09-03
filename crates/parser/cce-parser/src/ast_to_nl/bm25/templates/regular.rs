//! BM25 regular entity templates
//!
//! Provides keyword-optimized templates for regular entities without patterns.

use super::group_trait::{GroupTemplate, helpers};
use crate::ast_to_nl::common::{GroupTemplateBase, format_annotations};
use crate::grouper::types::EntityGroup;
use cce_types::entity::meta_keys;

pub struct RegularGroupTemplate;

impl RegularGroupTemplate {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RegularGroupTemplate {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupTemplateBase for RegularGroupTemplate {}

impl GroupTemplate for RegularGroupTemplate {
    fn generate(&self, group: &EntityGroup) -> String {
        let mut all_parts: Vec<String> = Vec::new();

        if let Some(header) = &group.header {
            // Header entity already provides name, keywords, parameters, return types, doc, modifiers
            Self::push_entity_features(&mut all_parts, header);
        } else {
            // No header entity, add minimal group-level info directly
            let name = group.name.as_str();
            all_parts.push(name.to_string());
            all_parts.extend(Self::extract_entity_name_keywords(name));
            all_parts.push(group.kind.to_string());

            // Include annotations if available (header-only path) - text only
            if let Some(annotations) = group.metadata.get(meta_keys::ANNOTATIONS) {
                if let Some(formatted) =
                    format_annotations(annotations, cce_types::OutputMode::Bm25)
                {
                    all_parts.push(formatted);
                }
            }
        }

        // Include auto trait implementation names, with an aggregated summary
        // that anchors the relationship between the parent type and its trait impls.
        // Without this summary (e.g. "traits: Sync Send ..."), query terms like
        // "OnceCell Sync" would rely solely on cross-chunk co-occurrence rather
        // than explicit in-chunk relationship text.
        if let Some(auto_traits_str) = group.metadata.get(meta_keys::AUTO_TRAITS).or_else(|| {
            group
                .header
                .as_ref()
                .and_then(|h| h.metadata.get(meta_keys::AUTO_TRAITS))
        }) {
            let trait_names: Vec<&str> = auto_traits_str
                .split(',')
                .filter(|s| !s.is_empty())
                .collect();
            if !trait_names.is_empty() {
                all_parts.push(format!("traits: {}", trait_names.join(" ")));
            }
            for trait_name in trait_names {
                all_parts.push(trait_name.to_string());
                all_parts.extend(helpers::extract_keywords(trait_name));
            }
        }

        // Include member names and structured type/doc info.
        // For module-level entities, use compact mode to avoid massive text expansion:
        // only include member names and doc keywords, skip full feature expansion.
        // Import-like members (import/require/include/export) are skipped: they carry
        // no retrieval value in the BM25 path (file-level summary and the relation
        // index cover them instead).
        let is_module = group.kind.is_module_like();
        for member in &group.members {
            if member.kind.is_import_like() {
                continue;
            }
            if is_module {
                all_parts.push(member.name.clone());
                if let Some(ref doc) = member.doc_comment {
                    let clean_doc = Self::clean_doc_comment(doc);
                    if !clean_doc.is_empty() {
                        all_parts.push(clean_doc);
                    }
                }
            } else {
                Self::push_entity_features(&mut all_parts, member);
            }

            // Include impl_source as searchable keyword when present
            if let Some(source) = member.metadata.get(meta_keys::IMPL_SOURCE) {
                if source != meta_keys::IMPL_SOURCE_INHERENT {
                    all_parts.push(format!("impl_{}", source));
                    all_parts.extend(helpers::extract_keywords(source));
                    all_parts.push("impl".to_string());
                } else {
                    all_parts.push(meta_keys::IMPL_SOURCE_INHERENT.to_string());
                }
            }
        }

        // Include inherent impl count from metadata
        if let Some(count_str) = group.metadata.get(meta_keys::INHERENT_IMPL_COUNT) {
            if let Ok(count) = count_str.parse::<usize>() {
                if count > 0 {
                    all_parts.push(format!("{}_inherent_impls", count));
                }
            }
        }

        let refs: Vec<&str> = all_parts.iter().map(|s| s.as_str()).collect();
        helpers::join_parts(&refs)
    }
}

impl RegularGroupTemplate {
    /// Filter out noise keywords not useful for BM25 search.
    /// Removes single-character tokens (type params, loop vars) and tokens
    /// containing angle brackets (generic noise like "cell<t>").
    fn filter_keywords(keywords: Vec<String>) -> Vec<String> {
        keywords
            .into_iter()
            .filter(|k| k.len() > 1 && !k.contains('<') && !k.contains('>'))
            .collect()
    }

    /// Extract keywords from the parsed entity name.
    fn extract_entity_name_keywords(name: &str) -> Vec<String> {
        let keywords = helpers::extract_keywords(name);
        Self::filter_keywords(keywords)
    }

    fn push_entity_features(
        all_parts: &mut Vec<String>,
        entity: &cce_types::entity::GroupedEntity,
    ) {
        all_parts.push(entity.name.clone());
        all_parts.extend(Self::extract_entity_name_keywords(&entity.name));

        if !entity.signature.is_empty() {
            let tokens = Self::extract_type_tokens(&entity.signature);
            all_parts.extend(tokens);
        }

        for (param_name, param_type) in &entity.parameters {
            all_parts.push(param_name.to_string());
            all_parts.extend(Self::filter_keywords(helpers::extract_keywords(param_name)));
            if let Some(param_type) = param_type {
                Self::push_type_tokens(all_parts, param_type);
            }
        }

        if let Some(return_type) = &entity.return_type {
            Self::push_type_tokens(all_parts, return_type);
        }

        if let Some(ref doc) = entity.doc_comment {
            let clean_doc = Self::clean_doc_comment(doc);
            if !clean_doc.is_empty() {
                all_parts.push(clean_doc);
            }
        }

        // Push modifiers as body text.
        // Visibility keywords are now included directly to support
        // precise queries like "pub(crate) fn initialize".
        for modifier in &entity.modifiers {
            all_parts.push(modifier.to_lowercase());
        }

        // Push subtype if present (e.g., "generator", "media", "class" for CSS)
        if let Some(ref subtype) = entity.subtype {
            all_parts.push(subtype.to_lowercase());
            all_parts.extend(helpers::extract_keywords(subtype));
        }

        // Push key attribute values (e.g., HTML/CSS class names, IDs)
        let mut attribute_values: Vec<&str> = entity
            .attributes
            .values()
            .map(|value| value.as_str())
            .collect();
        attribute_values.sort();
        for value in attribute_values {
            if !value.is_empty() {
                all_parts.extend(helpers::extract_keywords(value));
            }
        }

        // Push annotation/decorator metadata to template text for full-text search.
        // Annotations like #[cfg(...)], #[derive(...)] are directly included in the
        // description text so they are searchable, but are NOT extracted as keywords
        // (keywords have extra BM25 weighting and should only be entity names).
        if let Some(annotations) = entity.metadata.get(meta_keys::ANNOTATIONS) {
            if let Some(formatted) = format_annotations(annotations, cce_types::OutputMode::Bm25) {
                all_parts.push(formatted);
            }
        }
    }

    fn push_type_tokens(all_parts: &mut Vec<String>, type_text: &str) {
        for token in Self::extract_type_tokens(type_text) {
            all_parts.push(token);
        }
    }

    fn extract_type_tokens(text: &str) -> Vec<String> {
        const NOISE: &[&str] = &[
            "fn",
            "function",
            "def",
            "class",
            "struct",
            "enum",
            "trait",
            "interface",
            "self",
            "mut",
            "pub",
            "public",
            "private",
            "protected",
            "static",
            "async",
            "return",
            "let",
            "const",
            "type",
            "impl",
            "ref",
            "dyn",
            "move",
            "unsafe",
            "where",
            "as",
            "in",
            "for",
            "while",
            "loop",
            "if",
            "else",
            "match",
            "mod",
            "use",
            "extern",
            "crate",
            "super",
            "true",
            "false",
        ];

        text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-' || c == ':' || c == '.'))
            .flat_map(|part| {
                let clean = part
                    .trim_matches('_')
                    .trim_matches(':')
                    .trim_matches('.')
                    .to_string();
                helpers::extract_keywords(&clean)
            })
            .filter(|token| {
                !token.is_empty()
                    && !NOISE.contains(&token.as_str())
                    && !token.chars().all(|c| c.is_ascii_digit())
                    && token.len() > 1
                    && !token.contains('<')
                    && !token.contains('>')
            })
            .collect()
    }

    fn clean_doc_comment(doc: &str) -> String {
        let cleaned = Self::clean_doc_text(doc);
        cleaned.replace('`', "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::entity::EntityKind;

    #[test]
    fn test_regular_template_class() {
        let group = EntityGroup {
            name: "UserService".into(),
            kind: EntityKind::Class,
            ..Default::default()
        };

        let template = RegularGroupTemplate::new();
        let text = template.generate(&group);

        assert!(text.contains("userservice"));
        assert!(text.contains("class"));
    }

    #[test]
    fn test_regular_template_enum() {
        let group = EntityGroup {
            name: "Status".into(),
            kind: EntityKind::Enum,
            members: [
                cce_types::entity::GroupedEntity {
                    id: cce_types::entity::EntityId(1),
                    name: "Active".to_string(),
                    ..Default::default()
                },
                cce_types::entity::GroupedEntity {
                    id: cce_types::entity::EntityId(2),
                    name: "Inactive".to_string(),
                    ..Default::default()
                },
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };

        let template = RegularGroupTemplate::new();
        let text = template.generate(&group);

        assert!(text.contains("status"));
        assert!(text.contains("enum"));
        assert!(text.contains("active"));
        assert!(text.contains("inactive"));
    }

    #[test]
    fn test_regular_template_function() {
        let group = EntityGroup {
            name: "calculate_total".into(),
            kind: EntityKind::Function,
            ..Default::default()
        };

        let template = RegularGroupTemplate::new();
        let text = template.generate(&group);

        assert!(text.contains("calculate_total"));
        assert!(text.contains("function"));
    }

    #[test]
    fn test_bm25_clean_doc_comment_removes_backticks() {
        let doc = "Use `HashMap` for key-value storage.";
        let cleaned = RegularGroupTemplate::clean_doc_comment(doc);
        assert!(cleaned.contains("HashMap"));
        assert!(!cleaned.contains("`"));
    }

    #[test]
    fn test_bm25_doc_comment_preserves_symbols() {
        let doc = "Access arr[0] with std::collections::HashMap.";
        let cleaned = RegularGroupTemplate::clean_doc_comment(doc);
        assert!(cleaned.contains("arr[0]"));
        assert!(cleaned.contains("std::collections::HashMap"));
    }

    #[test]
    fn test_regular_template_does_not_fallback_to_raw_signature() {
        let group = EntityGroup {
            header: Some(cce_types::entity::GroupedEntity {
                name: "documented_function".to_string(),
                signature: "fn documented_function() -> LeakedSignatureType".to_string(),
                ..Default::default()
            }),
            name: "documented_function".into(),
            kind: EntityKind::Function,
            ..Default::default()
        };

        let template = RegularGroupTemplate::new();
        let text = template.generate(&group);

        assert!(text.contains("documented_function"));
        assert!(!text.contains("leakedsignaturetype"));
    }

    #[test]
    fn test_bm25_excludes_import_like_members() {
        // Import-like members carry no retrieval value in the BM25
        // path (file-level summary and the relation index cover them instead).
        // Excluding them keeps import text out of retrieval chunks even when
        // an import entity remains a member of a non-import group.
        let group = EntityGroup {
            name: "Config".into(),
            kind: EntityKind::Struct,
            header: Some(cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(1),
                name: "Config".to_string(),
                kind: EntityKind::Struct,
                ..Default::default()
            }),
            members: [
                cce_types::entity::GroupedEntity {
                    id: cce_types::entity::EntityId(2),
                    name: "use std::fmt;".to_string(),
                    kind: EntityKind::Import,
                    ..Default::default()
                },
                cce_types::entity::GroupedEntity {
                    id: cce_types::entity::EntityId(3),
                    name: "pub use crate::x".to_string(),
                    kind: EntityKind::Export,
                    ..Default::default()
                },
                cce_types::entity::GroupedEntity {
                    id: cce_types::entity::EntityId(4),
                    name: "timeout".to_string(),
                    kind: EntityKind::Field,
                    ..Default::default()
                },
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };

        let template = RegularGroupTemplate::new();
        let text = template.generate(&group);

        assert!(
            text.contains("timeout"),
            "real members must still be indexed in the BM25 text"
        );
        assert!(
            !text.contains("std::fmt") && !text.contains("crate::x"),
            "import-like members must not appear in the BM25 text"
        );
    }
}
