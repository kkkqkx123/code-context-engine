//! Embedding noise filter
//!
//! Removes low-value text fragments that dilute embedding vectors without
//! contributing semantic content:
//!
//! - Empty markdown section markers (`# Example.`, `# Panics.`, `# Safety.`)
//!   that remain after their fenced code blocks were stripped. Always applied,
//!   regardless of language.
//! - Safety boilerplate comments (`Safe due to ...`, `Safe b/c ...`) that
//!   restate the enclosing `unsafe` block's contract without new information.
//!   Applied only when the language profile enables it (Rust).
//!
//! Safety contract paragraphs that carry real content (`SAFETY: ...`,
//! `Safety: ...` followed by explanation text) are preserved.

use crate::ast_to_nl::noise::NoiseProfile;

/// Filter embedding noise from NL text.
///
/// Empty markdown marker filtering always runs; language-specific rules are
/// governed by the profile.
pub fn filter_embedding_noise(text: &str, profile: NoiseProfile) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        let trimmed = line.trim();
        if is_empty_marker(trimmed) {
            continue;
        }
        if profile.filter_safety_boilerplate && is_safety_boilerplate(trimmed) {
            continue;
        }
        out.push(line);
    }
    out.join("\n")
}

/// A standalone markdown section marker with a single-word title
/// (e.g. `# Example.`, `# Panics`).
fn is_empty_marker(line: &str) -> bool {
    let trimmed = line.trim().trim_end_matches('.');
    let Some(rest) = trimmed.strip_prefix('#') else {
        return false;
    };
    let rest = rest.trim();
    if rest.is_empty() || rest.contains(char::is_whitespace) {
        return false;
    }
    matches!(
        rest.to_ascii_lowercase().as_str(),
        "example" | "examples" | "panics" | "safety" | "error" | "errors" | "note" | "notes"
    )
}

/// Safety justification boilerplate comments.
fn is_safety_boilerplate(line: &str) -> bool {
    let lower = line.trim().to_ascii_lowercase();
    lower.starts_with("safe due to")
        || lower.starts_with("safe b/c")
        || lower.starts_with("safe because")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::Language;

    #[test]
    fn test_filters_empty_markers() {
        let input =
            "Sets the contents of this cell to `value`.\n\nReturns `Ok(())`.\n# Example.\n# Panics";
        let cleaned = filter_embedding_noise(input, NoiseProfile::none());
        assert!(!cleaned.contains("# Example"));
        assert!(!cleaned.contains("# Panics"));
        assert!(cleaned.contains("Sets the contents"));
    }

    #[test]
    fn test_keeps_real_headings_and_marker_content() {
        let input = "# Overview\n# Example\nSome real example prose here.";
        let cleaned = filter_embedding_noise(input, NoiseProfile::none());
        assert!(cleaned.contains("# Overview"));
        assert!(!cleaned.contains("# Example"));
        assert!(cleaned.contains("Some real example prose here."));
    }

    #[test]
    fn test_filters_safety_boilerplate() {
        let input =
            "Safe due to `inner`'s invariant of being written to at most once.\nGets a reference.";
        let cleaned = filter_embedding_noise(input, NoiseProfile::for_language(Language::Rust));
        assert!(!cleaned.contains("Safe due to"));
        assert!(cleaned.contains("Gets a reference."));
    }

    #[test]
    fn test_keeps_safety_contract() {
        let input =
            "SAFETY: Pointer-to-integer transmutes are valid.\nSafety: synchronizes with store.";
        let cleaned = filter_embedding_noise(input, NoiseProfile::for_language(Language::Rust));
        assert!(cleaned.contains("SAFETY:"));
        assert!(cleaned.contains("synchronizes"));
    }

    #[test]
    fn test_keeps_safety_boilerplate_for_non_rust() {
        let input = "Safe due to the batching guarantees of this library.";
        let cleaned = filter_embedding_noise(input, NoiseProfile::for_language(Language::Python));
        assert!(cleaned.contains("Safe due to"));
    }
}
