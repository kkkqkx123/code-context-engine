use crate::parser::extractor::utils;
use crate::tree_sitter_query::capture::{
    CallCategory, CallSubcategory, extract_call_category, extract_call_subcategory,
};
use crate::tree_sitter_query::executor::{Capture, QueryMatch};
use cce_parser_core::{CapturedItem, LanguageRules, default_language_rules};
use cce_types::language::Language;
use cce_types::{Entity, EntityKind, Relation, RelationTarget, RelationType};

/// Receiver capture names that identify the object/type the call is made on.
///
/// These exclude the method-name captures (`call.method.function`,
/// `call.method.chained.to`, etc.) so the receiver is never confused with
/// the callee. The names vary per language but all end with a receiver
/// marker; they are matched by suffix to stay language-agnostic.
pub(crate) const RECEIVER_CAPTURE_SUFFIXES: &[&str] = &[
    ".method.object",
    ".method.receiver",
    ".method.instance.object",
    ".method.static.object",
    ".method.class.object",
    ".method.extension.object",
    ".method.chained.from",
    ".method.nullsafe.object",
    ".method.static.class",
    ".constructor.member.object",
    ".template.method.object",
    ".async.method.object",
];

/// Normalize a callee capture into a searchable name.
///
/// Deterministic replacement for the previous `split('(')` heuristic: all
/// string operations are linear scans without substring splits so nested
/// parentheses do not corrupt the result. Strips call arguments `(...)`,
/// macro `!`, and generics `<...>` but preserves qualified paths.
pub(crate) fn normalize_callee_name(text: &str) -> String {
    let mut s = text.trim();
    // Strip trailing macro `!` (e.g. `println!` -> `println`)
    if let Some(stripped) = s.strip_suffix('!') {
        s = stripped.trim();
    }
    // Strip arguments: truncate at first '(' found outside string literals is
    // overkill for capture text (captures never contain string literals with
    // parentheses); a simple first '(' scan is deterministic and preserves
    // qualification.
    if let Some(pos) = s.find('(') {
        s = s[..pos].trim();
        if let Some(stripped) = s.strip_suffix('!') {
            s = stripped.trim();
        }
    }
    // Strip generics: truncate at first '<'
    if let Some(pos) = s.find('<') {
        s = s[..pos].trim();
    }
    s.to_string()
}

/// Extract a callee name from a capture using AST-based access.
///
/// Uses `tree_sitter::Node` field-based access via `build_callee_name` instead
/// of source-text scanning. Falls back to `normalize_callee_name` when the
/// node cannot be retrieved from the tree.
fn ast_name_from_capture(
    capture: &Capture,
    tree: &tree_sitter::Tree,
    source: &str,
) -> Option<String> {
    // When capture text is explicitly set and differs from the AST node text,
    // prefer the capture text. This allows callers to override the source-level
    // text (e.g., normalizing `self` to `Self` for Rust type dispatch).
    if let Some(node) = tree
        .root_node()
        .descendant_for_byte_range(capture.start_byte, capture.end_byte)
    {
        if let Some(ast_name) =
            cce_parser_core::ast_accessor::build_callee_name(node, source.as_bytes())
        {
            if ast_name == capture.text || capture.text.is_empty() {
                return Some(ast_name);
            }
            // Capture text was overridden; use it instead of the AST text.
            return Some(capture.text.clone());
        }
    }
    // Fallback to capture text when AST lookup fails.
    if !capture.text.is_empty() {
        return Some(capture.text.clone());
    }
    None
}

/// Build the full callee name for a call match, preserving the receiver or
/// type path instead of dropping it.
///
/// Without this, `obj.method()` produced only `method` and `Vec::new()`
/// produced only `new`: the receiver/type segment is captured in a sibling
/// capture but was discarded. Reconstructing the full path keeps the
/// method-call graph resolvable and lets stdlib detection see the qualified
/// name it expects (`console.log` -> `console`, `Vec::new` -> `Vec`).
pub(crate) fn build_full_callee_name(
    mat: &QueryMatch,
    language: &Language,
    tree: &tree_sitter::Tree,
    source: &str,
) -> Option<String> {
    // Rust closure variable calls: `let f = |x| x; f(42)` -> callee name is the variable name
    if let Some(name) = utils::find_capture_by_name(&mat.captures, |name| {
        name.ends_with(".closure_variable.name")
    }) {
        return ast_name_from_capture(name, tree, source);
    }

    // Go function literal calls (go func() { ... }() / defer func() { ... }())
    // These have no named callee, use a placeholder
    if utils::find_capture_by_name(&mat.captures, |name| {
        name.ends_with(".goroutine.function_literal")
            || name.ends_with(".deferred.function_literal")
    })
    .is_some()
    {
        return Some("<func_literal>".to_string());
    }

    // Higher-order function calls (e.g., arr.map(x => x + 1))
    // The callee is the function receiving the callback
    if let Some(hof_name) = build_hof_callee_name(mat, language, tree, source) {
        return Some(hof_name);
    }

    // Method references (Java/Kotlin: ClassName::methodName)
    // Parse the raw source text to extract qualified name
    if let Some(reference_text) = utils::find_capture_by_name(&mat.captures, |name| {
        name.ends_with(".reference") || name.ends_with(".hof.callback")
    }) {
        if let Some(qualified_name) = parse_method_reference(&reference_text.text, language) {
            return Some(qualified_name);
        }
    }

    // Rust associated calls: `Type::func` and `mod::Type::func`.
    if let Some(path) = utils::find_capture_by_name(&mat.captures, |name| {
        name.ends_with(".associated.nested.path")
    }) {
        if let Some(func) = utils::find_capture_by_name(&mat.captures, |name| {
            name.ends_with(".associated.nested.function.name")
        }) {
            let path_name = ast_name_from_capture(path, tree, source)
                .unwrap_or_else(|| normalize_callee_name(&path.text));
            let func_name = ast_name_from_capture(func, tree, source)
                .unwrap_or_else(|| normalize_callee_name(&func.text));
            return Some(format!("{}::{}", path_name, func_name));
        }
    }
    if let Some(type_name) = utils::find_capture_by_name(&mat.captures, |name| {
        name.ends_with(".associated.type.name")
    }) {
        if let Some(func) = utils::find_capture_by_name(&mat.captures, |name| {
            name.ends_with(".associated.function.name")
        }) {
            let type_name_str = ast_name_from_capture(type_name, tree, source)
                .unwrap_or_else(|| normalize_callee_name(&type_name.text));
            let func_name = ast_name_from_capture(func, tree, source)
                .unwrap_or_else(|| normalize_callee_name(&func.text));
            return Some(format!("{}::{}", type_name_str, func_name));
        }
    }

    // Method-style calls: `<receiver>.<method>`. The receiver may itself be
    // a chained expression (`a.b.method()` -> `a.b`), which is kept as-is so
    // the chain prefix survives in the call graph.
    let method_capture = utils::find_capture_by_name(&mat.captures, |name| {
        name.ends_with(".method.function")
            || name.ends_with(".method.name")
            || name.ends_with(".method.chained.to")
            || name.ends_with(".method.chained.to.name")
            || name.ends_with(".method.instance.function")
            || name.ends_with(".method.static.function")
            || name.ends_with(".method.class.function")
            || name.ends_with(".method.extension.function")
            || name.ends_with(".method.static.qualified.function")
            || name.ends_with(".generic.method.name")
    })?;

    let receiver = utils::find_capture_by_name(&mat.captures, |name| {
        RECEIVER_CAPTURE_SUFFIXES
            .iter()
            .any(|suffix| name.ends_with(suffix))
    })?;

    let method_name = ast_name_from_capture(method_capture, tree, source)
        .unwrap_or_else(|| normalize_callee_name(&method_capture.text));
    let receiver_name = ast_name_from_capture(receiver, tree, source)
        .unwrap_or_else(|| normalize_callee_name(&receiver.text));

    Some(format_method_callee(&receiver_name, &method_name, language))
}

/// Format a method callee name from receiver and method components.
///
/// Handles trivial receiver skipping (`this`/`self`/`Self`) and Rust-specific
/// qualified dispatch (`Self::method`).
fn format_method_callee(receiver_name: &str, method_name: &str, language: &Language) -> String {
    if receiver_name.is_empty() {
        return method_name.to_string();
    }

    // Skip trivial receivers: `this`/`self` add no discrimination value and
    // would flood the graph with `this.xxx` names that never resolve.
    // For Rust, `self`/`Self` carry type information (`Self::method` dispatch)
    // and must be preserved so the resolver can handle them via
    // `resolve_via_type_member` instead of falling back to a bare name.
    if matches!(receiver_name, "this" | "self" | "Self") {
        if *language == Language::Rust {
            return format!("{receiver_name}.{method_name}");
        }
        return method_name.to_string();
    }

    format!("{receiver_name}.{method_name}")
}

/// Build the callee name for higher-order function calls.
///
/// For HOF calls like `arr.map(x => x + 1)`, the callee is the function
/// receiving the callback (e.g., `map`), not the callback itself.
fn build_hof_callee_name(
    mat: &QueryMatch,
    language: &Language,
    tree: &tree_sitter::Tree,
    source: &str,
) -> Option<String> {
    // Try to find the HOF function name capture
    if let Some(name) = utils::find_capture_by_name(&mat.captures, |name| {
        name.ends_with(".hof.name") || name.ends_with(".hof.method.name")
    }) {
        return ast_name_from_capture(name, tree, source)
            .or_else(|| Some(normalize_callee_name(&name.text)));
    }

    // For method calls with receiver (e.g., arr.map), find the receiver
    let method_capture =
        utils::find_capture_by_name(&mat.captures, |name| name.ends_with(".hof.method.name"));

    if let Some(method) = method_capture {
        let receiver = utils::find_capture_by_name(&mat.captures, |name| {
            RECEIVER_CAPTURE_SUFFIXES
                .iter()
                .any(|suffix| name.ends_with(suffix))
        });

        if let Some(recv) = receiver {
            let method_name = ast_name_from_capture(method, tree, source)
                .unwrap_or_else(|| normalize_callee_name(&method.text));
            let receiver_name = ast_name_from_capture(recv, tree, source)
                .unwrap_or_else(|| normalize_callee_name(&recv.text));
            return Some(format_method_callee(&receiver_name, &method_name, language));
        }
    }

    None
}

/// Parse a method reference expression to extract the qualified name.
///
/// For Java/Kotlin method references like `ClassName::methodName`, this function
/// splits the source text by `::` to extract the class/type and method names.
///
/// Returns `Some("ClassName::methodName")` on success, or `None` if the format
/// doesn't match a simple method reference pattern.
fn parse_method_reference(text: &str, language: &Language) -> Option<String> {
    let trimmed = text.trim();

    // Handle simple method references: ClassName::methodName
    if let Some(pos) = trimmed.find("::") {
        let receiver = trimmed[..pos].trim();
        let method = trimmed[pos + 2..].trim();

        // Skip trivial receivers
        if receiver.is_empty() || method.is_empty() {
            return None;
        }
        // receiver and method are already trimmed from `::` split, so they
        // don't contain `!`, `(`, or `<` — no need for normalize_callee_name.
        if matches!(receiver, "this" | "self" | "Self" | "super") {
            if *language == Language::Rust && matches!(receiver, "self" | "Self") {
                return Some(format!("{}.{}", receiver, method));
            }
            return Some(method.to_string());
        }

        return Some(format!("{}::{}", receiver, method));
    }

    None
}

/// Find the callee name capture using structured capture rules.
///
/// This function uses the `LanguageRules` trait and `CaptureRule` structs to
/// provide a deterministic, table-driven approach to capture selection.
/// The rules are statically defined per language, eliminating runtime
/// string matching heuristics.
pub(crate) fn find_callee_capture(mat: &QueryMatch) -> Option<&Capture> {
    // Convert Capture to CapturedItem for the new rule system
    let captured_items: Vec<CapturedItem> = mat
        .captures
        .iter()
        .map(|c| CapturedItem {
            name: c.name.clone(),
            text: c.text.clone(),
        })
        .collect();

    // Get the default language rules
    let rules = default_language_rules();

    // Try each rule in order
    for rule in rules.call_rules() {
        // Check if all required captures are present
        let all_required_present = rule
            .required
            .iter()
            .all(|required| captured_items.iter().any(|c| c.name.ends_with(required)));

        if all_required_present {
            // Apply the extraction function
            if let Some(_extracted_name) = (rule.extract)(&captured_items) {
                // Find the original Capture that matches the first required suffix
                // This maintains backward compatibility with the existing API
                for required in rule.required {
                    if let Some(capture) =
                        utils::find_capture_by_name(&mat.captures, |name| name.ends_with(required))
                    {
                        return Some(capture);
                    }
                }
            }
        }
    }

    None
}

/// Find the dependency name capture using structured capture rules.
///
/// This function uses the `LanguageRules` trait and `CaptureRule` structs to
/// provide a deterministic, table-driven approach to capture selection.
/// The rules are statically defined per language, eliminating runtime
/// string matching heuristics.
pub(crate) fn find_dependency_capture(mat: &QueryMatch) -> Option<&Capture> {
    // Convert Capture to CapturedItem for the new rule system
    let captured_items: Vec<CapturedItem> = mat
        .captures
        .iter()
        .map(|c| CapturedItem {
            name: c.name.clone(),
            text: c.text.clone(),
        })
        .collect();

    // Get the default language rules
    let rules = default_language_rules();

    // Try each rule in order
    for rule in rules.dependency_rules() {
        // Check if all required captures are present
        let all_required_present = rule
            .required
            .iter()
            .all(|required| captured_items.iter().any(|c| c.name.ends_with(required)));

        if all_required_present {
            // Apply the extraction function
            if let Some(_extracted_name) = (rule.extract)(&captured_items) {
                // Find the original Capture that matches the first required suffix
                // This maintains backward compatibility with the existing API
                for required in rule.required {
                    if let Some(capture) =
                        utils::find_capture_by_name(&mat.captures, |name| name.ends_with(required))
                    {
                        return Some(capture);
                    }
                }
            }
        }
    }

    None
}

/// Determine relation type from capture name
///
/// Maps capture names to fine-grained RelationType using typed CallCategory/CallSubcategory enums.
pub(crate) fn determine_call_relation_type(capture_name: &str) -> RelationType {
    let category = extract_call_category(capture_name);
    let subcategory = extract_call_subcategory(capture_name);

    let call_cat = category.and_then(CallCategory::from_capture_name);
    let call_sub = subcategory.and_then(CallSubcategory::from_capture_name);

    match (call_cat, call_sub) {
        // Function calls
        (Some(CallCategory::Function), _) => RelationType::DirectCall,

        // Method calls - distinguish instance vs static vs chained
        (Some(CallCategory::Method), Some(CallSubcategory::Static)) => {
            RelationType::StaticMethodCall
        }
        (Some(CallCategory::Method), Some(CallSubcategory::Chained)) => {
            RelationType::ChainedMethodCall
        }
        (Some(CallCategory::Method), _) => RelationType::InstanceMethodCall,

        // Constructor calls
        (Some(CallCategory::Constructor), _) => RelationType::ConstructorCall,

        // Pointer calls
        (Some(CallCategory::Pointer), _) => RelationType::PointerCall,

        // Callback calls
        (Some(CallCategory::Callback), _) => RelationType::CallbackCall,

        // Template/Generic calls
        (Some(CallCategory::Template) | Some(CallCategory::Generic), _) => {
            RelationType::GenericCall
        }

        // Macro calls
        (Some(CallCategory::Macro), _) => RelationType::MacroCall,

        // Goroutine calls (Go)
        (Some(CallCategory::Goroutine), _) => RelationType::GoroutineCall,

        // Deferred calls (Go)
        (Some(CallCategory::Deferred), _) => RelationType::DeferredCall,

        // Async calls
        (Some(CallCategory::Async), _) => RelationType::AsyncCall,

        // Closure calls (Rust)
        (Some(CallCategory::Closure), _) => RelationType::CallbackCall,

        // Closure variable calls (Rust: let f = |x| x; f(42))
        (Some(CallCategory::ClosureVariable), _) => RelationType::CallbackCall,

        // Inline closure calls (Rust: || {}())
        (Some(CallCategory::ClosureInline), _) => RelationType::CallbackCall,

        // Higher-order function calls (passing callbacks as arguments)
        (Some(CallCategory::HigherOrder), _) => RelationType::HigherOrderCall,

        // Super calls
        (Some(CallCategory::Super), _) => RelationType::InstanceMethodCall,

        // Yield calls (JS generator)
        (Some(CallCategory::Yield), _) => RelationType::DirectCall,

        // Associated function calls (Rust)
        (Some(CallCategory::Associated), _) => RelationType::DirectCall,

        // Reference calls (Java method reference)
        (Some(CallCategory::Reference), _) => RelationType::CallbackCall,

        // Field access
        (Some(CallCategory::Field), _) => RelationType::FieldAccess,

        // Promise chains (JS)
        (Some(CallCategory::Promise), _) => RelationType::InstanceMethodCall,

        // Special function calls (JS call/apply/bind)
        (Some(CallCategory::Special), _) => RelationType::InstanceMethodCall,

        // Delegate calls (C#)
        (Some(CallCategory::Delegate), _) => RelationType::CallbackCall,

        // Return expression calls
        (Some(CallCategory::Return), _) => RelationType::DirectCall,

        // Component instantiation (JSX/Vue/Svelte)
        (Some(CallCategory::Component), _) => RelationType::ConstructorCall,

        // Event callbacks
        (Some(CallCategory::Event), Some(CallSubcategory::CallbackEvent)) => {
            RelationType::EventCallback
        }
        (Some(CallCategory::Event), _) => RelationType::EventCallback,

        // Scala apply
        (Some(CallCategory::Apply), _) => RelationType::DirectCall,

        // Ruby binary operator calls
        (Some(CallCategory::Binary), _) => RelationType::DirectCall,

        // Dart/Ruby getter access
        (Some(CallCategory::Getter), _) => RelationType::FieldAccess,

        // Scala infix operator calls
        (Some(CallCategory::Infix), _) => RelationType::DirectCall,

        // PHP parent::method()
        (Some(CallCategory::Parent), _) => RelationType::InstanceMethodCall,

        // Ruby scope resolution
        (Some(CallCategory::Scope), _) => RelationType::InstanceMethodCall,

        // PHP self::method()
        (Some(CallCategory::Self_), _) => RelationType::StaticMethodCall,

        // Static method call via subcategory
        (_, Some(CallSubcategory::Static)) => RelationType::StaticMethodCall,

        // Chained method call via subcategory
        (_, Some(CallSubcategory::Chained)) => RelationType::ChainedMethodCall,

        _ => {
            tracing::warn!("Unknown call category in capture: {}", capture_name);
            RelationType::DirectCall
        }
    }
}

/// Determine dependency relation type from capture name
///
/// Maps capture names from spec.md to fine-grained RelationType.
/// Uses precise segment matching for type safety.
pub(crate) fn determine_dependency_relation_type(capture_name: &str) -> RelationType {
    // Parse capture name: dependency.category[.subtype][.attribute]
    let parts: Vec<&str> = capture_name.split('.').collect();

    if parts.len() < 2 {
        tracing::warn!(
            "Invalid dependency capture name (too few parts): {}",
            capture_name
        );
        return RelationType::ImportStandard;
    }

    let category = parts[1];
    let subtype = parts.get(2).copied();

    match category {
        // Include (C/C++)
        "include" => RelationType::IncludeLocal,

        // Import - handle various subtypes
        "import" => match subtype {
            Some("standard") => RelationType::ImportStandard,
            Some("named") => RelationType::ImportNamed,
            Some("default") => RelationType::ImportDefault,
            Some("namespace") => RelationType::ImportNamespace,
            Some("dynamic") => RelationType::ImportDynamic,
            Some("source") => RelationType::ImportStandard,
            // Python specific imports
            Some("module") => RelationType::ImportStandard,
            Some("from") => RelationType::ImportNamed,
            Some("relative") => RelationType::ImportStandard,
            Some("wildcard") => RelationType::ImportNamespace,
            Some("future") => RelationType::ImportStandard,
            // Go specific imports
            Some("alias") => RelationType::ImportNamed,
            Some("dot") => RelationType::ImportNamespace,
            Some("blank") => RelationType::ImportStandard,
            // Frontend specific imports
            Some("component") => RelationType::ImportStandard,
            Some("action") => RelationType::ImportStandard,
            Some("transition") => RelationType::ImportStandard,
            Some("animation") => RelationType::ImportStandard,
            _ => RelationType::ImportStandard,
        },

        // Use (Rust)
        "use" => RelationType::Use,

        // Using (C#/C++)
        "using" => RelationType::Using,

        // Namespace (C#/C++)
        "namespace" => RelationType::Using,

        // Require (JS/TS CommonJS)
        "require" => RelationType::ImportStandard,

        // Export (JS/TS)
        "export" => RelationType::ModuleDependency,

        // Extern crate (Rust)
        "extern_crate" => RelationType::ModuleDependency,

        // Reference (Rust)
        "reference" => RelationType::TypeReference,

        // Macro dependency (C/C++ preprocessor)
        "macro" => RelationType::MacroDependency,

        // Module dependency
        "module" => RelationType::ModuleDependency,

        // Package (Go/Java)
        "package" => RelationType::ModuleDependency,

        // Inheritance/Extension
        "extend" => RelationType::Inheritance,

        // Implementation
        "implement" => RelationType::Implementation,

        // Interface dependency
        "interface" => match subtype {
            Some("extends") => RelationType::Inheritance,
            _ => RelationType::Implementation,
        },

        // Typed class/interface captures used by TypeScript/TSX and similar schemes
        "class_extends" => RelationType::Inheritance,
        "class_implements" => RelationType::Implementation,
        "interface_extends" => RelationType::Inheritance,

        // Type-related dependencies
        "type" => match subtype {
            Some("base") => RelationType::Inheritance,
            Some("interface") => RelationType::Implementation,
            Some("extends") => RelationType::Inheritance,
            Some("reference") => RelationType::TypeReference,
            _ => RelationType::TypeReference,
        },

        // Trait bound (Rust)
        "trait_bound" => RelationType::TraitBound,

        // Implementation (Rust trait impl and similarly named captures)
        "implementation" => RelationType::Implementation,

        // Impl association (Rust impl blocks)
        "impl_association" => RelationType::ImplAssociation,

        // Embedding (Go embedded fields - from dependency_query)
        "embedding" => RelationType::Embedding,

        // Go interface embedding (`interface F { Stringer; ... }`): the
        // outer interface inherits the embedded interface's method set.
        "interface_embedding" => RelationType::TraitInheritance,

        // Type parameter bounds
        "type_parameter" => RelationType::TraitBound,

        // Generic/type constraints
        "generic_constraint" | "type_constraint" => RelationType::TraitBound,

        // Where clause (Rust)
        "where_clause" | "where_predicate" => RelationType::TraitBound,

        // ===== Missing dependency categories (added for scheme compatibility) =====

        // Ruby autoload
        "autoload" => RelationType::ImportStandard,

        // Ruby gem require
        "gem" => RelationType::ImportStandard,

        // PHP include_once
        "include_once" => RelationType::ImportStandard,

        // Ruby inheritance
        "inheritance" => RelationType::Inheritance,

        // Ruby load
        "load" => RelationType::ImportStandard,

        // Dart mixin / PHP trait use
        "mixin" => RelationType::Mixin,
        "trait" => RelationType::Mixin,

        // Dart part
        "part" => RelationType::ModuleDependency,

        // Dart part of
        "part_of" => RelationType::ModuleDependency,

        // Ruby prepend
        "prepend" => RelationType::Inheritance,

        // PHP require_once
        "require_once" => RelationType::ImportStandard,

        // Ruby require_relative
        "require_relative" => RelationType::ImportStandard,

        // HTML script tag dependency
        "script" => RelationType::ImportStandard,

        _ => {
            tracing::warn!("Unknown dependency category in capture: {}", capture_name);
            RelationType::ImportStandard
        }
    }
}

/// Derive Implementation/ImplAssociation relations from parsed impl block
/// entities.
///
/// Rust impl blocks are the single source of truth for these structural
/// relations: entity extraction parses `impl Trait for Type` once and
/// stores the trait path and target type in entity metadata
/// (`impl_trait`, `impl_for_type`). The dependency query does not
/// re-match `impl_item` nodes, so both relation types must be derived
/// here from the entity kinds.
pub(crate) fn extract_impl_block_relations(entities: &[Entity]) -> Vec<Relation> {
    let mut relations = Vec::new();
    for entity in entities {
        match entity.kind {
            EntityKind::TraitImpl => {
                // impl_trait metadata is already a clean path from tree-sitter capture
                if let Some(trait_path) = entity.get_metadata("impl_trait") {
                    relations.push(Relation::entity_relation(
                        entity.id.0 as i64,
                        RelationTarget::unresolved(trait_path.clone()),
                        RelationType::Implementation,
                        entity.span,
                    ));
                }
                // impl_for_type metadata is already a simple name (generics stripped)
                if let Some(for_type) = entity.get_metadata("impl_for_type") {
                    relations.push(Relation::entity_relation(
                        entity.id.0 as i64,
                        RelationTarget::unresolved(for_type.clone()),
                        RelationType::ImplAssociation,
                        entity.span,
                    ));
                }
            }
            EntityKind::InherentImpl => {
                if let Some(for_type) = entity.get_metadata("impl_for_type") {
                    relations.push(Relation::entity_relation(
                        entity.id.0 as i64,
                        RelationTarget::unresolved(for_type.clone()),
                        RelationType::ImplAssociation,
                        entity.span,
                    ));
                }
            }
            _ => {}
        }
    }
    relations
}
