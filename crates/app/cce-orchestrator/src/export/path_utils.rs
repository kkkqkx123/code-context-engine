//! Path utilities for export module
//!
//! Provides unified path handling to ensure consistent matching
//! across different data sources that may use different path formats.
//!
//! Path normalization lives in `cce_utils::path` as the single
//! canonical implementation; this module only adapts it to the export
//! module's project-root-relative view.

use std::path::{Path, PathBuf};

use cce_types::path::{normalize_project_path, normalized_equals};

/// Convert a (possibly absolute) source path to a project-relative path.
///
/// Handles:
/// - Absolute paths with project root prefix stripping (component-safe)
/// - Windows long-path prefix (`\\?\`, `//?/`)
/// - Path separator normalization (`\` → `/`)
/// - Already-relative paths passed through (normalized)
///
/// # Examples
///
/// ```text
/// relative_source_path("/project/src/main.rs", "/project") → "src/main.rs"
/// relative_source_path("src/main.rs", "/project")          → "src/main.rs"
/// ```
pub fn relative_source_path(source_path: &str, project_root: &Path) -> PathBuf {
    let root_str = project_root
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();

    // Normalize path separators and strip Windows long-path prefix (\\?\)
    let normalized = source_path
        .replace('\\', "/")
        .trim_start_matches("\\\\?\\")
        .trim_start_matches("//?/")
        .to_string();

    // Component-safe prefix stripping against the normalized root, so a root
    // like `/proj` never strips a sibling like `/projects/foo`.
    if let Ok(stripped) = Path::new(&normalized).strip_prefix(Path::new(&root_str))
        && !stripped.as_os_str().is_empty()
    {
        return PathBuf::from(normalize_project_path(&stripped.to_string_lossy()));
    }

    // Already relative (or outside the root): use as-is, normalized
    PathBuf::from(normalize_project_path(&normalized))
}

/// Compute the NL document output path for a source file.
///
/// Converts the source path to a project-relative path, then outputs to
/// `<output_dir>/<relative_path>.md`, preserving the original extension
/// (e.g. `lib.rs` → `lib.rs.md`).
pub fn compute_nl_doc_output_path(
    source_path: &str,
    output_dir: &Path,
    project_root: &Path,
) -> PathBuf {
    let mut output_path = output_dir.to_path_buf();
    let relative = relative_source_path(source_path, project_root);
    output_path.push(&relative);
    // Append .md suffix, preserving original extension (e.g. lib.rs -> lib.rs.md)
    let new_ext = output_path
        .extension()
        .map(|e| format!("{}.md", e.to_string_lossy()))
        .unwrap_or_else(|| "md".to_string());
    output_path.set_extension(new_ext);
    output_path
}

/// Strip index-only sidecar text from export content.
///
/// Export should use pure presentation text, not the enriched index text
/// that contains control-flow and behavior sidecars.
pub fn strip_index_context(text: &str) -> String {
    text.trim_end().to_string()
}

/// Write a file atomically (write to temp, then rename over the target).
///
/// This prevents external readers from observing partially-written export
/// documents during hot updates. On Windows, rename fails if the target
/// already exists, so the target is removed before the final rename there.
pub async fn write_file_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let tmp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default()
    ));

    tokio::fs::write(&tmp_path, content).await?;

    match tokio::fs::rename(&tmp_path, path).await {
        Ok(()) => Ok(()),
        Err(first_err) => {
            // Windows: rename fails if destination exists. Remove and retry.
            if path.exists() {
                let _ = tokio::fs::remove_file(path).await;
                return tokio::fs::rename(&tmp_path, path).await;
            }
            let _ = tokio::fs::remove_file(&tmp_path).await;
            Err(first_err)
        }
    }
}

/// Remove a stale temp file left over from an interrupted atomic write.
pub async fn cleanup_temp_file(path: &Path) -> std::io::Result<()> {
    let tmp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    if tmp_path.exists() {
        tokio::fs::remove_file(&tmp_path).await?;
    }
    Ok(())
}

/// Check if two paths match (after normalization)
///
/// Strict normalized comparison (handles `/` vs `\`, absolute vs relative
/// via leading-slash tolerance). The former suffix-based fallback was
/// removed: `a/b/c` and `b/c` must not compare equal.
pub fn paths_match(path1: &str, path2: &str) -> bool {
    normalized_equals(path1, path2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relative_source_path_absolute() {
        assert_eq!(
            relative_source_path("/project/src/main.rs", Path::new("/project")),
            PathBuf::from("src/main.rs")
        );
        assert_eq!(
            relative_source_path("/projects/foo.rs", Path::new("/project")),
            PathBuf::from("/projects/foo.rs")
        );
    }

    #[test]
    fn test_relative_source_path_windows() {
        assert_eq!(
            relative_source_path("C:\\project\\src\\main.rs", Path::new("C:\\project")),
            PathBuf::from("src/main.rs")
        );
        assert_eq!(
            relative_source_path("\\\\?\\C:\\project\\src\\main.rs", Path::new("C:\\project")),
            PathBuf::from("src/main.rs")
        );
    }

    #[test]
    fn test_relative_source_path_relative() {
        assert_eq!(
            relative_source_path("src\\main.rs", Path::new("/project")),
            PathBuf::from("src/main.rs")
        );
    }

    #[test]
    fn test_paths_match() {
        assert!(paths_match("src/main.rs", "src\\main.rs"));
        assert!(paths_match("src/main.rs", "/src/main.rs"));
        // Strict comparison: `/project/src/main.rs` and `src/main.rs` are
        // different files (the old suffix heuristic no longer applies).
        assert!(!paths_match("/project/src/main.rs", "src/main.rs"));
        assert!(!paths_match("src/main.rs", "src/lib.rs"));
        assert!(!paths_match("a/b/c.rs", "b/c.rs"));
    }

    #[test]
    fn test_paths_match_same() {
        assert!(paths_match("src/main.rs", "src/main.rs"));
    }
}
