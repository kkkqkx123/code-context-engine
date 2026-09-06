//! Kind mapper: tree-sitter capture name → EntityKind mapping
//!
//! Maps capture names from spec.md to EntityKind.
//! Format: entity.category[.subtype][.role][.attribute]

use crate::tree_sitter_query::capture;
use cce_types::EntityKind;

/// Determine entity kind from capture name
pub fn determine_entity_kind(capture_name: &str) -> Option<EntityKind> {
    // Parse capture name using utility functions:
    // - extract_category returns the category part (e.g., "function" from "entity.function.name")
    // - extract_subcategory returns the subcategory if it's not a known suffix
    //   (e.g., "operator" from "entity.method.operator", but None from "entity.function.name")
    let category = capture::extract_category(capture_name)?;
    let subtype = capture::extract_subcategory(capture_name);

    match (category, subtype) {
        // Type definitions
        (capture::CATEGORY_CLASS, _) => Some(EntityKind::Class),
        (capture::CATEGORY_STRUCT, _) => Some(EntityKind::Struct),
        (capture::CATEGORY_ENUM, _) => Some(EntityKind::Enum),
        (capture::CATEGORY_ENUM_VARIANT, _) => Some(EntityKind::EnumVariant),
        (capture::CATEGORY_ENUM_CONSTANT, _) => Some(EntityKind::EnumVariant),
        (capture::CATEGORY_ENUM_MEMBER, _) => Some(EntityKind::EnumVariant),

        (capture::CATEGORY_UNION, _) => Some(EntityKind::Union),
        (capture::CATEGORY_TRAIT, _) => Some(EntityKind::Trait),
        (capture::CATEGORY_INTERFACE, _) => Some(EntityKind::Interface),
        (capture::CATEGORY_TYPE, sub) => match sub {
            Some(capture::CATEGORY_STRUCT) => Some(EntityKind::Struct),
            Some(capture::CATEGORY_ENUM) => Some(EntityKind::Enum),
            Some(capture::CATEGORY_CLASS) => Some(EntityKind::Class),
            _ => Some(EntityKind::TypeAlias),
        },
        (capture::CATEGORY_TYPE_ALIAS, _) => Some(EntityKind::TypeAlias),
        (capture::CATEGORY_RECORD, _) => Some(EntityKind::Class),

        // Functions
        (capture::CATEGORY_FUNCTION, _) => Some(EntityKind::Function),

        // Methods
        (capture::CATEGORY_METHOD, sub) => match sub {
            Some(capture::SUBCATEGORY_OPERATOR) => Some(EntityKind::Operator),
            Some(capture::SUBCATEGORY_GETTER) => Some(EntityKind::Method),
            Some(capture::SUBCATEGORY_SETTER) => Some(EntityKind::Method),
            _ => Some(EntityKind::Method),
        },

        // Constructors and destructors
        (capture::CATEGORY_CONSTRUCTOR, _) => Some(EntityKind::Constructor),
        (capture::CATEGORY_DESTRUCTOR, _) => Some(EntityKind::Destructor),

        // Variables and constants
        (capture::CATEGORY_VARIABLE, _) => Some(EntityKind::Variable),
        (capture::CATEGORY_CONST, _) => Some(EntityKind::Constant),
        (capture::CATEGORY_CONSTANT, _) => Some(EntityKind::Constant),
        (capture::CATEGORY_STATIC, _) => Some(EntityKind::Variable),

        // Fields and properties
        (capture::CATEGORY_FIELD, _)
        | (capture::CATEGORY_FIELD_TAGGED, _)
        | (capture::CATEGORY_BITFIELD, _) => Some(EntityKind::Field),
        (capture::CATEGORY_PROPERTY, _) => Some(EntityKind::Property),

        // Modules, namespaces, packages
        (capture::CATEGORY_MODULE, _) => Some(EntityKind::Module),
        (capture::CATEGORY_NAMESPACE, _) => Some(EntityKind::Namespace),
        (capture::CATEGORY_PACKAGE, _) => Some(EntityKind::Package),
        (capture::CATEGORY_IMPORT, _) => Some(EntityKind::Import),
        (capture::CATEGORY_EXPORT, _) => Some(EntityKind::Export),
        (capture::CATEGORY_REQUIRE, _) => Some(EntityKind::Require),
        (capture::CATEGORY_INCLUDE, _) => Some(EntityKind::Include),

        // Annotations and decorators
        (capture::CATEGORY_ANNOTATION, _) | (capture::CATEGORY_DECORATOR, _) => {
            Some(EntityKind::Annotation)
        }
        (capture::CATEGORY_ATTRIBUTE, _) => Some(EntityKind::Annotation),

        // Lambda expressions
        (capture::CATEGORY_LAMBDA, _) => Some(EntityKind::Function),

        // Closure expressions (Rust)
        (capture::CATEGORY_CLOSURE, _) => Some(EntityKind::Function),

        // Function literals (Go)
        (capture::CATEGORY_FUNCTION_LITERAL, _) => Some(EntityKind::Function),

        (capture::CATEGORY_COMPREHENSION, _) => Some(EntityKind::Variable),

        // Macros
        // Note: extract_subcategory only reads the second segment, so both
        // `entity.macro.attribute` and `entity.macro.attribute.inner` resolve
        // to subcategory "attribute". The "attribute.inner" arm was unreachable.
        (capture::CATEGORY_MACRO, sub) => match sub {
            Some("attribute") => Some(EntityKind::Annotation),
            _ => Some(EntityKind::Macro),
        },
        (capture::CATEGORY_PREPROCESSOR, _) => Some(EntityKind::Macro),

        // Impl blocks (Rust)
        (capture::CATEGORY_IMPL, sub) => match sub {
            Some("trait") => Some(EntityKind::TraitImpl),
            _ => Some(EntityKind::InherentImpl),
        },

        // ===== Frontend-specific entities =====
        (capture::CATEGORY_JSX, sub) => match sub {
            Some(capture::CATEGORY_ELEMENT) => Some(EntityKind::Element),
            Some(capture::CATEGORY_COMPONENT) => Some(EntityKind::Component),
            Some(capture::CATEGORY_ATTRIBUTE) => Some(EntityKind::Attribute),
            Some(capture::CATEGORY_EXPRESSION) => Some(EntityKind::Expression),
            Some(capture::CATEGORY_TEXT)
            | Some(capture::CATEGORY_RAW_TEXT)
            | Some(capture::CATEGORY_COMMENT) => None,
            _ => None,
        },
        (capture::CATEGORY_COMPONENT, _) => Some(EntityKind::Component),
        (capture::CATEGORY_TEMPLATE, _) => Some(EntityKind::Template),
        (capture::CATEGORY_DIRECTIVE, _) => Some(EntityKind::Directive),
        (capture::CATEGORY_INTERPOLATION, _) => None,

        // Svelte
        (capture::CATEGORY_DOCUMENT, _) => Some(EntityKind::Module),
        (
            capture::CATEGORY_IF
            | capture::CATEGORY_ELSE
            | capture::CATEGORY_ELSE_IF
            | capture::CATEGORY_EACH
            | capture::CATEGORY_AWAIT
            | capture::CATEGORY_CATCH
            | capture::CATEGORY_THEN
            | capture::CATEGORY_KEY,
            _,
        ) => Some(EntityKind::ControlFlow),
        (capture::CATEGORY_TRANSITION | capture::CATEGORY_ANIMATION, _) => {
            Some(EntityKind::Animation)
        }
        (capture::CATEGORY_BINDING, _) => Some(EntityKind::Binding),

        // CSS entities
        (capture::CATEGORY_STYLE_RULE, _) => Some(EntityKind::StyleRule),
        (capture::CATEGORY_STYLE_SELECTOR, _) => Some(EntityKind::StyleSelector),
        (capture::CATEGORY_STYLE_PROPERTY, _) => Some(EntityKind::StyleProperty),
        (capture::CATEGORY_STYLE_VALUE, _) => None,
        (capture::CATEGORY_AT, _) => Some(EntityKind::AtRule),
        (capture::CATEGORY_KEYFRAME, _) => Some(EntityKind::Keyframe),

        // HTML entities
        (capture::CATEGORY_ELEMENT, _) => Some(EntityKind::Element),
        (capture::CATEGORY_TAG, _) => None,
        (capture::CATEGORY_DOCTYPE, _) => None,

        // Embedded blocks
        (capture::CATEGORY_EMBEDDED, _) => Some(EntityKind::Field),
        (capture::CATEGORY_SCRIPT, sub) => match sub {
            Some(capture::SUBCATEGORY_SCRIPT_CONTENT) => Some(EntityKind::ScriptContent),
            _ => None,
        },
        (capture::CATEGORY_STYLE, sub) => match sub {
            Some(capture::SUBCATEGORY_STYLE_CONTENT) | Some("scope") => {
                Some(EntityKind::StyleContent)
            }
            _ => None,
        },
        (capture::CATEGORY_CSS_IN_JS, _) => Some(EntityKind::StyleRule),
        (capture::CATEGORY_CONTROL, _) => Some(EntityKind::ControlFlow),

        // Event handlers
        (capture::CATEGORY_EVENT, sub) => match sub {
            Some(capture::SUBCATEGORY_EVENT_HANDLER) => Some(EntityKind::EventHandler),
            Some(capture::SUBCATEGORY_EVENT_MODIFIER) => None,
            _ => Some(EntityKind::EventHandler),
        },
        (capture::CATEGORY_CLASS_DIRECTIVE | capture::CATEGORY_STYLE_DIRECTIVE, _) => {
            Some(EntityKind::Directive)
        }
        (capture::CATEGORY_USE_DIRECTIVE, _) => Some(EntityKind::Action),

        // Raw text (skip)
        (
            capture::CATEGORY_RAW_TEXT
            | capture::CATEGORY_RAW_TEXT_EXPR
            | capture::CATEGORY_RAW_TEXT_AWAIT
            | capture::CATEGORY_RAW_TEXT_EACH
            | capture::CATEGORY_TEXT,
            _,
        ) => None,
        (capture::CATEGORY_COMMENT, _) => None,
        (capture::CATEGORY_CONTAINS, _) => None,

        // ===== Missing entity categories =====
        (capture::CATEGORY_INTERFACE_METHOD, _) => Some(EntityKind::Method),
        (capture::CATEGORY_CLASS_EXPRESSION, _) => Some(EntityKind::Class),
        (capture::CATEGORY_TYPE_CONSTRAINT, _) => Some(EntityKind::TypeAlias),
        (capture::CATEGORY_DELEGATE, _) => Some(EntityKind::Function),
        (capture::CATEGORY_TYPEDEF, _) => Some(EntityKind::TypeAlias),
        (capture::CATEGORY_TYPEDEF_STRUCT, _) => Some(EntityKind::TypeAlias),
        (capture::CATEGORY_TYPEDEF_UNION, _) => Some(EntityKind::TypeAlias),
        (capture::CATEGORY_TYPEDEF_ENUM, _) => Some(EntityKind::TypeAlias),
        (capture::CATEGORY_TYPEDEF_FUNCTION_POINTER, _) => Some(EntityKind::TypeAlias),
        (capture::CATEGORY_OBJECT, _) => Some(EntityKind::Class),
        (capture::CATEGORY_ANCHOR, _) => Some(EntityKind::Element),
        (capture::CATEGORY_BUTTON, _) => Some(EntityKind::Element),
        (capture::CATEGORY_FORM, _) => Some(EntityKind::Element),
        (capture::CATEGORY_INPUT, _) => Some(EntityKind::Element),
        (capture::CATEGORY_SELECT, _) => Some(EntityKind::Element),
        (capture::CATEGORY_TEXTAREA, _) => Some(EntityKind::Element),
        (capture::CATEGORY_ATTR, _) => {
            // Ruby attr_reader/attr_writer/attr_accessor define instance
            // variables (fields), not generic attributes.
            Some(EntityKind::Field)
        }
        (capture::CATEGORY_COMPANION, _) => Some(EntityKind::Class),
        (capture::CATEGORY_STRUCT_ANON, _) => Some(EntityKind::Struct),
        (capture::CATEGORY_UNION_ANON, _) => Some(EntityKind::Union),
        (capture::CATEGORY_ENUM_ANON, _) => Some(EntityKind::Enum),
        (capture::CATEGORY_ENUM_CASE, _) => Some(EntityKind::Enum),
        (capture::CATEGORY_EXTENSION, _) => Some(EntityKind::Function),
        (capture::CATEGORY_GIVEN, _) => Some(EntityKind::Function),
        (capture::CATEGORY_MIXIN, _) => Some(EntityKind::Class),
        (capture::CATEGORY_SINGLETON, _) => Some(EntityKind::Method),
        (capture::CATEGORY_SLOT, _) => Some(EntityKind::Template),
        (capture::CATEGORY_SLOT_CONTENT, _) => Some(EntityKind::Template),
        (capture::CATEGORY_TEMPLATE_REFERENCE, _) => Some(EntityKind::Variable),
        (capture::CATEGORY_UNDEF, _) => None,
        (capture::CATEGORY_USING, _) => Some(EntityKind::Directive),
        (capture::CATEGORY_TABLE, _) => Some(EntityKind::Module),
        (capture::CATEGORY_ALIAS, _) => Some(EntityKind::Function),

        // ===== Test entity categories =====
        // These are reserved for future tree-sitter query direct capture.
        // Currently, test entities are inferred via annotation detection in
        // the entity extractor and test suite detector.
        // See: process_match in entity_extractor.rs for annotation merging.
        (capture::CATEGORY_TEST_SUITE, _) => Some(EntityKind::TestSuite),
        (capture::CATEGORY_TEST_CASE, _) => Some(EntityKind::TestCase),
        (capture::CATEGORY_TEST_HOOK, _) => Some(EntityKind::TestHook),
        (capture::CATEGORY_ASSERTION, _) => Some(EntityKind::Assertion),
        (capture::CATEGORY_MOCK, _) => Some(EntityKind::Mock),

        _ => {
            tracing::warn!("Unknown entity category in capture: {}", capture_name);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_class_kind() {
        assert_eq!(
            determine_entity_kind("entity.type.class"),
            Some(EntityKind::Class)
        );
    }

    #[test]
    fn test_function_kind() {
        assert_eq!(
            determine_entity_kind("entity.function.definition"),
            Some(EntityKind::Function)
        );
    }

    #[test]
    fn test_operator_method() {
        assert_eq!(
            determine_entity_kind("entity.method.operator"),
            Some(EntityKind::Operator)
        );
    }

    #[test]
    fn test_impl_trait() {
        assert_eq!(
            determine_entity_kind("entity.impl.trait"),
            Some(EntityKind::TraitImpl)
        );
    }

    #[test]
    fn test_unknown_returns_none() {
        assert!(determine_entity_kind("entity.unknown.category").is_none());
    }

    #[test]
    fn test_invalid_format() {
        assert!(determine_entity_kind("entity").is_none());
    }

    #[test]
    fn test_module_kind() {
        assert_eq!(
            determine_entity_kind("entity.module"),
            Some(EntityKind::Module)
        );
    }

    #[test]
    fn test_struct_kind() {
        assert_eq!(
            determine_entity_kind("entity.type.struct"),
            Some(EntityKind::Struct)
        );
    }

    #[test]
    fn test_enum_kind() {
        assert_eq!(
            determine_entity_kind("entity.type.enum"),
            Some(EntityKind::Enum)
        );
    }
}
