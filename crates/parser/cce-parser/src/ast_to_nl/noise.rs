//! Language-aware noise filtering profile.
//!
//! The noise rules are grouped by syntactic construct and enabled per language
//! through a `NoiseProfile`. Rules specific to a language's syntax (e.g. Rust's
//! `unsafe` blocks, `&*` dereference markers) must not be applied to languages
//! where those constructs carry real semantics (e.g. C/C++ `&*`).
//!
//! Generic rules (empty markdown section markers like `# Example.`) always
//! apply regardless of language.

use cce_types::language::Language;

/// Deterministic noise filtering profile derived from a language.
///
/// All rules are statically determined at construction time; no dynamic
/// dispatch is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoiseProfile {
    /// Strip `unsafe { ... }` wrappers while keeping the inner content.
    /// Only meaningful for Rust, where `unsafe` is a keyword.
    pub unwrap_unsafe_blocks: bool,
    /// Remove `&*` / `&mut *` dereference markers.
    /// Only meaningful for Rust; in C/C++ `&*` is valid and meaningful source.
    pub strip_deref_markers: bool,
    /// Filter `Safe due to...` safety boilerplate comments.
    /// Only meaningful for Rust doc conventions; in other languages such text
    /// may be real content.
    pub filter_safety_boilerplate: bool,
    /// Remove macro repetition wrappers (`$( ... )*`), keeping the inner pattern.
    /// Only meaningful for Rust; in other languages `$()` may carry semantics
    /// (e.g. Bash command substitution).
    pub strip_macro_repetition: bool,
}

impl NoiseProfile {
    /// Derive a profile for the given language.
    ///
    /// Rust-specific syntax rules are enabled only for Rust.
    pub fn for_language(language: Language) -> Self {
        match language {
            Language::Rust => Self {
                unwrap_unsafe_blocks: true,
                strip_deref_markers: true,
                filter_safety_boilerplate: true,
                strip_macro_repetition: true,
            },
            _ => Self::none(),
        }
    }

    /// Profile with no language-specific rules enabled.
    ///
    /// Generic rules (empty marker filtering) always apply regardless of the
    /// profile.
    pub fn none() -> Self {
        Self {
            unwrap_unsafe_blocks: false,
            strip_deref_markers: false,
            filter_safety_boilerplate: false,
            strip_macro_repetition: false,
        }
    }
}

impl Default for NoiseProfile {
    fn default() -> Self {
        Self::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_profile_enables_all_rules() {
        let profile = NoiseProfile::for_language(Language::Rust);
        assert!(profile.unwrap_unsafe_blocks);
        assert!(profile.strip_deref_markers);
        assert!(profile.filter_safety_boilerplate);
        assert!(profile.strip_macro_repetition);
    }

    #[test]
    fn test_non_rust_profile_disables_all_rules() {
        for language in [
            Language::C,
            Language::Cpp,
            Language::Python,
            Language::JavaScript,
            Language::Go,
            Language::Unknown,
        ] {
            let profile = NoiseProfile::for_language(language);
            assert!(!profile.unwrap_unsafe_blocks, "{language:?}");
            assert!(!profile.strip_deref_markers, "{language:?}");
            assert!(!profile.filter_safety_boilerplate, "{language:?}");
            assert!(!profile.strip_macro_repetition, "{language:?}");
        }
    }
}
