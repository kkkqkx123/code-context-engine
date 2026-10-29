//! Language-specific syntax noise stripping for index sidecar text.
//!
//! For profiles where they are enabled (Rust), removes `unsafe { ... }`
//! wrappers (keeping inner content) and the `&*` / `&mut *` dereference
//! markers. These tokens carry no semantic signal for embedding and dilute
//! the vector representation. For other languages the text is passed through
//! unchanged, since constructs like `&*` are valid and meaningful source.

use crate::ast_to_nl::noise::NoiseProfile;

/// Strip language-specific syntax noise from code fragments while preserving
/// implementation logic.
pub(super) fn strip_syntax_noise(text: &str, profile: NoiseProfile) -> String {
    let text = if profile.unwrap_unsafe_blocks {
        strip_unsafe_blocks(text)
    } else {
        text.to_string()
    };
    let text = if profile.strip_deref_markers {
        text.replace("&mut *", "").replace("&*", "")
    } else {
        text
    };
    let text = if profile.strip_macro_repetition {
        strip_macro_repetitions(&text)
    } else {
        text
    };
    let text = text.replace(" .", ".");
    let text = text.replace(" ;", ";");
    // Collapse horizontal whitespace artifacts left by block removal while
    // preserving newline structure of the code fragment.
    cce_utils::normalize_whitespace_preserving_newlines(&text)
        .trim()
        .to_string()
}

/// Remove `unsafe { ... }` wrappers, keeping the inner content.
/// Handles nested brace blocks.
fn strip_unsafe_blocks(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if !text.is_char_boundary(i) {
            i = cce_utils::text::ceil_char_boundary(text, i);
            continue;
        }
        let is_keyword_start =
            i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if is_keyword_start && bytes[i..].starts_with(b"unsafe") {
            let mut j = i + "unsafe".len();
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'{' {
                let mut depth = 1usize;
                let mut k = j + 1;
                while k < bytes.len() && depth > 0 {
                    match bytes[k] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    k += 1;
                }
                if depth == 0 {
                    result.push_str(&text[j + 1..k - 1]);
                    i = k;
                    continue;
                }
            }
        }
        let ch = text[i..].chars().next().expect("valid utf8 boundary");
        result.push(ch);
        i += ch.len_utf8();
    }
    result
}

/// Remove macro repetition wrappers (`$( ... )*`), keeping the inner pattern.
///
/// Macro-body fragments use `$(...)` repetition syntax whose wrapper tokens
/// carry no lexical meaning for embedding; the inner pattern (e.g. `tt`) and
/// `$metavar` identifiers are preserved.
fn strip_macro_repetitions(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'(') {
            i += 2;
            continue;
        }
        if matches!(bytes[i], b')' | b'}')
            && matches!(bytes.get(i + 1), Some(b'*') | Some(b'+') | Some(b'?'))
        {
            i += 2;
            continue;
        }
        let ch = text[i..].chars().next().expect("valid utf8 boundary");
        result.push(ch);
        i += ch.len_utf8();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::language::Language;

    #[test]
    fn strip_syntax_noise_rust_profile() {
        let source = "unsafe { &mut *self.inner.get() }.as_mut()";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Rust));
        assert!(!cleaned.contains("unsafe"), "got: {cleaned}");
        assert!(!cleaned.contains("&mut *"), "got: {cleaned}");
        assert!(
            cleaned.contains("self.inner.get().as_mut()"),
            "got: {cleaned}"
        );
    }

    #[test]
    fn strip_syntax_noise_non_rust_profile() {
        let source = "auto x = &*ptr;";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Cpp));
        assert!(
            cleaned.contains("&*ptr"),
            "should preserve C++ deref, got: {cleaned}"
        );
    }

    #[test]
    fn strip_syntax_noise_keeps_unsafe_for_non_rust() {
        let source = "unsafe { &mut *self.inner.get() }";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Python));
        assert!(cleaned.contains("unsafe"), "got: {cleaned}");
        assert!(cleaned.contains("&mut *"), "got: {cleaned}");
    }

    #[test]
    fn strip_macro_repetitions_rust_profile() {
        let source = "write!($($arg)*) { $($tt)* }";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Rust));
        assert!(
            cleaned.contains("write!($arg)"),
            "should strip $()* wrappers while keeping $metavar, got: {cleaned}"
        );
        assert!(
            cleaned.contains("$tt"),
            "should keep metavariables, got: {cleaned}"
        );
        assert!(
            !cleaned.contains("$(") && !cleaned.contains(")*"),
            "should remove repetition wrappers, got: {cleaned}"
        );
    }

    #[test]
    fn strip_macro_repetitions_non_rust_profile() {
        let source = "$(cmd)";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Bash));
        assert!(
            cleaned.contains("$(cmd)"),
            "Bash command substitution must be preserved, got: {cleaned}"
        );
    }

    /// Sync guard: the noise profile is an explicit expectation table, not
    /// a silent default. Only Rust enables language-specific rules today;
    /// a new language must be added here with its rules (or a documented
    /// all-off entry) when extending `Language::ast_parsable_languages`.
    #[test]
    fn noise_profile_covers_all_parsable_languages() {
        let enabled: &[Language] = &[Language::Rust];
        for language in Language::ast_parsable_languages() {
            let profile = NoiseProfile::for_language(*language);
            if enabled.contains(language) {
                assert!(
                    profile.unwrap_unsafe_blocks
                        && profile.strip_deref_markers
                        && profile.filter_safety_boilerplate
                        && profile.strip_macro_repetition,
                    "{language:?} listed as enabled but its profile is partial"
                );
            } else {
                assert!(
                    !profile.unwrap_unsafe_blocks
                        && !profile.strip_deref_markers
                        && !profile.filter_safety_boilerplate
                        && !profile.strip_macro_repetition,
                    "{language:?} is parsable but missing from the noise expectation: \
                     add it to enabled or accept the all-off profile explicitly"
                );
            }
        }
    }
}
