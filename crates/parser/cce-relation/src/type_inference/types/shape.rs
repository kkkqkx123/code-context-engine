//! Structured type representation and parsing.

use cce_types::language::Language;

/// Instantiate a type shape by substituting type parameters.
///
/// Replaces every `Param(name)` with its bound shape, recursing through
/// generic arguments, arrays, unions, intersections and references.
/// Unbound parameters are kept as-is so partially known shapes stay
/// usable instead of collapsing to unknown.
pub fn instantiate_type_shape(
    shape: &TypeShape,
    bindings: &std::collections::HashMap<String, TypeShape>,
) -> TypeShape {
    match shape {
        TypeShape::Param(name) => bindings.get(name).cloned().unwrap_or_else(|| shape.clone()),
        TypeShape::Generic { base, args } => TypeShape::Generic {
            base: base.clone(),
            args: args
                .iter()
                .map(|arg| instantiate_type_shape(arg, bindings))
                .collect(),
        },
        TypeShape::Array(inner) => {
            TypeShape::Array(Box::new(instantiate_type_shape(inner, bindings)))
        }
        TypeShape::Union(members) => TypeShape::Union(
            members
                .iter()
                .map(|m| instantiate_type_shape(m, bindings))
                .collect(),
        ),
        TypeShape::Intersection(members) => TypeShape::Intersection(
            members
                .iter()
                .map(|m| instantiate_type_shape(m, bindings))
                .collect(),
        ),
        TypeShape::Reference { inner, mutable } => TypeShape::Reference {
            inner: Box::new(instantiate_type_shape(inner, bindings)),
            mutable: *mutable,
        },
        TypeShape::Named(_) | TypeShape::Wildcard { .. } => shape.clone(),
    }
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
/// Tracks angle, square and round brackets independently so nested forms
/// such as `Tuple[str, Tuple[int, bool]]` stay intact. A comma splits only
/// when every depth is zero.
fn split_type_args(inner: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut angle = 0usize;
    let mut square = 0usize;
    let mut round = 0usize;
    let mut current = String::new();
    for ch in inner.chars() {
        match ch {
            '<' => {
                angle += 1;
                current.push(ch);
            }
            '>' => {
                angle = angle.saturating_sub(1);
                current.push(ch);
            }
            '[' => {
                square += 1;
                current.push(ch);
            }
            ']' => {
                square = square.saturating_sub(1);
                current.push(ch);
            }
            '(' => {
                round += 1;
                current.push(ch);
            }
            ')' => {
                round = round.saturating_sub(1);
                current.push(ch);
            }
            ',' if angle == 0 && square == 0 && round == 0 => {
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
pub fn parse_type_shape(type_name: &str, language: Language) -> Option<TypeShape> {
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
    // Handle Go-style leading slice/array prefix `[]T` or `[N]T` before
    // bracket-generic parsing (which requires a non-empty base name).
    if trimmed.starts_with('[') {
        if let Some(end) = trimmed.find(']') {
            let inner_marker = trimmed[1..end].trim();
            let is_go_prefix = inner_marker.is_empty()
                || (inner_marker.len() <= 12 && inner_marker.chars().all(|c| c.is_ascii_digit()));
            if is_go_prefix {
                let elem = trimmed[end + 1..].trim();
                if !elem.is_empty() {
                    if let Some(inner) = parse_type_shape(elem, language) {
                        return Some(TypeShape::Array(Box::new(inner)));
                    }
                }
            }
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
    // Handle Rust parenthesized tuple types `(A, B)` positionally.
    // Destructuring (`let (num, text) = pair` with `pair: (i32, String)`)
    // maps parts by index, so the tuple must parse to positional
    // arguments rather than an opaque name. Gated to Rust: other
    // languages either lack paren tuples or overload the syntax
    // (TypeScript arrow types, grouping parens). A single wrapped type
    // (`(T)`) and the unit type (`()`) keep the previous opaque form.
    if language == Language::Rust
        && let Some(inner) = trimmed.strip_prefix('(').and_then(|s| s.strip_suffix(')'))
    {
        let arg_strs = split_type_args(inner);
        if arg_strs.len() > 1 {
            let args: Vec<TypeShape> = arg_strs
                .iter()
                .filter_map(|a| parse_type_shape(a.trim(), language))
                .collect();
            if args.len() == arg_strs.len() {
                return Some(TypeShape::Generic {
                    base: "Tuple".to_string(),
                    args,
                });
            }
        }
    }
    // Python canonical names: the shared literal vocabulary (`string`,
    // `array`, `boolean`) names no Python builtin, so bare occurrences
    // normalize to the builtins the stdlib index and member lookup
    // understand (`str`, `list`, `bool`). Structural forms above are
    // unaffected; only the final bare-name fallthrough maps.
    if language == Language::Python
        && let Some(canonical) = python_canonical_literal_name(trimmed)
    {
        return Some(TypeShape::Named(canonical.to_string()));
    }
    Some(TypeShape::Named(trimmed.to_string()))
}

/// Map the shared literal-vocabulary name to the Python builtin.
///
/// Returns `None` for names that are already canonical or out of scope
/// (`number` stays untouched: it cannot resolve to `int` vs `float`
/// without value information).
pub fn python_canonical_literal_name(name: &str) -> Option<&'static str> {
    match name.trim() {
        "string" => Some("str"),
        "array" => Some("list"),
        "boolean" => Some("bool"),
        _ => None,
    }
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

/// Extract the element type nested `depth` levels deep.
///
/// Depth 1 matches `element_type_of_shape`; deeper levels peel nested
/// containers (`Vec<Vec<T>>` at depth 2 yields `T`), which models nested
/// loop iteration. Returns `None` when any level lacks an element type.
pub fn element_type_at_depth(shape: &TypeShape, depth: usize) -> Option<TypeShape> {
    if depth == 0 {
        return Some(shape.clone());
    }
    let mut current = shape.clone();
    for _ in 0..depth {
        current = element_type_of_shape(&current)?;
    }
    Some(current)
}

/// Build call-site bindings from formal type parameters to actual shapes.
///
/// Pairs each formal parameter name with the corresponding actual argument
/// shape by position; extras on either side are ignored so partially known
/// call sites still yield usable bindings for substitution.
pub fn build_shape_bindings(
    params: &[String],
    args: &[TypeShape],
) -> std::collections::HashMap<String, TypeShape> {
    params
        .iter()
        .zip(args.iter())
        .map(|(param, arg)| (param.clone(), arg.clone()))
        .collect()
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

#[cfg(test)]
mod tests {

    use super::*;

    use cce_types::language::Language;

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
    fn test_parse_type_shape_go_slice() {
        let shape = parse_type_shape("[]T", Language::Go).unwrap();
        assert_eq!(
            shape,
            TypeShape::Array(Box::new(TypeShape::Param("T".to_string())))
        );
        let shape = parse_type_shape("[]int", Language::Go).unwrap();
        assert_eq!(
            shape,
            TypeShape::Array(Box::new(TypeShape::Named("int".to_string())))
        );
        let shape = parse_type_shape("[3]byte", Language::Go).unwrap();
        assert_eq!(
            shape,
            TypeShape::Array(Box::new(TypeShape::Named("byte".to_string())))
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
    fn test_element_type_at_depth_zero_returns_shape() {
        let shape = TypeShape::Named("User".to_string());
        assert_eq!(element_type_at_depth(&shape, 0).expect("depth zero"), shape);
    }

    #[test]
    fn test_element_type_at_depth_one_matches_single_level() {
        let shape = TypeShape::Generic {
            base: "List".to_string(),
            args: vec![TypeShape::Named("User".to_string())],
        };
        let element = element_type_at_depth(&shape, 1).expect("element type");
        assert_eq!(element, TypeShape::Named("User".to_string()));
    }

    #[test]
    fn test_parse_nested_bracket_generics_keep_position() {
        let shape =
            parse_type_shape("Tuple[str, Tuple[int, bool]]", Language::Python).expect("shape");
        let TypeShape::Generic { args, .. } = &shape else {
            panic!("outer shape is generic, got {shape:?}");
        };
        assert_eq!(args.len(), 2);
        assert_eq!(type_shape_to_string(&args[1]), "Tuple<int, bool>");
        let leaf = element_type_at_depth(&args[1], 1).expect("nested element");
        assert_eq!(leaf, TypeShape::Named("int".to_string()));
    }

    #[test]
    fn test_element_type_at_depth_two_peels_nested_generic() {
        let shape = TypeShape::Generic {
            base: "Vec".to_string(),
            args: vec![TypeShape::Generic {
                base: "Vec".to_string(),
                args: vec![TypeShape::Named("T".to_string())],
            }],
        };
        let element = element_type_at_depth(&shape, 2).expect("nested element");
        assert_eq!(element, TypeShape::Named("T".to_string()));
        let middle = element_type_at_depth(&shape, 1).expect("middle level");
        assert_eq!(type_shape_to_string(&middle), "Vec<T>");
    }

    #[test]
    fn test_element_type_at_depth_beyond_nesting_is_none() {
        let shape = TypeShape::Array(Box::new(TypeShape::Named("int".to_string())));
        assert!(element_type_at_depth(&shape, 2).is_none());
    }

    #[test]
    fn test_instantiate_type_shape_substitutes_params() {
        use std::collections::HashMap;
        let shape = parse_type_shape("Pair[T, String]", Language::Java).expect("shape");
        let mut bindings = HashMap::new();
        bindings.insert("T".to_string(), TypeShape::Named("Integer".to_string()));
        let instantiated = instantiate_type_shape(&shape, &bindings);
        assert_eq!(type_shape_to_string(&instantiated), "Pair<Integer, String>");
    }

    #[test]
    fn test_instantiate_type_shape_keeps_unbound_params() {
        use std::collections::HashMap;
        let shape = parse_type_shape("Pair[T, U]", Language::Java).expect("shape");
        let mut bindings = HashMap::new();
        bindings.insert("T".to_string(), TypeShape::Named("Integer".to_string()));
        let instantiated = instantiate_type_shape(&shape, &bindings);
        assert_eq!(type_shape_to_string(&instantiated), "Pair<Integer, U>");
    }

    #[test]
    fn test_instantiate_type_shape_recurses_nested() {
        use std::collections::HashMap;
        let shape = parse_type_shape("Vec<Vec<T>>", Language::Rust).expect("shape");
        let mut bindings = HashMap::new();
        bindings.insert("T".to_string(), TypeShape::Named("String".to_string()));
        let instantiated = instantiate_type_shape(&shape, &bindings);
        assert_eq!(type_shape_to_string(&instantiated), "Vec<Vec<String>>");
    }

    #[test]
    fn test_rust_paren_tuple_parses_positionally() {
        let shape = parse_type_shape("(i32, String)", Language::Rust).expect("shape");
        let TypeShape::Generic { base, args } = &shape else {
            panic!("tuple shape is generic, got {shape:?}");
        };
        assert_eq!(base, "Tuple");
        assert_eq!(args.len(), 2);
        assert_eq!(type_shape_to_string(&args[0]), "i32");
        assert_eq!(type_shape_to_string(&args[1]), "String");
    }

    #[test]
    fn test_rust_paren_tuple_nests_inside_generic() {
        let shape = parse_type_shape("Option<(i32, i32)>", Language::Rust).expect("shape");
        let TypeShape::Generic { args, .. } = &shape else {
            panic!("outer shape is generic, got {shape:?}");
        };
        assert_eq!(type_shape_to_string(&args[0]), "Tuple<i32, i32>");
    }

    #[test]
    fn test_rust_grouping_paren_stays_opaque() {
        let shape = parse_type_shape("(String)", Language::Rust).expect("shape");
        assert_eq!(shape, TypeShape::Named("(String)".to_string()));
    }

    #[test]
    fn test_paren_tuple_only_applies_to_rust() {
        let shape = parse_type_shape("(i32, String)", Language::TypeScript).expect("shape");
        assert_eq!(shape, TypeShape::Named("(i32, String)".to_string()));
    }
}
