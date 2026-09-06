//! Function overload resolution framework.
//!
//! Provides structures for grouping same-name methods on a type into an
//! overload set and selecting the best matching overload based on argument
//! types and specificity scoring.

use std::collections::HashMap;

use cce_types::entity::EntityId;
use cce_types::language::Language;

use super::types::{TypeShape, type_shape_to_string};

/// An overload set: all methods with the same name on a type.
#[derive(Debug, Clone)]
pub struct OverloadSet {
    /// Method name
    pub name: String,
    /// Owner type qualified name
    pub owner_type: String,
    /// All overloads sorted by specificity
    pub candidates: Vec<OverloadCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatibilityLevel {
    Exact,
    Coerce,
    Incompatible,
}

#[derive(Debug, Clone)]
pub struct TypeCompatibility {
    compatibility: HashMap<(String, String), CompatibilityLevel>,
}

impl TypeCompatibility {
    pub fn new_for_language(lang: Language) -> Self {
        let mut compatibility = HashMap::new();
        match lang {
            Language::Rust => {
                for (a, b) in [
                    ("i32", "i64"),
                    ("i32", "f64"),
                    ("i32", "f32"),
                    ("i64", "f64"),
                    ("u32", "u64"),
                    ("u32", "i64"),
                    ("f32", "f64"),
                    ("&str", "String"),
                    ("String", "&str"),
                    ("i32", "isize"),
                    ("u32", "usize"),
                ] {
                    compatibility
                        .insert((a.to_string(), b.to_string()), CompatibilityLevel::Coerce);
                    compatibility
                        .insert((b.to_string(), a.to_string()), CompatibilityLevel::Coerce);
                }
                compatibility.insert(
                    ("i32".to_string(), "isize".to_string()),
                    CompatibilityLevel::Coerce,
                );
            }
            Language::Python => {
                for (a, b) in [
                    ("int", "float"),
                    ("str", "Any"),
                    ("int", "Any"),
                    ("float", "Any"),
                    ("bool", "int"),
                    ("list", "Any"),
                    ("dict", "Any"),
                ] {
                    compatibility
                        .insert((a.to_string(), b.to_string()), CompatibilityLevel::Coerce);
                }
                compatibility.insert(
                    ("float".to_string(), "int".to_string()),
                    CompatibilityLevel::Coerce,
                );
            }
            Language::Java => {
                for (a, b) in [
                    ("int", "long"),
                    ("int", "double"),
                    ("int", "float"),
                    ("long", "double"),
                    ("float", "double"),
                    ("String", "Object"),
                    ("Integer", "int"),
                    ("Double", "double"),
                ] {
                    compatibility
                        .insert((a.to_string(), b.to_string()), CompatibilityLevel::Coerce);
                }
            }
            Language::TypeScript | Language::JavaScript | Language::Tsx | Language::Jsx => {
                for (a, b) in [("number", "int"), ("number", "float"), ("string", "String")] {
                    compatibility
                        .insert((a.to_string(), b.to_string()), CompatibilityLevel::Coerce);
                    compatibility
                        .insert((b.to_string(), a.to_string()), CompatibilityLevel::Coerce);
                }
            }
            Language::Go => {
                for (a, b) in [
                    ("int", "int64"),
                    ("int", "float64"),
                    ("int32", "int64"),
                    ("float32", "float64"),
                ] {
                    compatibility
                        .insert((a.to_string(), b.to_string()), CompatibilityLevel::Coerce);
                }
            }
            _ => {
                // generic numeric coercions
                for (a, b) in [("int", "float"), ("int", "double"), ("float", "double")] {
                    compatibility
                        .insert((a.to_string(), b.to_string()), CompatibilityLevel::Coerce);
                }
            }
        }
        Self { compatibility }
    }

    pub fn is_compatible(&self, expected: &str, actual: &str) -> CompatibilityLevel {
        if expected == actual {
            return CompatibilityLevel::Exact;
        }
        // case-insensitive exact
        if expected.eq_ignore_ascii_case(actual) {
            return CompatibilityLevel::Exact;
        }
        if let Some(level) = self
            .compatibility
            .get(&(expected.to_string(), actual.to_string()))
        {
            return level.clone();
        }
        if let Some(level) = self
            .compatibility
            .get(&(actual.to_string(), expected.to_string()))
        {
            if *level == CompatibilityLevel::Coerce {
                return CompatibilityLevel::Coerce;
            }
        }
        // numeric family heuristic
        let numeric = [
            "int", "i32", "i64", "u32", "u64", "float", "f32", "f64", "double", "long", "number",
        ];
        let exp_is_num = numeric.iter().any(|n| n.eq_ignore_ascii_case(expected));
        let act_is_num = numeric.iter().any(|n| n.eq_ignore_ascii_case(actual));
        if exp_is_num && act_is_num {
            return CompatibilityLevel::Coerce;
        }
        CompatibilityLevel::Incompatible
    }
}

impl Default for TypeCompatibility {
    fn default() -> Self {
        Self::new_for_language(Language::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OverloadScore {
    pub exact_matches: usize,
    pub coerce_matches: usize,
    pub incompatible: usize,
    pub return_type_match: bool,
    pub specificity: u32,
}

impl OverloadScore {
    /// Check if this score is better than another
    pub fn better_than(&self, other: &OverloadScore) -> bool {
        if self.exact_matches != other.exact_matches {
            return self.exact_matches > other.exact_matches;
        }
        if self.return_type_match != other.return_type_match {
            return self.return_type_match;
        }
        if self.coerce_matches != other.coerce_matches {
            return self.coerce_matches > other.coerce_matches;
        }
        self.specificity > other.specificity
    }
}

/// A single overload candidate.
#[derive(Debug, Clone)]
pub struct OverloadCandidate {
    pub entity_id: EntityId,
    pub parameter_types: Vec<TypeShape>,
    pub return_type: TypeShape,
    /// Specificity score for ranking (higher = more specific)
    pub specificity: u32,
}

impl OverloadCandidate {
    pub fn score(
        &self,
        arg_types: &[Option<&TypeShape>],
        expected_return: Option<&str>,
        language: Language,
    ) -> OverloadScore {
        let compatibility = TypeCompatibility::new_for_language(language);
        let mut score = OverloadScore {
            exact_matches: 0,
            coerce_matches: 0,
            incompatible: 0,
            return_type_match: false,
            specificity: self.specificity,
        };
        // Derive call-site generic bindings from this candidate's own
        // formal parameters (`process<T>(x: T)` called with a `string`
        // binds `T = string`), so generic candidates score by their
        // substituted types instead of failing on bare parameters.
        let generic_bindings =
            super::generics::bind_call_site_generics(&self.parameter_types, arg_types, language);
        for (i, param_type) in self.parameter_types.iter().enumerate() {
            if let Some(arg_type) = arg_types.get(i).and_then(|a| *a) {
                let expected_str = type_shape_to_string(param_type);
                let actual_str = type_shape_to_string(arg_type);
                match compatibility.is_compatible(&expected_str, &actual_str) {
                    CompatibilityLevel::Exact => score.exact_matches += 1,
                    CompatibilityLevel::Coerce => score.coerce_matches += 1,
                    CompatibilityLevel::Incompatible => {
                        if is_assignable_with_shapes(arg_type, param_type, &generic_bindings) {
                            if arg_type == param_type {
                                score.exact_matches += 1;
                            } else {
                                score.coerce_matches += 1;
                            }
                        } else {
                            score.incompatible += 1;
                        }
                    }
                }
            }
        }
        if let Some(expected) = expected_return {
            let ret_str = type_shape_to_string(&self.return_type);
            score.return_type_match = compatibility.is_compatible(&ret_str, expected)
                != CompatibilityLevel::Incompatible
                || is_assignable(
                    &TypeShape::Named(expected.to_string()),
                    &self.return_type,
                    &HashMap::new(),
                )
                || is_assignable(
                    &self.return_type,
                    &TypeShape::Named(expected.to_string()),
                    &HashMap::new(),
                );
        }
        score
    }
}

impl OverloadSet {
    pub fn new(name: String, owner_type: String) -> Self {
        Self {
            name,
            owner_type,
            candidates: Vec::new(),
        }
    }

    pub fn add_candidate(&mut self, candidate: OverloadCandidate) {
        self.candidates.push(candidate);
        // Keep sorted by specificity descending
        self.candidates
            .sort_by_key(|b| std::cmp::Reverse(b.specificity));
    }

    /// Find the best matching overload for given argument types.
    ///
    /// When `arg_types` is empty (argument types unknown), falls back to
    /// selecting the most specific candidate by specificity score.
    pub fn resolve(&self, arg_types: &[Option<&TypeShape>]) -> Option<&OverloadCandidate> {
        if self.candidates.is_empty() {
            return None;
        }
        // When argument types are unknown, fall back to specificity-based selection
        if arg_types.is_empty() {
            return self.candidates.first();
        }
        // 1. Filter: parameter count must match
        let mut filtered: Vec<&OverloadCandidate> = self
            .candidates
            .iter()
            .filter(|c| c.parameter_types.len() == arg_types.len())
            .collect();
        if filtered.is_empty() {
            return None;
        }
        // 2. Score each candidate
        let mut scored: Vec<(&OverloadCandidate, u32)> = Vec::new();
        for candidate in filtered.drain(..) {
            let mut score = 0u32;
            let mut feasible = true;
            for (actual, expected) in arg_types.iter().zip(candidate.parameter_types.iter()) {
                match actual {
                    None => {
                        // Unknown actual type: neutral scoring
                        score += 1;
                    }
                    Some(actual_shape) => {
                        if is_assignable(actual_shape, expected, &HashMap::new()) {
                            // Exact match gets higher score
                            if *actual_shape == expected {
                                score += 10;
                            } else {
                                score += 5;
                            }
                        } else {
                            feasible = false;
                            break;
                        }
                    }
                }
            }
            if feasible {
                // Add specificity bonus
                score += candidate.specificity;
                scored.push((candidate, score));
            }
        }
        if scored.is_empty() {
            return None;
        }
        // 3. Sort by score descending, pick best
        scored.sort_by_key(|b| std::cmp::Reverse(b.1));
        Some(scored[0].0)
    }

    /// Resolve and annotate the winning signature.
    ///
    /// Returns the resolved entity id together with a rendered signature
    /// (`name(params) -> return`) so callers can record which overload a
    /// call site dispatched to without re-deriving it.
    pub fn resolve_with_signature(
        &self,
        arg_types: &[Option<&TypeShape>],
    ) -> Option<(EntityId, String)> {
        let candidate = self.resolve(arg_types)?;
        let signature = format_overload_signature(&self.name, candidate);
        Some((candidate.entity_id, signature))
    }

    /// Score-based resolution that also annotates the winning signature.
    ///
    /// Preferred dispatch entry when call-site argument types and language
    /// scoring are available: the winner is chosen by compatibility scoring
    /// and returned with its rendered signature for persistence.
    pub fn resolve_with_score_signature(
        &self,
        arg_types: &[Option<&TypeShape>],
        expected_return: Option<&str>,
        language: Language,
    ) -> Option<(EntityId, String)> {
        let candidate = self.resolve_with_score(arg_types, expected_return, language)?;
        let signature = format_overload_signature(&self.name, candidate);
        Some((candidate.entity_id, signature))
    }

    /// Resolve with explicit generic type param bindings.
    pub fn resolve_with_generics(
        &self,
        arg_types: &[Option<&TypeShape>],
        type_params: &HashMap<String, String>,
    ) -> Option<&OverloadCandidate> {
        let shape_bindings: HashMap<String, TypeShape> = type_params
            .iter()
            .map(|(param, bound)| (param.clone(), TypeShape::Named(bound.clone())))
            .collect();
        self.resolve_with_shape_generics(arg_types, &shape_bindings)
    }

    /// Resolve with structured generic bindings.
    ///
    /// This is the preferred entry point for generic-aware resolution:
    /// bindings carry parsed shapes so substitution and assignability stay
    /// structural instead of string-based. The string-keyed overload above
    /// delegates here after lifting its bounds to named shapes.
    pub fn resolve_with_shape_generics(
        &self,
        arg_types: &[Option<&TypeShape>],
        type_params: &HashMap<String, TypeShape>,
    ) -> Option<&OverloadCandidate> {
        if self.candidates.is_empty() {
            return None;
        }
        let mut filtered: Vec<&OverloadCandidate> = self
            .candidates
            .iter()
            .filter(|c| c.parameter_types.len() == arg_types.len())
            .collect();
        if filtered.is_empty() {
            return None;
        }
        let mut scored: Vec<(&OverloadCandidate, u32)> = Vec::new();
        for candidate in filtered.drain(..) {
            let mut score = 0u32;
            let mut feasible = true;
            for (actual, expected) in arg_types.iter().zip(candidate.parameter_types.iter()) {
                match actual {
                    None => {
                        score += 1;
                    }
                    Some(actual_shape) => {
                        if is_assignable_with_shapes(actual_shape, expected, type_params) {
                            if *actual_shape == expected {
                                score += 10;
                            } else {
                                score += 5;
                            }
                        } else {
                            feasible = false;
                            break;
                        }
                    }
                }
            }
            if feasible {
                score += candidate.specificity;
                scored.push((candidate, score));
            }
        }
        if scored.is_empty() {
            return None;
        }
        scored.sort_by_key(|b| std::cmp::Reverse(b.1));
        Some(scored[0].0)
    }

    /// Enhanced overload resolution using actual argument types.
    ///
    /// Scores candidates by type compatibility and specificity. This is the
    /// preferred entry point for Java/Kotlin/C# overload resolution where
    /// call-site argument types are available.
    pub fn resolve_with_args(
        &self,
        arg_types: &[Option<&TypeShape>],
        type_params: &HashMap<String, String>,
    ) -> Option<&OverloadCandidate> {
        self.resolve_with_generics(arg_types, type_params)
    }

    /// Resolve using per-candidate inferred generic bindings.
    ///
    /// Unlike [`OverloadSet::resolve_with_shape_generics`], which scores
    /// every candidate against one shared binding map, this derives bindings
    /// from each candidate's own formal parameters against the call-site
    /// argument shapes. Generic candidates (`process<T>(x: T)`) therefore
    /// compete fairly with concrete ones instead of being filtered out for
    /// mentioning an unbound parameter.
    pub fn resolve_with_inferred_generics(
        &self,
        arg_types: &[Option<&TypeShape>],
        language: Language,
    ) -> Option<&OverloadCandidate> {
        if self.candidates.is_empty() {
            return None;
        }
        let mut filtered: Vec<&OverloadCandidate> = self
            .candidates
            .iter()
            .filter(|c| c.parameter_types.len() == arg_types.len())
            .collect();
        if filtered.is_empty() {
            return None;
        }
        let mut scored: Vec<(&OverloadCandidate, u32)> = Vec::new();
        for candidate in filtered.drain(..) {
            let bindings = super::generics::bind_call_site_generics(
                &candidate.parameter_types,
                arg_types,
                language,
            );
            let mut score = 0u32;
            let mut feasible = true;
            for (actual, expected) in arg_types.iter().zip(candidate.parameter_types.iter()) {
                match actual {
                    None => {
                        score += 1;
                    }
                    Some(actual_shape) => {
                        if is_assignable_with_shapes(actual_shape, expected, &bindings) {
                            if *actual_shape == expected {
                                score += 10;
                            } else {
                                score += 5;
                            }
                        } else {
                            feasible = false;
                            break;
                        }
                    }
                }
            }
            if feasible {
                score += candidate.specificity;
                scored.push((candidate, score));
            }
        }
        if scored.is_empty() {
            return None;
        }
        scored.sort_by_key(|b| std::cmp::Reverse(b.1));
        Some(scored[0].0)
    }

    pub fn resolve_with_score(
        &self,
        arg_types: &[Option<&TypeShape>],
        expected_return: Option<&str>,
        language: Language,
    ) -> Option<&OverloadCandidate> {
        if self.candidates.is_empty() {
            return None;
        }
        if arg_types.is_empty() {
            return self.candidates.first();
        }
        let mut best: Option<(&OverloadCandidate, OverloadScore)> = None;
        for candidate in &self.candidates {
            let score = Self::score_candidate(candidate, arg_types, expected_return, language);
            if score.incompatible > 0 {
                continue;
            }
            let is_better = match &best {
                None => true,
                Some((_, prev_score)) => score.better_than(prev_score),
            };
            if is_better {
                best = Some((candidate, score));
            }
        }
        best.map(|(candidate, _)| candidate)
    }

    fn score_candidate(
        candidate: &OverloadCandidate,
        arg_types: &[Option<&TypeShape>],
        expected_return: Option<&str>,
        language: Language,
    ) -> OverloadScore {
        let mut score = OverloadScore::default();
        let param_count = candidate.parameter_types.len();
        let arg_count = arg_types.len();
        let is_varargs = matches!(language, Language::Python | Language::Rust);
        let min_params = if is_varargs {
            param_count.saturating_sub(1)
        } else {
            param_count
        };
        if is_varargs {
            if arg_count < min_params {
                score.incompatible += 1;
                return score;
            }
        } else if arg_count != param_count {
            score.incompatible += 1;
            return score;
        }
        let compatibility = TypeCompatibility::new_for_language(language);
        for (i, arg_type_opt) in arg_types.iter().enumerate() {
            if let Some(arg_type) = *arg_type_opt {
                if i < candidate.parameter_types.len() {
                    let param_type = &candidate.parameter_types[i];
                    let expected_str = type_shape_to_string(param_type);
                    let actual_str = type_shape_to_string(arg_type);
                    match compatibility.is_compatible(&expected_str, &actual_str) {
                        CompatibilityLevel::Exact => score.exact_matches += 1,
                        CompatibilityLevel::Coerce => score.coerce_matches += 1,
                        CompatibilityLevel::Incompatible => {
                            if is_assignable(arg_type, param_type, &HashMap::new()) {
                                if arg_type == param_type {
                                    score.exact_matches += 1;
                                } else {
                                    score.coerce_matches += 1;
                                }
                            } else {
                                score.incompatible += 1;
                                return score;
                            }
                        }
                    }
                } else if is_varargs {
                    score.coerce_matches += 1;
                } else {
                    score.incompatible += 1;
                    return score;
                }
            } else {
                score.coerce_matches += 1;
            }
        }
        if let Some(expected) = expected_return {
            let ret_str = type_shape_to_string(&candidate.return_type);
            if compatibility.is_compatible(&ret_str, expected) != CompatibilityLevel::Incompatible
                || is_assignable(
                    &TypeShape::Named(expected.to_string()),
                    &candidate.return_type,
                    &HashMap::new(),
                )
                || is_assignable(
                    &candidate.return_type,
                    &TypeShape::Named(expected.to_string()),
                    &HashMap::new(),
                )
            {
                score.return_type_match = true;
            }
        }
        score.specificity = candidate.specificity;
        score
    }

    /// Enhanced resolution returning both candidate and score for external ranking.
    pub fn resolve_with_score_detailed(
        &self,
        arg_types: &[Option<TypeShape>],
        expected_return: Option<&TypeShape>,
        language: Language,
    ) -> Option<(EntityId, OverloadScore)> {
        let arg_refs: Vec<Option<&TypeShape>> = arg_types.iter().map(|o| o.as_ref()).collect();
        let expected_str = expected_return.map(type_shape_to_string);
        self.resolve_with_score(&arg_refs, expected_str.as_deref(), language)
            .map(|c| {
                let score = Self::score_candidate(c, &arg_refs, expected_str.as_deref(), language);
                (c.entity_id, score)
            })
    }
}

/// Check if `actual` is assignable to `expected` (with generic resolution).
pub fn is_assignable(
    actual: &TypeShape,
    expected: &TypeShape,
    type_params: &HashMap<String, String>,
) -> bool {
    let shape_bindings: HashMap<String, TypeShape> = type_params
        .iter()
        .map(|(param, bound)| (param.clone(), TypeShape::Named(bound.clone())))
        .collect();
    is_assignable_with_shapes(actual, expected, &shape_bindings)
}

/// Check if `actual` is assignable to `expected` with structured bindings.
///
/// This is the single structural implementation behind generic-aware
/// assignability: type parameters resolve against parsed shapes, unbound
/// parameters stay unassignable, and union handling recurses structurally.
pub fn is_assignable_with_shapes(
    actual: &TypeShape,
    expected: &TypeShape,
    type_params: &HashMap<String, TypeShape>,
) -> bool {
    if actual == expected {
        return true;
    }
    // Handle TypeShape::Param: check against shape bindings
    if let TypeShape::Param(param_name) = expected {
        if let Some(bound) = type_params.get(param_name) {
            return actual == bound;
        }
    }
    // Handle generic params in Named form (legacy): if expected is a type param name
    if let TypeShape::Named(exp_name) = expected {
        if let Some(bound) = type_params.get(exp_name) {
            return actual == bound;
        }
    }
    // Handle TypeShape::Param on actual side: a param matches if it's the same param
    if let (TypeShape::Param(a), TypeShape::Param(e)) = (actual, expected) {
        return a == e;
    }
    // Union handling
    match (actual, expected) {
        (TypeShape::Named(a), TypeShape::Union(members)) => members.iter().any(|m| {
            if let TypeShape::Named(n) = m {
                n == a
            } else {
                false
            }
        }),
        (TypeShape::Union(actual_members), TypeShape::Named(_)) => actual_members
            .iter()
            .any(|m| is_assignable_with_shapes(m, expected, type_params)),
        (TypeShape::Param(p), TypeShape::Union(members)) => {
            if let Some(bound) = type_params.get(p) {
                members.iter().any(|m| m == bound)
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Render an overload candidate as an annotated signature.
///
/// Produces `name(param, ...) -> Return` so resolution call sites can
/// record the dispatched signature alongside the resolved entity.
pub fn format_overload_signature(name: &str, candidate: &OverloadCandidate) -> String {
    let params = candidate
        .parameter_types
        .iter()
        .map(type_shape_to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{}({}) -> {}",
        name,
        params,
        type_shape_to_string(&candidate.return_type)
    )
}

/// Compute specificity score for a parameter type (higher = more specific).
pub fn compute_specificity(shape: &TypeShape) -> u32 {
    match shape {
        TypeShape::Named(s) => {
            if s.len() == 1 && s.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                1 // generic param least specific
            } else {
                10
            }
        }
        TypeShape::Param(_) => 1,
        TypeShape::Wildcard { .. } => 1,
        TypeShape::Generic { args, .. } => 5 + args.len() as u32,
        TypeShape::Union(members) => 2 + members.len() as u32,
        TypeShape::Intersection(members) => 3 + members.len() as u32,
        TypeShape::Array(inner) => 5 + compute_specificity(inner),
        TypeShape::Reference { inner, .. } => compute_specificity(inner),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(s: &str) -> TypeShape {
        TypeShape::Named(s.to_string())
    }

    #[test]
    fn test_overload_resolve_exact() {
        let mut set = OverloadSet::new("parse".to_string(), "Parser".to_string());
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(1),
            parameter_types: vec![named("string")],
            return_type: named("number"),
            specificity: 10,
        });
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(2),
            parameter_types: vec![named("number")],
            return_type: named("string"),
            specificity: 10,
        });

        let arg = named("string");
        let resolved = set.resolve(&[Some(&arg)]).unwrap();
        assert_eq!(resolved.entity_id, EntityId(1));

        let arg2 = named("number");
        let resolved2 = set.resolve(&[Some(&arg2)]).unwrap();
        assert_eq!(resolved2.entity_id, EntityId(2));
    }

    #[test]
    fn test_overload_no_match() {
        let mut set = OverloadSet::new("foo".to_string(), "Bar".to_string());
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(1),
            parameter_types: vec![named("string")],
            return_type: named("void"),
            specificity: 10,
        });
        let arg = named("number");
        let resolved = set.resolve(&[Some(&arg)]);
        // string expected but number provided -> not assignable unless generic
        assert!(resolved.is_none());
    }

    #[test]
    fn test_overload_arity_mismatch() {
        let mut set = OverloadSet::new("foo".to_string(), "Bar".to_string());
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(1),
            parameter_types: vec![named("string"), named("number")],
            return_type: named("void"),
            specificity: 10,
        });
        let arg = named("string");
        let resolved = set.resolve(&[Some(&arg)]);
        assert!(resolved.is_none());
    }

    #[test]
    fn test_is_assignable_exact() {
        let a = named("string");
        let b = named("string");
        assert!(is_assignable(&a, &b, &HashMap::new()));
    }

    #[test]
    fn test_is_assignable_union() {
        let actual = named("string");
        let expected = TypeShape::Union(vec![named("string"), named("number")]);
        assert!(is_assignable(&actual, &expected, &HashMap::new()));
    }

    #[test]
    fn test_is_assignable_unbound_generic_param() {
        let actual = named("string");
        let expected = named("T");
        // Unbound type parameters are NOT assignable without a bound mapping
        assert!(!is_assignable(&actual, &expected, &HashMap::new()));
    }

    #[test]
    fn test_is_assignable_bound_generic_param() {
        let actual = named("string");
        let expected = named("T");
        let mut type_params = HashMap::new();
        type_params.insert("T".to_string(), "string".to_string());
        // Bound type parameter matches when bound equals actual
        assert!(is_assignable(&actual, &expected, &type_params));
    }

    #[test]
    fn test_specificity() {
        let generic_param = named("T");
        let concrete = named("String");
        assert!(compute_specificity(&concrete) > compute_specificity(&generic_param));
    }

    #[test]
    fn test_is_assignable_param_variant() {
        let actual = named("string");
        let expected = TypeShape::Param("T".to_string());
        let mut type_params = HashMap::new();
        type_params.insert("T".to_string(), "string".to_string());
        assert!(is_assignable(&actual, &expected, &type_params));
    }

    #[test]
    fn test_is_assignable_unbound_param_variant() {
        let actual = named("string");
        let expected = TypeShape::Param("T".to_string());
        assert!(!is_assignable(&actual, &expected, &HashMap::new()));
    }

    #[test]
    fn test_is_assignable_param_to_param() {
        let a = TypeShape::Param("T".to_string());
        let b = TypeShape::Param("T".to_string());
        assert!(is_assignable(&a, &b, &HashMap::new()));
        let c = TypeShape::Param("U".to_string());
        assert!(!is_assignable(&a, &c, &HashMap::new()));
    }

    #[test]
    fn test_resolve_with_generics() {
        let mut set = OverloadSet::new("parse".to_string(), "Parser".to_string());
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(1),
            parameter_types: vec![TypeShape::Param("T".to_string())],
            return_type: TypeShape::Param("T".to_string()),
            specificity: 5,
        });
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(2),
            parameter_types: vec![named("number")],
            return_type: named("string"),
            specificity: 10,
        });

        let arg = named("string");
        let mut type_params = HashMap::new();
        type_params.insert("T".to_string(), "string".to_string());
        let resolved = set.resolve_with_generics(&[Some(&arg)], &type_params);
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().entity_id, EntityId(1));
    }

    #[test]
    fn test_resolve_with_args() {
        let mut set = OverloadSet::new("compute".to_string(), "Math".to_string());
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(10),
            parameter_types: vec![named("int")],
            return_type: named("int"),
            specificity: 10,
        });
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(11),
            parameter_types: vec![named("string")],
            return_type: named("string"),
            specificity: 10,
        });

        let arg_int = named("int");
        let resolved = set.resolve_with_args(&[Some(&arg_int)], &HashMap::new());
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().entity_id, EntityId(10));

        let arg_str = named("string");
        let resolved2 = set.resolve_with_args(&[Some(&arg_str)], &HashMap::new());
        assert_eq!(resolved2.unwrap().entity_id, EntityId(11));

        // Unknown arg type should still return a candidate (neutral scoring)
        let resolved_unknown = set.resolve_with_args(&[None], &HashMap::new());
        assert!(resolved_unknown.is_some());
    }

    #[test]
    fn test_resolve_with_args_generic() {
        let mut set = OverloadSet::new("process".to_string(), "Processor".to_string());
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(1),
            parameter_types: vec![TypeShape::Param("T".to_string())],
            return_type: named("void"),
            specificity: 5,
        });
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(2),
            parameter_types: vec![named("string")],
            return_type: named("void"),
            specificity: 10,
        });
        let arg = named("string");
        let mut params = HashMap::new();
        params.insert("T".to_string(), "string".to_string());
        let resolved = set.resolve_with_args(&[Some(&arg)], &params);
        assert!(resolved.is_some());
    }

    #[test]
    fn test_format_overload_signature() {
        let candidate = OverloadCandidate {
            entity_id: EntityId(1),
            parameter_types: vec![named("string"), named("number")],
            return_type: named("boolean"),
            specificity: 10,
        };
        assert_eq!(
            format_overload_signature("parse", &candidate),
            "parse(string, number) -> boolean"
        );
    }

    #[test]
    fn test_resolve_with_signature_annotates_winner() {
        let mut set = OverloadSet::new("parse".to_string(), "Parser".to_string());
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(1),
            parameter_types: vec![named("string")],
            return_type: named("number"),
            specificity: 10,
        });
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(2),
            parameter_types: vec![named("number")],
            return_type: named("string"),
            specificity: 10,
        });
        let arg = named("string");
        let (entity_id, signature) = set
            .resolve_with_signature(&[Some(&arg)])
            .expect("resolution succeeds");
        assert_eq!(entity_id, EntityId(1));
        assert_eq!(signature, "parse(string) -> number");
    }

    #[test]
    fn test_resolve_with_signature_empty_set_is_none() {
        let set = OverloadSet::new("parse".to_string(), "Parser".to_string());
        let arg = named("string");
        assert!(set.resolve_with_signature(&[Some(&arg)]).is_none());
    }

    #[test]
    fn test_is_assignable_with_shapes_structural_bound() {
        let actual = TypeShape::Generic {
            base: "Vec".to_string(),
            args: vec![named("string")],
        };
        let expected = TypeShape::Param("T".to_string());
        let mut bindings = HashMap::new();
        bindings.insert("T".to_string(), actual.clone());
        assert!(is_assignable_with_shapes(&actual, &expected, &bindings));
        assert!(!is_assignable_with_shapes(
            &named("int"),
            &expected,
            &bindings
        ));
    }

    #[test]
    fn test_resolve_with_shape_generics_prefers_bound_candidate() {
        let mut set = OverloadSet::new("parse".to_string(), "Parser".to_string());
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(1),
            parameter_types: vec![TypeShape::Param("T".to_string())],
            return_type: TypeShape::Param("T".to_string()),
            specificity: 5,
        });
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(2),
            parameter_types: vec![named("number")],
            return_type: named("string"),
            specificity: 10,
        });
        let arg = named("string");
        let mut bindings = HashMap::new();
        bindings.insert("T".to_string(), named("string"));
        let resolved = set.resolve_with_shape_generics(&[Some(&arg)], &bindings);
        assert!(resolved.is_some());
        assert_eq!(resolved.expect("candidate").entity_id, EntityId(1));
    }

    #[test]
    fn test_shape_bindings_drive_instantiation() {
        use crate::type_inference::types::{
            build_shape_bindings, instantiate_type_shape, parse_type_shape, type_shape_to_string,
        };
        use cce_types::language::Language;
        let declared = parse_type_shape("Pair[T, String]", Language::Java).expect("declared shape");
        let actual = named("Integer");
        let bindings = build_shape_bindings(&["T".to_string()], &[actual]);
        let instantiated = instantiate_type_shape(&declared, &bindings);
        assert_eq!(type_shape_to_string(&instantiated), "Pair<Integer, String>");
    }
}
