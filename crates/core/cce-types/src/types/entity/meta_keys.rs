//! Centralized metadata key constants
//!
//! All entity producers (parsers, extractors, processors) and consumers
//! (templates, relationship processors) must reference these constants
//! instead of raw string literals to ensure consistency.

pub const AUTO_TRAITS: &str = "auto_traits";

pub const IMPL_SOURCE: &str = "impl_source";

pub const ANNOTATIONS: &str = "annotations";

pub const INHERENT_IMPL_COUNT: &str = "inherent_impl_count";

pub const CALL_PATHS: &str = "call_paths";

pub const TYPE_BOUNDS: &str = "type_bounds";

pub const BASE_CLASSES: &str = "base_classes";

pub const IMPL_SOURCE_INHERENT: &str = "inherent";
