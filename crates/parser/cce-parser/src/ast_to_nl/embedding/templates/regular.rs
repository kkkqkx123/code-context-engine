//! Regular entity templates for embedding generation
//!
//! Provides templates for regular entities without detected patterns:
//! - Class, Struct, Enum (Type definitions)
//! - Function, Method (Callable entities)
//! - Module, Trait (Container entities)
//!
//! # Design Principles
//!
//! - Preserve original identifiers (no name normalization for embedding path)
//! - Preserve type signatures in code form (no "Takes X of type Y" conversion)
//! - Append behavioral descriptions (doc comments) as-is — they are already natural language
//! - Filter member descriptions based on MemberRole
//!
//! # Output Format
//!
//! ```text
//! function get_mut(&mut self) -> Option<&mut T). Attribute: inline.
//! Returns a mutable reference to the underlying value.
//! ```
//!
//! # Architecture
//!
//! See docs/ast_to_nl/group_templates.md for detailed design.

use super::group_trait::GroupTemplate;
use crate::ast_to_nl::common::{GroupTemplateBase, TemplateHelpers};
use crate::ast_to_nl::noise::NoiseProfile;
use crate::grouper::types::{EntityGroup, GroupType};
use cce_types::entity::{EntityKind, meta_keys};
use cce_utils::normalize_whitespace;

/// Regular entity group template
///
/// Generates descriptions for regular entities without detected patterns.
/// Handles classes, structs, functions, modules, traits, and enums.
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
    fn generate(&self, group: &EntityGroup) -> Vec<String> {
        // File documentation groups output their doc comment directly
        if group.group_type == GroupType::FileDocumentation {
            if let Some(ref header) = group.header {
                if let Some(ref doc) = header.doc_comment {
                    return vec![doc.clone()];
                }
            }
            return Vec::new();
        }

        let profile = NoiseProfile::for_language(group.language);
        let mut results = Vec::new();

        let group_desc = self.generate_group_description(group, profile);
        let group_desc_clone = group_desc.clone();
        if !group_desc.is_empty() {
            results.push(group_desc);
        }

        let mut suppressed_modules = Vec::new();

        for member in self.members_for_description(group) {
            // Collect pure declaration names for summary
            if Self::is_pure_declaration(member) {
                suppressed_modules.push(member.name.as_str());
                continue;
            }

            let member_desc = self.generate_member_description(member, group, profile);
            if !member_desc.is_empty()
                && !Self::is_duplicate_member(&group_desc_clone, &member_desc, member)
            {
                results.push(member_desc);
            }
        }

        if !suppressed_modules.is_empty() {
            results.push(format!(
                "Contains modules: {}.",
                suppressed_modules.join(", ")
            ));
        }

        results
    }
}

impl RegularGroupTemplate {
    fn generate_group_description(&self, group: &EntityGroup, profile: NoiseProfile) -> String {
        // Suppress pure module declarations (no doc comment, no annotations)
        if group.kind == EntityKind::Module
            && group.header.as_ref().is_some_and(|h| {
                h.doc_comment.is_none() && !h.metadata.contains_key(meta_keys::ANNOTATIONS)
            })
        {
            return String::new();
        }

        let mut desc = Self::semantic_entity_description(
            group.name.as_str(),
            group.kind,
            group.header.as_ref(),
        );

        if let Some(doc) = group
            .header
            .as_ref()
            .and_then(|h| Self::clean_doc_comment(h.doc_comment.as_deref(), profile))
        {
            if !Self::is_text_duplicate(&desc, &doc) {
                desc.push('\n');
                desc.push_str(&doc);
            }
        }

        desc
    }

    /// Generate member description
    fn generate_member_description(
        &self,
        member: &cce_types::entity::GroupedEntity,
        group: &EntityGroup,
        profile: NoiseProfile,
    ) -> String {
        // Pure declarations (e.g., `pub mod globset;`) produce zero-information NL
        if Self::is_pure_declaration(member) {
            return String::new();
        }

        let mut member_desc =
            Self::semantic_entity_description(&member.name, member.kind, Some(member));

        if let Some(doc) = Self::clean_doc_comment(member.doc_comment.as_deref(), profile) {
            if !Self::is_text_duplicate(&member_desc, &doc) {
                member_desc.push('\n');
                member_desc.push_str(&doc);
            }
        }

        // Preserve call path info in compact form for identifier-level recall
        if let Some(call_paths) = member.metadata.get(meta_keys::CALL_PATHS) {
            if !call_paths.is_empty() {
                member_desc.push_str(&format!("\ncalls: {}", call_paths));
            }
        }

        Self::append_member_group_name(member_desc, member, group)
    }

    fn members_for_description<'a>(
        &self,
        group: &'a EntityGroup,
    ) -> Vec<&'a cce_types::entity::GroupedEntity> {
        let role_map = group.build_role_map();
        group
            .members
            .iter()
            .filter(|member| {
                if member.kind == EntityKind::TraitImpl {
                    return false;
                }
                // Skip import-like entities — they carry no semantic retrieval
                // value in the embedding path and produce verbose structured text.
                if matches!(
                    member.kind,
                    EntityKind::Import
                        | EntityKind::Require
                        | EntityKind::Include
                        | EntityKind::Export
                ) {
                    return false;
                }
                // Skip boilerplate methods — they are compressed into group description
                if let Some(role) = role_map.get(&member.id) {
                    if role.is_boilerplate() {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    fn is_duplicate_member(
        group_desc: &str,
        member_desc: &str,
        member: &cce_types::entity::GroupedEntity,
    ) -> bool {
        // Use original identifier for duplicate detection
        if group_desc.contains(&member.name) {
            return true;
        }

        // Check text containment between normalized descriptions to catch
        // members whose full description is already subsumed by group description
        let group_norm = normalize_whitespace(group_desc).to_lowercase();
        let member_norm = normalize_whitespace(member_desc).to_lowercase();
        if !member_norm.is_empty()
            && !group_norm.is_empty()
            && (group_norm.contains(&member_norm) || member_norm.contains(&group_norm))
        {
            return true;
        }

        false
    }

    /// Check if two texts are duplicates via substring matching.
    ///
    /// After normalization, if one text is a substring of the other,
    /// they are considered duplicates and one should be skipped.
    fn is_text_duplicate(a: &str, b: &str) -> bool {
        let a_norm = normalize_whitespace(a).to_lowercase();
        let b_norm = normalize_whitespace(b).to_lowercase();
        if a_norm.is_empty() || b_norm.is_empty() {
            return false;
        }
        a_norm.contains(&b_norm) || b_norm.contains(&a_norm)
    }

    fn clean_doc_comment(doc: Option<&str>, profile: NoiseProfile) -> Option<String> {
        let doc = doc?;
        let cleaned = Self::clean_doc_text(doc);
        // Remove empty markdown section markers and, when enabled by the
        // profile, safety boilerplate.
        let cleaned = crate::ast_to_nl::embedding::filter_embedding_noise(&cleaned, profile);
        let cleaned = cleaned.trim();

        if cleaned.is_empty() {
            None
        } else if cleaned.ends_with('.') {
            Some(cleaned.to_string())
        } else {
            Some(format!("{}.", cleaned))
        }
    }

    fn semantic_entity_description(
        name: &str,
        kind: EntityKind,
        entity: Option<&cce_types::entity::GroupedEntity>,
    ) -> String {
        let kind_text =
            Self::kind_label_with_subtype(kind, entity.and_then(|e| e.subtype.as_deref()));

        let mut base = format!("{} {}", kind_text, name);

        // For fields, include type information from parameters
        if matches!(kind, EntityKind::Field | EntityKind::Property) {
            if let Some(entity) = entity {
                if let Some((_, Some(field_type))) = entity.parameters.first() {
                    base.push_str(&format!(": {}", field_type));
                }
            }
        } else if let Some(entity) = entity {
            // Build signature from structured parameters and return type for non-field entities
            if !entity.parameters.is_empty() || entity.return_type.is_some() {
                let sig = TemplateHelpers::build_signature_from_fields(
                    &entity.parameters,
                    entity.return_type.as_deref(),
                );
                base.push(' ');
                base.push_str(&sig);
            }
        }

        let modifier_prefix = entity.and_then(|e| Self::modifier_text(&e.modifiers));

        match modifier_prefix {
            Some(prefix) => format!("{} {}.", prefix, base),
            None => {
                if base.ends_with('.') {
                    base
                } else {
                    format!("{}.", base)
                }
            }
        }
    }

    fn modifier_text(modifiers: &[String]) -> Option<String> {
        let known: std::collections::BTreeSet<&str> = [
            "pub",
            "pub(crate)",
            "pub(super)",
            "pub(self)",
            "private",
            "protected",
            "static",
            "async",
            "abstract",
            "virtual",
            "override",
            "const",
            "unsafe",
            "default",
        ]
        .into_iter()
        .collect();

        let relevant: Vec<&str> = modifiers
            .iter()
            .filter_map(|m| {
                let trimmed = m.trim();
                if known.contains(trimmed) {
                    Some(trimmed)
                } else {
                    None
                }
            })
            .collect();

        if relevant.is_empty() {
            None
        } else {
            Some(relevant.join(" "))
        }
    }

    fn kind_label_with_subtype(kind: EntityKind, subtype: Option<&str>) -> String {
        let base = kind.to_string();
        match subtype {
            Some(s) if !s.is_empty() => format!("{} {}", s, base),
            _ => base.to_string(),
        }
    }

    /// Check if an entity is a pure declaration with no informational content.
    ///
    /// Forward module declarations like `pub mod globset;` produce NL output
    /// that is tautological ("module globset.") — the kind label plus name
    /// adds no information beyond the identifier itself.
    ///
    /// A module is considered "pure" when it has no doc comment and no
    /// meaningful annotations/attributes beyond the declaration itself.
    fn is_pure_declaration(member: &cce_types::entity::GroupedEntity) -> bool {
        match member.kind {
            EntityKind::Module => {
                member.doc_comment.is_none()
                    && !member.metadata.contains_key(meta_keys::ANNOTATIONS)
            }
            _ => false,
        }
    }

    fn append_member_group_name(
        member_desc: String,
        member: &cce_types::entity::GroupedEntity,
        group: &EntityGroup,
    ) -> String {
        if group.name.is_empty() || group.name == member.name {
            member_desc
        } else {
            format!("{}\n{}.{}", member_desc, group.name, member.name)
        }
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
        let results = template.generate(&group);

        assert!(!results.is_empty());
        assert!(results[0].contains("UserService"));
        assert!(results[0].contains("class"));
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
        let results = template.generate(&group);

        assert!(!results.is_empty());
        assert!(results[0].contains("Status"));
        assert!(results[0].contains("enum"));
        assert!(
            results.len() >= 2,
            "expected at least 2 results, got {}: {:?}",
            results.len(),
            results
        );
        assert!(
            results[1].contains("Active"),
            "results[1] expected 'Active', got {:?}",
            results[1]
        );
        assert!(
            results[1].contains("Status.Active"),
            "results[1] expected a compact path, got {:?}",
            results[1]
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
        let results = template.generate(&group);

        assert!(!results.is_empty());
        assert!(results[0].contains("calculate_total"));
    }

    #[test]
    fn test_member_with_doc_comment_preserves_entity_name_and_kind() {
        let group = EntityGroup {
            name: "OnceCell".into(),
            kind: EntityKind::Struct,
            members: vec![cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(1),
                name: "get_mut".to_string(),
                kind: EntityKind::Method,
                doc_comment: Some(
                    "Returns a mutable reference to the underlying value.".to_string(),
                ),
                ..Default::default()
            }]
            .into(),
            ..Default::default()
        };

        let template = RegularGroupTemplate::new();
        let results = template.generate(&group);

        assert!(
            results.len() >= 2,
            "expected group + member, got {:?}",
            results
        );
        let member_desc = &results[1];
        assert!(
            member_desc.contains("get_mut"),
            "member description should contain original identifier, got: {}",
            member_desc
        );
        assert!(
            member_desc.contains("method"),
            "member description should contain kind label, got: {}",
            member_desc
        );
        assert!(
            member_desc.contains("Returns a mutable reference"),
            "member description should contain doc comment, got: {}",
            member_desc
        );
    }

    #[test]
    fn test_semantic_entity_description_preserves_original_name() {
        let field = cce_types::entity::GroupedEntity {
            name: "state".to_string(),
            kind: EntityKind::Field,
            modifiers: vec!["pub".to_string()],
            ..Default::default()
        };
        let constant = cce_types::entity::GroupedEntity {
            name: "INCOMPLETE".to_string(),
            kind: EntityKind::Constant,
            ..Default::default()
        };
        let method = cce_types::entity::GroupedEntity {
            name: "get_mut".to_string(),
            kind: EntityKind::Method,
            ..Default::default()
        };

        assert_eq!(
            RegularGroupTemplate::semantic_entity_description(
                &field.name,
                field.kind,
                Some(&field)
            ),
            "pub field state."
        );
        assert_eq!(
            RegularGroupTemplate::semantic_entity_description(
                &constant.name,
                constant.kind,
                Some(&constant)
            ),
            "constant INCOMPLETE."
        );
        assert_eq!(
            RegularGroupTemplate::semantic_entity_description(
                &method.name,
                method.kind,
                Some(&method)
            ),
            "method get_mut."
        );
    }

    #[test]
    fn test_semantic_entity_description_field_with_type() {
        use compact_str::CompactString;

        let field = cce_types::entity::GroupedEntity {
            name: "value".to_string(),
            kind: EntityKind::Field,
            parameters: smallvec::smallvec![(
                "value".into(),
                Some(CompactString::new("Option<u32>"))
            )],
            ..Default::default()
        };

        let desc = RegularGroupTemplate::semantic_entity_description(
            &field.name,
            field.kind,
            Some(&field),
        );
        assert!(
            desc.contains("Option<u32>"),
            "field description should include type, got: {}",
            desc
        );
    }

    #[test]
    fn test_build_signature_from_fields() {
        use compact_str::CompactString;

        assert_eq!(
            TemplateHelpers::build_signature_from_fields::<CompactString, CompactString>(&[], None,),
            "()"
        );
        let params = vec![
            (CompactString::new("x"), Some(CompactString::new("i32"))),
            (CompactString::new("y"), Some(CompactString::new("String"))),
        ];
        assert_eq!(
            TemplateHelpers::build_signature_from_fields(&params, Some("bool")),
            "(x: i32, y: String) -> bool"
        );
        // Single param, no return type
        let params = vec![(
            CompactString::new("self"),
            Some(CompactString::new("&mut Self")),
        )];
        assert_eq!(
            TemplateHelpers::build_signature_from_fields::<CompactString, CompactString>(
                &params, None
            ),
            "(self: &mut Self)"
        );
        // Empty param name, no return
        assert_eq!(
            TemplateHelpers::build_signature_from_fields::<CompactString, CompactString>(&[], None),
            "()"
        );
    }

    #[test]
    fn test_member_with_duplicate_doc_comment_is_deduplicated() {
        let group = EntityGroup {
            name: "OnceCell".into(),
            kind: EntityKind::Struct,
            members: vec![cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(1),
                name: "get_mut".to_string(),
                kind: EntityKind::Method,
                doc_comment: Some("get_mut method.".to_string()),
                ..Default::default()
            }]
            .into(),
            ..Default::default()
        };

        let template = RegularGroupTemplate::new();
        let results = template.generate(&group);

        assert!(
            results.len() >= 2,
            "expected group + member, got {:?}",
            results
        );
        let member_desc = &results[1];
        let get_mut_count = member_desc.matches("get_mut").count();
        // get_mut appears in: kind label, doc_comment, and path anchor
        assert_eq!(
            get_mut_count, 3,
            "expected 'get_mut' in kind label, doc_comment, and path, got: {}",
            member_desc
        );
    }

    #[test]
    fn test_group_with_doc_comment_does_not_emit_relationship_metadata() {
        let mut group = EntityGroup {
            name: "OnceCell".into(),
            kind: EntityKind::Struct,
            header: Some(cce_types::entity::GroupedEntity {
                id: cce_types::entity::EntityId(0),
                name: "OnceCell".to_string(),
                kind: EntityKind::Struct,
                doc_comment: Some("A cell that can be written to only once.".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        group.metadata.insert(
            "trait_impls".to_string(),
            "Debug, RefUnwindSafe".to_string(),
        );

        let template = RegularGroupTemplate::new();
        let results = template.generate(&group);

        assert!(!results.is_empty());
        let group_desc = &results[0];
        assert!(
            !group_desc.contains("Trait implementations"),
            "group description should not emit trait_impls metadata, got: {}",
            group_desc
        );
        assert!(
            !group_desc.contains("Debug"),
            "group description should not contain Debug trait metadata, got: {}",
            group_desc
        );
        assert!(
            !group_desc.contains("RefUnwindSafe"),
            "group description should not contain RefUnwindSafe trait metadata, got: {}",
            group_desc
        );
    }
}
