//! BM25 regular entity templates
//!
//! Provides keyword-optimized templates for regular entities without patterns.

use super::group_trait::GroupTemplate;
use crate::ast_to_nl::common::GroupTemplateBase;
use crate::ast_to_nl::embedding::filter_embedding_noise;
use crate::ast_to_nl::noise::NoiseProfile;
use crate::grouper::types::EntityGroup;
use cce_types::entity::EntityKind;
use cce_types::language::Language;

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
            Self::push_entity_features(&mut all_parts, header, group.language);
        } else {
            let name = group.name.as_str();
            if !name.is_empty() {
                all_parts.push(format!("{} ({}).", name, group.kind.kind_label()));
            } else {
                all_parts.push(group.kind.to_string());
            }
        }

        Self::push_member_signals(&mut all_parts, group);

        let refs: Vec<&str> = all_parts.iter().map(|s| s.as_str()).collect();
        refs.join(" ")
    }
}

impl RegularGroupTemplate {
    /// Bounded member-name table and member type signals appended to the
    /// group header text (content field only).
    ///
    /// Exact-name and fuzzy queries against members of large classes
    /// otherwise lose every term: the header carries only the class
    /// signature. The table is intentionally bounded and stays out of the
    /// title/keywords fields so head precision is preserved while deep
    /// recall recovers member and overload terms.
    const MAX_MEMBER_NAMES: usize = 32;
    /// Bounded parameter/return type signals collected from members.
    const MAX_MEMBER_TYPES: usize = 16;

    fn push_entity_features(
        all_parts: &mut Vec<String>,
        entity: &cce_types::entity::GroupedEntity,
        language: Language,
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
                // Language-gated doc filtering mirrors the embedding path:
                // generic rules (empty markdown markers) always apply, while
                // language-specific rules (Rust safety boilerplate) apply
                // only for that language so rich docstrings elsewhere keep
                // their recall terms.
                let profile = NoiseProfile::for_language(language);
                let filtered = filter_embedding_noise(&clean_doc, profile);
                if !filtered.trim().is_empty() {
                    all_parts.push(filtered);
                }
            }
        }
    }

    fn push_member_signals(all_parts: &mut Vec<String>, group: &EntityGroup) {
        let mut seen = std::collections::HashSet::new();
        let mut names: Vec<String> = Vec::new();
        for member in group.members.iter() {
            if member.is_stdlib || Self::is_import_like(member.kind) {
                continue;
            }
            let name = member.name.trim();
            if name.is_empty() {
                continue;
            }
            if seen.insert(name.to_lowercase()) {
                names.push(name.to_string());
                if names.len() >= Self::MAX_MEMBER_NAMES {
                    break;
                }
            }
        }
        if !names.is_empty() {
            all_parts.push(format!("Members: {}.", names.join(" ")));
        }

        let mut seen_types = std::collections::HashSet::new();
        let mut types: Vec<String> = Vec::new();
        for member in group.members.iter() {
            if member.is_stdlib || Self::is_import_like(member.kind) {
                continue;
            }
            for (_, ty) in member.parameters.iter() {
                if let Some(ty) = ty {
                    let ty = ty.trim();
                    if !ty.is_empty() && seen_types.insert(ty.to_string()) {
                        types.push(ty.to_string());
                        if types.len() >= Self::MAX_MEMBER_TYPES {
                            break;
                        }
                    }
                }
            }
            if types.len() >= Self::MAX_MEMBER_TYPES {
                break;
            }
            if let Some(ret) = member.return_type.as_deref() {
                let ret = ret.trim();
                if !ret.is_empty() && seen_types.insert(ret.to_string()) {
                    types.push(ret.to_string());
                    if types.len() >= Self::MAX_MEMBER_TYPES {
                        break;
                    }
                }
            }
        }
        if !types.is_empty() {
            all_parts.push(format!("Types: {}.", types.join(" ")));
        }
    }

    fn is_import_like(kind: EntityKind) -> bool {
        matches!(
            kind,
            EntityKind::Import | EntityKind::Require | EntityKind::Include | EntityKind::Export
        )
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
            text.contains("Active") && text.contains("Inactive"),
            "bounded member table keeps variant names searchable: {text}"
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
            text.contains("timeout"),
            "semantic member names stay in the bounded member table: {text}"
        );
        assert!(
            !text.contains("std::fmt") && !text.contains("crate::x"),
            "import-like members must not appear in the BM25 text"
        );
    }

    #[test]
    fn test_regular_template_member_table_bounded() {
        let members: Vec<cce_types::entity::GroupedEntity> = (0..40)
            .map(|i| cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(i),
                name: format!("method_{i}"),
                kind: EntityKind::Method,
                ..Default::default()
            })
            .collect();
        let group = EntityGroup {
            name: "Big".into(),
            kind: EntityKind::Class,
            header: Some(cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(1000),
                name: "Big".to_string(),
                kind: EntityKind::Class,
                ..Default::default()
            }),
            members: members.into_iter().collect(),
            ..Default::default()
        };

        let template = RegularGroupTemplate::new();
        let text = template.generate(&group);

        assert!(text.contains("Members:"), "member table present: {text}");
        assert!(text.contains("method_0"), "early members kept: {text}");
        assert!(
            !text.contains("method_39"),
            "member table is bounded: {text}"
        );
    }

    #[test]
    fn test_regular_template_member_types() {
        let group = EntityGroup {
            name: "Builder".into(),
            kind: EntityKind::Struct,
            header: Some(cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(1),
                name: "Builder".to_string(),
                kind: EntityKind::Struct,
                ..Default::default()
            }),
            members: [cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(2),
                name: "add_line".to_string(),
                kind: EntityKind::Method,
                parameters: smallvec::smallvec![("from".into(), Some("PathBuf".into()))],
                return_type: Some("Result<GitignoreBuilder>".to_string()),
                ..Default::default()
            }]
            .into_iter()
            .collect(),
            ..Default::default()
        };

        let template = RegularGroupTemplate::new();
        let text = template.generate(&group);

        assert!(text.contains("add_line"), "member name kept: {text}");
        assert!(
            text.contains("PathBuf") && text.contains("Result<GitignoreBuilder>"),
            "member type signals kept: {text}"
        );
    }

    #[test]
    fn test_regular_template_rust_safety_boilerplate_dropped() {
        let group = EntityGroup {
            name: "Cell".into(),
            kind: EntityKind::Struct,
            language: cce_types::language::Language::Rust,
            header: Some(cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(1),
                name: "Cell".to_string(),
                kind: EntityKind::Struct,
                doc_comment: Some(
                    "Safe due to the write-once invariant.\n\nSets the contents.".to_string(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        };

        let template = RegularGroupTemplate::new();
        let text = template.generate(&group);

        assert!(
            !text.contains("Safe due to"),
            "rust safety boilerplate dropped: {text}"
        );
        assert!(text.contains("Sets the contents."));
    }

    #[test]
    fn test_regular_template_keeps_safety_boilerplate_for_non_rust() {
        let group = EntityGroup {
            name: "Cell".into(),
            kind: EntityKind::Class,
            language: cce_types::language::Language::Python,
            header: Some(cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(1),
                name: "Cell".to_string(),
                kind: EntityKind::Class,
                doc_comment: Some(
                    "Safe due to the batching guarantees.\nSets the contents.".to_string(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        };

        let template = RegularGroupTemplate::new();
        let text = template.generate(&group);

        assert!(
            text.contains("Safe due to"),
            "non-rust docs keep their wording: {text}"
        );
    }

    #[test]
    fn test_regular_template_drops_empty_doc_markers() {
        let group = EntityGroup {
            name: "Cell".into(),
            kind: EntityKind::Struct,
            language: cce_types::language::Language::Rust,
            header: Some(cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(1),
                name: "Cell".to_string(),
                kind: EntityKind::Struct,
                doc_comment: Some("# Example.\nSets the contents.".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let template = RegularGroupTemplate::new();
        let text = template.generate(&group);

        assert!(!text.contains("# Example"), "empty markers dropped: {text}");
        assert!(text.contains("Sets the contents."));
    }
}
