use super::processor::ClassMethodProcessor;
use super::types::PatternProcessingResult;
use crate::grouper::processors::method_utils::MethodType;
use crate::grouper::types::{GetterSetterSummary, MemberRolesBuilder, PatternInfo};
use cce_config::NestProcessorConfig;
use cce_types::entity::{Entity, EntityKind};
use cce_types::language::Language;

impl ClassMethodProcessor {
    /// Apply getter/setter based processing to methods
    ///
    /// Returns filtered methods along with getter/setter pattern information
    /// and member roles to be stored in EntityGroup for later use in
    /// natural language conversion.
    pub(super) fn apply_pattern_processing(
        &self,
        _class: &Entity,
        _fields: &[&Entity],
        methods: &[Entity],
        language: &Language,
        config: &NestProcessorConfig,
    ) -> PatternProcessingResult {
        let mut filtered_methods = Vec::new();
        let mut roles_builder = MemberRolesBuilder::new();

        // Track getter/setter properties for GetterSetterSummary
        let mut getter_setter_properties: Vec<String> = Vec::new();

        // Process each method
        for method in methods {
            let mut include_method = true;

            // Constructors and destructors are structural members: always
            // keep them, even when name-matched as stdlib (e.g. a Dart
            // `Point` colliding with a same-named library type). Dropping
            // them here strands same-named standalone groups that trip the
            // nesting invariant downstream.
            if matches!(
                method.kind,
                EntityKind::Constructor | EntityKind::Destructor
            ) {
                roles_builder.mark_significant(method.id);
                filtered_methods.push(method.clone());
                continue;
            }

            // Check if this is a standard library entity
            if method.is_stdlib {
                roles_builder.mark_boilerplate(method.id);
                continue;
            }

            // Apply Getter/Setter filtering
            if config.enable_getter_setter_merging {
                let method_type = self
                    .getter_setter_detector
                    .detect_method_type(method, language);
                match method_type {
                    MethodType::SimpleGetter | MethodType::SimpleSetter | MethodType::Property => {
                        if let Some(field_name) = self.getter_setter_detector.get_field_name(method)
                        {
                            if !getter_setter_properties.contains(&field_name) {
                                getter_setter_properties.push(field_name);
                            }
                        }
                        include_method = false;
                        roles_builder.mark_boilerplate(method.id);
                    }
                    MethodType::Constructor => {
                        roles_builder.mark_significant(method.id);
                    }
                    MethodType::ComplexMethod => {
                        roles_builder.mark_significant(method.id);
                    }
                }
            }

            if include_method {
                filtered_methods.push(method.clone());
            }
        }

        // Build pattern_info based on detection results
        let pattern_info =
            if !getter_setter_properties.is_empty() && config.enable_getter_setter_merging {
                PatternInfo::GetterSetter(GetterSetterSummary::new(getter_setter_properties))
            } else {
                PatternInfo::None
            };

        PatternProcessingResult {
            methods: filtered_methods,
            pattern_info,
            member_roles: roles_builder.build(),
        }
    }
}
