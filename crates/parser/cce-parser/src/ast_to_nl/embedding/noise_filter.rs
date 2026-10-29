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
//! - Per-language boilerplate lines (header guards, strict-mode directives,
//!   main guards, shebang markers and similar) that restate file scaffolding
//!   without new information. Each shape is enabled only for its language.
//!
//! Safety contract paragraphs that carry real content (`SAFETY: ...`,
//! `Safety: ...` followed by explanation text) are preserved. When filtering
//! would empty a non-empty input the original text is kept.

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
        if profile.strip_c_guards && is_c_noise_line(trimmed) {
            continue;
        }
        if profile.strip_python_boilerplate && is_python_noise_line(trimmed) {
            continue;
        }
        if profile.strip_js_wrapper && is_js_strict_line(trimmed) {
            continue;
        }
        if profile.strip_go_boilerplate && is_go_noise_line(trimmed) {
            continue;
        }
        if profile.strip_bash_header && is_bash_noise_line(trimmed) {
            continue;
        }
        if profile.strip_java_boilerplate && is_java_noise_line(trimmed) {
            continue;
        }
        if profile.strip_csharp_markers && is_csharp_noise_line(trimmed) {
            continue;
        }
        if profile.strip_php_markers && is_php_tag_line(trimmed) {
            continue;
        }
        if profile.strip_ruby_markers && is_ruby_noise_line(trimmed) {
            continue;
        }
        if profile.weaken_lua_return && is_lua_noise_line(trimmed) {
            continue;
        }
        // Wrapper prefixes keep their inner content: `export default foo`
        // carries the `foo` signal, `namespace A;` carries `A`, and
        // `return { a }` carries `a`.
        let mut owned: Option<String> = None;
        if profile.strip_js_wrapper {
            if let Some(rest) = strip_js_export_prefix(line) {
                owned = Some(rest);
            }
        }
        if profile.strip_php_markers && owned.is_none() {
            if let Some(name) = strip_php_namespace(line) {
                owned = Some(name);
            }
        }
        if profile.weaken_lua_return && owned.is_none() {
            if let Some(inner) = strip_lua_table(line) {
                owned = Some(inner);
            }
        }
        if profile.strip_c_guards && owned.is_none() {
            if let Some(rest) = strip_c_extern_prefix(line) {
                if rest.trim().is_empty() {
                    continue;
                }
                owned = Some(rest);
            }
        }
        if let Some(replacement) = owned {
            if replacement.trim().is_empty() {
                continue;
            }
            out.push(replacement);
        } else {
            out.push(line.to_string());
        }
    }
    if out.is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
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

fn is_c_noise_line(line: &str) -> bool {
    let t = line.trim();
    if t.to_ascii_lowercase().starts_with("#pragma once") {
        return true;
    }
    if t.starts_with("#ifndef ") || t.starts_with("#ifndef\t") {
        return true;
    }
    if t.starts_with("#define ") || t.starts_with("#define\t") {
        let token = t
            .split_whitespace()
            .nth(1)
            .unwrap_or("")
            .trim_end_matches(';');
        if token.ends_with("_H")
            || token.ends_with("_HPP")
            || token.ends_with("_HXX")
            || token.ends_with("_H_")
            || token.contains("_INCLUDED")
            || token.contains("_GUARD")
        {
            return true;
        }
        return false;
    }
    if t.starts_with("#ifdef __cplusplus")
        || t.starts_with("#if defined(__cplusplus")
        || t.starts_with("#if defined __cplusplus")
    {
        return true;
    }
    if t.starts_with("#endif") || t.starts_with("#else") {
        return t.contains("__cplusplus")
            || t.contains("_H")
            || t.contains("_GUARD")
            || t.contains("_INCLUDED")
            || t.trim() == "#endif"
            || t.trim() == "#else";
    }
    false
}

fn strip_c_extern_prefix(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("extern \"C\"")?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('{').unwrap_or(rest).trim_start();
    let indent_len = line.len() - trimmed.len();
    let indent: String = line.chars().take(indent_len).collect();
    Some(format!("{indent}{rest}"))
}

fn is_python_noise_line(line: &str) -> bool {
    let t = line.trim();
    if t.starts_with('#') {
        let lower = t.to_ascii_lowercase();
        if lower.contains("coding:") || lower.contains("coding=") {
            return true;
        }
    }
    if t.starts_with("from __future__ import ") {
        return true;
    }
    let normalized: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    if normalized.starts_with("if__name__==")
        && (normalized.contains("\"__main__\"") || normalized.contains("'__main__'"))
    {
        return true;
    }
    false
}

fn is_js_strict_line(line: &str) -> bool {
    let t = line.trim().trim_end_matches(';').trim();
    t == "\"use strict\"" || t == "'use strict'"
}

fn strip_js_export_prefix(line: &str) -> Option<String> {
    let trimmed_start = line.trim_start();
    let rest = trimmed_start.strip_prefix("export default")?;
    if rest.is_empty() {
        return Some(String::new());
    }
    let next = rest.chars().next().unwrap_or(' ');
    if next.is_whitespace() {
        let indent_len = line.len() - trimmed_start.len();
        let indent: String = line.chars().take(indent_len).collect();
        Some(format!("{indent}{}", rest.trim_start()))
    } else {
        None
    }
}

fn is_go_noise_line(line: &str) -> bool {
    let t = line.trim();
    if t.starts_with("var _") {
        return t.contains('=');
    }
    if t.starts_with('_') {
        let after: String = t.chars().skip(1).collect();
        let after = after.trim_start();
        if after.starts_with('=') || after.starts_with(',') || after.starts_with(':') {
            return true;
        }
    }
    if t.starts_with("if err != nil") {
        return true;
    }
    false
}

fn is_bash_noise_line(line: &str) -> bool {
    let t = line.trim();
    if t.starts_with("#!") {
        return true;
    }
    if let Some(rest) = t.strip_prefix("set ") {
        if rest.contains('=') || rest.contains('(') || rest.contains(')') {
            return false;
        }
        let allowed_words = [
            "pipefail",
            "errexit",
            "nounset",
            "xtrace",
            "verbose",
            "allexport",
            "braceexpand",
        ];
        for token in rest.split_whitespace() {
            if token == "-o" || token == "+o" {
                continue;
            }
            if allowed_words.contains(&token) {
                continue;
            }
            if token.starts_with('-') || token.starts_with('+') {
                let flags = token.trim_start_matches(['-', '+']);
                if flags.is_empty() {
                    return false;
                }
                if !flags
                    .chars()
                    .all(|c| matches!(c, 'e' | 'u' | 'x' | 'E' | 'o'))
                {
                    return false;
                }
                continue;
            }
            return false;
        }
        return true;
    }
    false
}

fn is_java_noise_line(line: &str) -> bool {
    let t = line.trim();
    if t == "@Override" {
        return true;
    }
    let body = t.trim_end_matches(';').trim();
    if let Some(rest) = body.strip_prefix("return this.") {
        return !rest.trim().is_empty() && rest.chars().all(|c| c.is_alphanumeric() || c == '_');
    }
    false
}

fn is_csharp_noise_line(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("#region") || t.starts_with("#endregion") || t.starts_with("[assembly:")
}

fn is_php_tag_line(line: &str) -> bool {
    matches!(line.trim(), "<?php" | "<?" | "?>")
}

fn strip_php_namespace(line: &str) -> Option<String> {
    let trimmed_start = line.trim_start();
    let rest = trimmed_start.strip_prefix("namespace ")?;
    let name = rest.trim().trim_end_matches(';').trim();
    if name.is_empty() {
        return Some(String::new());
    }
    let indent_len = line.len() - trimmed_start.len();
    let indent: String = line.chars().take(indent_len).collect();
    Some(format!("{indent}{name}"))
}

fn is_ruby_noise_line(line: &str) -> bool {
    let t = line.trim();
    if t.starts_with('#') {
        let lower = t.to_ascii_lowercase();
        if lower.contains("frozen_string_literal")
            || lower.contains("encoding:")
            || lower.contains("encoding=")
            || lower.contains("coding:")
            || lower.contains("coding=")
        {
            return true;
        }
    }
    matches!(t, "private" | "protected" | "public")
}

fn is_lua_noise_line(line: &str) -> bool {
    let t = line.trim().trim_end_matches(';').trim();
    if t.starts_with("local ") {
        if let Some(eq_pos) = t.find('=') {
            let rhs = t[eq_pos + 1..].trim();
            if rhs == "{}" {
                return true;
            }
        }
    }
    if let Some(rest) = t.strip_prefix("return ") {
        let name = rest.trim();
        if !name.is_empty()
            && !name.contains('{')
            && !name.contains('}')
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        {
            // `return { ... }` keeps its inner table via `strip_lua_table`;
            // bare `return M` table exports are boilerplate.
            return true;
        }
    }
    false
}

fn strip_lua_table(line: &str) -> Option<String> {
    let trimmed_start = line.trim_start();
    let t = trimmed_start.trim();
    let rest = t.strip_prefix("return")?;
    if rest.is_empty() {
        return None;
    }
    if !rest.chars().next().unwrap_or(' ').is_whitespace() {
        return None;
    }
    let rest = rest.trim_start();
    let inner = rest.strip_prefix('{')?.strip_suffix('}')?;
    let inner = inner.trim().trim_end_matches(';').trim();
    let indent_len = line.len() - trimmed_start.len();
    let indent: String = line.chars().take(indent_len).collect();
    Some(format!("{indent}{inner}"))
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

    #[test]
    fn test_batch_one_c_denoise_and_reverse() {
        let input = "#pragma once\nint value;\n#define MAX_SIZE 1024";
        let cleaned = filter_embedding_noise(input, NoiseProfile::for_language(Language::C));
        assert!(!cleaned.contains("pragma once"), "got: {cleaned}");
        assert!(cleaned.contains("int value;"), "got: {cleaned}");
        assert!(cleaned.contains("MAX_SIZE"), "got: {cleaned}");

        let other = filter_embedding_noise(input, NoiseProfile::for_language(Language::Python));
        assert!(other.contains("pragma once"), "got: {other}");
    }

    #[test]
    fn test_batch_one_python_denoise_and_reverse() {
        let input = "# coding: utf-8\nfrom __future__ import annotations\ndef foo():";
        let cleaned = filter_embedding_noise(input, NoiseProfile::for_language(Language::Python));
        assert!(!cleaned.contains("coding"), "got: {cleaned}");
        assert!(!cleaned.contains("__future__"), "got: {cleaned}");
        assert!(cleaned.contains("def foo():"), "got: {cleaned}");

        let other = filter_embedding_noise(input, NoiseProfile::for_language(Language::Go));
        assert!(other.contains("coding"), "got: {other}");
    }

    #[test]
    fn test_batch_one_js_denoise_and_reverse() {
        let input = "\"use strict\";\nexport default function foo() {}";
        let cleaned =
            filter_embedding_noise(input, NoiseProfile::for_language(Language::JavaScript));
        assert!(!cleaned.contains("use strict"), "got: {cleaned}");
        assert!(!cleaned.contains("export default"), "got: {cleaned}");
        assert!(cleaned.contains("function foo()"), "got: {cleaned}");

        let other = filter_embedding_noise(input, NoiseProfile::for_language(Language::Python));
        assert!(other.contains("use strict"), "got: {other}");
    }

    #[test]
    fn test_batch_one_go_and_bash_denoise() {
        let go = "_ = x\nprocess()\nif err != nil";
        let cleaned = filter_embedding_noise(go, NoiseProfile::for_language(Language::Go));
        assert!(!cleaned.contains("_ = x"), "got: {cleaned}");
        assert!(cleaned.contains("process()"), "got: {cleaned}");

        let bash = "#!/bin/bash\nset -euo pipefail\necho hello";
        let cleaned_bash = filter_embedding_noise(bash, NoiseProfile::for_language(Language::Bash));
        assert!(!cleaned_bash.contains("#!/bin/bash"), "got: {cleaned_bash}");
        assert!(cleaned_bash.contains("echo hello"), "got: {cleaned_bash}");

        let other = filter_embedding_noise(bash, NoiseProfile::for_language(Language::Go));
        assert!(other.contains("#!/bin/bash"), "got: {other}");
    }

    #[test]
    fn test_batch_two_denoise_and_reverse() {
        let java = "@Override\npublic String getName()";
        let cleaned = filter_embedding_noise(java, NoiseProfile::for_language(Language::Java));
        assert!(!cleaned.contains("@Override"), "got: {cleaned}");
        assert!(cleaned.contains("getName"), "got: {cleaned}");

        let csharp = "#region Helpers\nvoid foo() {}";
        let cleaned_cs =
            filter_embedding_noise(csharp, NoiseProfile::for_language(Language::CSharp));
        assert!(!cleaned_cs.contains("#region"), "got: {cleaned_cs}");

        let php = "<?php\nnamespace App\\Ctrl;\nfunction index() {}";
        let cleaned_php = filter_embedding_noise(php, NoiseProfile::for_language(Language::Php));
        assert!(!cleaned_php.contains("<?php"), "got: {cleaned_php}");
        assert!(cleaned_php.contains("App\\Ctrl"), "got: {cleaned_php}");

        let ruby = "# frozen_string_literal: true\nprivate\ndef bar; end";
        let cleaned_ruby = filter_embedding_noise(ruby, NoiseProfile::for_language(Language::Ruby));
        assert!(
            !cleaned_ruby.contains("frozen_string_literal"),
            "got: {cleaned_ruby}"
        );
        assert!(cleaned_ruby.contains("def bar"), "got: {cleaned_ruby}");

        let lua = "local M = {}\nfunction M.foo() end\nreturn M";
        let cleaned_lua = filter_embedding_noise(lua, NoiseProfile::for_language(Language::Lua));
        assert!(!cleaned_lua.contains("local M = {}"), "got: {cleaned_lua}");
        assert!(cleaned_lua.contains("M.foo"), "got: {cleaned_lua}");

        // Reverse: other languages must not strip.
        let other = filter_embedding_noise(java, NoiseProfile::for_language(Language::Go));
        assert!(other.contains("@Override"), "got: {other}");
    }

    #[test]
    fn test_empty_guard_keeps_original() {
        let input = "#!/bin/bash";
        let cleaned = filter_embedding_noise(input, NoiseProfile::for_language(Language::Bash));
        assert!(!cleaned.trim().is_empty(), "got: {cleaned}");
    }

    #[test]
    fn test_pending_languages_stay_off() {
        let input = "#pragma once\n\"use strict\";\n#!/bin/bash";
        for language in [
            Language::Dart,
            Language::Kotlin,
            Language::Scala,
            Language::Html,
            Language::Css,
            Language::Vue,
            Language::Svelte,
            Language::Ejs,
        ] {
            let cleaned = filter_embedding_noise(input, NoiseProfile::for_language(language));
            assert_eq!(cleaned, input, "got: {cleaned} for {language:?}");
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
            "filter-remap-guard",
            &["filterrg".to_string()],
            &Language::Go.to_string(),
        );
        let native = register_plugin_language("filter-native-guard", &["filterng".to_string()]);
        let input = "_ = x\nprocess()";
        let remapped_cleaned = filter_embedding_noise(
            input,
            NoiseProfile::for_language(Language::Custom(remapped)),
        );
        let host_cleaned = filter_embedding_noise(input, NoiseProfile::for_language(Language::Go));
        assert_eq!(
            remapped_cleaned, host_cleaned,
            "remapped language must inherit host filtering"
        );
        let native_cleaned =
            filter_embedding_noise(input, NoiseProfile::for_language(Language::Custom(native)));
        assert!(
            native_cleaned.contains("_ = x"),
            "native plugin language must stay conservative, got: {native_cleaned}"
        );
        clear_plugin_languages();
    }
}
