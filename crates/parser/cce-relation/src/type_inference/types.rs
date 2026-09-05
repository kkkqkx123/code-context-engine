//! Core type definitions for type inference.

use cce_types::entity::EntityId;
use cce_types::language::Language;
use std::collections::HashMap;

/// A single type binding: a variable or parameter name maps to a type.
#[derive(Debug, Clone, Default)]
pub struct TypeBinding {
    /// The type's qualified name (e.g., "path::to::Struct", "builtins.int")
    pub type_name: String,
    /// Entity ID of the type definition (if resolved within the project)
    pub type_entity_id: Option<EntityId>,
    /// Source span where this binding was inferred
    pub span: cce_types::Span,
    /// Origin tag for debugging and ranking; determines priority when merging.
    pub origin: Option<InferenceOrigin>,
    /// Parsed structured type (None for simple types)
    pub shape: Option<TypeShape>,
}

/// Confidence level for variable type bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeConfidence {
    High,
    Medium,
    Low,
}

/// Variable type binding with support for conditional assignments.
#[derive(Debug, Clone)]
pub struct VariableTypeBinding {
    /// Primary type binding
    pub primary: TypeBinding,
    /// Alternative types from conditional assignments
    pub alternatives: Vec<TypeBinding>,
    /// Confidence level of this binding
    pub confidence: TypeConfidence,
}

impl VariableTypeBinding {
    /// Create a new variable type binding
    pub fn new(primary: TypeBinding) -> Self {
        Self {
            primary,
            alternatives: Vec::new(),
            confidence: TypeConfidence::Medium,
        }
    }

    /// Add an alternative type binding
    pub fn add_alternative(&mut self, alternative: TypeBinding) {
        self.alternatives.push(alternative);
    }

    /// Get all possible types (primary + alternatives)
    pub fn all_types(&self) -> Vec<&TypeBinding> {
        let mut types = vec![&self.primary];
        types.extend(self.alternatives.iter());
        types
    }

    /// Merge two variable type bindings
    pub fn merge(self, other: VariableTypeBinding) -> Self {
        let mut result = self;
        result
            .alternatives
            .extend(other.all_types().into_iter().cloned());
        result
    }

    /// Get the most specific type (highest priority origin)
    pub fn most_specific(&self) -> &TypeBinding {
        self.all_types()
            .iter()
            .max_by_key(|b| origin_priority(b.origin))
            .copied()
            .unwrap_or(&self.primary)
    }
}

/// Pattern for pattern matching
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Identifier pattern: `let x = ...`
    Identifier(String),
    /// Tuple pattern: `let (a, b) = ...`
    Tuple(Vec<String>),
    /// Struct pattern: `let Point { x, y } = ...`
    Struct(Vec<String>),
    /// Wildcard pattern: `let _ = ...`
    Wildcard,
}

/// Origin tag for a type inference binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InferenceOrigin {
    TypeAnnotation,
    ConstructorCall,
    LiteralType,
    FunctionReturn,
    GenericInference,
    OverloadResolution,
    CrossFilePropagation,
    ControlFlowNarrowing,
    PatternMatching,
    DestructuringAssignment,
}

/// Priority for an inference origin (higher = more reliable).
///
/// Deterministic ranking replaces the former `TypeConfidence` two-level
/// confidence scale. `None` (unknown origin) has the lowest priority.
pub fn origin_priority(origin: Option<InferenceOrigin>) -> u8 {
    match origin {
        Some(InferenceOrigin::TypeAnnotation) => 8,
        Some(InferenceOrigin::ControlFlowNarrowing) => 7,
        Some(InferenceOrigin::FunctionReturn) => 6,
        Some(InferenceOrigin::PatternMatching) => 6,
        Some(InferenceOrigin::OverloadResolution) => 5,
        Some(InferenceOrigin::DestructuringAssignment) => 5,
        Some(InferenceOrigin::CrossFilePropagation) => 4,
        Some(InferenceOrigin::GenericInference) => 3,
        Some(InferenceOrigin::ConstructorCall) => 2,
        Some(InferenceOrigin::LiteralType) => 1,
        None => 0,
    }
}

/// Priority for a non-optional origin.
pub fn origin_priority_of(origin: InferenceOrigin) -> u8 {
    origin_priority(Some(origin))
}

/// Structured type representation for compound types.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeShape {
    /// Simple named type (e.g., "String", "int", "Vec<T>")
    Named(String),
    /// Union type (e.g., TypeScript `string | number`)
    Union(Vec<TypeShape>),
    /// Intersection type (e.g., TypeScript `A & B`)
    Intersection(Vec<TypeShape>),
    /// Array/slice type
    Array(Box<TypeShape>),
    /// Generic type with type arguments (e.g., `Vec<T>`, `Map<K,V>`)
    Generic { base: String, args: Vec<TypeShape> },
    /// Reference type (e.g., `&str`, `&mut Vec<T>`)
    Reference {
        inner: Box<TypeShape>,
        mutable: bool,
    },
    /// Type parameter reference (e.g., `T`, `K`, `V`)
    Param(String),
    /// Wildcard type argument (e.g., `?`, `? extends Number`)
    Wildcard { bound: Option<String> },
}

impl TypeShape {
    /// Convert shape back to string representation.
    pub fn to_type_string(&self) -> String {
        type_shape_to_string(self)
    }
}

/// Convert a TypeShape to its string form.
pub fn type_shape_to_string(shape: &TypeShape) -> String {
    match shape {
        TypeShape::Named(s) => s.clone(),
        TypeShape::Union(members) => members
            .iter()
            .map(type_shape_to_string)
            .collect::<Vec<_>>()
            .join(" | "),
        TypeShape::Intersection(members) => members
            .iter()
            .map(type_shape_to_string)
            .collect::<Vec<_>>()
            .join(" & "),
        TypeShape::Array(inner) => format!("{}[]", type_shape_to_string(inner)),
        TypeShape::Generic { base, args } => {
            let args_str = args
                .iter()
                .map(type_shape_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", base, args_str)
        }
        TypeShape::Reference { inner, mutable } => {
            if *mutable {
                format!("&mut {}", type_shape_to_string(inner))
            } else {
                format!("&{}", type_shape_to_string(inner))
            }
        }
        TypeShape::Param(p) => p.clone(),
        TypeShape::Wildcard { bound: Some(b) } => format!("? extends {}", b),
        TypeShape::Wildcard { bound: None } => "?".to_string(),
    }
}

/// Split comma-separated type arguments while respecting nested bracket depth.
///
/// E.g., `"String, List<Integer>"` → `["String", "List<Integer>"]`
fn split_type_args(inner: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in inner.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }
    args
}

/// Check if a name looks like a generic type parameter.
///
/// Matches single uppercase letters (T, K, V, E) and common multi-letter
/// parameter names (Value, Key, etc.).
fn is_type_param_name(name: &str) -> bool {
    if name.len() == 1 {
        let is_upper = name.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        if is_upper {
            // Single-letter type params outside the well-known set (`T`/`K`/`V`/`E`
            // etc.) are still accepted, but log a warning so unknown
            // conventions are visible for future deterministic tightening.
            const KNOWN: &[&str] = &["T", "K", "V", "E", "U", "R", "A", "B", "C"];
            if !KNOWN.contains(&name) {
                tracing::warn!(
                    type_param = name,
                    "unrecognized single-letter type param, treating as generic"
                );
            }
        }
        return is_upper;
    }
    matches!(
        name,
        "T" | "K" | "V" | "E" | "U" | "R" | "A" | "B" | "C" | "Value" | "Key" | "Element" | "Item"
    )
}

/// Parse a type name string into a TypeShape.
#[allow(clippy::only_used_in_recursion)]
pub fn parse_type_shape(
    type_name: &str,
    language: cce_types::language::Language,
) -> Option<TypeShape> {
    let trimmed = type_name.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Handle wildcard `?`, `? extends T`, `? super T`
    if trimmed == "?" {
        return Some(TypeShape::Wildcard { bound: None });
    }
    if let Some(rest) = trimmed
        .strip_prefix("?")
        .and_then(|r| r.strip_prefix("extends"))
    {
        let bound = rest.trim().to_string();
        if !bound.is_empty() {
            return Some(TypeShape::Wildcard { bound: Some(bound) });
        }
        return Some(TypeShape::Wildcard { bound: None });
    }
    if let Some(rest) = trimmed
        .strip_prefix("?")
        .and_then(|r| r.strip_prefix("super"))
    {
        let bound = rest.trim().to_string();
        if !bound.is_empty() {
            return Some(TypeShape::Wildcard { bound: Some(bound) });
        }
        return Some(TypeShape::Wildcard { bound: None });
    }
    // Handle type parameter: single uppercase letter or common param names
    if is_type_param_name(trimmed) {
        return Some(TypeShape::Param(trimmed.to_string()));
    }
    // Handle union ` | `
    if trimmed.contains(" | ") {
        let parts: Vec<TypeShape> = trimmed
            .split(" | ")
            .filter_map(|p| parse_type_shape(p.trim(), language))
            .collect();
        if parts.len() > 1 {
            return Some(TypeShape::Union(parts));
        }
    }
    // Handle intersection ` & `
    if trimmed.contains(" & ") {
        let parts: Vec<TypeShape> = trimmed
            .split(" & ")
            .filter_map(|p| parse_type_shape(p.trim(), language))
            .collect();
        if parts.len() > 1 {
            return Some(TypeShape::Intersection(parts));
        }
    }
    // Handle array suffix `[]`
    if let Some(elem) = trimmed.strip_suffix("[]") {
        if let Some(inner) = parse_type_shape(elem.trim(), language) {
            return Some(TypeShape::Array(Box::new(inner)));
        }
    }
    // Handle reference `&` or `&mut` (Rust)
    if trimmed.starts_with('&') {
        let rest = trimmed.trim_start_matches('&').trim();
        let (is_mut, inner_str) = if let Some(stripped) = rest.strip_prefix("mut") {
            (true, stripped.trim())
        } else {
            (false, rest)
        };
        // Strip lifetime `'a`
        let inner_str = if inner_str.starts_with('\'') {
            inner_str
                .split_whitespace()
                .nth(1)
                .unwrap_or(inner_str)
                .trim()
        } else {
            inner_str
        };
        if let Some(inner) = parse_type_shape(inner_str, language) {
            return Some(TypeShape::Reference {
                inner: Box::new(inner),
                mutable: is_mut,
            });
        }
        return Some(TypeShape::Named(trimmed.to_string()));
    }
    // Handle bracket generics `Base[Args]` (Python `tuple[int, str]`,
    // `dict[str, int]`, TypeScript `Array<string>`-adjacent forms).
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            if start < end && start > 0 {
                let base = trimmed[..start].trim().to_string();
                if !base.is_empty()
                    && base
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ' ')
                {
                    let inner = trimmed[start + 1..end].trim();
                    let arg_strs = split_type_args(inner);
                    let args: Vec<TypeShape> = arg_strs
                        .iter()
                        .filter_map(|a| parse_type_shape(a.trim(), language))
                        .collect();
                    if !args.is_empty() {
                        return Some(TypeShape::Generic { base, args });
                    }
                }
            }
        }
    }
    // Handle generic `Base<Args>`
    if let Some(start) = trimmed.find('<') {
        if let Some(end) = trimmed.rfind('>') {
            if start < end {
                let base = trimmed[..start].trim().to_string();
                let inner = trimmed[start + 1..end].trim();
                let arg_strs = split_type_args(inner);
                let args: Vec<TypeShape> = arg_strs
                    .iter()
                    .filter_map(|a| parse_type_shape(a.trim(), language))
                    .collect();
                if !args.is_empty() {
                    return Some(TypeShape::Generic { base, args });
                }
            }
        }
    }
    Some(TypeShape::Named(trimmed.to_string()))
}

/// Get all possible member names from a TypeShape (union/intersection flattening).
pub fn shape_members(shape: &TypeShape) -> Vec<String> {
    match shape {
        TypeShape::Named(s) => vec![s.clone()],
        TypeShape::Union(m) | TypeShape::Intersection(m) => {
            m.iter().flat_map(shape_members).collect()
        }
        TypeShape::Array(inner) => shape_members(inner),
        TypeShape::Generic { base, .. } => vec![base.clone()],
        TypeShape::Reference { inner, .. } => shape_members(inner),
        TypeShape::Param(p) => vec![p.clone()],
        TypeShape::Wildcard { bound: Some(b), .. } => vec![b.clone()],
        TypeShape::Wildcard { bound: None, .. } => vec!["?".to_string()],
    }
}

/// Extract the iterated element type of a container shape.
///
/// Arrays yield their inner type. Generic containers with at least one type
/// argument yield the first argument, which models iteration over sequences
/// and key iteration over maps. All other shapes yield `None` so callers
/// stay conservative instead of guessing.
pub fn element_type_of_shape(shape: &TypeShape) -> Option<TypeShape> {
    match shape {
        TypeShape::Array(inner) => Some((**inner).clone()),
        TypeShape::Generic { args, .. } => args.first().cloned(),
        _ => None,
    }
}

/// Check if a TypeShape is compatible with another (for narrowing).
pub fn shape_is_subtype(sub: &TypeShape, super_: &TypeShape) -> bool {
    if sub == super_ {
        return true;
    }
    match super_ {
        TypeShape::Union(members) => members.iter().any(|m| shape_is_subtype(sub, m)),
        TypeShape::Named(super_name) => {
            if let TypeShape::Named(sub_name) = sub {
                sub_name == super_name
            } else {
                false
            }
        }
        TypeShape::Param(_) => {
            // A type parameter is compatible with itself (handled by == above)
            // or with a named type if they match by name
            if let (TypeShape::Param(sub_p), TypeShape::Param(super_p)) = (sub, super_) {
                sub_p == super_p
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Strip reference/lifetime annotations from a Rust type name.
pub fn strip_references(type_name: &str) -> (String, bool, bool) {
    let trimmed = type_name.trim();
    let is_ref = trimmed.starts_with('&');
    if !is_ref {
        return (trimmed.to_string(), false, false);
    }
    let rest = trimmed.trim_start_matches('&').trim();
    let (is_mut, rest) = if let Some(stripped) = rest.strip_prefix("mut") {
        (true, stripped.trim())
    } else {
        (false, rest)
    };
    // Strip lifetime `'a`
    let rest = if rest.starts_with('\'') {
        rest.split_whitespace()
            .skip(1)
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        rest.to_string()
    };
    let rest = rest.trim().to_string();
    if rest.is_empty() {
        (trimmed.to_string(), is_mut, is_ref)
    } else {
        (rest, is_mut, is_ref)
    }
}

/// Check if a type is a reference type.
pub fn is_reference(type_name: &str) -> bool {
    type_name.trim().starts_with('&')
}

/// Check if a type is a mutable reference.
pub fn is_mut_reference(type_name: &str) -> bool {
    let trimmed = type_name.trim();
    trimmed.starts_with("&mut")
}

/// Narrow a union type using a discriminated field value.
///
/// Deterministic strategy: only `TypeMemberIndex` field lookup is used.
/// Heuristic case-insensitive name matching (`Pass 2`) has been thoroughly
/// removed. If the index is unavailable or no member contains the field,
/// narrowing returns `None` instead of guessing via string containment.
pub fn narrow_discriminated_union(
    shape: &TypeShape,
    field_name: &str,
    _field_value: &str,
    type_index: Option<&crate::symbol_table::TypeMemberIndex>,
) -> Option<TypeShape> {
    if let TypeShape::Union(members) = shape {
        if let Some(index) = type_index {
            let mut matched: Vec<TypeShape> = Vec::new();
            for member in members {
                let names = shape_members(member);
                for name in &names {
                    if let Some(entry) = index.get_type(name) {
                        if entry.fields.contains_key(field_name) {
                            matched.push(member.clone());
                            break;
                        }
                    }
                }
            }
            if !matched.is_empty() {
                return match matched.len() {
                    1 => Some(matched.into_iter().next().expect("one")),
                    _ => Some(TypeShape::Union(matched)),
                };
            }
        }
        return None;
    }
    None
}

/// Filter a union type based on truthiness assumption.
pub fn narrow_truthiness(
    shape: &TypeShape,
    assume_true: bool,
    language: Language,
) -> Option<TypeShape> {
    if let TypeShape::Union(members) = shape {
        let filtered: Vec<TypeShape> = if assume_true {
            members
                .iter()
                .filter(|m| !is_falsy_type(m, language))
                .cloned()
                .collect()
        } else {
            members
                .iter()
                .filter(|m| is_falsy_type(m, language))
                .cloned()
                .collect()
        };
        return match filtered.len() {
            0 => None,
            1 => filtered.into_iter().next(),
            _ => Some(TypeShape::Union(filtered)),
        };
    }
    // Non-union single type: if the type itself is falsy and we assume true,
    // narrowing removes it (returns None). Otherwise keep as is for conservative.
    if is_falsy_type(shape, language) {
        if assume_true {
            None
        } else {
            Some(shape.clone())
        }
    } else if assume_true {
        Some(shape.clone())
    } else {
        None
    }
}

/// Pure union subtraction for negated narrowing (`not isinstance(x, T)`,
/// `x is not None`, `x !== null`, `typeof x !== "string"`).
///
/// Accepts `Union(members)` and `Generic{Union | Optional, args}`
/// (`Optional` implies a `None` member). Members matching any entry of
/// `excluded` (compared by rendered type string, quote-insensitive) are
/// removed. Returns `None` when the shape is not union-like or nothing
/// remains, so callers emit no binding rather than a guess.
pub fn subtract_union_members(shape: &TypeShape, excluded: &[String]) -> Option<TypeShape> {
    let mut members: Vec<TypeShape> = match shape {
        TypeShape::Union(members) => members.clone(),
        TypeShape::Generic { base, args } if base == "Union" => args.clone(),
        TypeShape::Generic { base, args } if base == "Optional" => {
            let mut with_none = args.clone();
            if !with_none
                .iter()
                .any(|m| matches!(m, TypeShape::Named(n) if n == "None" || n == "NoneType"))
            {
                with_none.push(TypeShape::Named("None".to_string()));
            }
            with_none
        }
        _ => return None,
    };
    let normalize = |s: &str| {
        s.trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_string()
    };
    members.retain(|member| {
        let rendered = normalize(&type_shape_to_string(member));
        !excluded.iter().any(|e| normalize(e) == rendered)
    });
    match members.len() {
        0 => None,
        1 => Some(members.into_iter().next().expect("one member")),
        _ => Some(TypeShape::Union(members)),
    }
}

/// Look up the declared type of a variable for narrowing.
///
/// Parameter annotations of the enclosing function win (they describe the
/// value at every program point); otherwise fall back to an already-known
/// variable binding. Returns the parsed shape, or `None` when the variable
/// has no usable declared type.
pub fn declared_shape(
    ctx: &ScopedTypeContext,
    params: &[(String, Option<String>)],
    language: Language,
    name: &str,
) -> Option<TypeShape> {
    if let Some(ty) = params
        .iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, ty)| ty.as_deref())
    {
        if let Some(shape) = parse_type_shape(ty, language) {
            return Some(shape);
        }
    }
    ctx.get_variable_type(name).and_then(|binding| {
        binding
            .shape
            .clone()
            .or_else(|| parse_type_shape(&binding.type_name, language))
    })
}

/// Check if a type shape represents a falsy value for a given language.
///
/// Deterministic singleton check: only language-specific falsy singletons
/// (None/null/nil) are considered falsy. Broad types such as `bool`/`int`
/// /`boolean`/`number` are intentionally excluded because they contain both
/// truthy and falsy values and filtering them is heuristic. The truthiness
/// narrowing is therefore limited to `TypeIndex`-verified unions and explicit
/// singleton members, matching the AST-pattern deterministic contract in
/// `docs/plan/symbol-resolution-deterministic.md`.
pub fn is_falsy_type(shape: &TypeShape, language: Language) -> bool {
    let check_single = |s: &str| -> bool {
        match language {
            Language::Python => matches!(s, "None" | "NoneType"),
            Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
                matches!(s, "null" | "undefined")
            }
            Language::Rust => matches!(s, "Option::None" | "None"),
            Language::Go => matches!(s, "nil"),
            _ => false,
        }
    };
    match shape {
        TypeShape::Named(s) => check_single(s.as_str()),
        TypeShape::Union(members) | TypeShape::Intersection(members) => {
            // Conservative: only if any member is falsy
            members.iter().any(|m| is_falsy_type(m, language))
        }
        TypeShape::Array(_) => false,
        TypeShape::Generic { base, .. } => check_single(base.as_str()),
        TypeShape::Reference { inner, .. } => is_falsy_type(inner, language),
        TypeShape::Param(_) => false,
        TypeShape::Wildcard { .. } => false,
    }
}

/// A single scope frame containing variable bindings.
#[derive(Debug, Clone, Default)]
pub struct ScopeFrame {
    /// Variable bindings in this scope
    pub bindings: HashMap<String, TypeBinding>,
    /// Narrowed type bindings (for control flow analysis)
    pub narrowed: HashMap<String, Vec<TypeBinding>>,
}

/// Scoped type context for per-file type inference.
///
/// Built during symbol table construction and queried by the relation resolver
/// when disambiguating method calls on dynamically-typed receivers.
///
/// Supports nested scopes with proper shadowing semantics:
/// - Variables are looked up from innermost to outermost scope
/// - New bindings in inner scopes shadow outer bindings
/// - Popping a scope restores visibility of outer bindings
#[derive(Debug, Clone)]
pub struct ScopedTypeContext {
    /// Scope frames stack. `frames[0]` is the outermost scope.
    frames: Vec<ScopeFrame>,
    /// Return types (not affected by scope nesting)
    return_types: HashMap<EntityId, TypeBinding>,
    /// Parameter types (not affected by scope nesting)
    parameter_types: HashMap<EntityId, Vec<TypeBinding>>,
    /// Language this context was built for
    language: Language,
    /// Type parameter bindings (e.g., "T" -> "String") per scope
    type_param_bindings: Vec<HashMap<String, String>>,
    /// Type parameter ownership: outer_type -> { param -> concrete }.
    /// Tracks which generic class a type parameter belongs to, preventing
    /// cross-contamination between e.g. Vec<T> and HashMap<T,V>.
    generic_param_ownership: HashMap<String, HashMap<String, String>>,
}

impl Default for ScopedTypeContext {
    fn default() -> Self {
        Self {
            frames: vec![ScopeFrame::default()],
            return_types: HashMap::new(),
            parameter_types: HashMap::new(),
            language: Language::Unknown,
            type_param_bindings: vec![HashMap::new()],
            generic_param_ownership: HashMap::new(),
        }
    }
}

impl ScopedTypeContext {
    /// Create a new empty type inference context for a language.
    pub fn new(language: Language) -> Self {
        Self {
            language,
            ..Default::default()
        }
    }

    /// Push a new scope frame onto the stack.
    pub fn push_scope(&mut self) {
        self.frames.push(ScopeFrame::default());
        self.type_param_bindings.push(HashMap::new());
    }

    /// Pop the innermost scope frame from the stack.
    ///
    /// # Panics
    ///
    /// Panics if there is only one scope frame (the root scope cannot be popped).
    pub fn pop_scope(&mut self) {
        assert!(self.frames.len() > 1, "Cannot pop the root scope");
        self.frames.pop();
        self.type_param_bindings.pop();
    }

    /// Get the current scope depth (number of frames).
    pub fn scope_depth(&self) -> usize {
        self.frames.len()
    }

    /// Bind a type parameter in the current scope.
    pub fn bind_type_param(&mut self, param: String, concrete: String) {
        if let Some(frame) = self.type_param_bindings.last_mut() {
            frame.insert(param, concrete);
        }
    }

    /// Look up a type parameter binding (searches scope stack).
    pub fn get_type_param(&self, param: &str) -> Option<&str> {
        for frame in self.type_param_bindings.iter().rev() {
            if let Some(v) = frame.get(param) {
                return Some(v.as_str());
            }
        }
        None
    }

    /// Bind a type parameter with ownership tracking.
    ///
    /// Records that `param` belongs to `outer_type` and maps to `concrete`.
    /// This prevents cross-contamination between different generic classes.
    pub fn bind_type_param_owned(&mut self, outer_type: &str, param: String, concrete: String) {
        self.generic_param_ownership
            .entry(outer_type.to_string())
            .or_default()
            .insert(param.clone(), concrete.clone());
        // Also bind in the scope stack for backward compatibility
        self.bind_type_param(param, concrete);
    }

    /// Look up a type parameter binding scoped to a specific outer type.
    ///
    /// Returns the concrete type if `param` is bound within `outer_type`.
    /// Falls back to the global `get_type_param` if no ownership record exists.
    pub fn get_type_param_for_owner(&self, outer_type: &str, param: &str) -> Option<&str> {
        if let Some(owners) = self.generic_param_ownership.get(outer_type) {
            if let Some(v) = owners.get(param) {
                return Some(v.as_str());
            }
        }
        self.get_type_param(param)
    }

    /// Resolve a potentially-generic type name by substituting bound parameters.
    ///
    /// Handles nested generics recursively, e.g. `Vec<HashMap<T, V>>` with
    /// `T=String, V=i32` → `Vec<HashMap<String, i32>>`.
    pub fn resolve_type(&self, type_name: &str) -> String {
        // Direct type parameter substitution
        if let Some(concrete) = self.get_type_param(type_name) {
            return concrete.to_string();
        }
        // Try parsing as a generic type for recursive resolution
        if let Some(gt) = super::generics::parse_generic_type(type_name) {
            let resolved = self.resolve_generic_type(&gt);
            return super::generics::format_generic_type(&resolved);
        }
        type_name.to_string()
    }

    /// Recursively resolve a parsed generic type using scope bindings.
    fn resolve_generic_type(
        &self,
        gt: &super::generics::GenericType,
    ) -> super::generics::GenericType {
        let resolved_args = gt
            .args
            .iter()
            .map(|arg| match arg {
                super::generics::GenericTypeArg::Param(p) => {
                    if let Some(concrete) = self.get_type_param(p) {
                        // The resolved concrete might itself be generic, resolve recursively
                        if let Some(nested_gt) = super::generics::parse_generic_type(concrete) {
                            super::generics::GenericTypeArg::Nested(
                                self.resolve_generic_type(&nested_gt),
                            )
                        } else {
                            super::generics::GenericTypeArg::Concrete(concrete.to_string())
                        }
                    } else {
                        super::generics::GenericTypeArg::Param(p.clone())
                    }
                }
                super::generics::GenericTypeArg::Nested(nested) => {
                    super::generics::GenericTypeArg::Nested(self.resolve_generic_type(nested))
                }
                other => other.clone(),
            })
            .collect();
        super::generics::GenericType {
            base: gt.base.clone(),
            args: resolved_args,
        }
    }

    /// Look up variable type, but only within the current scope (not parent scopes).
    pub fn get_variable_type_current_scope(&self, name: &str) -> Option<&TypeBinding> {
        self.frames
            .last()
            .and_then(|frame| frame.bindings.get(name))
    }

    /// Get the number of bindings in the current scope (for diagnostics).
    pub fn current_scope_binding_count(&self) -> usize {
        self.frames.last().map(|f| f.bindings.len()).unwrap_or(0)
    }

    /// Narrow a union type by excluding a variant.
    pub fn narrow_union(&mut self, name: &str, exclude: &TypeShape) {
        // Find the binding for `name` and if its shape is Union, remove the excluded variant
        for frame in self.frames.iter_mut().rev() {
            if let Some(binding) = frame.bindings.get_mut(name) {
                if let Some(TypeShape::Union(members)) = binding.shape.clone() {
                    let filtered: Vec<TypeShape> =
                        members.into_iter().filter(|m| m != exclude).collect();
                    let new_shape = match filtered.len() {
                        0 => None,
                        1 => Some(filtered.into_iter().next().expect("one element")),
                        _ => Some(TypeShape::Union(filtered)),
                    };
                    if let Some(ns) = new_shape.clone() {
                        binding.type_name = type_shape_to_string(&ns);
                        binding.shape = Some(ns);
                    } else {
                        // No members left; keep original
                    }
                    return;
                }
                // Fallback: string-based narrowing for simple cases
                if let TypeShape::Named(exclude_name) = exclude {
                    let type_name_clone = binding.type_name.clone();
                    let parts: Vec<&str> = type_name_clone.split(" | ").collect();
                    if parts.len() > 1 {
                        let filtered: Vec<String> = parts
                            .into_iter()
                            .filter(|p| p.trim() != exclude_name)
                            .map(|s| s.to_string())
                            .collect();
                        if filtered.len() == 1 {
                            binding.type_name = filtered[0].clone();
                            binding.shape = Some(TypeShape::Named(filtered[0].clone()));
                        } else if filtered.len() > 1 {
                            let joined = filtered.join(" | ");
                            binding.type_name = joined;
                            // Re-parse shape if possible
                            binding.shape = Some(TypeShape::Union(
                                filtered.into_iter().map(TypeShape::Named).collect(),
                            ));
                        }
                    }
                }
                return;
            }
        }
    }

    /// Record a return type binding for a function entity.
    pub fn add_return_type(&mut self, entity_id: EntityId, binding: TypeBinding) {
        self.return_types.insert(entity_id, binding);
    }

    /// Record parameter type bindings for a function entity.
    pub fn add_parameter_types(&mut self, entity_id: EntityId, bindings: Vec<TypeBinding>) {
        self.parameter_types.insert(entity_id, bindings);
    }

    /// Record a variable type binding in the current (innermost) scope.
    pub fn add_variable_type(&mut self, name: String, binding: TypeBinding) {
        self.frames
            .last_mut()
            .expect("Scope stack is never empty")
            .bindings
            .insert(name, binding);
    }

    /// Record a narrowed type binding for a variable in the current scope.
    ///
    /// Narrowed bindings are produced by control-flow analysis (e.g.,
    /// `isinstance(x, str)` narrows `x` to `str` inside the true branch).
    /// Multiple narrowings for the same variable accumulate in a list.
    pub fn add_narrowed_type(&mut self, name: String, binding: TypeBinding) {
        self.frames
            .last_mut()
            .expect("Scope stack is never empty")
            .narrowed
            .entry(name)
            .or_default()
            .push(binding);
    }

    /// Look up the inferred return type for a function entity.
    pub fn get_return_type(&self, entity_id: EntityId) -> Option<&TypeBinding> {
        self.return_types.get(&entity_id)
    }

    /// Look up the inferred parameter types for a function entity.
    pub fn get_parameter_types(&self, entity_id: EntityId) -> Option<&[TypeBinding]> {
        self.parameter_types.get(&entity_id).map(|v| v.as_slice())
    }

    /// Look up the inferred type for a variable name.
    ///
    /// Searches from the innermost scope outward. For each frame:
    /// 1. If narrowed bindings exist for the name, return the highest-priority
    ///    narrowed binding (by `origin_priority`).
    /// 2. Otherwise return the original binding.
    pub fn get_variable_type(&self, name: &str) -> Option<&TypeBinding> {
        for frame in self.frames.iter().rev() {
            let has_binding = frame.bindings.contains_key(name);
            let has_narrowed = frame.narrowed.contains_key(name);

            if has_narrowed {
                if let Some(narrowed_list) = frame.narrowed.get(name) {
                    if let Some(narrowed) = narrowed_list
                        .iter()
                        .max_by_key(|b| origin_priority(b.origin))
                    {
                        return Some(narrowed);
                    }
                }
            }

            if has_binding {
                return frame.bindings.get(name);
            }
        }
        None
    }

    /// Get the language this context was built for.
    pub fn language(&self) -> Language {
        self.language
    }

    /// Iterate over scope frames (outermost to innermost).
    pub fn frames_iter(&self) -> impl Iterator<Item = &ScopeFrame> {
        self.frames.iter()
    }

    /// Check if this context has any inferred types.
    pub fn is_empty(&self) -> bool {
        self.return_types.is_empty()
            && self.parameter_types.is_empty()
            && self
                .frames
                .iter()
                .all(|f| f.bindings.is_empty() && f.narrowed.is_empty())
    }

    /// Iterate over return type bindings.
    pub fn return_types_iter(&self) -> impl Iterator<Item = (&EntityId, &TypeBinding)> {
        self.return_types.iter()
    }

    /// Iterate over parameter type bindings.
    pub fn parameter_types_iter(&self) -> impl Iterator<Item = (&EntityId, &Vec<TypeBinding>)> {
        self.parameter_types.iter()
    }

    /// Iterate over variable type bindings in the outermost scope.
    pub fn variable_types_iter(&self) -> impl Iterator<Item = (&String, &TypeBinding)> {
        self.frames
            .first()
            .into_iter()
            .flat_map(|f| f.bindings.iter())
    }

    /// Get all return type bindings as a vector.
    pub fn all_return_types(&self) -> Vec<(EntityId, TypeBinding)> {
        self.return_types
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }

    /// Merge another context into this one (for incremental updates).
    ///
    /// Only merges the top-level frame from `other` into the top-level frame
    /// of `self`. Respects origin priority: a binding is only overridden
    /// if the new binding has higher priority.
    ///
    /// `return_types` and `parameter_types` are merged directly (later values
    /// override earlier ones, as they are file-level and not scope-sensitive).
    pub fn merge_from(&mut self, other: &ScopedTypeContext) {
        if let Some(other_frame) = other.frames.last() {
            let self_frame = self.frames.last_mut().expect("Scope stack is never empty");
            for (name, binding) in &other_frame.bindings {
                self_frame
                    .bindings
                    .entry(name.clone())
                    .and_modify(|existing| {
                        if origin_priority(binding.origin) > origin_priority(existing.origin) {
                            *existing = binding.clone();
                        }
                    })
                    .or_insert_with(|| binding.clone());
            }
        }
        // return_types and parameter_types are merged directly
        for (k, v) in &other.return_types {
            self.return_types.insert(*k, v.clone());
        }
        for (k, v) in &other.parameter_types {
            self.parameter_types.insert(*k, v.clone());
        }
    }

    /// Add pattern match binding
    /// Handles: `let (a, b) = tuple()` or `let {x, y} = point`
    pub fn add_pattern_match_binding(&mut self, pattern: &Pattern, source_type: &TypeShape) {
        match pattern {
            Pattern::Tuple(elements) => {
                if let TypeShape::Generic { args, .. } = source_type {
                    for (i, element) in elements.iter().enumerate() {
                        if let Some(arg_type) = args.get(i) {
                            self.add_variable_type(
                                element.clone(),
                                TypeBinding {
                                    type_name: type_shape_to_string(arg_type),
                                    type_entity_id: None,
                                    span: cce_types::Span::default(),
                                    origin: Some(InferenceOrigin::PatternMatching),
                                    shape: Some(arg_type.clone()),
                                },
                            );
                        }
                    }
                }
            }
            Pattern::Struct(fields) => {
                for field in fields {
                    self.add_variable_type(
                        field.clone(),
                        TypeBinding {
                            type_name: "unknown".to_string(),
                            type_entity_id: None,
                            span: cce_types::Span::default(),
                            origin: Some(InferenceOrigin::PatternMatching),
                            shape: None,
                        },
                    );
                }
            }
            Pattern::Identifier(name) => {
                self.add_variable_type(
                    name.clone(),
                    TypeBinding {
                        type_name: type_shape_to_string(source_type),
                        type_entity_id: None,
                        span: cce_types::Span::default(),
                        origin: Some(InferenceOrigin::PatternMatching),
                        shape: Some(source_type.clone()),
                    },
                );
            }
            Pattern::Wildcard => {}
        }
    }

    /// Add destructuring assignment binding
    /// Handles: `a, b = tuple()` or `a = list[0]`
    ///
    /// Positional mapping applies to any generic container with enough type
    /// arguments, not just tuple-named shapes. Positions without a matching
    /// argument bind to `unknown` so unrelated parts never inherit a guess.
    pub fn add_destructuring_binding(
        &mut self,
        target: &str,
        source_type: &TypeShape,
        index: Option<usize>,
    ) {
        let resolved_type = match source_type {
            TypeShape::Generic { args, .. } => index
                .and_then(|i| args.get(i).cloned())
                .unwrap_or_else(|| TypeShape::Named("unknown".to_string())),
            TypeShape::Array(element_type) => (**element_type).clone(),
            _ => TypeShape::Named("unknown".to_string()),
        };

        self.add_variable_type(
            target.to_string(),
            TypeBinding {
                type_name: type_shape_to_string(&resolved_type),
                type_entity_id: None,
                span: cce_types::Span::default(),
                origin: Some(InferenceOrigin::DestructuringAssignment),
                shape: Some(resolved_type),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== VariableTypeBinding tests ====================

    #[test]
    fn test_variable_type_binding_new() {
        let primary = TypeBinding {
            type_name: "String".to_string(),
            ..Default::default()
        };
        let binding = VariableTypeBinding::new(primary.clone());
        assert_eq!(binding.primary.type_name, "String");
        assert!(binding.alternatives.is_empty());
        assert_eq!(binding.confidence, TypeConfidence::Medium);
    }

    #[test]
    fn test_variable_type_binding_add_alternative() {
        let primary = TypeBinding {
            type_name: "String".to_string(),
            ..Default::default()
        };
        let mut binding = VariableTypeBinding::new(primary);
        let alt = TypeBinding {
            type_name: "int".to_string(),
            ..Default::default()
        };
        binding.add_alternative(alt);
        assert_eq!(binding.alternatives.len(), 1);
        assert_eq!(binding.alternatives[0].type_name, "int");
    }

    #[test]
    fn test_variable_type_binding_all_types() {
        let primary = TypeBinding {
            type_name: "String".to_string(),
            ..Default::default()
        };
        let mut binding = VariableTypeBinding::new(primary);
        let alt = TypeBinding {
            type_name: "int".to_string(),
            ..Default::default()
        };
        binding.add_alternative(alt);
        let all = binding.all_types();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].type_name, "String");
        assert_eq!(all[1].type_name, "int");
    }

    #[test]
    fn test_variable_type_binding_merge() {
        let primary1 = TypeBinding {
            type_name: "String".to_string(),
            origin: Some(InferenceOrigin::TypeAnnotation),
            ..Default::default()
        };
        let binding1 = VariableTypeBinding::new(primary1);
        let primary2 = TypeBinding {
            type_name: "int".to_string(),
            origin: Some(InferenceOrigin::ConstructorCall),
            ..Default::default()
        };
        let binding2 = VariableTypeBinding::new(primary2);
        let merged = binding1.merge(binding2);
        assert_eq!(merged.primary.type_name, "String");
        assert_eq!(merged.alternatives.len(), 1);
        assert_eq!(merged.alternatives[0].type_name, "int");
    }

    #[test]
    fn test_variable_type_binding_most_specific() {
        let primary = TypeBinding {
            type_name: "String".to_string(),
            origin: Some(InferenceOrigin::LiteralType),
            ..Default::default()
        };
        let mut binding = VariableTypeBinding::new(primary);
        let alt = TypeBinding {
            type_name: "int".to_string(),
            origin: Some(InferenceOrigin::TypeAnnotation),
            ..Default::default()
        };
        binding.add_alternative(alt);
        let most = binding.most_specific();
        assert_eq!(most.type_name, "int");
        assert_eq!(most.origin, Some(InferenceOrigin::TypeAnnotation));
    }

    // ==================== origin_priority tests ====================

    #[test]
    fn test_origin_priority_all_variants() {
        assert_eq!(origin_priority(Some(InferenceOrigin::TypeAnnotation)), 8);
        assert_eq!(
            origin_priority(Some(InferenceOrigin::ControlFlowNarrowing)),
            7
        );
        assert_eq!(origin_priority(Some(InferenceOrigin::FunctionReturn)), 6);
        assert_eq!(origin_priority(Some(InferenceOrigin::PatternMatching)), 6);
        assert_eq!(
            origin_priority(Some(InferenceOrigin::OverloadResolution)),
            5
        );
        assert_eq!(
            origin_priority(Some(InferenceOrigin::DestructuringAssignment)),
            5
        );
        assert_eq!(
            origin_priority(Some(InferenceOrigin::CrossFilePropagation)),
            4
        );
        assert_eq!(origin_priority(Some(InferenceOrigin::GenericInference)), 3);
        assert_eq!(origin_priority(Some(InferenceOrigin::ConstructorCall)), 2);
        assert_eq!(origin_priority(Some(InferenceOrigin::LiteralType)), 1);
        assert_eq!(origin_priority(None), 0);
    }

    #[test]
    fn test_origin_priority_of() {
        assert_eq!(origin_priority_of(InferenceOrigin::TypeAnnotation), 8);
        assert_eq!(origin_priority_of(InferenceOrigin::LiteralType), 1);
    }

    // ==================== parse_type_shape tests ====================

    #[test]
    fn test_parse_type_shape_empty() {
        assert_eq!(parse_type_shape("", Language::Python), None);
        assert_eq!(parse_type_shape("   ", Language::Python), None);
    }

    #[test]
    fn test_parse_type_shape_simple_named() {
        let shape = parse_type_shape("String", Language::Python).unwrap();
        assert_eq!(shape, TypeShape::Named("String".to_string()));
    }

    #[test]
    fn test_parse_type_shape_union() {
        let shape = parse_type_shape("string | number", Language::TypeScript).unwrap();
        match shape {
            TypeShape::Union(members) => {
                assert_eq!(members.len(), 2);
                assert_eq!(members[0], TypeShape::Named("string".to_string()));
                assert_eq!(members[1], TypeShape::Named("number".to_string()));
            }
            _ => panic!("Expected Union"),
        }
    }

    #[test]
    fn test_parse_type_shape_intersection() {
        let shape = parse_type_shape("Readonly & Partial", Language::TypeScript).unwrap();
        match shape {
            TypeShape::Intersection(members) => {
                assert_eq!(members.len(), 2);
                assert_eq!(members[0], TypeShape::Named("Readonly".to_string()));
                assert_eq!(members[1], TypeShape::Named("Partial".to_string()));
            }
            _ => panic!("Expected Intersection"),
        }
    }

    #[test]
    fn test_parse_type_shape_array() {
        let shape = parse_type_shape("string[]", Language::TypeScript).unwrap();
        assert_eq!(
            shape,
            TypeShape::Array(Box::new(TypeShape::Named("string".to_string())))
        );
    }

    #[test]
    fn test_parse_type_shape_generic() {
        let shape = parse_type_shape("List<T>", Language::Python).unwrap();
        match shape {
            TypeShape::Generic { base, args } => {
                assert_eq!(base, "List");
                assert_eq!(args.len(), 1);
                assert_eq!(args[0], TypeShape::Param("T".to_string()));
            }
            _ => panic!("Expected Generic"),
        }
    }

    #[test]
    fn test_parse_type_shape_nested_generic() {
        let shape = parse_type_shape("Map<String, List<Integer>>", Language::Kotlin).unwrap();
        match shape {
            TypeShape::Generic { base, args } => {
                assert_eq!(base, "Map");
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], TypeShape::Named("String".to_string()));
                match &args[1] {
                    TypeShape::Generic { base, args } => {
                        assert_eq!(base, "List");
                        assert_eq!(args.len(), 1);
                        assert_eq!(args[0], TypeShape::Named("Integer".to_string()));
                    }
                    _ => panic!("Expected nested Generic"),
                }
            }
            _ => panic!("Expected Generic"),
        }
    }

    #[test]
    fn test_parse_type_shape_reference() {
        let shape = parse_type_shape("&str", Language::Rust).unwrap();
        assert_eq!(
            shape,
            TypeShape::Reference {
                inner: Box::new(TypeShape::Named("str".to_string())),
                mutable: false,
            }
        );
    }

    #[test]
    fn test_parse_type_shape_mut_reference() {
        let shape = parse_type_shape("&mut Vec<T>", Language::Rust).unwrap();
        assert_eq!(
            shape,
            TypeShape::Reference {
                inner: Box::new(TypeShape::Generic {
                    base: "Vec".to_string(),
                    args: vec![TypeShape::Param("T".to_string())]
                }),
                mutable: true,
            }
        );
    }

    #[test]
    fn test_parse_type_shape_lifetime_reference() {
        let shape = parse_type_shape("&'a str", Language::Rust).unwrap();
        assert_eq!(
            shape,
            TypeShape::Reference {
                inner: Box::new(TypeShape::Named("str".to_string())),
                mutable: false,
            }
        );
    }

    #[test]
    fn test_parse_type_shape_wildcard() {
        let shape = parse_type_shape("?", Language::Java).unwrap();
        assert_eq!(shape, TypeShape::Wildcard { bound: None });
    }

    #[test]
    fn test_parse_type_shape_wildcard_extends() {
        let shape = parse_type_shape("?extends Number", Language::Java).unwrap();
        assert_eq!(
            shape,
            TypeShape::Wildcard {
                bound: Some("Number".to_string())
            }
        );
    }

    #[test]
    fn test_parse_type_shape_wildcard_super() {
        let shape = parse_type_shape("?super String", Language::Java).unwrap();
        assert_eq!(
            shape,
            TypeShape::Wildcard {
                bound: Some("String".to_string())
            }
        );
    }

    #[test]
    fn test_parse_type_shape_type_param() {
        let shape = parse_type_shape("T", Language::Rust).unwrap();
        assert_eq!(shape, TypeShape::Param("T".to_string()));
        let shape = parse_type_shape("Value", Language::Rust).unwrap();
        assert_eq!(shape, TypeShape::Param("Value".to_string()));
    }

    #[test]
    fn test_parse_type_shape_union_single_member() {
        let shape = parse_type_shape("string |", Language::TypeScript);
        assert!(shape.is_some());
        assert_eq!(shape.unwrap(), TypeShape::Named("string |".to_string()));
    }

    #[test]
    fn test_parse_type_shape_intersection_single_member() {
        let shape = parse_type_shape("A &", Language::TypeScript);
        assert!(shape.is_some());
        assert_eq!(shape.unwrap(), TypeShape::Named("A &".to_string()));
    }

    // ==================== type_shape_to_string tests ====================

    #[test]
    fn test_type_shape_to_string_named() {
        let shape = TypeShape::Named("String".to_string());
        assert_eq!(type_shape_to_string(&shape), "String");
    }

    #[test]
    fn test_type_shape_to_string_union() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("string".to_string()),
            TypeShape::Named("number".to_string()),
        ]);
        assert_eq!(type_shape_to_string(&shape), "string | number");
    }

    #[test]
    fn test_type_shape_to_string_intersection() {
        let shape = TypeShape::Intersection(vec![
            TypeShape::Named("A".to_string()),
            TypeShape::Named("B".to_string()),
        ]);
        assert_eq!(type_shape_to_string(&shape), "A & B");
    }

    #[test]
    fn test_type_shape_to_string_array() {
        let shape = TypeShape::Array(Box::new(TypeShape::Named("string".to_string())));
        assert_eq!(type_shape_to_string(&shape), "string[]");
    }

    #[test]
    fn test_type_shape_to_string_generic() {
        let shape = TypeShape::Generic {
            base: "List".to_string(),
            args: vec![TypeShape::Named("String".to_string())],
        };
        assert_eq!(type_shape_to_string(&shape), "List<String>");
    }

    #[test]
    fn test_type_shape_to_string_reference() {
        let shape = TypeShape::Reference {
            inner: Box::new(TypeShape::Named("str".to_string())),
            mutable: false,
        };
        assert_eq!(type_shape_to_string(&shape), "&str");
    }

    #[test]
    fn test_type_shape_to_string_mut_reference() {
        let shape = TypeShape::Reference {
            inner: Box::new(TypeShape::Named("str".to_string())),
            mutable: true,
        };
        assert_eq!(type_shape_to_string(&shape), "&mut str");
    }

    #[test]
    fn test_type_shape_to_string_param() {
        let shape = TypeShape::Param("T".to_string());
        assert_eq!(type_shape_to_string(&shape), "T");
    }

    #[test]
    fn test_type_shape_to_string_wildcard_none() {
        let shape = TypeShape::Wildcard { bound: None };
        assert_eq!(type_shape_to_string(&shape), "?");
    }

    #[test]
    fn test_type_shape_to_string_wildcard_extends() {
        let shape = TypeShape::Wildcard {
            bound: Some("Number".to_string()),
        };
        assert_eq!(type_shape_to_string(&shape), "? extends Number");
    }

    // ==================== roundtrip tests ====================

    #[test]
    fn test_parse_type_shape_roundtrip_named() {
        let original = "String";
        let shape = parse_type_shape(original, Language::Python).unwrap();
        let back = type_shape_to_string(&shape);
        assert_eq!(back, original);
    }

    #[test]
    fn test_parse_type_shape_roundtrip_union() {
        let original = "string | number";
        let shape = parse_type_shape(original, Language::TypeScript).unwrap();
        let back = type_shape_to_string(&shape);
        assert_eq!(back, original);
    }

    #[test]
    fn test_parse_type_shape_roundtrip_generic() {
        let original = "List<String>";
        let shape = parse_type_shape(original, Language::Python).unwrap();
        let back = type_shape_to_string(&shape);
        assert_eq!(back, original);
    }

    // ==================== shape_members tests ====================

    #[test]
    fn test_shape_members_named() {
        let shape = TypeShape::Named("String".to_string());
        assert_eq!(shape_members(&shape), vec!["String".to_string()]);
    }

    #[test]
    fn test_shape_members_union() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("A".to_string()),
            TypeShape::Named("B".to_string()),
        ]);
        assert_eq!(
            shape_members(&shape),
            vec!["A".to_string(), "B".to_string()]
        );
    }

    #[test]
    fn test_shape_members_generic() {
        let shape = TypeShape::Generic {
            base: "List".to_string(),
            args: vec![TypeShape::Named("String".to_string())],
        };
        assert_eq!(shape_members(&shape), vec!["List".to_string()]);
    }

    #[test]
    fn test_shape_members_reference() {
        let shape = TypeShape::Reference {
            inner: Box::new(TypeShape::Named("str".to_string())),
            mutable: false,
        };
        assert_eq!(shape_members(&shape), vec!["str".to_string()]);
    }

    #[test]
    fn test_shape_members_param() {
        let shape = TypeShape::Param("T".to_string());
        assert_eq!(shape_members(&shape), vec!["T".to_string()]);
    }

    #[test]
    fn test_shape_members_wildcard_none() {
        let shape = TypeShape::Wildcard { bound: None };
        assert_eq!(shape_members(&shape), vec!["?".to_string()]);
    }

    #[test]
    fn test_shape_members_wildcard_bound() {
        let shape = TypeShape::Wildcard {
            bound: Some("Number".to_string()),
        };
        assert_eq!(shape_members(&shape), vec!["Number".to_string()]);
    }

    #[test]
    fn test_shape_members_array() {
        let shape = TypeShape::Array(Box::new(TypeShape::Named("String".to_string())));
        assert_eq!(shape_members(&shape), vec!["String".to_string()]);
    }

    // ==================== shape_is_subtype tests ====================

    #[test]
    fn test_shape_is_subtype_same() {
        let shape = TypeShape::Named("String".to_string());
        assert!(shape_is_subtype(&shape, &shape));
    }

    #[test]
    fn test_shape_is_subtype_union() {
        let sub = TypeShape::Named("A".to_string());
        let super_ = TypeShape::Union(vec![
            TypeShape::Named("A".to_string()),
            TypeShape::Named("B".to_string()),
        ]);
        assert!(shape_is_subtype(&sub, &super_));
    }

    #[test]
    fn test_shape_is_subtype_not_in_union() {
        let sub = TypeShape::Named("C".to_string());
        let super_ = TypeShape::Union(vec![
            TypeShape::Named("A".to_string()),
            TypeShape::Named("B".to_string()),
        ]);
        assert!(!shape_is_subtype(&sub, &super_));
    }

    #[test]
    fn test_shape_is_subtype_param_same() {
        let sub = TypeShape::Param("T".to_string());
        let super_ = TypeShape::Param("T".to_string());
        assert!(shape_is_subtype(&sub, &super_));
    }

    #[test]
    fn test_shape_is_subtype_param_different() {
        let sub = TypeShape::Param("T".to_string());
        let super_ = TypeShape::Param("U".to_string());
        assert!(!shape_is_subtype(&sub, &super_));
    }

    #[test]
    fn test_shape_is_subtype_named_different() {
        let sub = TypeShape::Named("A".to_string());
        let super_ = TypeShape::Named("B".to_string());
        assert!(!shape_is_subtype(&sub, &super_));
    }

    // ==================== strip_references tests ====================

    #[test]
    fn test_strip_references_not_ref() {
        let (name, is_mut, is_ref) = strip_references("String");
        assert_eq!(name, "String");
        assert!(!is_mut);
        assert!(!is_ref);
    }

    #[test]
    fn test_strip_references_immutable_ref() {
        let (name, is_mut, is_ref) = strip_references("&str");
        assert_eq!(name, "str");
        assert!(!is_mut);
        assert!(is_ref);
    }

    #[test]
    fn test_strip_references_mutable_ref() {
        let (name, is_mut, is_ref) = strip_references("&mut String");
        assert_eq!(name, "String");
        assert!(is_mut);
        assert!(is_ref);
    }

    #[test]
    fn test_strip_references_with_lifetime() {
        let (name, is_mut, is_ref) = strip_references("&'a str");
        assert_eq!(name, "str");
        assert!(!is_mut);
        assert!(is_ref);
    }

    #[test]
    fn test_strip_references_empty_after_strip() {
        let (name, is_mut, is_ref) = strip_references("&");
        assert_eq!(name, "&");
        assert!(!is_mut);
        assert!(is_ref);
    }

    // ==================== is_reference / is_mut_reference tests ====================

    #[test]
    fn test_is_reference() {
        assert!(is_reference("&str"));
        assert!(is_reference("&mut String"));
        assert!(is_reference("  &i32"));
        assert!(!is_reference("String"));
        assert!(!is_reference("int"));
    }

    #[test]
    fn test_is_mut_reference() {
        assert!(is_mut_reference("&mut String"));
        assert!(is_mut_reference("  &mut Vec<T>"));
        assert!(!is_mut_reference("&str"));
        assert!(!is_mut_reference("String"));
    }

    // ==================== ScopedTypeContext tests ====================

    #[test]
    fn test_scoped_type_context_new() {
        let ctx = ScopedTypeContext::new(Language::Python);
        assert_eq!(ctx.language(), Language::Python);
        assert!(ctx.is_empty());
        assert_eq!(ctx.scope_depth(), 1);
    }

    #[test]
    fn test_scoped_type_context_default() {
        let ctx = ScopedTypeContext::default();
        assert_eq!(ctx.language(), Language::Unknown);
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_scoped_type_context_push_pop_scope() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        assert_eq!(ctx.scope_depth(), 1);
        ctx.push_scope();
        assert_eq!(ctx.scope_depth(), 2);
        ctx.pop_scope();
        assert_eq!(ctx.scope_depth(), 1);
    }

    #[test]
    #[should_panic(expected = "Cannot pop the root scope")]
    fn test_scoped_type_context_pop_root_scope_panics() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.pop_scope();
    }

    #[test]
    fn test_scoped_type_context_shadowing() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "int");
        ctx.push_scope();
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "String");
        ctx.pop_scope();
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "int");
    }

    #[test]
    fn test_scoped_type_context_get_variable_type_current_scope() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                ..Default::default()
            },
        );
        assert!(ctx.get_variable_type_current_scope("x").is_some());
        ctx.push_scope();
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            ctx.get_variable_type_current_scope("x").unwrap().type_name,
            "String"
        );
        ctx.pop_scope();
        assert_eq!(
            ctx.get_variable_type_current_scope("x").unwrap().type_name,
            "int"
        );
    }

    #[test]
    fn test_scoped_type_context_return_type() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let entity_id = cce_types::EntityId(1);
        ctx.add_return_type(
            entity_id,
            TypeBinding {
                type_name: "String".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(ctx.get_return_type(entity_id).unwrap().type_name, "String");
        assert!(ctx.get_return_type(cce_types::EntityId(999)).is_none());
    }

    #[test]
    fn test_scoped_type_context_parameter_types() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let entity_id = cce_types::EntityId(1);
        ctx.add_parameter_types(
            entity_id,
            vec![
                TypeBinding {
                    type_name: "int".to_string(),
                    ..Default::default()
                },
                TypeBinding {
                    type_name: "String".to_string(),
                    ..Default::default()
                },
            ],
        );
        let params = ctx.get_parameter_types(entity_id).unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].type_name, "int");
        assert_eq!(params[1].type_name, "String");
        assert!(ctx.get_parameter_types(cce_types::EntityId(999)).is_none());
    }

    #[test]
    fn test_scoped_type_context_bind_type_param() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param("T".to_string(), "String".to_string());
        assert_eq!(ctx.get_type_param("T"), Some("String"));
        assert_eq!(ctx.get_type_param("U"), None);
    }

    #[test]
    fn test_scoped_type_context_type_param_scope() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param("T".to_string(), "int".to_string());
        ctx.push_scope();
        ctx.bind_type_param("T".to_string(), "String".to_string());
        assert_eq!(ctx.get_type_param("T"), Some("String"));
        ctx.pop_scope();
        assert_eq!(ctx.get_type_param("T"), Some("int"));
    }

    #[test]
    fn test_scoped_type_context_bind_type_param_owned() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param_owned("Vec", "T".to_string(), "String".to_string());
        assert_eq!(ctx.get_type_param_for_owner("Vec", "T"), Some("String"));
        // Falls back to global get_type_param since no ownership record for HashMap
        assert_eq!(ctx.get_type_param_for_owner("HashMap", "T"), Some("String"));
    }

    #[test]
    fn test_scoped_type_context_resolve_type_simple() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param("T".to_string(), "String".to_string());
        assert_eq!(ctx.resolve_type("T"), "String");
        assert_eq!(ctx.resolve_type("int"), "int");
    }

    #[test]
    fn test_scoped_type_context_resolve_type_generic() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param("T".to_string(), "String".to_string());
        assert_eq!(ctx.resolve_type("Vec<T>"), "Vec<String>");
    }

    #[test]
    fn test_scoped_type_context_narrow_union() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let shape = TypeShape::Union(vec![
            TypeShape::Named("String".to_string()),
            TypeShape::Named("None".to_string()),
        ]);
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: type_shape_to_string(&shape),
                shape: Some(shape),
                ..Default::default()
            },
        );
        ctx.narrow_union("x", &TypeShape::Named("None".to_string()));
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "String");
    }

    #[test]
    fn test_scoped_type_context_merge_from() {
        let mut ctx1 = ScopedTypeContext::new(Language::Python);
        ctx1.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                origin: Some(InferenceOrigin::LiteralType),
                ..Default::default()
            },
        );
        let mut ctx2 = ScopedTypeContext::new(Language::Python);
        ctx2.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                origin: Some(InferenceOrigin::TypeAnnotation),
                ..Default::default()
            },
        );
        ctx1.merge_from(&ctx2);
        assert_eq!(ctx1.get_variable_type("x").unwrap().type_name, "String");
    }

    #[test]
    fn test_scoped_type_context_merge_from_lower_priority() {
        let mut ctx1 = ScopedTypeContext::new(Language::Python);
        ctx1.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                origin: Some(InferenceOrigin::TypeAnnotation),
                ..Default::default()
            },
        );
        let mut ctx2 = ScopedTypeContext::new(Language::Python);
        ctx2.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "String".to_string(),
                origin: Some(InferenceOrigin::LiteralType),
                ..Default::default()
            },
        );
        ctx1.merge_from(&ctx2);
        assert_eq!(ctx1.get_variable_type("x").unwrap().type_name, "int");
    }

    #[test]
    fn test_scoped_type_context_add_pattern_match_binding_identifier() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let pattern = Pattern::Identifier("x".to_string());
        let source_type = TypeShape::Named("String".to_string());
        ctx.add_pattern_match_binding(&pattern, &source_type);
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "String");
    }

    #[test]
    fn test_scoped_type_context_add_pattern_match_binding_tuple() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let pattern = Pattern::Tuple(vec!["a".to_string(), "b".to_string()]);
        let source_type = TypeShape::Generic {
            base: "Tuple".to_string(),
            args: vec![
                TypeShape::Named("int".to_string()),
                TypeShape::Named("String".to_string()),
            ],
        };
        ctx.add_pattern_match_binding(&pattern, &source_type);
        assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "int");
        assert_eq!(ctx.get_variable_type("b").unwrap().type_name, "String");
    }

    #[test]
    fn test_scoped_type_context_add_pattern_match_binding_struct() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let pattern = Pattern::Struct(vec!["x".to_string(), "y".to_string()]);
        let source_type = TypeShape::Named("Point".to_string());
        ctx.add_pattern_match_binding(&pattern, &source_type);
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "unknown");
        assert_eq!(ctx.get_variable_type("y").unwrap().type_name, "unknown");
    }

    #[test]
    fn test_scoped_type_context_add_pattern_match_binding_wildcard() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let pattern = Pattern::Wildcard;
        let source_type = TypeShape::Named("Point".to_string());
        ctx.add_pattern_match_binding(&pattern, &source_type);
    }

    #[test]
    fn test_scoped_type_context_add_destructuring_binding_tuple() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source_type = TypeShape::Generic {
            base: "tuple".to_string(),
            args: vec![
                TypeShape::Named("int".to_string()),
                TypeShape::Named("String".to_string()),
            ],
        };
        ctx.add_destructuring_binding("a", &source_type, Some(0));
        assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "int");
        ctx.add_destructuring_binding("b", &source_type, Some(1));
        assert_eq!(ctx.get_variable_type("b").unwrap().type_name, "String");
    }

    #[test]
    fn test_scoped_type_context_add_destructuring_binding_array() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source_type = TypeShape::Array(Box::new(TypeShape::Named("int".to_string())));
        ctx.add_destructuring_binding("elem", &source_type, None);
        assert_eq!(ctx.get_variable_type("elem").unwrap().type_name, "int");
    }

    #[test]
    fn test_scoped_type_context_is_empty() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        assert!(ctx.is_empty());
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: "int".to_string(),
                ..Default::default()
            },
        );
        assert!(!ctx.is_empty());
    }

    // ==================== is_falsy_type tests ====================

    #[test]
    fn test_is_falsy_type_python() {
        assert!(is_falsy_type(
            &TypeShape::Named("None".to_string()),
            Language::Python
        ));
        assert!(is_falsy_type(
            &TypeShape::Named("NoneType".to_string()),
            Language::Python
        ));
        // Deterministic: broad types like bool/int/float are not singleton
        // falsy and therefore not considered falsy.
        assert!(!is_falsy_type(
            &TypeShape::Named("bool".to_string()),
            Language::Python
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("int".to_string()),
            Language::Python
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("float".to_string()),
            Language::Python
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("String".to_string()),
            Language::Python
        ));
    }

    #[test]
    fn test_is_falsy_type_typescript() {
        assert!(is_falsy_type(
            &TypeShape::Named("null".to_string()),
            Language::TypeScript
        ));
        assert!(is_falsy_type(
            &TypeShape::Named("undefined".to_string()),
            Language::TypeScript
        ));
        // Deterministic: boolean/number contain truthy values, not singleton falsy
        assert!(!is_falsy_type(
            &TypeShape::Named("boolean".to_string()),
            Language::TypeScript
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("number".to_string()),
            Language::TypeScript
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("string".to_string()),
            Language::TypeScript
        ));
    }

    #[test]
    fn test_is_falsy_type_rust() {
        assert!(is_falsy_type(
            &TypeShape::Named("Option::None".to_string()),
            Language::Rust
        ));
        assert!(is_falsy_type(
            &TypeShape::Named("None".to_string()),
            Language::Rust
        ));
        // Deterministic: bool is not singleton falsy (contains true)
        assert!(!is_falsy_type(
            &TypeShape::Named("bool".to_string()),
            Language::Rust
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("String".to_string()),
            Language::Rust
        ));
    }

    #[test]
    fn test_is_falsy_type_go() {
        assert!(is_falsy_type(
            &TypeShape::Named("nil".to_string()),
            Language::Go
        ));
        // Deterministic: error/bool are not singleton falsy
        assert!(!is_falsy_type(
            &TypeShape::Named("error".to_string()),
            Language::Go
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("bool".to_string()),
            Language::Go
        ));
        assert!(!is_falsy_type(
            &TypeShape::Named("string".to_string()),
            Language::Go
        ));
    }

    #[test]
    fn test_is_falsy_type_union() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("String".to_string()),
            TypeShape::Named("None".to_string()),
        ]);
        assert!(is_falsy_type(&shape, Language::Python));
    }

    #[test]
    fn test_is_falsy_type_array() {
        let shape = TypeShape::Array(Box::new(TypeShape::Named("int".to_string())));
        assert!(!is_falsy_type(&shape, Language::Python));
    }

    // ==================== narrow_truthiness tests ====================

    #[test]
    fn test_narrow_truthiness_union_assume_true() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("String".to_string()),
            TypeShape::Named("None".to_string()),
        ]);
        let result = narrow_truthiness(&shape, true, Language::Python).unwrap();
        assert_eq!(result, TypeShape::Named("String".to_string()));
    }

    #[test]
    fn test_narrow_truthiness_union_assume_false() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("String".to_string()),
            TypeShape::Named("None".to_string()),
        ]);
        let result = narrow_truthiness(&shape, false, Language::Python).unwrap();
        assert_eq!(result, TypeShape::Named("None".to_string()));
    }

    #[test]
    fn test_narrow_truthiness_single_falsy_assume_true() {
        let shape = TypeShape::Named("None".to_string());
        let result = narrow_truthiness(&shape, true, Language::Python);
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_truthiness_single_non_falsy_assume_false() {
        let shape = TypeShape::Named("String".to_string());
        let result = narrow_truthiness(&shape, false, Language::Python);
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_truthiness_single_non_falsy_assume_true() {
        let shape = TypeShape::Named("String".to_string());
        let result = narrow_truthiness(&shape, true, Language::Python).unwrap();
        assert_eq!(result, TypeShape::Named("String".to_string()));
    }

    // ==================== narrow_discriminated_union tests ====================

    #[test]
    fn test_narrow_discriminated_union_non_union() {
        let shape = TypeShape::Named("String".to_string());
        let result = narrow_discriminated_union(&shape, "kind", "success", None);
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_discriminated_union_exact_match() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("Success".to_string()),
            TypeShape::Named("Error".to_string()),
        ]);
        let result = narrow_discriminated_union(&shape, "kind", "success", None);
        // Deterministic narrowing without TypeMemberIndex now returns None
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_discriminated_union_case_insensitive() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("Success".to_string()),
            TypeShape::Named("Error".to_string()),
        ]);
        let result = narrow_discriminated_union(&shape, "kind", "SUCCESS", None);
        // Heuristic case-insensitive matching removed; without index returns None
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_discriminated_union_contains_match() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("SuccessResult".to_string()),
            TypeShape::Named("ErrorResult".to_string()),
        ]);
        let result = narrow_discriminated_union(&shape, "kind", "success", None);
        // Heuristic contains matching removed; without index returns None
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_discriminated_union_no_match() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("Success".to_string()),
            TypeShape::Named("Error".to_string()),
        ]);
        let result = narrow_discriminated_union(&shape, "kind", "pending", None);
        assert!(result.is_none());
    }

    #[test]
    fn test_narrow_discriminated_union_multiple_matches() {
        let shape = TypeShape::Union(vec![
            TypeShape::Named("Success".to_string()),
            TypeShape::Named("SuccessResult".to_string()),
        ]);
        let result = narrow_discriminated_union(&shape, "kind", "success", None);
        // Deterministic: without TypeMemberIndex no heuristic fallback
        assert!(result.is_none());
    }

    // ==================== split_type_args tests ====================

    #[test]
    fn test_split_type_args_simple() {
        let result = split_type_args("String, Integer");
        assert_eq!(result, vec!["String", "Integer"]);
    }

    #[test]
    fn test_split_type_args_nested() {
        let result = split_type_args("String, List<Integer>");
        assert_eq!(result, vec!["String", "List<Integer>"]);
    }

    #[test]
    fn test_split_type_args_single() {
        let result = split_type_args("String");
        assert_eq!(result, vec!["String"]);
    }

    #[test]
    fn test_split_type_args_empty() {
        let result = split_type_args("");
        assert!(result.is_empty());
    }

    // ==================== is_type_param_name tests ====================

    #[test]
    fn test_is_type_param_name_single_uppercase() {
        assert!(is_type_param_name("T"));
        assert!(is_type_param_name("K"));
        assert!(is_type_param_name("V"));
        assert!(is_type_param_name("E"));
        assert!(!is_type_param_name("a"));
        assert!(!is_type_param_name("x"));
    }

    #[test]
    fn test_is_type_param_name_multi_letter() {
        assert!(is_type_param_name("Value"));
        assert!(is_type_param_name("Key"));
        assert!(is_type_param_name("Element"));
        assert!(is_type_param_name("Item"));
        assert!(!is_type_param_name("String"));
        assert!(!is_type_param_name("Int"));
    }

    // ==================== complex type_shape tests ====================

    #[test]
    fn test_parse_type_shape_complex_generic() {
        let shape = parse_type_shape("HashMap<String, List<Integer>>", Language::Kotlin).unwrap();
        match shape {
            TypeShape::Generic { base, args } => {
                assert_eq!(base, "HashMap");
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], TypeShape::Named("String".to_string()));
                match &args[1] {
                    TypeShape::Generic { base, args } => {
                        assert_eq!(base, "List");
                        assert_eq!(args.len(), 1);
                        assert_eq!(args[0], TypeShape::Named("Integer".to_string()));
                    }
                    _ => panic!("Expected nested Generic"),
                }
            }
            _ => panic!("Expected Generic"),
        }
    }

    #[test]
    fn test_parse_type_shape_reference_generic() {
        let shape = parse_type_shape("&Vec<T>", Language::Rust).unwrap();
        assert_eq!(
            shape,
            TypeShape::Reference {
                inner: Box::new(TypeShape::Generic {
                    base: "Vec".to_string(),
                    args: vec![TypeShape::Param("T".to_string())]
                }),
                mutable: false,
            }
        );
    }

    #[test]
    fn test_parse_type_shape_union_with_generic() {
        let shape = parse_type_shape("List<String> | None", Language::Python).unwrap();
        match shape {
            TypeShape::Union(members) => {
                assert_eq!(members.len(), 2);
                assert_eq!(
                    members[0],
                    TypeShape::Generic {
                        base: "List".to_string(),
                        args: vec![TypeShape::Named("String".to_string())]
                    }
                );
                assert_eq!(members[1], TypeShape::Named("None".to_string()));
            }
            _ => panic!("Expected Union"),
        }
    }

    // ==================== TypeShape::to_type_string method ====================

    #[test]
    fn test_type_shape_to_type_string_method() {
        let shape = TypeShape::Generic {
            base: "Vec".to_string(),
            args: vec![TypeShape::Named("String".to_string())],
        };
        assert_eq!(shape.to_type_string(), "Vec<String>");
    }

    // ==================== Nested destructuring pattern tests ====================

    #[test]
    fn test_pattern_match_nested_tuple() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let inner_tuple = Pattern::Tuple(vec!["x".to_string(), "y".to_string()]);
        let source_type = TypeShape::Generic {
            base: "Tuple".to_string(),
            args: vec![
                TypeShape::Named("i32".to_string()),
                TypeShape::Named("String".to_string()),
            ],
        };
        ctx.add_pattern_match_binding(&inner_tuple, &source_type);
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "i32");
        assert_eq!(ctx.get_variable_type("y").unwrap().type_name, "String");
    }

    #[test]
    fn test_pattern_match_struct_multiple_fields() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let pattern = Pattern::Struct(vec![
            "name".to_string(),
            "age".to_string(),
            "email".to_string(),
        ]);
        let source_type = TypeShape::Named("User".to_string());
        ctx.add_pattern_match_binding(&pattern, &source_type);
        assert_eq!(ctx.get_variable_type("name").unwrap().type_name, "unknown");
        assert_eq!(ctx.get_variable_type("age").unwrap().type_name, "unknown");
        assert_eq!(ctx.get_variable_type("email").unwrap().type_name, "unknown");
    }

    #[test]
    fn test_pattern_match_tuple_three_elements() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let pattern = Pattern::Tuple(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        let source_type = TypeShape::Generic {
            base: "Tuple".to_string(),
            args: vec![
                TypeShape::Named("int".to_string()),
                TypeShape::Named("String".to_string()),
                TypeShape::Named("bool".to_string()),
            ],
        };
        ctx.add_pattern_match_binding(&pattern, &source_type);
        assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "int");
        assert_eq!(ctx.get_variable_type("b").unwrap().type_name, "String");
        assert_eq!(ctx.get_variable_type("c").unwrap().type_name, "bool");
    }

    #[test]
    fn test_pattern_match_tuple_fewer_args_than_source() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        let pattern = Pattern::Tuple(vec!["a".to_string()]);
        let source_type = TypeShape::Generic {
            base: "Tuple".to_string(),
            args: vec![
                TypeShape::Named("i32".to_string()),
                TypeShape::Named("String".to_string()),
            ],
        };
        ctx.add_pattern_match_binding(&pattern, &source_type);
        assert_eq!(ctx.get_variable_type("a").unwrap().type_name, "i32");
        assert!(ctx.get_variable_type("b").is_none());
    }

    #[test]
    fn test_pattern_match_identifier_with_generic_source() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let pattern = Pattern::Identifier("items".to_string());
        let source_type = TypeShape::Generic {
            base: "List".to_string(),
            args: vec![TypeShape::Named("String".to_string())],
        };
        ctx.add_pattern_match_binding(&pattern, &source_type);
        let binding = ctx.get_variable_type("items").unwrap();
        assert_eq!(binding.type_name, "List<String>");
        assert_eq!(
            binding.shape,
            Some(TypeShape::Generic {
                base: "List".to_string(),
                args: vec![TypeShape::Named("String".to_string())]
            })
        );
    }

    // ==================== Destructuring assignment tests ====================

    #[test]
    fn test_destructuring_binding_tuple_index_out_of_bounds() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source_type = TypeShape::Generic {
            base: "tuple".to_string(),
            args: vec![TypeShape::Named("int".to_string())],
        };
        ctx.add_destructuring_binding("a", &source_type, Some(5));
        let binding = ctx.get_variable_type("a").unwrap();
        assert_eq!(binding.type_name, "unknown");
    }

    #[test]
    fn test_destructuring_binding_array_element() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source_type = TypeShape::Array(Box::new(TypeShape::Named("String".to_string())));
        ctx.add_destructuring_binding("elem", &source_type, None);
        assert_eq!(ctx.get_variable_type("elem").unwrap().type_name, "String");
        assert_eq!(
            ctx.get_variable_type("elem").unwrap().shape,
            Some(TypeShape::Named("String".to_string()))
        );
    }

    #[test]
    fn test_destructuring_binding_non_destructured_type() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source_type = TypeShape::Named("String".to_string());
        ctx.add_destructuring_binding("x", &source_type, Some(0));
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "unknown");
        assert_eq!(
            binding.origin,
            Some(InferenceOrigin::DestructuringAssignment)
        );
    }

    #[test]
    fn test_destructuring_binding_generic_tuple() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source_type = TypeShape::Generic {
            base: "Tuple".to_string(),
            args: vec![
                TypeShape::Named("i32".to_string()),
                TypeShape::Named("bool".to_string()),
                TypeShape::Named("String".to_string()),
            ],
        };
        ctx.add_destructuring_binding("x", &source_type, Some(2));
        assert_eq!(ctx.get_variable_type("x").unwrap().type_name, "String");
    }

    // ==================== Nested generic TypeShape parsing tests ====================

    #[test]
    fn test_parse_type_shape_triple_nested_generic() {
        let shape = parse_type_shape("Map<String, List<Option<i32>>>", Language::Kotlin).unwrap();
        match shape {
            TypeShape::Generic { base, args } => {
                assert_eq!(base, "Map");
                assert_eq!(args.len(), 2);
                match &args[1] {
                    TypeShape::Generic { base, args } => {
                        assert_eq!(base, "List");
                        match &args[0] {
                            TypeShape::Generic { base, args } => {
                                assert_eq!(base, "Option");
                                assert_eq!(args[0], TypeShape::Named("i32".to_string()));
                            }
                            _ => panic!("Expected Generic for Option<i32>"),
                        }
                    }
                    _ => panic!("Expected Generic for List<Option<i32>>"),
                }
            }
            _ => panic!("Expected Generic for Map<String, List<Option<i32>>>"),
        }
    }

    #[test]
    fn test_parse_type_shape_reference_to_generic() {
        let shape = parse_type_shape("&HashMap<String, i32>", Language::Rust).unwrap();
        assert_eq!(
            shape,
            TypeShape::Reference {
                inner: Box::new(TypeShape::Generic {
                    base: "HashMap".to_string(),
                    args: vec![
                        TypeShape::Named("String".to_string()),
                        TypeShape::Named("i32".to_string())
                    ]
                }),
                mutable: false,
            }
        );
    }

    #[test]
    fn test_parse_type_shape_mut_reference_to_generic() {
        let shape = parse_type_shape("&mut Vec<Option<String>>", Language::Rust).unwrap();
        assert_eq!(
            shape,
            TypeShape::Reference {
                inner: Box::new(TypeShape::Generic {
                    base: "Vec".to_string(),
                    args: vec![TypeShape::Generic {
                        base: "Option".to_string(),
                        args: vec![TypeShape::Named("String".to_string())]
                    }]
                }),
                mutable: true,
            }
        );
    }

    #[test]
    fn test_type_shape_roundtrip_nested_generic() {
        let original = "HashMap<String, List<i32>>";
        let shape = parse_type_shape(original, Language::Kotlin).unwrap();
        let back = type_shape_to_string(&shape);
        assert_eq!(back, original);
    }

    #[test]
    fn test_type_shape_roundtrip_reference_generic() {
        let original = "&Vec<String>";
        let shape = parse_type_shape(original, Language::Rust).unwrap();
        let back = type_shape_to_string(&shape);
        assert_eq!(back, original);
    }

    // ==================== ScopedTypeContext resolve_type with nested generics ====================

    #[test]
    fn test_resolve_type_nested_generic() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param("T".to_string(), "String".to_string());
        ctx.bind_type_param("K".to_string(), "i32".to_string());
        assert_eq!(ctx.resolve_type("HashMap<K, T>"), "HashMap<i32, String>");
    }

    #[test]
    fn test_resolve_type_deeply_nested() {
        let mut ctx = ScopedTypeContext::new(Language::Rust);
        ctx.bind_type_param("T".to_string(), "String".to_string());
        assert_eq!(ctx.resolve_type("Vec<Option<T>>"), "Vec<Option<String>>");
    }

    #[test]
    fn test_resolve_type_no_bindings() {
        let ctx = ScopedTypeContext::new(Language::Rust);
        assert_eq!(ctx.resolve_type("Vec<T>"), "Vec<T>");
    }

    // ==================== Narrow union with multiple operations ====================

    #[test]
    fn test_narrow_union_sequential() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let shape = TypeShape::Union(vec![
            TypeShape::Named("String".to_string()),
            TypeShape::Named("int".to_string()),
            TypeShape::Named("None".to_string()),
        ]);
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: type_shape_to_string(&shape),
                shape: Some(shape),
                ..Default::default()
            },
        );
        ctx.narrow_union("x", &TypeShape::Named("None".to_string()));
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "String | int");
    }

    #[test]
    fn test_narrow_union_single_member_left() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let shape = TypeShape::Union(vec![TypeShape::Named("String".to_string())]);
        ctx.add_variable_type(
            "x".to_string(),
            TypeBinding {
                type_name: type_shape_to_string(&shape),
                shape: Some(shape),
                ..Default::default()
            },
        );
        ctx.narrow_union("x", &TypeShape::Named("String".to_string()));
        let binding = ctx.get_variable_type("x").unwrap();
        assert_eq!(binding.type_name, "String");
    }

    #[test]
    fn test_element_type_of_array_shape() {
        let shape = TypeShape::Array(Box::new(TypeShape::Named("User".to_string())));
        let element = element_type_of_shape(&shape).expect("array has element type");
        assert_eq!(element, TypeShape::Named("User".to_string()));
    }

    #[test]
    fn test_element_type_of_generic_shape() {
        let shape = TypeShape::Generic {
            base: "List".to_string(),
            args: vec![TypeShape::Named("User".to_string())],
        };
        let element = element_type_of_shape(&shape).expect("generic has element type");
        assert_eq!(element, TypeShape::Named("User".to_string()));
    }

    #[test]
    fn test_element_type_of_plain_shape_is_none() {
        let shape = TypeShape::Named("User".to_string());
        assert!(element_type_of_shape(&shape).is_none());
    }

    #[test]
    fn test_destructuring_binding_maps_generic_position() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source = TypeShape::Generic {
            base: "Pair".to_string(),
            args: vec![
                TypeShape::Named("String".to_string()),
                TypeShape::Named("int".to_string()),
            ],
        };
        ctx.add_destructuring_binding("second", &source, Some(1));
        let binding = ctx
            .get_variable_type("second")
            .expect("destructured binding exists");
        assert_eq!(binding.type_name, "int");
    }

    #[test]
    fn test_destructuring_binding_out_of_range_stays_unknown() {
        let mut ctx = ScopedTypeContext::new(Language::Python);
        let source = TypeShape::Generic {
            base: "Pair".to_string(),
            args: vec![TypeShape::Named("String".to_string())],
        };
        ctx.add_destructuring_binding("second", &source, Some(1));
        let binding = ctx
            .get_variable_type("second")
            .expect("destructured binding exists");
        assert_eq!(binding.type_name, "unknown");
    }
}
