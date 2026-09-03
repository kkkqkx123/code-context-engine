//! CodeFormConverter - converts Entity/EntityGroup into structured code form descriptions

use crate::grouper::EntityGroup;
use cce_types::ParsedFile;
use cce_types::entity::{Entity, EntityKind, GroupedEntity};

use super::types::{CodeFormContext, CodeFormEntity, CodeFormGroup};

/// Converter for transforming Entity/EntityGroup into code form structures
///
/// This converter provides the lightweight bridge between parsed entities and
/// code form representations, enabling downstream consumers (Summary, Export, etc.)
/// to work with entity structure without re-parsing or duplicating work.
pub struct CodeFormConverter;

impl CodeFormConverter {
    /// Convert a single Entity to CodeFormEntity
    pub fn convert_entity(entity: &Entity, doc_comment: Option<&str>) -> CodeFormEntity {
        let summary_hint = doc_comment
            .and_then(|dc| dc.lines().next().map(|s| s.trim().to_string()))
            .or_else(|| {
                // Auto-generate summary hint from entity kind and name
                CodeFormConverter::generate_summary_hint(entity)
            })
            .unwrap_or_default();

        CodeFormEntity {
            id: entity.id,
            name: entity.name.clone(),
            kind: entity.kind,
            modifiers: entity.modifiers.clone(),
            type_annotation: entity.return_type.clone(),
            doc_comment: doc_comment.map(|s| s.to_string()),
            summary_hint,
            signature: if entity.signature.is_empty() {
                None
            } else {
                Some(entity.signature.clone())
            },
            parameters: entity.parameters.clone(),
            depth: entity.depth,
        }
    }

    /// Convert a grouped entity to CodeFormEntity
    pub fn convert_grouped_entity(
        entity: &GroupedEntity,
        doc_comment: Option<&str>,
    ) -> CodeFormEntity {
        let summary_hint = doc_comment
            .and_then(|dc| dc.lines().next().map(|s| s.trim().to_string()))
            .or_else(|| {
                // Auto-generate summary hint from entity kind and name
                CodeFormConverter::generate_summary_hint_from_kind(&entity.kind, &entity.name)
            })
            .unwrap_or_default();

        // Convert SmallVec parameters to Vec
        let parameters: Vec<(String, Option<String>)> = entity
            .parameters
            .iter()
            .map(|(name, ty)| (name.to_string(), ty.as_ref().map(|t| t.to_string())))
            .collect();

        CodeFormEntity {
            id: entity.id,
            name: entity.name.clone(),
            kind: entity.kind,
            modifiers: entity.modifiers.clone(),
            type_annotation: entity.return_type.clone(),
            doc_comment: doc_comment.map(|s| s.to_string()),
            summary_hint,
            signature: if entity.signature.is_empty() {
                None
            } else {
                Some(entity.signature.clone())
            },
            parameters,
            depth: 0, // GroupedEntity doesn't track depth, so we default to 0
        }
    }

    /// Convert an EntityGroup to CodeFormGroup
    pub fn convert_group(group: &EntityGroup, _parsed_file: &ParsedFile) -> CodeFormGroup {
        // Convert header entity
        let header = group.header.as_ref().unwrap_or_else(|| {
            panic!(
                "EntityGroup must have a header entity: group_id={}",
                group.group_id
            )
        });

        let header_entity = Self::convert_grouped_entity(header, header.doc_comment.as_deref());

        // Convert member entities (they are already GroupedEntity)
        let members: Vec<CodeFormEntity> = group
            .members
            .iter()
            .map(|member| Self::convert_grouped_entity(member, member.doc_comment.as_deref()))
            .collect();

        // Convert nested groups recursively
        let nested_groups: Vec<CodeFormGroup> = group
            .nested_groups
            .iter()
            .map(|nested| Self::convert_group(nested, _parsed_file))
            .collect();

        CodeFormGroup {
            header: header_entity,
            members,
            nested_groups,
            group_type: format!("{:?}", group.group_type),
        }
    }

    /// Convert multiple EntityGroups to CodeFormGroups
    pub fn convert_groups(groups: &[EntityGroup], parsed_file: &ParsedFile) -> Vec<CodeFormGroup> {
        groups
            .iter()
            .map(|group| Self::convert_group(group, parsed_file))
            .collect()
    }

    /// Convert from AstToNL's GroupedEntity representation
    pub fn convert_grouped_group(
        group: &EntityGroup,
        _grouped_entities: &[GroupedEntity],
        _parsed_file: &ParsedFile,
    ) -> CodeFormGroup {
        // Convert header
        let header = group
            .header
            .as_ref()
            .unwrap_or_else(|| panic!("EntityGroup must have a header entity"));

        let header_code_form = Self::convert_grouped_entity(header, header.doc_comment.as_deref());

        // Convert members (they are already GroupedEntity in EntityGroup)
        let members: Vec<CodeFormEntity> = group
            .members
            .iter()
            .map(|member| Self::convert_grouped_entity(member, member.doc_comment.as_deref()))
            .collect();

        // Convert nested groups
        let nested_groups = group
            .nested_groups
            .iter()
            .map(|nested| Self::convert_group(nested, _parsed_file))
            .collect();

        CodeFormGroup {
            header: header_code_form,
            members,
            nested_groups,
            group_type: format!("{:?}", group.group_type),
        }
    }

    /// Build context for conversion (optional utility)
    pub fn build_context(
        groups: &[CodeFormGroup],
        language: &str,
        file_path: &str,
    ) -> CodeFormContext {
        let mut context = CodeFormContext::new(language.to_string(), file_path.to_string());

        // Add all entities from all groups to the lookup map
        for group in groups {
            context.add_entity(group.header.clone());
            for member in &group.members {
                context.add_entity(member.clone());
            }

            // Also add from nested groups
            Self::add_nested_entities(group, &mut context);
        }

        context
    }

    /// Helper: add entities from nested groups to context
    fn add_nested_entities(group: &CodeFormGroup, context: &mut CodeFormContext) {
        for nested in &group.nested_groups {
            context.add_entity(nested.header.clone());
            for member in &nested.members {
                context.add_entity(member.clone());
            }
            Self::add_nested_entities(nested, context);
        }
    }

    /// Generate a summary hint based on entity kind and name
    fn generate_summary_hint(entity: &Entity) -> Option<String> {
        Self::generate_summary_hint_from_kind(&entity.kind, &entity.name)
    }

    /// Generate a summary hint from EntityKind and name
    fn generate_summary_hint_from_kind(kind: &EntityKind, name: &str) -> Option<String> {
        let hint = match kind {
            EntityKind::Class => format!("Class for {}", Self::humanize_name(name)),
            EntityKind::Struct => format!("Struct for {}", Self::humanize_name(name)),
            EntityKind::Enum => "Enum with variants".to_string(),
            EntityKind::Interface => format!("Interface for {}", Self::humanize_name(name)),
            EntityKind::Trait => format!("Trait for {}", Self::humanize_name(name)),
            EntityKind::Function => format!("Function {}", name),
            EntityKind::Method => format!("Method {}", name),
            EntityKind::Constructor => "Constructor".to_string(),
            EntityKind::Field => format!("Field {}", name),
            EntityKind::Variable => format!("Variable {}", name),
            EntityKind::Constant => format!("Constant {}", name),
            EntityKind::Module => format!("Module {}", name),
            EntityKind::Namespace => format!("Namespace {}", name),
            _ => format!("{:?} entity", kind),
        };
        Some(hint)
    }

    /// Helper: convert snake_case or camelCase to human-readable text
    fn humanize_name(name: &str) -> String {
        // Convert camelCase to spaces and lowercase everything
        let mut result = String::new();
        for (i, ch) in name.chars().enumerate() {
            if i > 0 && ch.is_uppercase() {
                result.push(' ');
                result.push(ch.to_lowercase().next().unwrap_or(ch));
            } else {
                result.push(ch.to_lowercase().next().unwrap_or(ch));
            }
        }

        // Convert underscores to spaces
        result = result.replace('_', " ");

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;

    #[test]
    fn test_humanize_name() {
        assert_eq!(CodeFormConverter::humanize_name("MyClass"), "my class");
        assert_eq!(
            CodeFormConverter::humanize_name("my_function"),
            "my function"
        );
        assert_eq!(
            CodeFormConverter::humanize_name("HTTPServer"),
            "h t t p server"
        );
    }

    #[test]
    fn test_generate_summary_hint() {
        let hint = CodeFormConverter::generate_summary_hint_from_kind(
            &EntityKind::Function,
            "process_data",
        );
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("process_data"));
    }

    #[test]
    fn test_convert_entity() {
        use cce_types::EntityId;
        let entity = Entity::new(
            EntityId(1),
            EntityKind::Function,
            "my_function".to_string(),
            Span::default(),
        );

        let code_form = CodeFormConverter::convert_entity(&entity, None);
        assert_eq!(code_form.id, EntityId(1));
        assert_eq!(code_form.name, "my_function");
        assert_eq!(code_form.kind, EntityKind::Function);
    }
}
