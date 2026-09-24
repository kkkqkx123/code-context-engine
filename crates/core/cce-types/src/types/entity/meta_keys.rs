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

pub const INHERENT_IMPL_COUNT: &str = "inherent_impl_count";

pub const CALL_PATHS: &str = "call_paths";

pub const TYPE_BOUNDS: &str = "type_bounds";

pub const BASE_CLASSES: &str = "base_classes";

pub const PARAM_DEFAULTS: &str = "param_defaults";

pub const IMPL_SOURCE_INHERENT: &str = "inherent";
