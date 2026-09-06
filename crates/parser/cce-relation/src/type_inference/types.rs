//! Core type definitions for type inference.
//!
//! Submodules group related items by responsibility. This file only declares
//! the submodules and re-exports the public API so existing import paths keep
//! working.

pub mod binding;
pub mod context;
pub mod narrowing;
pub mod origin;
pub mod reference;
pub mod shape;

pub use binding::{NestedPatternPart, Pattern, TypeBinding, TypeConfidence, VariableTypeBinding};
pub use context::{ScopeFrame, ScopedTypeContext};
pub use narrowing::{
    BranchPolarity, add_polarity_aware_narrowings, declared_shape, else_branch_complement,
    fact_has_else_branch, is_falsy_type, narrow_discriminated_union, narrow_truthiness,
    subtract_union_members,
};
pub use origin::{
    AUTHORITATIVE_ORIGIN_THRESHOLD, InferenceOrigin, binding_supersedes, bindings_supersede,
    origin_is_authoritative, origin_priority, origin_priority_of, origin_supersedes,
};
pub use reference::{is_mut_reference, is_reference, strip_references};
pub use shape::{
    TypeShape, build_shape_bindings, element_type_at_depth, element_type_of_shape,
    instantiate_type_shape, parse_type_shape, python_canonical_literal_name, shape_is_subtype,
    shape_members, type_shape_to_string,
};
