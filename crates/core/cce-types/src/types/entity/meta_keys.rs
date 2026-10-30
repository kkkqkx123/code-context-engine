//! Centralized metadata key constants
//!
//! All entity producers (parsers, extractors, processors) and consumers
//! (templates, relationship processors) must reference these constants
//! instead of raw string literals to ensure consistency.

pub const AUTO_TRAITS: &str = "auto_traits";

pub const IMPL_SOURCE: &str = "impl_source";

pub const ANNOTATIONS: &str = "annotations";

/// Conditional-compilation predicates (`#[cfg(...)]` / `#[cfg_attr(...)]`) that
/// gate an entity. Stored separately from the general `annotations` bag so the
/// stable symbol identity can fold the compilation condition that selects this
/// definition among mutually-exclusive `cfg` variants.
pub const CFG_PREDICATE: &str = "cfg_predicate";

/// Zero-based source-order index among siblings that share the same resolved
/// scoped name and kind. The stable symbol key does not hash the signature, so
/// both rebound names (a test function defining `class Module` twice) and true
/// overloads (same name, distinct signatures) need the ordinal to stay
/// addressable on the stable symbol key. Absent for entities without such
/// duplicates.
pub const DUPLICATE_SIBLING_ORDINAL: &str = "duplicate_sibling_ordinal";

pub const INHERENT_IMPL_COUNT: &str = "inherent_impl_count";

pub const CALL_PATHS: &str = "call_paths";

/// Same-file resolved call targets (subset of [`CALL_PATHS`]).
///
/// Populated by the grouper via `LocalCallResolver`: every name listed here
/// resolved to an internal entity in the same file, so it is by definition
/// distinctive even when its spelling collides with stdlib/boilerplate.
/// Call pruning keeps resolved names unconditionally, mirroring the relation
/// layer where resolution runs before stdlib filtering. Cross-file
/// resolution happens later in the codegraph index builder and is not
/// visible here; that gap is conservative (keeps pruning name-based).
pub const RESOLVED_CALLS: &str = "resolved_calls";

pub const TYPE_BOUNDS: &str = "type_bounds";

pub const BASE_CLASSES: &str = "base_classes";

pub const PARAM_DEFAULTS: &str = "param_defaults";

pub const IMPL_SOURCE_INHERENT: &str = "inherent";
