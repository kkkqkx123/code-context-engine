//! Core domain types for the code context engine

pub mod build_system;
pub mod path;
pub mod serialization;
pub mod types;

pub use build_system::{
    BuildSystemMetadata, MANIFEST_SCAN_EXCLUDED_DIRS, all_build_config_file_names,
    canonicalize_package_name, get_affected_extensions, get_supported_build_systems,
    imports_match_package, is_build_config, is_build_config_name, is_build_config_name_lower,
};
pub use path::{
    extension_lower, file_name_str, group_id_base, is_non_utf8, normalize_project_path,
    normalized_equals, relativize, segments, stable_path_id,
};
pub use serialization::{SerializationError, deserialize_from_cache, serialize_for_cache};
pub use types::*;
