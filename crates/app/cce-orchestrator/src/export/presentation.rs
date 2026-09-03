//! Presentation text generator for export and compression
//!
//! This module builds lightweight, human-readable descriptions for display
//! surfaces. It deliberately avoids control-flow and behavior sidecar data.

use std::collections::HashSet;

use cce_parser::ast_to_nl::converter::GroupConversions;
use cce_parser::ast_to_nl::{TemplateHelpers, clean_comment_content};
use cce_parser::grouper::types::EntityGroup;
use cce_types::ast_to_nl::result::ConversionResult;
use cce_types::entity::{EntityId, EntityKind, GroupedEntity};

/// Generator for presentation-oriented text.
#[derive(Debug, Default, Clone, Copy)]
pub struct PresentationConverter;

impl PresentationConverter {
    /// Create a new presentation converter.
    pub fn new() -> Self {
        Self
    }

    /// Convert grouped entities into lightweight conversion results.
    pub fn convert_entity_groups(
        &self,
        groups: &[EntityGroup],
        file_path: &str,
    ) -> Vec<GroupConversions> {
        groups
            .iter()
            .map(|group| self.convert_group(group, file_path))
            .collect()
    }

    fn convert_group(&self, group: &EntityGroup, file_path: &str) -> GroupConversions {
        let header_conversion = group
            .header
            .as_ref()
            .map(|header| self.convert_entity(header, group, file_path, true, group.header_id));

        let member_conversions = group
            .members
            .iter()
            .map(|member| self.convert_entity(member, group, file_path, false, Some(member.id)))
            .collect();

        GroupConversions {
            group: group.clone(),
            header_conversion,
            member_conversions,
        }
    }

    fn convert_entity(
        &self,
        entity: &GroupedEntity,
        group: &EntityGroup,
        file_path: &str,
        is_header: bool,
        entity_id: Option<EntityId>,
    ) -> ConversionResult {
        let embedding_text = self.build_embedding_text(entity, group, is_header);
        let bm25_text = self.build_bm25_text(entity, group);
        let keywords = self.extract_keywords(entity);
        let span = self.resolve_span(group, entity_id);

        ConversionResult::new(
            entity.id,
            entity.kind,
            entity.name.clone(),
            file_path.to_string(),
            bm25_text,
            embedding_text,
            keywords,
        )
        .with_source_entity_ids(vec![entity.id])
        .with_source_span(span)
    }

    fn resolve_span(&self, group: &EntityGroup, entity_id: Option<EntityId>) -> cce_types::Span {
        entity_id
            .and_then(|id| group.entity_spans.get(&id).copied())
            .unwrap_or(group.span)
    }

    fn build_embedding_text(
        &self,
        entity: &GroupedEntity,
        group: &EntityGroup,
        is_header: bool,
    ) -> String {
        let mut parts = Vec::new();

        let title = self.entity_title(entity);
        parts.push(title);

        if let Some(doc) = self.cleaned_doc_comment(entity) {
            parts.push(doc);
        }

        if let Some(summary) = self.signature_summary(entity) {
            parts.push(summary);
        }

        if is_header && group.count_all_nested() > 0 {
            parts.push(format!(
                "Contains {} nested group(s).",
                group.count_all_nested()
            ));
        }

        self.join_sentences(parts)
    }

    fn build_bm25_text(&self, entity: &GroupedEntity, _group: &EntityGroup) -> String {
        let mut parts = Vec::new();

        parts.push(self.entity_title(entity));
        parts.push(self.signature_keywords(entity));

        if let Some(doc) = self.cleaned_doc_comment(entity) {
            parts.push(doc);
        }

        self.join_sentences(parts)
    }

    fn entity_title(&self, entity: &GroupedEntity) -> String {
        format!("{} {}.", self.kind_phrase(entity.kind), entity.name)
    }

    fn kind_phrase(&self, kind: EntityKind) -> &'static str {
        match kind {
            EntityKind::Function => "Function",
            EntityKind::Method => "Method",
            EntityKind::Constructor => "Constructor",
            EntityKind::Destructor => "Destructor",
            EntityKind::Operator => "Operator",
            EntityKind::Class => "Class",
            EntityKind::Struct => "Struct",
            EntityKind::Enum => "Enum",
            EntityKind::Interface => "Interface",
            EntityKind::Trait => "Trait",
            EntityKind::TraitImpl => "Trait implementation",
            EntityKind::InherentImpl => "Impl block",
            EntityKind::Module => "Module",
            EntityKind::Namespace => "Namespace",
            EntityKind::Package => "Package",
            EntityKind::Field => "Field",
            EntityKind::Property => "Property",
            EntityKind::Constant => "Constant",
            EntityKind::TypeAlias => "Type alias",
            EntityKind::TestSuite => "Test suite",
            EntityKind::TestCase => "Test case",
            EntityKind::TestHook => "Test hook",
            EntityKind::Variable => "Variable",
            EntityKind::Unknown => "Entity",
            _ => "Entity",
        }
    }

    fn cleaned_doc_comment(&self, entity: &GroupedEntity) -> Option<String> {
        let doc = entity.doc_comment.as_deref()?;
        let cleaned = clean_comment_content(doc);
        if cleaned.is_empty() {
            None
        } else {
            Some(self.ensure_sentence(cleaned))
        }
    }

    fn signature_summary(&self, entity: &GroupedEntity) -> Option<String> {
        if !entity.kind.is_function_like() {
            return None;
        }

        let mut parts = Vec::new();

        if !entity.parameters.is_empty() {
            let params = entity
                .parameters
                .iter()
                .map(|(name, ty)| {
                    ty.as_ref()
                        .map(|ty| format!("{}: {}", name, ty))
                        .unwrap_or_else(|| name.to_string())
                })
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("accepts {}", params));
        }

        if let Some(ret) = entity.return_type.as_deref().filter(|ret| !ret.is_empty()) {
            parts.push(format!("returns {}", ret));
        }

        if parts.is_empty() {
            None
        } else {
            Some(self.ensure_sentence(parts.join(" and ")))
        }
    }

    fn signature_keywords(&self, entity: &GroupedEntity) -> String {
        if !entity.kind.is_function_like() {
            return String::new();
        }

        let mut parts = Vec::new();
        parts.push(format!("{} {}", self.kind_phrase(entity.kind), entity.name));

        if !entity.parameters.is_empty() {
            let params = entity
                .parameters
                .iter()
                .map(|(name, ty)| {
                    ty.as_ref()
                        .map(|ty| format!("{} {}", name, ty))
                        .unwrap_or_else(|| name.to_string())
                })
                .collect::<Vec<_>>()
                .join(" ");
            parts.push(params);
        }

        if let Some(ret) = entity.return_type.as_deref().filter(|ret| !ret.is_empty()) {
            parts.push(ret.to_string());
        }

        parts.join(" ")
    }

    fn extract_keywords(&self, entity: &GroupedEntity) -> Vec<String> {
        let mut keywords = Vec::new();
        let mut seen = HashSet::new();

        for token in TemplateHelpers::extract_keywords(&entity.name) {
            if seen.insert(token.clone()) {
                keywords.push(token);
            }
        }

        if !entity.signature.is_empty() {
            for token in TemplateHelpers::extract_keywords(&entity.signature) {
                if seen.insert(token.clone()) {
                    keywords.push(token);
                }
            }
        }

        keywords
    }

    fn ensure_sentence(&self, text: String) -> String {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        if trimmed.ends_with('.') {
            trimmed.to_string()
        } else {
            format!("{}.", trimmed)
        }
    }

    fn join_sentences(&self, parts: Vec<String>) -> String {
        parts
            .into_iter()
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .map(|part| self.ensure_sentence(part))
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_parser::grouper::GroupType;
    use cce_types::Span;
    use cce_types::entity::EntityKind;
    use smallvec::smallvec;
    use std::collections::HashMap;

    fn make_entity(name: &str, kind: EntityKind, doc: Option<&str>) -> GroupedEntity {
        GroupedEntity {
            id: EntityId(1),
            name: name.to_string(),
            kind,
            signature: format!("{}()", name),
            parameters: smallvec![],
            return_type: Some("bool".to_string()),
            doc_comment: doc.map(|s| s.to_string()),
            modifiers: vec!["pub".to_string(), "async".to_string(), "inline".to_string()],
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: HashMap::new(),
        }
    }

    fn make_group(entity: GroupedEntity, kind: GroupType) -> EntityGroup {
        let span = Span::from_lines(1, 3);
        let mut entity_spans = HashMap::new();
        entity_spans.insert(entity.id, span);

        EntityGroup {
            group_id: "group_1".into(),
            group_type: kind,
            header: Some(entity.clone()),
            header_id: Some(entity.id),
            members: smallvec![],
            member_ids: smallvec![],
            entity_spans,
            combined_source: None,
            combined_source_lazy: std::sync::OnceLock::new(),
            span,
            kind: entity.kind,
            name: entity.name.clone().into(),
            language: cce_types::language::Language::Rust,
            pattern_info: Default::default(),
            member_roles: smallvec![],
            nested_groups: Box::new([]),
            nesting_level: 0,
            parent_group_id: None,
            has_significant_nested: false,
            metadata: HashMap::new(),
            test_info: cce_types::TestInfo::unknown(),
        }
    }

    #[test]
    fn test_convert_entity_groups_produces_text() {
        let converter = PresentationConverter::new();
        let entity = make_entity(
            "calculate_total",
            EntityKind::Function,
            Some("/// Calculate total."),
        );
        let group = make_group(entity, GroupType::Standalone);
        let conversions = converter.convert_entity_groups(&[group], "src/lib.rs");

        assert_eq!(conversions.len(), 1);
        let header = conversions[0]
            .header_conversion
            .as_ref()
            .expect("header conversion should exist");
        let text = header
            .embedding_text
            .as_ref()
            .expect("embedding text should exist");
        assert!(text.contains("Function calculate_total."));
        assert!(text.contains("Calculate total."));
    }

    #[test]
    fn test_keyword_extraction_keeps_name_tokens() {
        let converter = PresentationConverter::new();
        let entity = make_entity("getUserID", EntityKind::Function, None);
        let keywords = converter.extract_keywords(&entity);

        assert!(keywords.contains(&"get".to_string()));
        assert!(keywords.contains(&"user".to_string()));
        assert!(keywords.contains(&"id".to_string()));
    }
}
