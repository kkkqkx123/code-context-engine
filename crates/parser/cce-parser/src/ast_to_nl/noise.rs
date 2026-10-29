//! Language-aware noise filtering profile.
//!
//! The noise rules are grouped by syntactic construct and enabled per language
//! through a `NoiseProfile`. Rules specific to a language's syntax (e.g. Rust's
//! `unsafe` blocks, `&*` dereference markers) must not be applied to languages
//! where those constructs carry real semantics (e.g. C/C++ `&*`).
//!
//! Generic rules (empty markdown section markers like `# Example.`) always
//! apply regardless of language.
//!
//! Each language explicitly declares the rule set it enables. Languages without
//! a dedicated denoising shape keep an all-off profile by design (templates,
//! style languages, data formats and languages pending sampling evaluation).
//! Remapped plugin languages inherit their host grammar's profile; native
//! plugin languages keep the conservative all-off fallback.

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
    /// Strip C/C++ header guards, pragma-once directives and `extern "C"`
    /// guard wrappers while keeping the inner content.
    pub strip_c_guards: bool,
    /// Strip Python coding declarations, `__future__` imports and the
    /// `if __name__ == "__main__"` guard line while keeping the guarded body.
    pub strip_python_boilerplate: bool,
    /// Strip JavaScript-family `'use strict'` directives and `export default`
    /// wrappers while keeping the exported content.
    pub strip_js_wrapper: bool,
    /// Weaken Go blank-identifier assignments and `if err != nil` guard lines
    /// in embedding text while preserving the surrounding semantics.
    pub strip_go_boilerplate: bool,
    /// Strip Bash shebang and `set` strict-mode switch lines.
    pub strip_bash_header: bool,
    /// Weaken Java `@Override` markers and trivial getter/setter bodies in
    /// vector text while leaving entity division untouched.
    pub strip_java_boilerplate: bool,
    /// Strip C# `#region` markers and assembly attributes.
    pub strip_csharp_markers: bool,
    /// Strip PHP open/close tags while keeping namespace names.
    pub strip_php_markers: bool,
    /// Strip Ruby magic comments and visibility section markers.
    pub strip_ruby_markers: bool,
    /// Weaken Lua module return-table wrappers while keeping return names.
    pub weaken_lua_return: bool,
    /// Strip Kotlin JVM-interop annotation lines.
    /// Platform markers only (`@JvmStatic`, `@JvmOverloads`, `@JvmField`,
    /// `@JvmName`, `@Throws`); the annotated declaration keeps its text.
    /// Decided by sampling over in-repo snippets: whole-line matches are
    /// unambiguous, and dropping them never removes business logic.
    pub strip_kotlin_markers: bool,
    /// Strip Scala compiler-hint annotation lines.
    /// Storage/codegen hints only (`@inline`, `@specialized`,
    /// `@transient`, `@volatile`, `@SerialVersionUID`); same sampling
    /// basis and whole-line guard as the Kotlin markers.
    pub strip_scala_markers: bool,
    /// Strip Dart override markers and lint-suppression comments.
    /// `@override` mirrors the Java rule; `// ignore:` directives mirror
    /// the Ruby magic-comment rule. Whole-line matches only.
    pub strip_dart_markers: bool,
}

impl NoiseProfile {
    /// Derive a profile for the given language.
    ///
    /// Each language enables only its own denoising shape. Remapped plugin
    /// languages inherit their host grammar's profile.
    pub fn for_language(language: Language) -> Self {
        // Remapped plugin languages inherit their host grammar's profile.
        match cce_types::language::effective_language(&language) {
            Language::Rust => Self {
                unwrap_unsafe_blocks: true,
                strip_deref_markers: true,
                filter_safety_boilerplate: true,
                strip_macro_repetition: true,
                ..Self::none()
            },
            Language::C | Language::Cpp => Self {
                strip_c_guards: true,
                ..Self::none()
            },
            Language::Python => Self {
                strip_python_boilerplate: true,
                ..Self::none()
            },
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => Self {
                strip_js_wrapper: true,
                ..Self::none()
            },
            Language::Go => Self {
                strip_go_boilerplate: true,
                ..Self::none()
            },
            Language::Bash => Self {
                strip_bash_header: true,
                ..Self::none()
            },
            Language::Java => Self {
                strip_java_boilerplate: true,
                ..Self::none()
            },
            Language::CSharp => Self {
                strip_csharp_markers: true,
                ..Self::none()
            },
            Language::Php => Self {
                strip_php_markers: true,
                ..Self::none()
            },
            Language::Ruby => Self {
                strip_ruby_markers: true,
                ..Self::none()
            },
            Language::Lua => Self {
                weaken_lua_return: true,
                ..Self::none()
            },
            Language::Kotlin => Self {
                strip_kotlin_markers: true,
                ..Self::none()
            },
            Language::Scala => Self {
                strip_scala_markers: true,
                ..Self::none()
            },
            Language::Dart => Self {
                strip_dart_markers: true,
                ..Self::none()
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
            strip_c_guards: false,
            strip_python_boilerplate: false,
            strip_js_wrapper: false,
            strip_go_boilerplate: false,
            strip_bash_header: false,
            strip_java_boilerplate: false,
            strip_csharp_markers: false,
            strip_php_markers: false,
            strip_ruby_markers: false,
            weaken_lua_return: false,
            strip_kotlin_markers: false,
            strip_scala_markers: false,
            strip_dart_markers: false,
        }
    }

    /// Whether any language-specific rule is enabled.
    pub fn has_language_rules(&self) -> bool {
        self.unwrap_unsafe_blocks
            || self.strip_deref_markers
            || self.filter_safety_boilerplate
            || self.strip_macro_repetition
            || self.strip_c_guards
            || self.strip_python_boilerplate
            || self.strip_js_wrapper
            || self.strip_go_boilerplate
            || self.strip_bash_header
            || self.strip_java_boilerplate
            || self.strip_csharp_markers
            || self.strip_php_markers
            || self.strip_ruby_markers
            || self.weaken_lua_return
            || self.strip_kotlin_markers
            || self.strip_scala_markers
            || self.strip_dart_markers
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
        assert!(!profile.strip_c_guards);
        assert!(!profile.strip_python_boilerplate);
    }

    #[test]
    fn test_batch_one_profiles_enable_own_rules() {
        let c = NoiseProfile::for_language(Language::C);
        assert!(c.strip_c_guards);
        assert!(c.has_language_rules());
        let cpp = NoiseProfile::for_language(Language::Cpp);
        assert!(cpp.strip_c_guards);

        let python = NoiseProfile::for_language(Language::Python);
        assert!(python.strip_python_boilerplate);

        for language in [
            Language::JavaScript,
            Language::TypeScript,
            Language::Jsx,
            Language::Tsx,
        ] {
            let profile = NoiseProfile::for_language(language);
            assert!(profile.strip_js_wrapper, "{language:?}");
        }

        let go = NoiseProfile::for_language(Language::Go);
        assert!(go.strip_go_boilerplate);

        let bash = NoiseProfile::for_language(Language::Bash);
        assert!(bash.strip_bash_header);
    }

    #[test]
    fn test_batch_two_profiles_enable_own_rules() {
        assert!(NoiseProfile::for_language(Language::Java).strip_java_boilerplate);
        assert!(NoiseProfile::for_language(Language::CSharp).strip_csharp_markers);
        assert!(NoiseProfile::for_language(Language::Php).strip_php_markers);
        assert!(NoiseProfile::for_language(Language::Ruby).strip_ruby_markers);
        assert!(NoiseProfile::for_language(Language::Lua).weaken_lua_return);
    }

    #[test]
    fn test_batch_three_profiles_enable_own_rules() {
        // Sampling batch: JVM-interop markers, compiler-hint annotations
        // and override/lint markers. Each rule is whole-line and
        // independently tested in the syntax-strip suite.
        assert!(NoiseProfile::for_language(Language::Kotlin).strip_kotlin_markers);
        assert!(NoiseProfile::for_language(Language::Scala).strip_scala_markers);
        assert!(NoiseProfile::for_language(Language::Dart).strip_dart_markers);
        for language in [Language::Kotlin, Language::Scala, Language::Dart] {
            assert!(
                NoiseProfile::for_language(language).has_language_rules(),
                "{language:?}"
            );
        }
    }

    #[test]
    fn test_pending_and_template_languages_stay_off() {
        for language in [
            Language::Html,
            Language::Css,
            Language::Vue,
            Language::Svelte,
            Language::Ejs,
            Language::Unknown,
        ] {
            let profile = NoiseProfile::for_language(language);
            assert_eq!(profile, NoiseProfile::none(), "{language:?}");
            assert!(!profile.has_language_rules(), "{language:?}");
        }
    }

    #[test]
    fn test_noise_profile_covers_all_parsable_languages() {
        for language in Language::ast_parsable_languages() {
            let profile = NoiseProfile::for_language(*language);
            match language {
                Language::Rust => {
                    assert!(
                        profile.unwrap_unsafe_blocks
                            && profile.strip_deref_markers
                            && profile.filter_safety_boilerplate
                            && profile.strip_macro_repetition,
                        "Rust must enable all syntax rules"
                    );
                }
                Language::C | Language::Cpp => assert!(
                    profile.strip_c_guards && !profile.strip_python_boilerplate,
                    "{language:?} must enable C guards only"
                ),
                Language::Python => assert!(
                    profile.strip_python_boilerplate,
                    "{language:?} must enable Python boilerplate"
                ),
                Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                    assert!(
                        profile.strip_js_wrapper,
                        "{language:?} must enable JS wrapper"
                    )
                }
                Language::Go => assert!(
                    profile.strip_go_boilerplate,
                    "{language:?} must enable Go boilerplate"
                ),
                Language::Bash => assert!(
                    profile.strip_bash_header,
                    "{language:?} must enable Bash header"
                ),
                Language::Java => assert!(
                    profile.strip_java_boilerplate,
                    "{language:?} must enable Java boilerplate"
                ),
                Language::CSharp => assert!(
                    profile.strip_csharp_markers,
                    "{language:?} must enable C# markers"
                ),
                Language::Php => assert!(
                    profile.strip_php_markers,
                    "{language:?} must enable PHP markers"
                ),
                Language::Ruby => assert!(
                    profile.strip_ruby_markers,
                    "{language:?} must enable Ruby markers"
                ),
                Language::Lua => assert!(
                    profile.weaken_lua_return,
                    "{language:?} must enable Lua return weakening"
                ),
                Language::Kotlin => assert!(
                    profile.strip_kotlin_markers
                        && !profile.strip_scala_markers
                        && !profile.strip_dart_markers,
                    "{language:?} must enable Kotlin markers only"
                ),
                Language::Scala => assert!(
                    profile.strip_scala_markers
                        && !profile.strip_kotlin_markers
                        && !profile.strip_dart_markers,
                    "{language:?} must enable Scala markers only"
                ),
                Language::Dart => assert!(
                    profile.strip_dart_markers
                        && !profile.strip_kotlin_markers
                        && !profile.strip_scala_markers,
                    "{language:?} must enable Dart markers only"
                ),
                Language::Html
                | Language::Css
                | Language::Vue
                | Language::Svelte
                | Language::Ejs => assert_eq!(
                    profile,
                    NoiseProfile::none(),
                    "{language:?} stays all-off by design (pending evaluation or template/style)"
                ),
                _ => assert_eq!(
                    profile,
                    NoiseProfile::none(),
                    "{language:?} has no language-specific noise rules by design"
                ),
            }
        }
    }

    #[test]
    fn test_custom_language_inheritance_and_fallback() {
        use cce_types::language::{
            clear_plugin_languages, register_plugin_language, register_remap_plugin_language,
        };

        let _guard = crate::tree_sitter_init::GLOBAL_TABLE_LOCK
            .lock()
            .expect("test lock poisoned");
        clear_plugin_languages();
        let remapped = register_remap_plugin_language(
            "noise-remap-guard",
            &["noiserg".to_string()],
            &Language::Rust.to_string(),
        );
        let native = register_plugin_language("noise-native-guard", &["noiseng".to_string()]);
        assert_eq!(
            NoiseProfile::for_language(Language::Custom(remapped)),
            NoiseProfile::for_language(Language::Rust),
            "remapped language must inherit host profile"
        );
        assert_eq!(
            NoiseProfile::for_language(Language::Custom(native)),
            NoiseProfile::none(),
            "native plugin language must stay conservative all-off"
        );
        clear_plugin_languages();
    }
}
