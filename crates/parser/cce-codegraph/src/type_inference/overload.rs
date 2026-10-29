//! Function overload resolution framework.
//!
//! Provides structures for grouping same-name methods on a type into an
//! overload set and selecting the best matching overload based on argument
//! types and specificity scoring.

use std::collections::HashMap;

use cce_types::entity::EntityId;
use cce_types::language::{Language, effective_language};

pub use super::compatibility::{
    CompatibilityLevel, TypeCompatibility, is_assignable, is_assignable_with_shapes,
};
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

/// Per-entity parameter arity snapshot.
///
/// Recorded at extraction time from the declared parameters (names,
/// defaults and rest markers), so overload resolution judges call arity
/// without re-deriving syntax per language. `known == false` means no
/// extraction record exists (hand-built test contexts, legacy caches):
/// arity filtering stays neutral instead of guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParamArity {
    /// Declared parameter count, including untyped entries.
    pub total: usize,
    /// Minimum accepted argument count (defaults and rest markers discounted).
    pub required: usize,
    /// Whether the last parameter collects the remaining arguments.
    pub is_varargs: bool,
    /// Whether this record came from real extraction data.
    pub known: bool,
}

impl ParamArity {
    /// Neutral record for candidates without extraction data.
    pub fn unknown() -> Self {
        Self::default()
    }

    /// Exact record for a fixed-arity parameter list.
    pub fn exact(total: usize) -> Self {
        Self {
            total,
            required: total,
            is_varargs: false,
            known: true,
        }
    }

    /// Whether a call site with `arg_count` arguments may dispatch here.
    pub fn accepts(&self, arg_count: usize) -> bool {
        if !self.known {
            return true;
        }
        if arg_count < self.required {
            return false;
        }
        if self.is_varargs {
            return true;
        }
        arg_count <= self.total
    }
}

/// Whether one declared parameter is a rest collector.
///
/// Detection is per language because the marker lives in different places:
/// name prefixes (`*args`, `...args`, `&block`), name keywords (`vararg`),
/// or type spellings (`String...`, `T*`, `...int`, `params int[]`).
/// Languages without rest syntax (Dart optional parameters are
/// default-driven, templates and data formats carry no signatures) answer
/// false. Remapped plugin languages resolve to their host language first;
/// native plugin languages stay conservative.
///
/// Callers pass the raw declared name and type text; both are trimmed here.
/// Either side may be absent (untyped entries keep their name marker).
/// Precision-first: unknown spellings answer false.
pub fn is_varargs_param(name: &str, type_text: Option<&str>, language: Language) -> bool {
    let language = effective_language(&language);
    let name = name.trim();
    let ty = type_text.map(str::trim).unwrap_or("");
    match language {
        Language::Python => name.starts_with('*'),
        Language::Rust => name == "...",
        Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
            name.starts_with("...")
        }
        Language::Java => name.starts_with("...") || ty.ends_with("..."),
        Language::Kotlin => {
            name.strip_prefix("vararg ").is_some() || ty.strip_prefix("vararg ").is_some()
        }
        Language::Scala => ty.ends_with('*'),
        Language::CSharp => ty == "params" || ty.strip_prefix("params ").is_some(),
        Language::Go => ty.starts_with("..."),
        Language::C | Language::Cpp => name == "..." || ty.ends_with("..."),
        Language::Php => name.starts_with("..."),
        Language::Ruby => name.starts_with('*') || name.starts_with('&'),
        Language::Lua => name == "...",
        _ => false,
    }
}

/// Derive the arity record for one extracted function entity.
///
/// `total` counts every declared entry (typed or not); `required`
/// discounts defaulted entries and a trailing rest collector. Default
/// counts come from the extractor-recorded defaults metadata; a missing
/// or unreadable payload simply discounts nothing.
///
/// Only the last entry can be a rest collector; a rest marker anywhere
/// else is ignored so malformed declarations stay conservative.
/// Every call site records, including zero-parameter functions, so
/// `known` always holds for extracted entities.
pub fn entity_param_arity(entity: &cce_types::Entity, language: Language) -> ParamArity {
    let total = entity.parameters.len();
    let defaulted = entity
        .metadata
        .get(cce_types::entity::meta_keys::PARAM_DEFAULTS)
        .and_then(|encoded| serde_json::from_str::<serde_json::Value>(encoded).ok())
        .and_then(|value| value.as_array().map(std::vec::Vec::len))
        .unwrap_or(0)
        .min(total);
    let last_is_varargs = entity
        .parameters
        .last()
        .is_some_and(|(name, ty)| is_varargs_param(name, ty.as_deref(), language));
    ParamArity {
        total,
        required: total
            .saturating_sub(defaulted)
            .saturating_sub(usize::from(last_is_varargs)),
        is_varargs: last_is_varargs,
        known: true,
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
    /// Extraction-time arity record; unknown stays neutral on arity.
    pub arity: ParamArity,
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
        // 1. Filter: arity record must accept the call (unknown stays neutral)
        let mut filtered: Vec<&OverloadCandidate> = self
            .candidates
            .iter()
            .filter(|c| c.arity.accepts(arg_types.len()))
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
            .filter(|c| c.arity.accepts(arg_types.len()))
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
            .filter(|c| c.arity.accepts(arg_types.len()))
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
        let arg_count = arg_types.len();
        // Arity comes from the extraction-time record (per-language rest
        // markers plus defaults); unknown records stay neutral here.
        if !candidate.arity.accepts(arg_count) {
            score.incompatible += 1;
            return score;
        }
        let is_varargs = candidate.arity.is_varargs;
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
                } else if is_varargs || (candidate.arity.known && i < candidate.arity.total) {
                    // Extra arguments collected by a rest parameter, or
                    // positions the extractor left untyped: neutral instead
                    // of a false mismatch.
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
        TypeShape::Function { params, .. } => {
            5 + params.iter().map(compute_specificity).sum::<u32>()
        }
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
            arity: ParamArity::exact(1),
        });
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(2),
            parameter_types: vec![named("number")],
            return_type: named("string"),
            specificity: 10,
            arity: ParamArity::exact(1),
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
            arity: ParamArity::exact(1),
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
            arity: ParamArity::exact(2),
        });
        let arg = named("string");
        let resolved = set.resolve(&[Some(&arg)]);
        assert!(resolved.is_none());
    }

    #[test]
    fn test_specificity() {
        let generic_param = named("T");
        let concrete = named("String");
        assert!(compute_specificity(&concrete) > compute_specificity(&generic_param));
    }

    #[test]
    fn test_resolve_with_generics() {
        let mut set = OverloadSet::new("parse".to_string(), "Parser".to_string());
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(1),
            parameter_types: vec![TypeShape::Param("T".to_string())],
            return_type: TypeShape::Param("T".to_string()),
            specificity: 5,
            arity: ParamArity::exact(1),
        });
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(2),
            parameter_types: vec![named("number")],
            return_type: named("string"),
            specificity: 10,
            arity: ParamArity::exact(1),
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
            arity: ParamArity::exact(1),
        });
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(11),
            parameter_types: vec![named("string")],
            return_type: named("string"),
            specificity: 10,
            arity: ParamArity::exact(1),
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
            arity: ParamArity::exact(1),
        });
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(2),
            parameter_types: vec![named("string")],
            return_type: named("void"),
            specificity: 10,
            arity: ParamArity::exact(1),
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
            arity: ParamArity::exact(2),
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
            arity: ParamArity::exact(1),
        });
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(2),
            parameter_types: vec![named("number")],
            return_type: named("string"),
            specificity: 10,
            arity: ParamArity::exact(1),
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
            arity: ParamArity::exact(1),
        });
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(2),
            parameter_types: vec![named("number")],
            return_type: named("string"),
            specificity: 10,
            arity: ParamArity::exact(1),
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

    #[test]
    fn test_param_arity_accepts_bounds() {
        assert!(ParamArity::exact(2).accepts(2));
        assert!(!ParamArity::exact(2).accepts(1));
        assert!(!ParamArity::exact(2).accepts(3));
        assert!(ParamArity::exact(0).accepts(0));
        assert!(!ParamArity::exact(0).accepts(1));
        assert!(ParamArity::unknown().accepts(0));
        assert!(ParamArity::unknown().accepts(7));
        let varargs = ParamArity {
            total: 2,
            required: 1,
            is_varargs: true,
            known: true,
        };
        assert!(!varargs.accepts(0));
        assert!(varargs.accepts(1));
        assert!(varargs.accepts(2));
        assert!(varargs.accepts(9));
        let optional = ParamArity {
            total: 2,
            required: 1,
            is_varargs: false,
            known: true,
        };
        assert!(!optional.accepts(0));
        assert!(optional.accepts(1));
        assert!(optional.accepts(2));
        assert!(!optional.accepts(3));
    }

    /// Sync guard: every parsable language owns a conscious rest-parameter
    /// decision. Add a positive sample (extend the per-language arm) or an
    /// explicit exact entry when extending `Language::ast_parsable_languages`.
    #[test]
    fn test_is_varargs_param_covers_all_parsable_languages() {
        use cce_types::language::Language;
        // (language, declared name, declared type): each must read varargs.
        let positives: &[(Language, &str, Option<&str>)] = &[
            (Language::Python, "*args", None),
            (Language::Python, "**kwargs", None),
            (Language::Rust, "...", None),
            (Language::JavaScript, "...args", None),
            (Language::TypeScript, "...args", Some("any[]")),
            (Language::Jsx, "...args", None),
            (Language::Tsx, "...args", None),
            (Language::Java, "args", Some("String...")),
            (Language::Java, "...args", None),
            (Language::Kotlin, "vararg args", Some("Int")),
            (Language::Scala, "args", Some("String*")),
            (Language::CSharp, "args", Some("params int[]")),
            (Language::Go, "args", Some("...int")),
            (Language::C, "...", None),
            (Language::Cpp, "...", None),
            (Language::Cpp, "args", Some("Args...")),
            (Language::Php, "...$args", None),
            (Language::Ruby, "*args", None),
            (Language::Ruby, "&block", None),
            (Language::Lua, "...", None),
        ];
        // Languages without rest syntax stay exact. Dart optional parameters
        // are default-driven, templates carry no signatures, Bash
        // overloading is out of scope.
        let exact: &[Language] = &[
            Language::Dart,
            Language::Bash,
            Language::Html,
            Language::Css,
            Language::Vue,
            Language::Svelte,
            Language::Ejs,
        ];
        for language in Language::ast_parsable_languages() {
            let has_positive = positives.iter().any(|(lang, _, _)| lang == language);
            if has_positive {
                continue;
            }
            assert!(
                exact.contains(language),
                "{language:?} is parsable but has no varargs decision: \
                 add a positive sample or an explicit exact entry"
            );
        }
        for (language, name, ty) in positives {
            assert!(
                is_varargs_param(name, *ty, *language),
                "{name:?} ({ty:?}) must read as varargs for {language:?}"
            );
        }
        // Near misses stay exact (precision over recall).
        let negatives: &[(Language, &str, Option<&str>)] = &[
            (Language::Python, "args", None),
            (Language::Rust, "args", Some("&[T]")),
            (Language::Java, "args", Some("String")),
            (Language::Scala, "args", Some("Int")),
            (Language::Go, "args", Some("int")),
            (Language::Ruby, "args", None),
            (Language::Dart, "args", Some("List<int>")),
            (Language::Bash, "$@", None),
            (Language::Html, "...args", None),
        ];
        for (language, name, ty) in negatives {
            assert!(
                !is_varargs_param(name, *ty, *language),
                "{name:?} ({ty:?}) must stay exact for {language:?}"
            );
        }
        // Native plugin languages stay conservative without host data.
        assert!(!is_varargs_param(
            "...args",
            None,
            Language::Custom(u32::MAX)
        ));
    }

    /// Remapped plugin languages inherit their host rest-parameter rule.
    #[test]
    fn test_is_varargs_param_remap_inherits_host() {
        use cce_types::language::{
            Language, clear_plugin_languages, register_plugin_language,
            register_remap_plugin_language,
        };
        clear_plugin_languages();
        let remapped = register_remap_plugin_language(
            "varargs-remap-guard",
            &["varg".to_string()],
            &Language::Java.to_string(),
        );
        let native = register_plugin_language("varargs-native-guard", &["varn".to_string()]);
        assert!(is_varargs_param(
            "args",
            Some("String..."),
            Language::Custom(remapped)
        ));
        assert!(!is_varargs_param(
            "args",
            Some("String..."),
            Language::Custom(native)
        ));
        clear_plugin_languages();
    }

    #[test]
    fn test_resolve_with_score_accepts_varargs_extras() {
        use cce_types::language::Language;
        // Java `void log(String... parts)`: one declared entry, rest open.
        let mut set = OverloadSet::new("log".to_string(), "Logger".to_string());
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(1),
            parameter_types: vec![named("String")],
            return_type: named("void"),
            specificity: 10,
            arity: ParamArity {
                total: 1,
                required: 0,
                is_varargs: true,
                known: true,
            },
        });
        for count in [0, 1, 3] {
            let args: Vec<Option<TypeShape>> = (0..count).map(|_| Some(named("String"))).collect();
            let refs: Vec<Option<&TypeShape>> = args.iter().map(|a| a.as_ref()).collect();
            let winner = set
                .resolve_with_score(&refs, None, Language::Java)
                .expect("varargs accepts any count");
            assert_eq!(winner.entity_id, EntityId(1));
        }
    }

    #[test]
    fn test_resolve_accepts_defaulted_arity_range() {
        // `f(a, b=1)`: one or two arguments dispatch, three do not.
        let mut set = OverloadSet::new("f".to_string(), String::new());
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(1),
            parameter_types: vec![named("int"), named("int")],
            return_type: named("void"),
            specificity: 10,
            arity: ParamArity {
                total: 2,
                required: 1,
                is_varargs: false,
                known: true,
            },
        });
        let one = named("int");
        assert!(set.resolve(&[Some(&one)]).is_some());
        let two = [Some(one.clone()), Some(one.clone())];
        let refs: Vec<Option<&TypeShape>> = two.iter().map(|a| a.as_ref()).collect();
        assert!(set.resolve(&refs).is_some());
        let three = [Some(one.clone()), Some(one.clone()), Some(one.clone())];
        let refs: Vec<Option<&TypeShape>> = three.iter().map(|a| a.as_ref()).collect();
        assert!(set.resolve(&refs).is_none());
    }

    #[test]
    fn test_rust_plain_params_stay_exact() {
        // Rust has no C-style variadics; only a bare `...` marker counts.
        // The previous language-wide leniency is intentionally removed.
        use cce_types::language::Language;
        let mut set = OverloadSet::new("f".to_string(), String::new());
        set.add_candidate(OverloadCandidate {
            entity_id: EntityId(1),
            parameter_types: vec![named("int"), named("int")],
            return_type: named("void"),
            specificity: 10,
            arity: ParamArity::exact(2),
        });
        let args = [named("int"), named("int"), named("int")];
        let refs: Vec<Option<&TypeShape>> = args.iter().map(Some).collect();
        assert!(
            set.resolve_with_score(&refs, None, Language::Rust)
                .is_none()
        );
    }

    #[test]
    fn test_entity_param_arity_counts_defaults_and_rest() {
        use cce_types::entity::meta_keys;
        use cce_types::language::Language;
        use cce_types::{Entity, EntityKind, Span};
        let mut rest = Entity::new(
            EntityId(1),
            EntityKind::Function,
            "f".to_string(),
            Span::default(),
        );
        rest.parameters = vec![
            ("a".to_string(), Some("int".to_string())),
            ("args".to_string(), Some("String...".to_string())),
        ];
        assert_eq!(
            entity_param_arity(&rest, Language::Java),
            ParamArity {
                total: 2,
                required: 1,
                is_varargs: true,
                known: true,
            }
        );
        let mut defaulted = Entity::new(
            EntityId(2),
            EntityKind::Function,
            "g".to_string(),
            Span::default(),
        );
        defaulted.parameters = vec![
            ("a".to_string(), Some("int".to_string())),
            ("b".to_string(), Some("int".to_string())),
        ];
        defaulted.metadata.insert(
            meta_keys::PARAM_DEFAULTS.to_string(),
            r#"[["b", "1"]]"#.to_string(),
        );
        assert_eq!(
            entity_param_arity(&defaulted, Language::Python),
            ParamArity {
                total: 2,
                required: 1,
                is_varargs: false,
                known: true,
            }
        );
        let bare = Entity::new(
            EntityId(3),
            EntityKind::Function,
            "h".to_string(),
            Span::default(),
        );
        assert_eq!(
            entity_param_arity(&bare, Language::Go),
            ParamArity {
                total: 0,
                required: 0,
                is_varargs: false,
                known: true,
            }
        );
    }
}
