//! BM25 regular entity templates
//!
//! Provides keyword-optimized templates for regular entities without patterns.

use super::group_trait::GroupTemplate;
use crate::ast_to_nl::common::GroupTemplateBase;
use crate::grouper::types::EntityGroup;

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
            Self::push_entity_features(&mut all_parts, header);
        } else {
            let name = group.name.as_str();
            if !name.is_empty() {
                all_parts.push(format!("{} ({}).", name, group.kind.kind_label()));
            } else {
                all_parts.push(group.kind.to_string());
            }
        }

        let refs: Vec<&str> = all_parts.iter().map(|s| s.as_str()).collect();
        refs.join(" ")
    }
}

impl RegularGroupTemplate {
    fn push_entity_features(
        all_parts: &mut Vec<String>,
        entity: &cce_types::entity::GroupedEntity,
    ) {
        if !entity.signature.is_empty() {
            all_parts.push(entity.signature.clone());
        } else if !entity.name.is_empty() {
            all_parts.push(format!("{} ({}).", entity.name, entity.kind.kind_label()));
        }

        if let Some(bases_str) = entity
            .metadata
            .get(cce_types::entity::meta_keys::BASE_CLASSES)
        {
            let bases: Vec<&str> = bases_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if !bases.is_empty() {
                all_parts.push(bases.join(" "));
            }
        }

        if let Some(ref doc) = entity.doc_comment {
            let clean_doc = Self::clean_doc_comment(doc);
            if !clean_doc.is_empty() {
                all_parts.push(clean_doc);
            }
        }
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

        assert!(text.contains("UserService"));
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

        assert!(text.contains("Status"));
        assert!(text.contains("enum"));
        assert!(
            !text.contains("Active") && !text.contains("Inactive"),
            "member names must not be spread into the group header text: {text}"
        );
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
    fn test_regular_template_keeps_signature_identity() {
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
        assert!(
            text.contains("LeakedSignatureType"),
            "identity line keeps the signature: {text}"
        );
    }

    #[test]
    fn test_bm25_excludes_import_like_members() {
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
            text.contains("Config"),
            "header identity must remain: {text}"
        );
        assert!(
            !text.contains("timeout"),
            "member names must not be spread into the header text: {text}"
        );
        assert!(
            !text.contains("std::fmt") && !text.contains("crate::x"),
            "import-like members must not appear in the BM25 text"
        );
    }
}
