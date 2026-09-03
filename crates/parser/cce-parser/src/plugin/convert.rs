//! Plugin-entity conversion helpers
//!
//! The host converts [`PluginEntity`]s produced by the `FormatParse`,
//! `EntityExtract`, and custom-language extraction capabilities into the
//! existing pipeline types (`GroupedEntity` / `EntityGroup` + `entity_spans`)
//! so downstream stages remain unaware of the plugin origin.
//!
//! The reverse direction (`Entity` → [`PluginEntity`], `RawRelationData` →
//! [`PluginRelation`]) feeds the `Group` full-override tier, which receives a
//! serialized view of the parsed file.

use cce_types::entity::{Entity, EntityId, EntityKind, GroupedEntity, RawRelationData};
use cce_types::grouper::{EntityGroup, GroupType};
use cce_types::language::Language;
use cce_types::plugin::{PluginEntity, PluginRelation};
use compact_str::CompactString;

/// Map a free-form plugin entity kind string to the closest [`EntityKind`].
///
/// Unknown kinds map to [`EntityKind::Unknown`], which still participates in
/// NL generation and chunking (kind is informational).
pub fn entity_kind_from_plugin_kind(kind: &str) -> EntityKind {
    match kind {
        "function" | "route" | "handler" => EntityKind::Function,
        "method" => EntityKind::Method,
        "class" => EntityKind::Class,
        "struct" => EntityKind::Struct,
        "enum" => EntityKind::Enum,
        "interface" => EntityKind::Interface,
        "trait" => EntityKind::Trait,
        "module" | "section" | "namespace" | "package" => EntityKind::Module,
        "constant" => EntityKind::Constant,
        "variable" | "field" | "property" => EntityKind::Variable,
        "component" => EntityKind::Component,
        "directive" => EntityKind::Directive,
        "template" => EntityKind::Template,
        "macro" => EntityKind::Macro,
        _ => EntityKind::Unknown,
    }
}

/// Convert a [`PluginEntity`] to a [`GroupedEntity`].
pub fn plugin_entity_to_grouped_entity(entity: &PluginEntity, id: EntityId) -> GroupedEntity {
    GroupedEntity {
        id,
        name: entity.name.clone(),
        kind: entity_kind_from_plugin_kind(&entity.kind),
        signature: entity.signature.clone().unwrap_or_default(),
        doc_comment: entity.doc_comment.clone(),
        metadata: entity.metadata.clone(),
        ..Default::default()
    }
}

/// Convert a [`PluginEntity`] into a standalone [`EntityGroup`].
pub fn plugin_entity_to_group(
    entity: &PluginEntity,
    language: Language,
    id: EntityId,
    group_id: String,
) -> EntityGroup {
    let header = plugin_entity_to_grouped_entity(entity, id);
    let mut group = EntityGroup::new(group_id, GroupType::Standalone);
    group.name = CompactString::from(entity.name.clone());
    group.kind = header.kind;
    group.language = language;
    group.header = Some(header);
    group.header_id = Some(id);
    if let Some(span) = entity.span {
        group.span = span;
        group.entity_spans.insert(id, span);
    }
    group
}

/// Allocate plugin-entity IDs starting after the highest existing entity ID.
pub fn allocate_plugin_ids(
    existing: impl IntoIterator<Item = EntityId>,
    count: usize,
) -> Vec<EntityId> {
    let max = existing.into_iter().map(|id| id.0).max().unwrap_or(0);
    (1..=count)
        .map(|offset| EntityId(max.saturating_add(offset as u64)))
        .collect()
}

/// Convert a parsed [`Entity`] into a [`PluginEntity`] for the `Group`
/// override tier.
///
/// `kind` uses the plugin-facing free-form string (via `EntityKind` display);
/// `id` is the stringified file-local entity id. Children are resolved from
/// the `children` id list (looked up in `entities_by_id`).
pub fn entity_to_plugin_entity(
    entity: &Entity,
    entities_by_id: &std::collections::HashMap<EntityId, &Entity>,
) -> PluginEntity {
    let children = entity
        .children
        .iter()
        .filter_map(|cid| entities_by_id.get(cid).copied())
        .map(|child| entity_to_plugin_entity(child, entities_by_id))
        .collect();
    let mut metadata = entity.metadata.clone();
    for m in &entity.modifiers {
        metadata
            .entry("modifiers".to_string())
            .and_modify(|v| v.push_str(&format!(",{m}")))
            .or_insert_with(|| m.clone());
    }
    if let Some(st) = &entity.subtype {
        metadata.insert("subtype".to_string(), st.clone());
    }
    PluginEntity {
        id: entity.id.0.to_string(),
        kind: format!("{:?}", entity.kind).to_lowercase(),
        name: entity.name.clone(),
        signature: Some(entity.signature.clone()),
        doc_comment: entity.doc_comment.clone(),
        metadata,
        span: Some(entity.span),
        children,
    }
}

/// Convert a [`RawRelationData`] into a [`PluginRelation`] for the `Group`
/// override tier.
pub fn raw_relation_to_plugin_relation(rel: &RawRelationData) -> PluginRelation {
    PluginRelation {
        from: rel.src.0.to_string(),
        to: rel.dst_name.clone(),
        relation_type: format!("{:?}", rel.relation_type).to_lowercase(),
        metadata: std::collections::HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::{EntityId, EntityKind};
    use cce_types::language::Language;
    use cce_types::plugin::PluginEntity;
    use cce_types::relation::RelationType;

    #[test]
    fn test_entity_kind_mapping_known_kinds() {
        assert_eq!(
            entity_kind_from_plugin_kind("function"),
            EntityKind::Function
        );
        assert_eq!(entity_kind_from_plugin_kind("route"), EntityKind::Function);
        assert_eq!(
            entity_kind_from_plugin_kind("handler"),
            EntityKind::Function
        );
        assert_eq!(entity_kind_from_plugin_kind("method"), EntityKind::Method);
        assert_eq!(entity_kind_from_plugin_kind("class"), EntityKind::Class);
        assert_eq!(entity_kind_from_plugin_kind("struct"), EntityKind::Struct);
        assert_eq!(entity_kind_from_plugin_kind("enum"), EntityKind::Enum);
        assert_eq!(
            entity_kind_from_plugin_kind("interface"),
            EntityKind::Interface
        );
        assert_eq!(entity_kind_from_plugin_kind("trait"), EntityKind::Trait);
        assert_eq!(entity_kind_from_plugin_kind("module"), EntityKind::Module);
        assert_eq!(entity_kind_from_plugin_kind("section"), EntityKind::Module);
        assert_eq!(
            entity_kind_from_plugin_kind("namespace"),
            EntityKind::Module
        );
        assert_eq!(entity_kind_from_plugin_kind("package"), EntityKind::Module);
        assert_eq!(
            entity_kind_from_plugin_kind("constant"),
            EntityKind::Constant
        );
        assert_eq!(
            entity_kind_from_plugin_kind("variable"),
            EntityKind::Variable
        );
        assert_eq!(entity_kind_from_plugin_kind("field"), EntityKind::Variable);
        assert_eq!(
            entity_kind_from_plugin_kind("property"),
            EntityKind::Variable
        );
        assert_eq!(
            entity_kind_from_plugin_kind("component"),
            EntityKind::Component
        );
        assert_eq!(
            entity_kind_from_plugin_kind("directive"),
            EntityKind::Directive
        );
        assert_eq!(
            entity_kind_from_plugin_kind("template"),
            EntityKind::Template
        );
        assert_eq!(entity_kind_from_plugin_kind("macro"), EntityKind::Macro);
    }

    #[test]
    fn test_entity_kind_mapping_unknown_kind() {
        assert_eq!(
            entity_kind_from_plugin_kind("route_handler"),
            EntityKind::Unknown
        );
        assert_eq!(entity_kind_from_plugin_kind(""), EntityKind::Unknown);
    }

    #[test]
    fn test_plugin_entity_to_grouped_entity_fields() {
        let entity = PluginEntity::new("1", "function", "load")
            .with_signature("fn load()")
            .with_doc_comment("Loads data.")
            .with_metadata("framework", "axum");
        let converted = plugin_entity_to_grouped_entity(&entity, EntityId(42));
        assert_eq!(converted.id, EntityId(42));
        assert_eq!(converted.name, "load");
        assert_eq!(converted.kind, EntityKind::Function);
        assert_eq!(converted.signature, "fn load()");
        assert_eq!(converted.doc_comment.as_deref(), Some("Loads data."));
        assert_eq!(
            converted.metadata.get("framework").map(String::as_str),
            Some("axum")
        );
    }

    #[test]
    fn test_plugin_entity_to_group_standalone_with_span() {
        let span = Span::new(10, 40, 1, 0, 3, 0);
        let entity = PluginEntity::new("1", "class", "User").with_span(span);
        let group = plugin_entity_to_group(
            &entity,
            Language::Python,
            EntityId(7),
            "plugin_1".to_string(),
        );
        assert_eq!(group.group_id.as_str(), "plugin_1");
        assert_eq!(group.name.as_str(), "User");
        assert_eq!(group.kind, EntityKind::Class);
        assert_eq!(group.language, Language::Python);
        assert_eq!(group.header_id, Some(EntityId(7)));
        let header = group.header.expect("standalone group has a header");
        assert_eq!(header.id, EntityId(7));
        assert_eq!(header.kind, EntityKind::Class);
        assert_eq!(group.span, span);
        assert_eq!(group.entity_spans.get(&EntityId(7)), Some(&span));
    }

    #[test]
    fn test_plugin_entity_to_group_without_span() {
        let entity = PluginEntity::new("1", "route", "/users");
        let group = plugin_entity_to_group(
            &entity,
            Language::Python,
            EntityId(1),
            "plugin_1".to_string(),
        );
        assert!(group.entity_spans.is_empty());
        assert_eq!(group.span, Span::default());
    }

    #[test]
    fn test_allocate_plugin_ids_after_existing_max() {
        let ids = allocate_plugin_ids(vec![EntityId(3), EntityId(10), EntityId(5)], 3);
        assert_eq!(ids, vec![EntityId(11), EntityId(12), EntityId(13)]);
    }

    #[test]
    fn test_allocate_plugin_ids_empty_existing() {
        let ids = allocate_plugin_ids(Vec::<EntityId>::new(), 2);
        assert_eq!(ids, vec![EntityId(1), EntityId(2)]);
    }

    #[test]
    fn test_entity_to_plugin_entity_roundtrip_fields() {
        let mut entity = Entity::new(
            EntityId(1),
            EntityKind::Function,
            "main".to_string(),
            Span::new(0, 10, 0, 0, 0, 10),
        );
        entity.signature = "fn main()".to_string();
        entity.doc_comment = Some("Entry point.".to_string());
        entity
            .metadata
            .insert("deprecated".to_string(), "true".to_string());
        entity.modifiers.push("pub".to_string());
        entity.subtype = Some("async".to_string());

        let by_id = std::collections::HashMap::from([(EntityId(1), &entity)]);
        let plugin = entity_to_plugin_entity(&entity, &by_id);
        assert_eq!(plugin.id, "1");
        assert_eq!(plugin.kind, "function");
        assert_eq!(plugin.name, "main");
        assert_eq!(plugin.signature.as_deref(), Some("fn main()"));
        assert_eq!(plugin.doc_comment.as_deref(), Some("Entry point."));
        assert_eq!(
            plugin.metadata.get("deprecated").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            plugin.metadata.get("modifiers").map(String::as_str),
            Some("pub")
        );
        assert_eq!(
            plugin.metadata.get("subtype").map(String::as_str),
            Some("async")
        );
        assert_eq!(plugin.span, Some(Span::new(0, 10, 0, 0, 0, 10)));
    }

    #[test]
    fn test_entity_to_plugin_entity_resolves_children() {
        let child = Entity::new(
            EntityId(2),
            EntityKind::Method,
            "inner".to_string(),
            Span::new(4, 8, 0, 0, 0, 8),
        );
        let mut parent = Entity::new(
            EntityId(1),
            EntityKind::Class,
            "Outer".to_string(),
            Span::new(0, 10, 0, 0, 0, 10),
        );
        parent.children.push(EntityId(2));

        let by_id =
            std::collections::HashMap::from([(EntityId(1), &parent), (EntityId(2), &child)]);
        let plugin = entity_to_plugin_entity(&parent, &by_id);
        assert_eq!(plugin.children.len(), 1);
        assert_eq!(plugin.children[0].id, "2");
        assert_eq!(plugin.children[0].kind, "method");
        assert_eq!(plugin.children[0].name, "inner");
    }

    #[test]
    fn test_entity_to_plugin_entity_skips_missing_children() {
        let mut parent = Entity::new(
            EntityId(1),
            EntityKind::Class,
            "Outer".to_string(),
            Span::new(0, 10, 0, 0, 0, 10),
        );
        parent.children.push(EntityId(999));
        let by_id = std::collections::HashMap::from([(EntityId(1), &parent)]);
        let plugin = entity_to_plugin_entity(&parent, &by_id);
        assert!(plugin.children.is_empty());
    }

    #[test]
    fn test_raw_relation_to_plugin_relation() {
        let rel = RawRelationData {
            src: EntityId(1),
            level: cce_types::RelationLevel::Entity,
            dst_name: "Helper".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::new(0, 2, 0, 0, 0, 2),
            stdlib_category: None,
        };
        let plugin = raw_relation_to_plugin_relation(&rel);
        assert_eq!(plugin.from, "1");
        assert_eq!(plugin.to, "Helper");
        assert_eq!(plugin.relation_type, "directcall");
        assert!(plugin.metadata.is_empty());
    }
}
