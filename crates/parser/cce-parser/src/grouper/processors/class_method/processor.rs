use super::types::NestedExtractionContext;
use crate::grouper::context::FileProcessingContext;
use crate::grouper::processors::method_utils::GetterSetterDetector;
use crate::grouper::types::{EntityGroup, GroupType, PatternInfo};
use cce_config::modules::pattern_detection::GetterSetterDetectionConfig;
use cce_types::entity::{Entity, EntityId, EntityKind, ParsedFile};
use std::collections::{HashMap, HashSet};

/// Class-method processor
///
/// Coordinates entity grouping and getter/setter based method merging.
/// Groups small classes with their methods, keeps large classes separate.
pub struct ClassMethodProcessor {
    pub(super) getter_setter_detector: GetterSetterDetector,
}

impl Default for ClassMethodProcessor {
    fn default() -> Self {
        Self::new(&GetterSetterDetectionConfig::default())
    }
}

impl ClassMethodProcessor {
    /// Create a new processor with the given getter/setter detection configuration
    pub fn new(config: &GetterSetterDetectionConfig) -> Self {
        Self {
            getter_setter_detector: GetterSetterDetector::with_config(config.clone()),
        }
    }

    /// Process classes and their methods
    ///
    /// Returns a tuple of (groups, association_count)
    pub fn process(&self, ctx: FileProcessingContext) -> (Vec<EntityGroup>, usize) {
        let mut groups = Vec::new();
        let mut processed_ids = std::collections::HashSet::new();
        let mut association_count = 0;
        let language = *ctx.language();

        // Step 1: Find auto-trait implementations targeting type definitions.
        // Auto-trait impls (Sync, Send, UnwindSafe, etc.) have no children
        // (no methods/consts/types) and serve as marker traits. Instead of creating
        // separate standalone groups, we collect their trait names into the parent
        // type definition's metadata for compact rendering.
        let mut auto_traits: HashMap<String, (Vec<String>, HashSet<EntityId>)> = HashMap::new();
        for entity in ctx.entities.iter() {
            if entity.kind != EntityKind::TraitImpl {
                continue;
            }
            if !entity.children.is_empty() {
                continue;
            }
            let Some(impl_for) = entity.get_metadata("impl_for_type") else {
                continue;
            };
            let entry = auto_traits.entry(impl_for.clone()).or_default();
            entry.0.push(entity.name.clone());
            entry.1.insert(entity.id);
        }

        // Step 2: Find all type definitions (classes, structs, etc.)
        // Note: InherentImpl and TraitImpl are excluded - they remain as standalone groups
        // so their methods appear as nested entities in the export output.
        for entity_ref in ctx.entities.iter().filter(|e| {
            e.kind.is_type_definition()
                && e.kind != EntityKind::InherentImpl
                && e.kind != EntityKind::TraitImpl
        }) {
            if processed_ids.contains(&entity_ref.id) {
                continue;
            }

            // Clone entity so we can attach auto_traits metadata without
            // mutating the shared parsed entity list.
            let mut entity = entity_ref.clone();
            if let Some((trait_names, trait_ids)) = auto_traits.remove(&entity.name) {
                entity.set_metadata("auto_traits", trait_names.join(","));
                for id in trait_ids {
                    processed_ids.insert(id);
                }
            }
            if processed_ids.contains(&entity.id) {
                continue;
            }

            // Extract nested groups if enabled.
            let nested_groups = if ctx.config.enable_nested_entity_grouping {
                let extract_ctx = NestedExtractionContext {
                    all_entities: ctx.entities,
                    max_depth: ctx.config.max_nesting_depth,
                    min_nested_size: ctx.config.min_nested_size,
                    processed_ids: &mut processed_ids,
                    language: ctx.language(),
                    parsed_file: ctx.parsed_file,
                    config: ctx.config,
                };
                self.extract_nested_groups(&entity, extract_ctx)
            } else {
                Vec::new()
            };

            // Get the source code to estimate line count
            // Note: line_count is kept for potential future use in nested group extraction
            let line_count = self.estimate_line_count(&entity, ctx.parsed_file);

            // Get child fields and methods
            let children: Vec<Entity> = entity
                .children
                .iter()
                .filter_map(|&child_id| ctx.entities.iter().find(|e| e.id == child_id))
                .cloned()
                .collect();

            let fields: Vec<&Entity> = children
                .iter()
                .filter(|e| e.kind.is_variable_like())
                .collect();

            let methods: Vec<Entity> = children
                .iter()
                .filter(|e| e.kind.is_function_like())
                .cloned()
                .collect();

            // Mark stdlib/constructor/complex methods roles and merge
            // simple getters/setters when enabled.
            let pattern_result = self.apply_pattern_processing(
                &entity,
                &fields,
                &methods,
                ctx.language(),
                ctx.config,
            );

            const LARGE_CLASS_THRESHOLD: usize = 2000;
            const MAX_MEMBERS_PER_CLASS: usize = 50;

            let has_getter_setter =
                matches!(pattern_result.pattern_info, PatternInfo::GetterSetter(_));
            let should_merge = if pattern_result.methods.len() > MAX_MEMBERS_PER_CLASS {
                false
            } else {
                line_count < LARGE_CLASS_THRESHOLD
                    && (!pattern_result.methods.is_empty() || has_getter_setter)
            };

            if should_merge {
                // Small class: merge with fields and methods
                let field_entities: Vec<Entity> = fields.iter().map(|f| (*f).clone()).collect();
                let mut group = EntityGroup::class_with_methods_with_fields(
                    entity.clone(),
                    field_entities,
                    pattern_result.methods.clone(),
                    language,
                );
                group.pattern_info = pattern_result.pattern_info;
                group.member_roles = pattern_result.member_roles;
                if let Some(traits) = entity.get_metadata("auto_traits") {
                    group
                        .metadata
                        .insert("auto_traits".to_string(), traits.clone());
                }

                group.nested_groups = nested_groups.into_boxed_slice();
                group.nesting_level = 0;
                group.has_significant_nested = !group.nested_groups.is_empty();

                if group.has_significant_nested {
                    group.group_type = match entity.kind {
                        cce_types::entity::EntityKind::Class => GroupType::ClassWithNestedClasses,
                        cce_types::entity::EntityKind::Struct => GroupType::StructWithNestedStructs,
                        _ => group.group_type,
                    };
                }

                groups.push(group);
                processed_ids.insert(entity.id);
                for method in &methods {
                    processed_ids.insert(method.id);
                }
                // Mark all children (including fields) as processed when merging
                for child_id in &entity.children {
                    processed_ids.insert(*child_id);
                }
                association_count += 1;
            } else {
                // Large class or no methods: keep separate
                let mut group = EntityGroup::from_entity(entity.clone(), language);

                group.nested_groups = nested_groups.into_boxed_slice();
                group.nesting_level = 0;
                group.has_significant_nested = !group.nested_groups.is_empty();

                if group.has_significant_nested {
                    group.group_type = match entity.kind {
                        cce_types::entity::EntityKind::Class => GroupType::ClassWithNestedClasses,
                        cce_types::entity::EntityKind::Struct => GroupType::StructWithNestedStructs,
                        _ => group.group_type,
                    };
                }

                groups.push(group);
                processed_ids.insert(entity.id);

                for child in &pattern_result.methods {
                    if !processed_ids.contains(&child.id) {
                        groups.push(EntityGroup::from_entity(child.clone(), language));
                        processed_ids.insert(child.id);
                    }
                }

                // Add fields as separate groups so they appear as children in the tree
                for field in fields.iter() {
                    if !processed_ids.contains(&field.id) {
                        groups.push(EntityGroup::from_entity((*field).clone(), language));
                        processed_ids.insert(field.id);
                    }
                }
            }
        }

        // Collect impl block children IDs (inherent and trait impls) so we can
        // skip them. These methods are handled separately by the pipeline to
        // create groups with their methods as members.
        let impl_children_ids: std::collections::HashSet<cce_types::entity::EntityId> = ctx
            .entities
            .iter()
            .filter(|e| e.kind.is_impl_block())
            .flat_map(|e| e.children.iter())
            .copied()
            .collect();

        // Add remaining entities as standalone groups,
        // skipping low-value entities (type parameters, function parameters,
        // local variables, annotations, placeholder types, and other
        // implementation details).
        for entity in ctx.entities {
            if !processed_ids.contains(&entity.id) {
                // Skip local variables
                if matches!(entity.kind, cce_types::entity::EntityKind::Variable) {
                    continue;
                }
                // Skip single-char name entities (typically generic params like T, F)
                if entity.name.len() == 1 {
                    continue;
                }
                // Skip zero-variant enums (placeholder/never types like `enum Void {}`)
                if matches!(entity.kind, cce_types::entity::EntityKind::Enum)
                    && entity.doc_comment.is_none()
                    && entity.span.len() < 80
                {
                    continue;
                }
                // Skip stub functions (no doc_comment, tiny span, underscore name)
                if matches!(entity.kind, cce_types::entity::EntityKind::Function)
                    && entity.doc_comment.is_none()
                    && entity.span.len() < 80
                    && entity.name.starts_with('_')
                {
                    continue;
                }
                // Skip impl block entities — they are handled separately
                // by the pipeline to create groups with their methods as members.
                if entity.kind.is_impl_block() {
                    continue;
                }
                // Skip entities that are children of impl blocks — they are
                // handled as members of the impl block group.
                if impl_children_ids.contains(&entity.id) {
                    continue;
                }

                groups.push(EntityGroup::from_entity(entity.clone(), language));
                processed_ids.insert(entity.id);
            }
        }

        (groups, association_count)
    }

    /// Estimate the line count of an entity
    fn estimate_line_count(&self, entity: &Entity, _parsed_file: &ParsedFile) -> usize {
        let span = &entity.span;
        if span.end_position.row >= span.start_position.row {
            span.end_position.row - span.start_position.row + 1
        } else {
            let source_len = span.len();
            source_len / 30 + 1
        }
    }
}
