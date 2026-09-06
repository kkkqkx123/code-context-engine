//! Inference origin tags and priority ranking.

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

/// Whether a candidate origin supersedes an existing binding.
///
/// Models the source-priority threshold: a new binding only replaces the
/// old one when its origin ranks at least as high, keeping deterministic
/// merges free of priority inversions.
pub fn origin_supersedes(candidate: InferenceOrigin, existing: Option<InferenceOrigin>) -> bool {
    origin_priority(Some(candidate)) >= origin_priority(existing)
}

/// Whether a candidate binding supersedes an existing binding.
///
/// Option-aware form of the supersession check so business modules never
/// compare priority numbers directly. An unknown candidate only replaces
/// an unknown existing binding; any known candidate replaces unknown.
pub fn binding_supersedes(
    candidate: Option<InferenceOrigin>,
    existing: Option<InferenceOrigin>,
) -> bool {
    match candidate {
        Some(origin) => origin_supersedes(origin, existing),
        None => existing.is_none(),
    }
}

/// Whether a set of new bindings supersedes an existing set.
///
/// Compares the highest priority on each side so multi-binding entries
/// (parameter lists) merge with the same threshold semantics as single
/// bindings without exposing numeric priorities to callers.
pub fn bindings_supersede(
    candidate: &[crate::type_inference::types::TypeBinding],
    existing: &[crate::type_inference::types::TypeBinding],
) -> bool {
    let best = |bindings: &[crate::type_inference::types::TypeBinding]| {
        bindings
            .iter()
            .map(|binding| origin_priority(binding.origin))
            .max()
            .unwrap_or(0)
    };
    best(candidate) >= best(existing)
}

/// Minimum priority whose bindings cross-file propagation must not override.
///
/// Covers declaration-level evidence (annotations and control-flow
/// narrowings); everything below remains eligible for propagation.
pub const AUTHORITATIVE_ORIGIN_THRESHOLD: u8 = 7;

/// Whether an origin ranks as authoritative evidence that propagation
/// must preserve.
pub fn origin_is_authoritative(origin: Option<InferenceOrigin>) -> bool {
    origin_priority(origin) >= AUTHORITATIVE_ORIGIN_THRESHOLD
}

#[cfg(test)]
mod tests {

    use super::*;

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

    #[test]
    fn test_origin_supersedes_threshold() {
        assert!(origin_supersedes(
            InferenceOrigin::TypeAnnotation,
            Some(InferenceOrigin::FunctionReturn)
        ));
        assert!(origin_supersedes(
            InferenceOrigin::DestructuringAssignment,
            Some(InferenceOrigin::DestructuringAssignment)
        ));
        assert!(!origin_supersedes(
            InferenceOrigin::LiteralType,
            Some(InferenceOrigin::TypeAnnotation)
        ));
        assert!(origin_supersedes(InferenceOrigin::LiteralType, None));
    }

    #[test]
    fn test_origin_is_authoritative_threshold() {
        assert!(origin_is_authoritative(Some(
            InferenceOrigin::TypeAnnotation
        )));
        assert!(origin_is_authoritative(Some(
            InferenceOrigin::ControlFlowNarrowing
        )));
        assert!(!origin_is_authoritative(Some(
            InferenceOrigin::FunctionReturn
        )));
        assert!(!origin_is_authoritative(None));
        assert_eq!(AUTHORITATIVE_ORIGIN_THRESHOLD, 7);
    }

    #[test]
    fn test_binding_supersedes_option_aware() {
        use crate::type_inference::types::TypeBinding;
        let annotated = TypeBinding {
            origin: Some(InferenceOrigin::TypeAnnotation),
            ..Default::default()
        };
        let literal = TypeBinding {
            origin: Some(InferenceOrigin::LiteralType),
            ..Default::default()
        };
        let unknown = TypeBinding {
            origin: None,
            ..Default::default()
        };
        assert!(binding_supersedes(annotated.origin, literal.origin));
        assert!(binding_supersedes(literal.origin, literal.origin));
        assert!(!binding_supersedes(literal.origin, annotated.origin));
        assert!(binding_supersedes(literal.origin, unknown.origin));
        assert!(!binding_supersedes(unknown.origin, literal.origin));
        assert!(binding_supersedes(unknown.origin, unknown.origin));
    }

    #[test]
    fn test_bindings_supersede_compares_best_origins() {
        use crate::type_inference::types::TypeBinding;
        let high = TypeBinding {
            origin: Some(InferenceOrigin::TypeAnnotation),
            ..Default::default()
        };
        let low = TypeBinding {
            origin: Some(InferenceOrigin::LiteralType),
            ..Default::default()
        };
        assert!(bindings_supersede(
            std::slice::from_ref(&high),
            std::slice::from_ref(&low)
        ));
        assert!(bindings_supersede(
            std::slice::from_ref(&low),
            std::slice::from_ref(&low)
        ));
        assert!(!bindings_supersede(
            std::slice::from_ref(&low),
            std::slice::from_ref(&high)
        ));
        assert!(bindings_supersede(
            &[low.clone(), high.clone()],
            std::slice::from_ref(&high)
        ));
    }
}
