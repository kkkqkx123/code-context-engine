//! Project-relative path utilities.
//!
//! Every storage and identity boundary in the system (SQLite keys, Qdrant
//! payloads and point IDs, BM25 document IDs, caches, relation snapshots)
//! uses one canonical string form: forward-slash separated, UTF-8,
//! project-relative. This module is the single implementation of that
//! representation; all other normalization copies live here.
//!
//! # Versioning
//!
//! The path normalization algorithm is versioned via `PATH_NORMALIZATION_VERSION`.
//! When modifying the normalization logic:
//! 1. Bump `PATH_NORMALIZATION_VERSION`
//! 2. Plan storage migration for all affected boundaries:
//!    - SQLite files table (path column)
//!    - SQLite chunks table (file_path column)
//!    - BM25 document IDs
//!    - Qdrant point payloads
//!    - Relation snapshots (already uses `RELATION_PATH_NORMALIZATION_VERSION`)
//!    - Cache keys
//! 3. Update `RELATION_PATH_NORMALIZATION_VERSION` in `types::relation::canonical`
//!    to maintain consistency

use std::path::Path;

/// Version of the path normalization algorithm.
///
/// Bump this when changing `normalize_project_path()` or `relativize()`.
/// All storage boundaries using normalized paths must be migrated when
/// this version changes.
pub const PATH_NORMALIZATION_VERSION: u32 = 1;

use sha2::{Digest, Sha256};

/// Normalize a project-relative path for every relationship identity boundary.
///
/// Converts `\` to `/`, collapses `.` and empty segments, resolves `..`
/// lexically (not via the filesystem), and re-adds a leading `/` only for
/// absolute paths.
///
/// The algorithm is versioned via `PATH_NORMALIZATION_VERSION` in this module;
/// never change it without bumping the version and planning a storage migration
/// for all affected boundaries (SQLite, BM25, Qdrant, relation snapshots, caches).
pub fn normalize_project_path(path: &str) -> String {
    let replaced = path.replace('\\', "/");
    let is_absolute = replaced.starts_with('/');
    let mut components: Vec<&str> = Vec::new();
    for component in replaced.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.last().is_some_and(|part| *part != "..") {
                    components.pop();
                } else {
                    components.push(component);
                }
            }
            _ => components.push(component),
        }
    }
    let normalized = components.join("/");
    if is_absolute {
        format!("/{normalized}")
    } else {
        normalized
    }
}

/// Express `path` relative to `root` using component-safe prefix stripping,
/// then normalize to the canonical project-relative form.
///
/// Paths outside the root fall back to the normalized raw path (with a
/// warning, as they break the project-relative path identity).
///
/// # Warning
///
/// When a path is outside the scan root, the returned string is NOT a valid
/// project-relative identifier. This can cause unexpected behavior in storage
/// operations (SQLite, Qdrant, BM25) that expect project-relative paths.
/// Callers should ensure paths are within the scan root whenever possible.
pub fn relativize(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or_else(|_| {
        tracing::warn!(
            path = %path.display(),
            root = %root.display(),
            "Path is outside the scan root; relative path falls back to the raw path"
        );
        path
    });
    normalize_project_path(&relative.to_string_lossy())
}

/// File name component of a path string, handling both `/` and `\`
/// separators. Returns the whole string when no separator is present.
pub fn file_name_str(path: &str) -> &str {
    match path.rfind(['/', '\\']) {
        Some(idx) => &path[idx + 1..],
        None => path,
    }
}

/// Split a path string into segments, handling both separators. Empty
/// segments (from leading, trailing or doubled separators) are skipped.
pub fn segments(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).filter(|s| !s.is_empty()).collect()
}

/// Lowercased file extension, handling both separators.
pub fn extension_lower(path: &str) -> Option<String> {
    let name = file_name_str(path);
    let (_, ext) = name.rsplit_once('.')?;
    if ext.is_empty() {
        return None;
    }
    Some(ext.to_lowercase())
}

/// Strict path equality after normalization.
///
/// Only a leading-slash (absolute vs relative) discrepancy is tolerated, by
/// comparing the full normalized strings with the leading `/` stripped from
/// either side. There is deliberately no suffix fallback: `a/b/c` and `b/c`
/// are different paths.
pub fn normalized_equals(left: &str, right: &str) -> bool {
    let normalized_left = normalize_project_path(left);
    let normalized_right = normalize_project_path(right);
    normalized_left == normalized_right
        || normalized_left.trim_start_matches('/') == normalized_right
        || normalized_left == normalized_right.trim_start_matches('/')
}

/// Stable, cross-toolchain SHA-256 identity of a path string.
///
/// Unlike `std::collections::hash_map::DefaultHasher` (whose output is not
/// guaranteed stable across Rust releases), this may be persisted in storage
/// identifiers such as BM25 document IDs.
pub fn stable_path_id(path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    hex::encode(hasher.finalize())
}

/// Whether an OS-level path contains bytes that are not valid UTF-8.
///
/// Such paths cannot be persisted losslessly and must be rejected at scan
/// boundaries instead of being mangled by `to_string_lossy` into a storage
/// key (SQLite `UNIQUE(project_id, epoch, path)`, Qdrant point IDs).
pub fn is_non_utf8(path: &Path) -> bool {
    path.as_os_str().to_str().is_none()
}

/// Group-ID base derived from a path string with an injective encoding.
///
/// Literal underscores are doubled first, then `/` and `\` separators are
/// collapsed to a single underscore. Because the separator underscore can
/// only ever be single while literal underscores are always even-length
/// runs, distinct paths can never collide (e.g. `a/b.c` → `a_b.c` and
/// `a_b.c` → `a__b.c` stay distinct). This keeps group IDs — which flow
/// into chunk IDs and Qdrant point IDs — stable across platforms without
/// sacrificing uniqueness.
pub fn group_id_base(path: &str) -> String {
    let escaped = path.replace('_', "__");
    escaped.replace(['/', '\\'], "_")
}

/// Canonical build/config file names are now defined in
/// `crate::build_system` (single source of truth). These helpers delegate
/// there so every consumer shares the same rule set.
/// Check a file name (as reported by the filesystem) against the canonical
/// build config file name rule set.
pub fn is_build_config_name(name: &str) -> bool {
    crate::build_system::is_build_config_name(name)
}

/// Case-insensitive variant of [`is_build_config_name`] for classifiers that
/// operate on lowercased path strings.
pub fn is_build_config_name_lower(lower_name: &str) -> bool {
    crate::build_system::is_build_config_name_lower(lower_name)
}

/// Well-known documentation file names without an extension
/// (README, LICENSE, ...). Single source of truth shared by the language
/// detector (`LanguageInfo::detect_from_path`) and every consumer that used
/// to keep a private copy of the list.
pub const EXTENSIONLESS_DOC_NAMES: [&str; 7] = [
    "readme",
    "changelog",
    "contributing",
    "copying",
    "license",
    "authors",
    "notice",
];

/// Whether a lowercased file name is one of the well-known extensionless
/// documentation names ([`EXTENSIONLESS_DOC_NAMES`]).
pub fn is_extensionless_doc_name(lower_name: &str) -> bool {
    EXTENSIONLESS_DOC_NAMES.contains(&lower_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_project_path_relative() {
        assert_eq!(normalize_project_path("src/main.rs"), "src/main.rs");
        assert_eq!(normalize_project_path("./src//main.rs"), "src/main.rs");
        assert_eq!(normalize_project_path("src/./main.rs"), "src/main.rs");
        assert_eq!(normalize_project_path("src/../lib.rs"), "lib.rs");
        assert_eq!(normalize_project_path("src\\main.rs"), "src/main.rs");
    }

    #[test]
    fn test_normalize_project_path_absolute() {
        assert_eq!(
            normalize_project_path("/workspace/src/lib.rs"),
            "/workspace/src/lib.rs"
        );
        assert_eq!(normalize_project_path("/a/../b"), "/b");
        // Leading `..` segments are preserved
        assert_eq!(normalize_project_path("../../x"), "../../x");
    }

    #[test]
    fn test_relativize() {
        let root = Path::new("/workspace/project");
        assert_eq!(relativize(root, &root.join("src/main.rs")), "src/main.rs");
        assert_eq!(
            relativize(root, Path::new("/elsewhere/file.rs")),
            "/elsewhere/file.rs"
        );
        assert_eq!(
            relativize(
                Path::new("C:/project"),
                Path::new("C:/project/src\\main.rs")
            ),
            "src/main.rs"
        );
    }

    #[test]
    fn test_file_name_str() {
        assert_eq!(file_name_str("src/main.rs"), "main.rs");
        assert_eq!(file_name_str("src\\main.rs"), "main.rs");
        assert_eq!(file_name_str("main.rs"), "main.rs");
        assert_eq!(file_name_str("src/"), "");
    }

    #[test]
    fn test_segments() {
        assert_eq!(
            segments("src/config/loader.rs"),
            vec!["src", "config", "loader.rs"]
        );
        assert_eq!(segments("/a//b/"), vec!["a", "b"]);
        assert_eq!(segments("a\\b.rs"), vec!["a", "b.rs"]);
    }

    #[test]
    fn test_extension_lower() {
        assert_eq!(extension_lower("src/main.RS").as_deref(), Some("rs"));
        assert_eq!(extension_lower("src/main.rs").as_deref(), Some("rs"));
        assert_eq!(extension_lower("src\\MAIN.TXT").as_deref(), Some("txt"));
        assert_eq!(extension_lower("noext"), None);
        assert_eq!(extension_lower("trailing."), None);
    }

    #[test]
    fn test_normalized_equals() {
        assert!(normalized_equals("src/main.rs", "src/main.rs"));
        assert!(normalized_equals("src/main.rs", "src\\main.rs"));
        assert!(normalized_equals(
            "/project/src/main.rs",
            "/project/src/main.rs"
        ));
        assert!(normalized_equals(
            "/project/src/main.rs",
            "project/src/main.rs"
        ));
        // Absolute and relative forms of the *same* trailing path are equal
        // only when the stripped forms match exactly.
        assert!(!normalized_equals("/project/src/main.rs", "src/main.rs"));
        // No suffix fallback: different files
        assert!(!normalized_equals("a/b/c.rs", "b/c.rs"));
        assert!(!normalized_equals("src/main.rs", "src/lib.rs"));
    }

    #[test]
    fn test_stable_path_id_stable_and_distinct() {
        let id1 = stable_path_id("src/main.rs");
        let id2 = stable_path_id("src/main.rs");
        let id3 = stable_path_id("src/lib.rs");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert_eq!(id1.len(), 64);
    }

    #[test]
    fn test_is_non_utf8() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        use std::path::PathBuf;
        let valid = Path::new("/workspace/src/main.rs");
        assert!(!is_non_utf8(valid));
        let invalid = PathBuf::from(OsString::from_vec(vec![b'/', b'a', 0xFF, b'b']));
        assert!(is_non_utf8(&invalid));
    }

    #[test]
    fn test_group_id_base() {
        assert_eq!(group_id_base("src/main.rs"), "src_main.rs");
        assert_eq!(group_id_base("src\\main.rs"), "src_main.rs");
        // Injective encoding: literal underscores are doubled, separators
        // become a single underscore, so distinct paths never collide.
        assert_eq!(group_id_base("a_b.c"), "a__b.c");
        assert_eq!(group_id_base("a/b.c"), "a_b.c");
        assert_ne!(group_id_base("a/b.c"), group_id_base("a_b.c"));
        assert_eq!(
            group_id_base("my_dir/file_name.rs"),
            "my__dir_file__name.rs"
        );
    }

    #[test]
    fn test_is_build_config_name() {
        assert!(is_build_config_name("Cargo.toml"));
        assert!(is_build_config_name("package.json"));
        assert!(is_build_config_name("Foo.csproj"));
        assert!(is_build_config_name("Gemfile"));
        assert!(is_build_config_name("Makefile"));
        assert!(is_build_config_name("GNUmakefile"));
        assert!(is_build_config_name("Dockerfile"));
        // Lowercase spellings are accepted as distinct file names
        assert!(is_build_config_name("makefile"));
        assert!(is_build_config_name("dockerfile"));
        assert!(!is_build_config_name("main.rs"));
        assert!(!is_build_config_name("cargo.toml")); // case-sensitive exact match
    }

    #[test]
    fn test_is_build_config_name_lower() {
        assert!(is_build_config_name_lower("cargo.toml"));
        assert!(is_build_config_name_lower("package-lock.json"));
        assert!(is_build_config_name_lower("foo.csproj"));
        assert!(!is_build_config_name_lower("main.rs"));
    }

    #[test]
    fn test_is_extensionless_doc_name() {
        for name in EXTENSIONLESS_DOC_NAMES {
            assert!(is_extensionless_doc_name(name));
            assert!(!is_extensionless_doc_name(&format!("{name}.md")));
        }
        assert!(!is_extensionless_doc_name("guide"));
        assert!(!is_extensionless_doc_name("main"));
    }
}
