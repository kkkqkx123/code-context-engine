//! Capture name constants for tree-sitter query entity categories.
//!
//! This module provides string constants for capture names used in tree-sitter queries.
//! All constants in this file are part of the active query system.

// =============================================================================
// Entity Capture Prefixes and Suffixes
// =============================================================================

/// Prefix for all entity captures
pub const ENTITY_PREFIX: &str = "entity.";

/// Prefix for all call captures
pub const CALL_PREFIX: &str = "call.";

/// Suffix for name captures (e.g., "entity.function.name")
pub const NAME_SUFFIX: &str = ".name";

// =============================================================================
// Entity Categories
// =============================================================================

/// Class entity category
pub const CATEGORY_CLASS: &str = "class";

/// Struct entity category
pub const CATEGORY_STRUCT: &str = "struct";

/// Enum entity category
pub const CATEGORY_ENUM: &str = "enum";

/// Enum variant entity category
pub const CATEGORY_ENUM_VARIANT: &str = "enum_variant";

/// Enum constant entity category
pub const CATEGORY_ENUM_CONSTANT: &str = "enum_constant";

/// Enum member entity category
pub const CATEGORY_ENUM_MEMBER: &str = "enum_member";

/// Union entity category
pub const CATEGORY_UNION: &str = "union";

/// Trait entity category
pub const CATEGORY_TRAIT: &str = "trait";

/// Interface entity category
pub const CATEGORY_INTERFACE: &str = "interface";

/// Type entity category
pub const CATEGORY_TYPE: &str = "type";

/// Type alias entity category
pub const CATEGORY_TYPE_ALIAS: &str = "type_alias";

/// Record entity category
pub const CATEGORY_RECORD: &str = "record";

/// Function entity category
pub const CATEGORY_FUNCTION: &str = "function";

/// Method entity category
pub const CATEGORY_METHOD: &str = "method";

/// Constructor entity category
pub const CATEGORY_CONSTRUCTOR: &str = "constructor";

/// Destructor entity category
pub const CATEGORY_DESTRUCTOR: &str = "destructor";

/// Variable entity category
pub const CATEGORY_VARIABLE: &str = "variable";

/// Const entity category
pub const CATEGORY_CONST: &str = "const";

/// Constant entity category
pub const CATEGORY_CONSTANT: &str = "constant";

/// Static entity category
pub const CATEGORY_STATIC: &str = "static";

/// Field entity category
pub const CATEGORY_FIELD: &str = "field";

/// Tagged field entity category
pub const CATEGORY_FIELD_TAGGED: &str = "field_tagged";

/// Bitfield entity category
pub const CATEGORY_BITFIELD: &str = "bitfield";

/// Property entity category
pub const CATEGORY_PROPERTY: &str = "property";

/// Module entity category
pub const CATEGORY_MODULE: &str = "module";

/// Namespace entity category
pub const CATEGORY_NAMESPACE: &str = "namespace";

/// Package entity category
pub const CATEGORY_PACKAGE: &str = "package";

/// Import statement entity category
pub const CATEGORY_IMPORT: &str = "import";

/// Export statement entity category (JavaScript/TypeScript export)
pub const CATEGORY_EXPORT: &str = "export";

/// Require statement entity category (Ruby require, Lua require, JS require, PHP require)
pub const CATEGORY_REQUIRE: &str = "require";

/// Include statement entity category (Ruby include, PHP include, C/C++ #include)
pub const CATEGORY_INCLUDE: &str = "include";

/// Annotation entity category
pub const CATEGORY_ANNOTATION: &str = "annotation";

/// Decorator entity category
pub const CATEGORY_DECORATOR: &str = "decorator";

/// Lambda entity category
pub const CATEGORY_LAMBDA: &str = "lambda";

/// Closure entity category (Rust closures)
pub const CATEGORY_CLOSURE: &str = "closure";

/// Function literal entity category (Go function literals)
pub const CATEGORY_FUNCTION_LITERAL: &str = "function_literal";

/// Comprehension entity category
pub const CATEGORY_COMPREHENSION: &str = "comprehension";

/// Macro entity category
pub const CATEGORY_MACRO: &str = "macro";

/// Preprocessor entity category
pub const CATEGORY_PREPROCESSOR: &str = "preprocessor";

/// Impl block entity category
pub const CATEGORY_IMPL: &str = "impl";

/// Operator subcategory
pub const SUBCATEGORY_OPERATOR: &str = "operator";

/// Getter subcategory
pub const SUBCATEGORY_GETTER: &str = "getter";

/// Setter subcategory
pub const SUBCATEGORY_SETTER: &str = "setter";

// =============================================================================
// Frontend Framework Categories
// =============================================================================

/// JSX entity category
pub const CATEGORY_JSX: &str = "jsx";

/// Component entity category
pub const CATEGORY_COMPONENT: &str = "component";

/// Attribute entity category
pub const CATEGORY_ATTRIBUTE: &str = "attribute";

/// Expression entity category
pub const CATEGORY_EXPRESSION: &str = "expression";

/// Template entity category
pub const CATEGORY_TEMPLATE: &str = "template";

/// Directive entity category
pub const CATEGORY_DIRECTIVE: &str = "directive";

/// Interpolation entity category
pub const CATEGORY_INTERPOLATION: &str = "interpolation";

/// Document entity category
pub const CATEGORY_DOCUMENT: &str = "document";

/// Control flow categories
pub const CATEGORY_IF: &str = "if";
pub const CATEGORY_ELSE: &str = "else";
pub const CATEGORY_ELSE_IF: &str = "else_if";
pub const CATEGORY_EACH: &str = "each";
pub const CATEGORY_AWAIT: &str = "await";
pub const CATEGORY_CATCH: &str = "catch";
pub const CATEGORY_THEN: &str = "then";
pub const CATEGORY_KEY: &str = "key";

/// Transition entity category
pub const CATEGORY_TRANSITION: &str = "transition";

/// Animation entity category
pub const CATEGORY_ANIMATION: &str = "animation";

/// Binding entity category
pub const CATEGORY_BINDING: &str = "binding";

// =============================================================================
// CSS Categories
// =============================================================================

/// Style rule entity category
pub const CATEGORY_STYLE_RULE: &str = "style_rule";

/// Style selector entity category
pub const CATEGORY_STYLE_SELECTOR: &str = "style_selector";

/// Style property entity category
pub const CATEGORY_STYLE_PROPERTY: &str = "style_property";

/// Style value entity category
pub const CATEGORY_STYLE_VALUE: &str = "style_value";

/// At-rule entity category
pub const CATEGORY_AT: &str = "at";

/// Keyframe entity category
pub const CATEGORY_KEYFRAME: &str = "keyframe";

// =============================================================================
// HTML Categories
// =============================================================================

/// Element entity category
pub const CATEGORY_ELEMENT: &str = "element";

/// Tag entity category
pub const CATEGORY_TAG: &str = "tag";

/// Doctype entity category
pub const CATEGORY_DOCTYPE: &str = "doctype";

// =============================================================================
// Embedded Content Categories
// =============================================================================

/// Embedded block entity category
pub const CATEGORY_EMBEDDED: &str = "embedded";

/// Script entity category
pub const CATEGORY_SCRIPT: &str = "script";

/// Script content subcategory
pub const SUBCATEGORY_SCRIPT_CONTENT: &str = "content";

/// Style entity category
pub const CATEGORY_STYLE: &str = "style";

/// Style content subcategory
pub const SUBCATEGORY_STYLE_CONTENT: &str = "content";

/// CSS-in-JS entity category
pub const CATEGORY_CSS_IN_JS: &str = "css_in_js";

/// Control entity category
pub const CATEGORY_CONTROL: &str = "control";

/// Event entity category
pub const CATEGORY_EVENT: &str = "event";

/// Event handler subcategory
pub const SUBCATEGORY_EVENT_HANDLER: &str = "handler";

/// Event modifier subcategory
pub const SUBCATEGORY_EVENT_MODIFIER: &str = "modifier";

/// Class directive category
pub const CATEGORY_CLASS_DIRECTIVE: &str = "class_directive";

/// Style directive category
pub const CATEGORY_STYLE_DIRECTIVE: &str = "style_directive";

/// Use directive category (Svelte actions)
pub const CATEGORY_USE_DIRECTIVE: &str = "use_directive";

// =============================================================================
// Skip Categories
// =============================================================================

/// Raw text entity category
pub const CATEGORY_RAW_TEXT: &str = "raw_text";

/// Raw text expression category
pub const CATEGORY_RAW_TEXT_EXPR: &str = "raw_text_expr";

/// Raw text await category
pub const CATEGORY_RAW_TEXT_AWAIT: &str = "raw_text_await";

/// Raw text each category
pub const CATEGORY_RAW_TEXT_EACH: &str = "raw_text_each";

/// Text entity category
pub const CATEGORY_TEXT: &str = "text";

/// Comment entity category
pub const CATEGORY_COMMENT: &str = "comment";

/// Contains entity category (structural only)
pub const CATEGORY_CONTAINS: &str = "contains";

// =============================================================================
// Missing Entity Categories (added for scheme compatibility)
// =============================================================================

/// Anchor entity category (HTML <a>)
pub const CATEGORY_ANCHOR: &str = "anchor";

/// Attr entity category (Ruby attr_accessor, Svelte attributes)
pub const CATEGORY_ATTR: &str = "attr";

/// Button entity category (HTML <button>)
pub const CATEGORY_BUTTON: &str = "button";

/// Companion entity category (Kotlin companion object)
pub const CATEGORY_COMPANION: &str = "companion";

/// Anonymous enum entity category (C)
pub const CATEGORY_ENUM_ANON: &str = "enum_anon";

/// Enum case entity category (Scala)
pub const CATEGORY_ENUM_CASE: &str = "enum_case";

/// Extension entity category (Dart, Kotlin extension functions)
pub const CATEGORY_EXTENSION: &str = "extension";

/// Form entity category (HTML <form>)
pub const CATEGORY_FORM: &str = "form";

/// Given entity category (Scala 3 given instances)
pub const CATEGORY_GIVEN: &str = "given";

/// Input entity category (HTML <input>)
pub const CATEGORY_INPUT: &str = "input";

/// Mixin entity category (Dart mixins)
pub const CATEGORY_MIXIN: &str = "mixin";

/// Select entity category (HTML <select>)
pub const CATEGORY_SELECT: &str = "select";

/// Singleton entity category (Ruby singleton methods)
pub const CATEGORY_SINGLETON: &str = "singleton";

/// Slot entity category (Vue slots)
pub const CATEGORY_SLOT: &str = "slot";

/// Slot content entity category (Vue slot content)
pub const CATEGORY_SLOT_CONTENT: &str = "slot_content";

/// Anonymous struct entity category (C)
pub const CATEGORY_STRUCT_ANON: &str = "struct_anon";

/// Template reference entity category (Svelte)
pub const CATEGORY_TEMPLATE_REFERENCE: &str = "template_reference";

/// Textarea entity category (HTML <textarea>)
pub const CATEGORY_TEXTAREA: &str = "textarea";

/// Undef entity category (Ruby undef)
pub const CATEGORY_UNDEF: &str = "undef";

/// Anonymous union entity category (C)
pub const CATEGORY_UNION_ANON: &str = "union_anon";

/// Using entity category (C++ using, C# using)
pub const CATEGORY_USING: &str = "using";

/// Table entity category (Lua table)
pub const CATEGORY_TABLE: &str = "table";

/// Alias entity category (Ruby alias)
pub const CATEGORY_ALIAS: &str = "alias";

// =============================================================================
// Additional Entity Categories (from ignored_entity_capture_analysis)
// =============================================================================

/// Interface method entity category (Go interface methods)
pub const CATEGORY_INTERFACE_METHOD: &str = "interface_method";

/// Class expression entity category (JavaScript class expressions)
pub const CATEGORY_CLASS_EXPRESSION: &str = "class_expression";

/// Type constraint entity category (Python TypeVar)
pub const CATEGORY_TYPE_CONSTRAINT: &str = "type_constraint";

/// Delegate entity category (C# delegate declarations)
pub const CATEGORY_DELEGATE: &str = "delegate";

/// Typedef entity category (C typedef definitions)
pub const CATEGORY_TYPEDEF: &str = "typedef";

/// Typedef struct entity category (C typedef struct)
pub const CATEGORY_TYPEDEF_STRUCT: &str = "typedef_struct";

/// Typedef union entity category (C typedef union)
pub const CATEGORY_TYPEDEF_UNION: &str = "typedef_union";

/// Typedef enum entity category (C typedef enum)
pub const CATEGORY_TYPEDEF_ENUM: &str = "typedef_enum";

/// Typedef function pointer entity category (C typedef function pointer)
pub const CATEGORY_TYPEDEF_FUNCTION_POINTER: &str = "typedef_function_pointer";

/// Object declaration entity category (Kotlin/Scala singleton objects)
pub const CATEGORY_OBJECT: &str = "object";

// =============================================================================
// Test Entity Categories (for future tree-sitter query direct capture)
// =============================================================================

/// Test suite entity category (describe, mod test, *Test class)
pub const CATEGORY_TEST_SUITE: &str = "test_suite";

/// Test case entity category (it, test, #[test] fn)
pub const CATEGORY_TEST_CASE: &str = "test_case";

/// Test hook entity category (before, after, beforeEach, afterEach)
pub const CATEGORY_TEST_HOOK: &str = "test_hook";

/// Assertion entity category (expect, assert)
pub const CATEGORY_ASSERTION: &str = "assertion";

/// Mock/Spy/Stub entity category
pub const CATEGORY_MOCK: &str = "mock";

// =============================================================================
// Parameter-related Substrings
pub const SUBSTRING_PARAMETER: &str = "parameter";

/// Param substring for matching
pub const SUBSTRING_PARAM: &str = "param";

/// Self parameter substring
pub const SUBSTRING_SELF_PARAM: &str = "self";

// =============================================================================
// Return Type and Doc Comment Substrings
// =============================================================================

/// Return substring for matching
pub const SUBSTRING_RETURN: &str = "return";

/// Result substring for matching
pub const SUBSTRING_RESULT: &str = "result";

/// Doc substring for matching
pub const SUBSTRING_DOC: &str = "doc";

/// Comment substring for matching
pub const SUBSTRING_COMMENT: &str = "comment";
