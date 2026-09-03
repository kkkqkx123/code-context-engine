//! Entity kind definitions (cross-language unified)

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// Cross-language unified entity kind
///
/// Different languages' same concept maps to unified kind:
/// - Python def / Rust fn / Java method -> Function
/// - Python class / Rust struct / Java class -> Class/Struct
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    SerdeSerialize,
    SerdeDeserialize,
    Default,
    Archive,
    RkyvDeserialize,
    Serialize,
)]
pub enum EntityKind {
    /// Unknown or unspecified entity kind
    #[default]
    #[serde(rename = "unknown")]
    Unknown,

    // Type definitions (cross-language unified)
    /// Python/Java/C++ class, Rust struct with methods
    #[serde(rename = "class")]
    Class,
    /// Rust/C struct
    #[serde(rename = "struct")]
    Struct,
    /// Rust/Python/Java enum
    #[serde(rename = "enum")]
    Enum,
    /// Java/TS interface, Rust trait object
    #[serde(rename = "interface")]
    Interface,
    /// Rust trait
    #[serde(rename = "trait")]
    Trait,
    /// Rust trait impl block (impl Trait for Type)
    #[serde(rename = "trait_impl")]
    TraitImpl,
    /// Rust inherent impl block (impl Type)
    #[serde(rename = "inherent_impl")]
    InherentImpl,
    /// type = ... / typedef
    #[serde(rename = "type_alias")]
    TypeAlias,
    /// C union
    #[serde(rename = "union")]
    Union,
    /// Enum variant/value/member (language-independent)
    #[serde(rename = "enum_variant")]
    EnumVariant,

    // Language-level annotations
    /// Code annotation/decorator (e.g., Java @Override, Python @decorator)
    #[serde(rename = "annotation")]
    Annotation,
    /// Macro definition (e.g., Rust macro_rules!, C #define)
    #[serde(rename = "macro")]
    Macro,
    // Function/Method
    /// Standalone function
    #[serde(rename = "function")]
    Function,
    /// Class/struct method
    #[serde(rename = "method")]
    Method,
    /// Constructor
    #[serde(rename = "constructor")]
    Constructor,
    /// Destructor
    #[serde(rename = "destructor")]
    Destructor,
    /// Operator overloading
    #[serde(rename = "operator")]
    Operator,

    // Variable/Field
    /// Class/struct field
    #[serde(rename = "field")]
    Field,
    /// Property (C#, Kotlin, etc.)
    #[serde(rename = "property")]
    Property,
    /// Local variable
    #[serde(rename = "variable")]
    Variable,
    /// Constant
    #[serde(rename = "constant")]
    Constant,

    // Module
    /// Python/Rust module
    #[serde(rename = "module")]
    Module,
    /// C++/C# namespace
    #[serde(rename = "namespace")]
    Namespace,
    /// Java package
    #[serde(rename = "package")]
    Package,
    /// Import/use statement (Python import, Rust use, etc.)
    #[serde(rename = "import")]
    Import,
    /// Require statement (Ruby require, Lua require, JS require(), PHP require)
    #[serde(rename = "require")]
    Require,
    /// Include statement (Ruby include, PHP include, C/C++ #include)
    #[serde(rename = "include")]
    Include,
    /// Export statement (JavaScript/TypeScript export)
    #[serde(rename = "export")]
    Export,

    // Style/CSS (language-independent)
    /// CSS Style Rule
    #[serde(rename = "style_rule")]
    StyleRule,
    /// CSS Selector
    #[serde(rename = "style_selector")]
    StyleSelector,
    /// CSS Property Declaration
    #[serde(rename = "style_property")]
    StyleProperty,
    /// CSS Keyframe Animation
    #[serde(rename = "keyframe")]
    Keyframe,

    // Template/Markup extensions
    /// HTML Element (treated as lightweight class-like structure)
    #[serde(rename = "element")]
    Element,
    /// Template attribute (e.g., Vue directive, HTML attribute)
    #[serde(rename = "attribute")]
    Attribute,
    /// Template expression (e.g., interpolation, binding expression)
    #[serde(rename = "expression")]
    Expression,

    // Frontend Framework extensions
    /// JSX/Vue/Svelte Component
    #[serde(rename = "component")]
    Component,
    /// Template block (Vue template, Svelte template)
    #[serde(rename = "template")]
    Template,
    /// Framework directive (v-if, v-for, @click, bind:, etc.)
    #[serde(rename = "directive")]
    Directive,
    /// Control flow block ({#if}, {:else}, etc.)
    #[serde(rename = "control_flow")]
    ControlFlow,
    /// Animation/Transition
    #[serde(rename = "animation")]
    Animation,
    /// Two-way binding
    #[serde(rename = "binding")]
    Binding,
    /// Svelte action (use:action)
    #[serde(rename = "action")]
    Action,
    /// CSS At-rule (@media, @keyframes, etc.)
    #[serde(rename = "at_rule")]
    AtRule,
    /// Event handler
    #[serde(rename = "event_handler")]
    EventHandler,
    /// Script content block
    #[serde(rename = "script_content")]
    ScriptContent,
    /// Style content block
    #[serde(rename = "style_content")]
    StyleContent,
    /// Embedded code block (script/style in SFC)
    #[serde(rename = "embedded_block")]
    EmbeddedBlock,

    // Test entities
    /// Test suite (describe, suite, context)
    #[serde(rename = "test_suite")]
    TestSuite,
    /// Test case (it, test, specify)
    #[serde(rename = "test_case")]
    TestCase,
    /// Test lifecycle hook (before, after, beforeEach, afterEach)
    #[serde(rename = "test_hook")]
    TestHook,
    /// Assertion (expect, assert)
    #[serde(rename = "assertion")]
    Assertion,
    /// Mock/Spy/Stub
    #[serde(rename = "mock")]
    Mock,
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityKind::Unknown => write!(f, "unknown"),
            EntityKind::Class => write!(f, "class"),
            EntityKind::Struct => write!(f, "struct"),
            EntityKind::Enum => write!(f, "enum"),
            EntityKind::Interface => write!(f, "interface"),
            EntityKind::Trait => write!(f, "trait"),
            EntityKind::TraitImpl => write!(f, "trait_impl"),
            EntityKind::InherentImpl => write!(f, "inherent_impl"),
            EntityKind::TypeAlias => write!(f, "type_alias"),
            EntityKind::Union => write!(f, "union"),
            EntityKind::EnumVariant => write!(f, "enum_variant"),
            EntityKind::Annotation => write!(f, "annotation"),
            EntityKind::Macro => write!(f, "macro"),
            EntityKind::Function => write!(f, "function"),
            EntityKind::Method => write!(f, "method"),
            EntityKind::Constructor => write!(f, "constructor"),
            EntityKind::Destructor => write!(f, "destructor"),
            EntityKind::Operator => write!(f, "operator"),
            EntityKind::Field => write!(f, "field"),
            EntityKind::Property => write!(f, "property"),
            EntityKind::Variable => write!(f, "variable"),
            EntityKind::Constant => write!(f, "constant"),
            EntityKind::Module => write!(f, "module"),
            EntityKind::Namespace => write!(f, "namespace"),
            EntityKind::Package => write!(f, "package"),
            EntityKind::Import => write!(f, "import"),
            EntityKind::Require => write!(f, "require"),
            EntityKind::Include => write!(f, "include"),
            EntityKind::Export => write!(f, "export"),
            EntityKind::StyleRule => write!(f, "style_rule"),
            EntityKind::StyleSelector => write!(f, "style_selector"),
            EntityKind::StyleProperty => write!(f, "style_property"),
            EntityKind::Keyframe => write!(f, "keyframe"),
            EntityKind::Element => write!(f, "element"),
            EntityKind::Attribute => write!(f, "attribute"),
            EntityKind::Expression => write!(f, "expression"),
            EntityKind::Component => write!(f, "component"),
            EntityKind::Template => write!(f, "template"),
            EntityKind::Directive => write!(f, "directive"),
            EntityKind::ControlFlow => write!(f, "control_flow"),
            EntityKind::Animation => write!(f, "animation"),
            EntityKind::Binding => write!(f, "binding"),
            EntityKind::Action => write!(f, "action"),
            EntityKind::AtRule => write!(f, "at_rule"),
            EntityKind::EventHandler => write!(f, "event_handler"),
            EntityKind::ScriptContent => write!(f, "script_content"),
            EntityKind::StyleContent => write!(f, "style_content"),
            EntityKind::EmbeddedBlock => write!(f, "embedded_block"),
            EntityKind::TestSuite => write!(f, "test_suite"),
            EntityKind::TestCase => write!(f, "test_case"),
            EntityKind::TestHook => write!(f, "test_hook"),
            EntityKind::Assertion => write!(f, "assertion"),
            EntityKind::Mock => write!(f, "mock"),
        }
    }
}

/// Broad domain of an entity kind, separating code from style/template/test concerns.
///
/// This domain split makes `EntityKind`'s 60+ variants easier to reason about
/// and avoids mixing unrelated predicates (e.g. `is_style_related` on code kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityDomain {
    Code,
    Module,
    Style,
    Template,
    Test,
    Unknown,
}

impl EntityKind {
    /// Domain of this kind.
    pub fn domain(&self) -> EntityDomain {
        if self.is_style_related() {
            EntityDomain::Style
        } else if self.is_template_entity() {
            EntityDomain::Template
        } else if self.is_test_related() {
            EntityDomain::Test
        } else if self.is_module_like() || self.is_import_like() {
            EntityDomain::Module
        } else if matches!(self, EntityKind::Unknown) {
            EntityDomain::Unknown
        } else {
            EntityDomain::Code
        }
    }

    /// Whether this kind belongs to the core code domain (types, functions, variables).
    pub fn is_code_domain(&self) -> bool {
        self.domain() == EntityDomain::Code
    }

    /// Check if this is a type definition
    pub fn is_type_definition(&self) -> bool {
        matches!(
            self,
            EntityKind::Class
                | EntityKind::Struct
                | EntityKind::Enum
                | EntityKind::Interface
                | EntityKind::Trait
                | EntityKind::TraitImpl
                | EntityKind::InherentImpl
                | EntityKind::TypeAlias
                | EntityKind::Union
        )
    }

    /// Check if this is a macro-like entity
    pub fn is_macro_like(&self) -> bool {
        matches!(self, EntityKind::Macro)
    }

    /// Check if this is an annotation-like entity
    pub fn is_annotation_like(&self) -> bool {
        matches!(self, EntityKind::Annotation)
    }

    /// Check if this is a function/method
    pub fn is_function_like(&self) -> bool {
        matches!(
            self,
            EntityKind::Function
                | EntityKind::Method
                | EntityKind::Constructor
                | EntityKind::Destructor
                | EntityKind::Operator
        )
    }

    /// Check if this is a variable/field/enum variant
    pub fn is_variable_like(&self) -> bool {
        matches!(
            self,
            EntityKind::Field
                | EntityKind::Property
                | EntityKind::Variable
                | EntityKind::Constant
                | EntityKind::EnumVariant
        )
    }

    /// Check if this is a module/namespace
    pub fn is_module_like(&self) -> bool {
        matches!(
            self,
            EntityKind::Module | EntityKind::Namespace | EntityKind::Package
        )
    }

    /// Check if this is a namespace entity.
    pub fn is_namespace(&self) -> bool {
        matches!(self, EntityKind::Namespace)
    }

    /// Check if this is an import-like entity (import/require/include/export).
    ///
    /// Import-like entities carry no retrieval value for vector/BM25 search;
    /// they are collected at the file level (summary) and in the relationship
    /// index instead. Grouping must keep them strictly apart from other
    /// entities so import-only groups can be dropped without losing content.
    pub fn is_import_like(&self) -> bool {
        matches!(
            self,
            EntityKind::Import | EntityKind::Require | EntityKind::Include | EntityKind::Export
        )
    }

    /// Check if this is a template/markup entity
    pub fn is_template_entity(&self) -> bool {
        matches!(
            self,
            EntityKind::Element
                | EntityKind::Attribute
                | EntityKind::Expression
                | EntityKind::Component
                | EntityKind::Template
                | EntityKind::Directive
                | EntityKind::ControlFlow
                | EntityKind::Binding
                | EntityKind::Action
                | EntityKind::EventHandler
        )
    }

    /// Check if this is a style-related entity (CSS/SCSS/LESS)
    pub fn is_style_related(&self) -> bool {
        matches!(
            self,
            EntityKind::StyleRule
                | EntityKind::StyleSelector
                | EntityKind::StyleProperty
                | EntityKind::Keyframe
        )
    }

    /// Check if this is an element-like entity (component-like)
    pub fn is_element_like(&self) -> bool {
        matches!(self, EntityKind::Element | EntityKind::Class)
    }

    /// Check if this entity can have children (methods, fields, etc.)
    pub fn can_have_children(&self) -> bool {
        self.is_type_definition()
            || self.is_module_like()
            || matches!(
                self,
                EntityKind::Element
                    | EntityKind::StyleRule
                    | EntityKind::Component
                    | EntityKind::Template
                    | EntityKind::ControlFlow
            )
    }

    /// Check if this entity can be a caller (can make function calls)
    pub fn can_be_caller(&self) -> bool {
        self.is_code_domain() && self.is_function_like()
    }

    /// Check if this is a test entity (test suite, test case, test hook)
    pub fn is_test_entity(&self) -> bool {
        matches!(
            self,
            EntityKind::TestSuite | EntityKind::TestCase | EntityKind::TestHook
        )
    }

    /// Check if this is test-related (including assertions and mocks)
    pub fn is_test_related(&self) -> bool {
        matches!(
            self,
            EntityKind::TestSuite
                | EntityKind::TestCase
                | EntityKind::TestHook
                | EntityKind::Assertion
                | EntityKind::Mock
        )
    }

    /// Check if this is an impl block (trait impl or inherent impl)
    pub fn is_impl_block(&self) -> bool {
        matches!(self, EntityKind::TraitImpl | EntityKind::InherentImpl)
    }

    /// Get a human-readable label for this entity kind.
    pub fn kind_label(&self) -> &'static str {
        match self {
            EntityKind::Module => "module",
            EntityKind::Namespace => "namespace",
            EntityKind::Package => "package",
            EntityKind::Class => "class",
            EntityKind::Struct => "struct",
            EntityKind::Enum => "enum",
            EntityKind::EnumVariant => "variant",
            EntityKind::Union => "union",
            EntityKind::Trait => "trait",
            EntityKind::Interface => "interface",
            EntityKind::TraitImpl => "trait implementation",
            EntityKind::InherentImpl => "inherent implementation",
            EntityKind::TypeAlias => "type alias",
            EntityKind::Function => "function",
            EntityKind::Method => "method",
            EntityKind::Constructor => "constructor",
            EntityKind::Destructor => "destructor",
            EntityKind::Operator => "operator",
            EntityKind::Field => "field",
            EntityKind::Property => "property",
            EntityKind::Variable => "variable",
            EntityKind::Constant => "constant",
            EntityKind::Import => "import",
            EntityKind::Require => "require",
            EntityKind::Include => "include",
            EntityKind::Export => "export",
            EntityKind::Annotation => "annotation",
            EntityKind::Macro => "macro",
            EntityKind::StyleRule => "style rule",
            EntityKind::StyleSelector => "style selector",
            EntityKind::StyleProperty => "style property",
            EntityKind::Keyframe => "keyframe",
            EntityKind::Element => "element",
            EntityKind::Attribute => "attribute",
            EntityKind::Expression => "expression",
            EntityKind::Component => "component",
            EntityKind::Template => "template",
            EntityKind::Directive => "directive",
            EntityKind::ControlFlow => "control flow",
            EntityKind::Animation => "animation",
            EntityKind::Binding => "binding",
            EntityKind::Action => "action",
            EntityKind::AtRule => "at-rule",
            EntityKind::EventHandler => "event handler",
            EntityKind::ScriptContent => "script content",
            EntityKind::StyleContent => "style content",
            EntityKind::EmbeddedBlock => "embedded block",
            EntityKind::TestSuite => "test suite",
            EntityKind::TestCase => "test case",
            EntityKind::TestHook => "test hook",
            EntityKind::Assertion => "assertion",
            EntityKind::Mock => "mock",
            EntityKind::Unknown => "unknown",
        }
    }

    /// Check if this is a named semantic entity suitable for keyword extraction.
    ///
    /// Returns `true` for entities that carry meaningful semantic names (types,
    /// functions, methods, constants, modules, components, test cases, etc.).
    /// Returns `false` for structural or incidental entities (macros, annotations,
    /// impl blocks, fields, variables, CSS rules, template internals, test hooks, etc.).
    ///
    /// This is an include-list: new variants are excluded by default and must be
    /// explicitly added after evaluation.
    pub fn is_named_semantic_entity(&self) -> bool {
        matches!(
            self,
            // Type definitions
            EntityKind::Class
                | EntityKind::Struct
                | EntityKind::Enum
                | EntityKind::Interface
                | EntityKind::Trait
                | EntityKind::TypeAlias
                | EntityKind::Union
            // Callable entities
            | EntityKind::Function
            | EntityKind::Method
            | EntityKind::Constructor
            | EntityKind::Destructor
            | EntityKind::Operator
            // Constants
            | EntityKind::Constant
            // Modules and namespaces
            | EntityKind::Module
            | EntityKind::Namespace
            | EntityKind::Package
            // Frontend components
            | EntityKind::Component
            // Test entities (named test suites and cases)
            | EntityKind::TestSuite
            | EntityKind::TestCase
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_import_like_covers_all_dependency_kinds() {
        // Import/require/include/export entities carry no retrieval
        // value for vector/BM25 search. They are collected at the file level
        // (summary) and in the relationship index instead.
        for kind in [
            EntityKind::Import,
            EntityKind::Require,
            EntityKind::Include,
            EntityKind::Export,
        ] {
            assert!(
                kind.is_import_like(),
                "{kind:?} must be treated as import-like"
            );
        }
    }

    #[test]
    fn test_is_import_like_excludes_regular_entities() {
        // Non-dependency entities must never be classified as import-like;
        // the grouper drops import-only groups, so a false positive would
        // silently remove real content from retrieval.
        for kind in [
            EntityKind::Module,
            EntityKind::Function,
            EntityKind::Struct,
            EntityKind::Class,
            EntityKind::Variable,
            EntityKind::TestCase,
            EntityKind::Unknown,
        ] {
            assert!(
                !kind.is_import_like(),
                "{kind:?} must NOT be treated as import-like"
            );
        }
    }
}
