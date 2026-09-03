//! Method utilities for entity processing
//!
//! This module provides utilities for analyzing and classifying methods,
//! particularly for identifying getter/setter patterns and boilerplate code.

use cce_config::modules::pattern_detection::GetterSetterDetectionConfig;
use cce_types::entity::{Entity, EntityKind};
use cce_types::language::Language;

use crate::grouper::language_patterns;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodType {
    SimpleGetter,
    SimpleSetter,
    Property,
    ComplexMethod,
    Constructor,
}

pub struct GetterSetterDetector {
    config: GetterSetterDetectionConfig,
}

impl Default for GetterSetterDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl GetterSetterDetector {
    pub fn new() -> Self {
        Self {
            config: GetterSetterDetectionConfig::default(),
        }
    }

    pub fn with_config(config: GetterSetterDetectionConfig) -> Self {
        Self { config }
    }

    pub fn detect_method_type(&self, method: &Entity, language: &Language) -> MethodType {
        if method.kind == EntityKind::Constructor {
            return MethodType::Constructor;
        }

        if self.is_property(method, language) {
            return MethodType::Property;
        }

        if self.matches_getter_pattern(method, language) {
            return MethodType::SimpleGetter;
        }

        if self.matches_setter_pattern(method, language) {
            return MethodType::SimpleSetter;
        }

        MethodType::ComplexMethod
    }

    pub fn is_property(&self, entity: &Entity, language: &Language) -> bool {
        if entity.kind == EntityKind::Property {
            return true;
        }

        if matches!(language, Language::CSharp | Language::Kotlin) {
            if !entity.parameters.is_empty() {
                return false;
            }

            if entity.return_type.is_none() {
                return false;
            }

            let name_lower = entity.name.to_lowercase();
            if name_lower.starts_with("get") || name_lower.starts_with("set") {
                return false;
            }

            if *language == Language::CSharp {
                let span = &entity.span;
                let line_count = if span.end_position.row >= span.start_position.row {
                    span.end_position.row - span.start_position.row + 1
                } else {
                    1
                };
                return line_count <= 3;
            }
        }

        false
    }

    pub fn is_simple_getter(&self, method: &Entity, language: &Language) -> bool {
        matches!(
            self.detect_method_type(method, language),
            MethodType::SimpleGetter | MethodType::Property
        )
    }

    pub fn is_simple_setter(&self, method: &Entity, language: &Language) -> bool {
        self.detect_method_type(method, language) == MethodType::SimpleSetter
    }

    pub fn get_field_name(&self, method: &Entity) -> Option<String> {
        let name = method.name.to_lowercase();

        if name.starts_with("get ") || name.starts_with("set ") {
            let field = name.split_whitespace().nth(1)?;
            if !field.is_empty() {
                let mut chars = field.chars();
                let first = chars.next()?;
                let mut result = String::new();
                result.push(first.to_ascii_lowercase());
                result.extend(chars);
                return Some(result);
            }
        }

        if name.starts_with("is ") {
            let field = name.split_whitespace().nth(1)?;
            if !field.is_empty() {
                let mut chars = field.chars();
                let first = chars.next()?;
                let mut result = String::new();
                result.push(first.to_ascii_lowercase());
                result.extend(chars);
                return Some(result);
            }
        }

        if name.starts_with("get") || name.starts_with("set") {
            let field = &name[3..];
            if !field.is_empty() {
                let mut chars = field.chars();
                let first = chars.next()?;
                let mut result = String::new();
                result.push(first.to_ascii_lowercase());
                result.extend(chars);
                return Some(result);
            }
        }

        if let Some(field) = name.strip_prefix("is") {
            if !field.is_empty() {
                let mut chars = field.chars();
                let first = chars.next()?;
                let mut result = String::new();
                result.push(first.to_ascii_lowercase());
                result.extend(chars);
                return Some(result);
            }
        }

        None
    }

    pub fn process_methods(
        &self,
        methods: &[Entity],
        language: &Language,
    ) -> (Vec<Entity>, Vec<Entity>, Vec<Entity>) {
        let mut getters = Vec::new();
        let mut setters = Vec::new();
        let mut complex = Vec::new();

        for method in methods {
            match self.detect_method_type(method, language) {
                MethodType::SimpleGetter | MethodType::Property => getters.push(method.clone()),
                MethodType::SimpleSetter => setters.push(method.clone()),
                MethodType::ComplexMethod | MethodType::Constructor => complex.push(method.clone()),
            }
        }

        (getters, setters, complex)
    }

    fn matches_getter_pattern(&self, method: &Entity, language: &Language) -> bool {
        let name = method.name.to_lowercase();

        let is_getter_name = if matches!(language, Language::JavaScript | Language::TypeScript) {
            name.starts_with("get ")
                || name.starts_with("is ")
                || name.starts_with("get")
                || name.starts_with("is")
        } else {
            name.starts_with("get") || name.starts_with("is")
        };

        if !is_getter_name {
            return false;
        }

        let param_count = method.parameters.len();
        let is_valid_param_count = match language {
            Language::Java | Language::Kotlin => param_count == 0,
            Language::Cpp | Language::C => param_count == 0 || param_count == 1,
            Language::Python | Language::Ruby => param_count == 1,
            Language::Php => param_count == 1,
            Language::Rust => param_count == 1,
            Language::TypeScript | Language::JavaScript => param_count == 0,
            Language::CSharp => param_count == 0,
            _ => param_count <= 1,
        };

        if !is_valid_param_count {
            return false;
        }

        if self.has_explicit_self_parameter(language) {
            if let Some((_, param_type)) = method.parameters.first() {
                if !self.is_self_reference(param_type, language) {
                    return false;
                }
            }
        }

        let has_return_type = !method.name.to_lowercase().starts_with("set");

        if let Some(ref return_type) = method.return_type {
            if return_type.to_lowercase().contains("void") {
                return false;
            }
        }

        let is_simple = self.is_simple_method(method);

        is_getter_name && is_valid_param_count && has_return_type && is_simple
    }

    fn matches_setter_pattern(&self, method: &Entity, language: &Language) -> bool {
        let name = method.name.to_lowercase();

        let is_setter_name = if matches!(language, Language::JavaScript | Language::TypeScript) {
            name.starts_with("set ") || name.starts_with("set")
        } else {
            name.starts_with("set")
        };

        if !is_setter_name {
            return false;
        }

        let param_count = method.parameters.len();
        let is_valid_param_count = match language {
            Language::Java | Language::Kotlin => param_count == 1,
            Language::Cpp | Language::C => param_count == 1 || param_count == 2,
            Language::Python | Language::Ruby => param_count == 2,
            Language::Php => param_count == 2,
            Language::Rust => param_count == 2,
            Language::TypeScript | Language::JavaScript => param_count == 1,
            Language::CSharp => param_count == 1,
            _ => param_count == 1 || param_count == 2,
        };

        if !is_valid_param_count {
            return false;
        }

        if self.has_explicit_self_parameter(language) && param_count > 1 {
            if let Some((_, param_type)) = method.parameters.first() {
                if !self.is_self_reference(param_type, language) {
                    return false;
                }
            }
            if param_count >= 2 {
                if let Some((_, param_type)) = method.parameters.get(1) {
                    if self.is_self_reference(param_type, language) {
                        return false;
                    }
                }
            }
        }

        let is_simple = self.is_simple_method(method);

        is_setter_name && is_valid_param_count && is_simple
    }

    fn has_explicit_self_parameter(&self, language: &Language) -> bool {
        language_patterns::has_explicit_self_parameter(language)
    }

    fn is_self_reference(&self, param_type: &Option<String>, language: &Language) -> bool {
        match param_type {
            None => false,
            Some(type_name) => language_patterns::is_self_reference(type_name, language),
        }
    }

    fn is_simple_method(&self, method: &Entity) -> bool {
        let span = &method.span;
        let line_count = if span.end_position.row >= span.start_position.row {
            span.end_position.row - span.start_position.row + 1
        } else {
            1
        };

        line_count <= self.config.max_simple_lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::Span;
    use cce_types::entity::EntityId;

    #[test]
    fn test_field_name_extraction() {
        let detector = GetterSetterDetector::new();

        let get_user_name = Entity::new(
            EntityId(0),
            EntityKind::Method,
            "getUserName".to_string(),
            Span::default(),
        );
        assert_eq!(
            detector.get_field_name(&get_user_name),
            Some("username".to_string())
        );

        let set_age = Entity::new(
            EntityId(1),
            EntityKind::Method,
            "setAge".to_string(),
            Span::default(),
        );
        assert_eq!(detector.get_field_name(&set_age), Some("age".to_string()));

        let is_active = Entity::new(
            EntityId(2),
            EntityKind::Method,
            "isActive".to_string(),
            Span::default(),
        );
        assert_eq!(
            detector.get_field_name(&is_active),
            Some("active".to_string())
        );
    }

    #[test]
    fn test_method_type_detection() {
        let detector = GetterSetterDetector::new();

        let mut getter = Entity::new(
            EntityId(0),
            EntityKind::Method,
            "getName".to_string(),
            Span {
                start_position: cce_types::Position { row: 0, column: 0 },
                end_position: cce_types::Position { row: 0, column: 20 },
                start_byte: 0,
                end_byte: 20,
            },
        );
        getter.parameters = Vec::new();

        assert_eq!(
            detector.detect_method_type(&getter, &Language::Java),
            MethodType::SimpleGetter
        );

        let mut setter = Entity::new(
            EntityId(1),
            EntityKind::Method,
            "setName".to_string(),
            Span {
                start_position: cce_types::Position { row: 0, column: 0 },
                end_position: cce_types::Position { row: 1, column: 20 },
                start_byte: 0,
                end_byte: 40,
            },
        );
        setter.parameters = vec![("name".to_string(), Some("String".to_string()))];

        assert_eq!(
            detector.detect_method_type(&setter, &Language::Java),
            MethodType::SimpleSetter
        );
    }

    #[test]
    fn test_process_methods() {
        let detector = GetterSetterDetector::new();

        let mut getter = Entity::new(
            EntityId(0),
            EntityKind::Method,
            "getName".to_string(),
            Span {
                start_position: cce_types::Position { row: 0, column: 0 },
                end_position: cce_types::Position { row: 0, column: 20 },
                start_byte: 0,
                end_byte: 20,
            },
        );
        getter.parameters = Vec::new();

        let mut setter = Entity::new(
            EntityId(1),
            EntityKind::Method,
            "setName".to_string(),
            Span {
                start_position: cce_types::Position { row: 0, column: 0 },
                end_position: cce_types::Position { row: 1, column: 20 },
                start_byte: 0,
                end_byte: 40,
            },
        );
        setter.parameters = vec![("name".to_string(), Some("String".to_string()))];

        let complex_method = Entity::new(
            EntityId(2),
            EntityKind::Method,
            "calculateTotal".to_string(),
            Span {
                start_position: cce_types::Position { row: 0, column: 0 },
                end_position: cce_types::Position {
                    row: 10,
                    column: 20,
                },
                start_byte: 0,
                end_byte: 200,
            },
        );

        let methods = vec![getter, setter, complex_method];

        let (getters, setters, complex) = detector.process_methods(&methods, &Language::Java);

        assert_eq!(getters.len(), 1);
        assert_eq!(setters.len(), 1);
        assert_eq!(complex.len(), 1);
        assert_eq!(getters[0].name, "getName");
        assert_eq!(setters[0].name, "setName");
        assert_eq!(complex[0].name, "calculateTotal");
    }

    #[test]
    fn test_strict_mode() {
        let config = GetterSetterDetectionConfig {
            strict_mode: true,
            max_simple_lines: 2,
            kotlin_accessor_support: true,
        };
        let detector = GetterSetterDetector::with_config(config);

        let mut getter = Entity::new(
            EntityId(0),
            EntityKind::Method,
            "getName".to_string(),
            Span {
                start_position: cce_types::Position { row: 0, column: 0 },
                end_position: cce_types::Position { row: 2, column: 20 },
                start_byte: 0,
                end_byte: 60,
            },
        );
        getter.parameters = Vec::new();

        assert_eq!(
            detector.detect_method_type(&getter, &Language::Java),
            MethodType::ComplexMethod
        );
    }
}
