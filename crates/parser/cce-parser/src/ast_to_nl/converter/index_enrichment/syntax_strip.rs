//! Language-specific syntax noise stripping for index sidecar text.
//!
//! Each language enables only its own denoising shape through `NoiseProfile`.
//! Wrapper rules strip only the outer marker and keep the inner content;
//! line rules drop whole boilerplate lines. When stripping would empty a
//! non-empty fragment the original text is kept so no signal is lost.
//! Remapped plugin languages inherit their host profile; native plugin
//! languages keep the conservative all-off fallback.

use crate::ast_to_nl::noise::NoiseProfile;

/// Strip language-specific syntax noise from code fragments while preserving
/// implementation logic.
pub(super) fn strip_syntax_noise(text: &str, profile: NoiseProfile) -> String {
    let original_trimmed = text.trim();
    let mut current = if profile.unwrap_unsafe_blocks {
        strip_unsafe_blocks(text)
    } else {
        text.to_string()
    };
    if profile.strip_deref_markers {
        current = current.replace("&mut *", "").replace("&*", "");
    }
    if profile.strip_macro_repetition {
        current = strip_macro_repetitions(&current);
    }
    if profile.strip_c_guards {
        current = strip_c_guards(&current);
    }
    if profile.strip_python_boilerplate {
        current = strip_python_boilerplate(&current);
    }
    if profile.strip_js_wrapper {
        current = strip_js_wrapper(&current);
    }
    if profile.strip_go_boilerplate {
        current = strip_go_boilerplate(&current);
    }
    if profile.strip_bash_header {
        current = strip_bash_header(&current);
    }
    if profile.strip_java_boilerplate {
        current = strip_java_boilerplate(&current);
    }
    if profile.strip_csharp_markers {
        current = strip_csharp_markers(&current);
    }
    if profile.strip_php_markers {
        current = strip_php_markers(&current);
    }
    if profile.strip_ruby_markers {
        current = strip_ruby_markers(&current);
    }
    if profile.weaken_lua_return {
        current = weaken_lua_return(&current);
    }
    if profile.strip_kotlin_markers {
        current = strip_kotlin_markers(&current);
    }
    if profile.strip_scala_markers {
        current = strip_scala_markers(&current);
    }
    if profile.strip_dart_markers {
        current = strip_dart_markers(&current);
    }
    let mut cleaned = current.replace(" .", ".");
    cleaned = cleaned.replace(" ;", ";");
    // Collapse horizontal whitespace artifacts left by block removal while
    // preserving newline structure of the code fragment.
    let normalized = cce_utils::normalize_whitespace_preserving_newlines(&cleaned)
        .trim()
        .to_string();
    if normalized.trim().is_empty() && !original_trimmed.is_empty() {
        return original_trimmed.to_string();
    }
    normalized
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

/// Strip C/C++ header guards, pragma-once directives and `extern "C"`
/// wrappers while keeping the guarded content.
fn strip_c_guards(text: &str) -> String {
    let unwrapped = strip_extern_c_blocks(text);
    let mut out = Vec::with_capacity(unwrapped.lines().count());
    for line in unwrapped.lines() {
        let trimmed = line.trim();
        if is_pragma_once(trimmed) {
            continue;
        }
        if is_header_guard_open(trimmed) {
            continue;
        }
        if is_header_guard_define(trimmed) {
            continue;
        }
        if is_cplusplus_guard(trimmed) {
            continue;
        }
        if is_guard_endif(trimmed) {
            continue;
        }
        if let Some(rest) = strip_extern_c_prefix(trimmed) {
            if rest.trim().is_empty() {
                continue;
            }
            out.push(rest);
            continue;
        }
        out.push(line.to_string());
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn is_pragma_once(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("#pragma once")
}

fn is_header_guard_open(line: &str) -> bool {
    line.starts_with("#ifndef ") || line.starts_with("#ifndef\t")
}

fn is_header_guard_define(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("#define ") else {
        let Some(rest_tab) = line.strip_prefix("#define\t") else {
            return false;
        };
        return is_guard_macro(rest_tab);
    };
    is_guard_macro(rest)
}

fn is_guard_macro(name: &str) -> bool {
    let token = name.split_whitespace().next().unwrap_or("").trim();
    if token.is_empty() {
        return false;
    }
    token.ends_with("_H")
        || token.ends_with("_HPP")
        || token.ends_with("_HXX")
        || token.ends_with("_H_")
        || token.contains("_INCLUDED")
        || token.contains("_GUARD")
}

fn is_cplusplus_guard(line: &str) -> bool {
    line.starts_with("#ifdef __cplusplus")
        || line.starts_with("#if defined(__cplusplus")
        || line.starts_with("#if defined __cplusplus")
}

fn is_guard_endif(line: &str) -> bool {
    if !(line.starts_with("#endif") || line.starts_with("#else")) {
        return false;
    }
    line.contains("__cplusplus")
        || line.contains("_H")
        || line.contains("_GUARD")
        || line.contains("_INCLUDED")
        || line.trim() == "#endif"
        || line.trim() == "#else"
}

fn strip_extern_c_prefix(line: &str) -> Option<String> {
    let rest = line.strip_prefix("extern \"C\"")?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('{').unwrap_or(rest).trim_start();
    Some(rest.to_string())
}

/// Remove `extern "C" { ... }` wrappers, keeping the inner content.
fn strip_extern_c_blocks(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    let marker = b"extern \"C\"";
    while i < bytes.len() {
        if !text.is_char_boundary(i) {
            i = cce_utils::text::ceil_char_boundary(text, i);
            continue;
        }
        let is_start = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if is_start && i + marker.len() <= bytes.len() && bytes[i..i + marker.len()] == *marker {
            let mut j = i + marker.len();
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

/// Strip Python coding declarations, `__future__` imports and the
/// `if __name__ == "__main__"` guard line while keeping the guarded body.
fn strip_python_boilerplate(text: &str) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        let trimmed = line.trim();
        if is_python_coding_line(trimmed) {
            continue;
        }
        if is_python_future_import(trimmed) {
            continue;
        }
        if is_python_main_guard(trimmed) {
            continue;
        }
        out.push(line);
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn is_python_coding_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.contains("coding:") || lower.contains("coding=")
}

fn is_python_future_import(line: &str) -> bool {
    line.starts_with("from __future__ import ")
}

fn is_python_main_guard(line: &str) -> bool {
    let normalized: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    normalized.starts_with("if__name__==")
        && (normalized.contains("\"__main__\"") || normalized.contains("'__main__'"))
}

/// Strip JavaScript-family `'use strict'` directives and `export default`
/// wrappers while keeping the exported content.
fn strip_js_wrapper(text: &str) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        let trimmed = line.trim();
        if is_use_strict_line(trimmed) {
            continue;
        }
        if let Some(rest) = strip_export_default_prefix(line) {
            if rest.trim().is_empty() {
                continue;
            }
            out.push(rest);
            continue;
        }
        out.push(line.to_string());
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn is_use_strict_line(line: &str) -> bool {
    let t = line.trim().trim_end_matches(';').trim();
    t == "\"use strict\"" || t == "'use strict'"
}

fn strip_export_default_prefix(line: &str) -> Option<String> {
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

/// Weaken Go blank-identifier assignments and `if err != nil` guard lines.
fn strip_go_boilerplate(text: &str) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        let trimmed = line.trim();
        if is_go_blank_assign(trimmed) {
            continue;
        }
        if let Some(kept) = strip_go_err_guard(trimmed) {
            if kept.trim().is_empty() {
                continue;
            }
            out.push(kept);
            continue;
        }
        out.push(line.to_string());
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn is_go_blank_assign(line: &str) -> bool {
    let t = line.trim();
    if t.starts_with("var _") {
        return t.contains('=');
    }
    if t.starts_with('_') {
        let after: String = t.chars().skip(1).collect();
        let after = after.trim_start();
        return after.starts_with('=') || after.starts_with(',') || after.starts_with(':');
    }
    false
}

fn strip_go_err_guard(line: &str) -> Option<String> {
    let t = line.trim();
    if !t.starts_with("if err != nil") {
        return None;
    }
    let rest = t.strip_prefix("if err != nil").unwrap_or("").trim();
    if rest.is_empty() {
        return Some(String::new());
    }
    let body = rest.strip_prefix('{').unwrap_or(rest).trim();
    let body = body.strip_suffix('}').unwrap_or(body).trim();
    Some(body.to_string())
}

/// Strip Bash shebang and `set` strict-mode switch lines.
fn strip_bash_header(text: &str) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#!") {
            continue;
        }
        if is_bash_strict_set(trimmed) {
            continue;
        }
        out.push(line.to_string());
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn is_bash_strict_set(line: &str) -> bool {
    let t = line.trim();
    let Some(rest) = t.strip_prefix("set ") else {
        if t == "set" {
            return false;
        }
        return false;
    };
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
    true
}

/// Weaken Java `@Override` markers and trivial getter/setter bodies.
fn strip_java_boilerplate(text: &str) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "@Override" {
            continue;
        }
        if is_java_trivial_accessor(trimmed) {
            continue;
        }
        out.push(line.to_string());
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn is_java_trivial_accessor(line: &str) -> bool {
    let t = line.trim().trim_end_matches(';').trim();
    if let Some(rest) = t.strip_prefix("return this.") {
        return !rest.trim().is_empty() && rest.chars().all(|c| c.is_alphanumeric() || c == '_');
    }
    if let Some(rest) = t.strip_prefix("return ") {
        let r = rest.trim();
        return !r.is_empty()
            && r.chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.');
    }
    if t.starts_with("this.") && t.contains('=') {
        let parts: Vec<&str> = t.splitn(2, '=').collect();
        if parts.len() == 2 {
            let left = parts[0].trim().strip_prefix("this.").unwrap_or("").trim();
            let right = parts[1].trim();
            let left_ok = !left.is_empty() && left.chars().all(|c| c.is_alphanumeric() || c == '_');
            let right_ok = !right.is_empty()
                && right
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '.');
            return left_ok && right_ok;
        }
    }
    false
}

/// Strip C# `#region` markers and assembly attributes.
fn strip_csharp_markers(text: &str) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        let trimmed = line.trim();
        if is_csharp_region(trimmed) {
            continue;
        }
        if is_assembly_attribute(trimmed) {
            continue;
        }
        out.push(line.to_string());
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn is_csharp_region(line: &str) -> bool {
    line.starts_with("#region") || line.starts_with("#endregion")
}

fn is_assembly_attribute(line: &str) -> bool {
    line.starts_with("[assembly:")
}

/// Strip PHP open/close tags while keeping namespace names.
fn strip_php_markers(text: &str) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "<?php" || trimmed == "<?" || trimmed == "?>" {
            continue;
        }
        if let Some(name) = strip_php_namespace_prefix(trimmed) {
            if name.trim().is_empty() {
                continue;
            }
            out.push(name);
            continue;
        }
        out.push(line.to_string());
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn strip_php_namespace_prefix(line: &str) -> Option<String> {
    let rest = line.strip_prefix("namespace ")?;
    let name = rest.trim().trim_end_matches(';').trim();
    if name.is_empty() {
        return Some(String::new());
    }
    Some(name.to_string())
}

/// Strip Ruby magic comments and visibility section markers.
fn strip_ruby_markers(text: &str) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        let trimmed = line.trim();
        if is_ruby_magic_comment(trimmed) {
            continue;
        }
        if is_ruby_visibility_marker(trimmed) {
            continue;
        }
        out.push(line.to_string());
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn is_ruby_magic_comment(line: &str) -> bool {
    let t = line.trim_start();
    if !t.starts_with('#') {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    lower.contains("frozen_string_literal")
        || lower.contains("encoding:")
        || lower.contains("encoding=")
        || lower.contains("coding:")
        || lower.contains("coding=")
}

fn is_ruby_visibility_marker(line: &str) -> bool {
    matches!(line.trim(), "private" | "protected" | "public")
}

/// Strip Kotlin JVM-interop annotation lines.
///
/// Sampling basis: in-repo Kotlin snippets use these markers on factory
/// and interop declarations; the annotated declaration keeps its own text
/// so no business logic leaves the fragment. Whole-line matches only, so
/// a marker sharing a line with a declaration is never touched.
fn strip_kotlin_markers(text: &str) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        if is_kotlin_interop_annotation(line.trim()) {
            continue;
        }
        out.push(line.to_string());
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn is_kotlin_interop_annotation(line: &str) -> bool {
    const BARE: &[&str] = &["@JvmStatic", "@JvmOverloads", "@JvmField", "@JvmSynthetic"];
    if BARE.contains(&line) {
        return true;
    }
    const WITH_ARGS: &[&str] = &["@JvmName", "@Throws"];
    WITH_ARGS.iter().any(|marker| {
        line == *marker
            || line
                .strip_prefix(marker)
                .is_some_and(|rest| rest.starts_with('('))
    })
}

/// Strip Scala compiler-hint annotation lines.
///
/// Sampling basis mirrors the Kotlin markers: storage and codegen hints
/// (`@inline`, `@specialized`, `@transient`, `@volatile`,
/// `@SerialVersionUID`) carry no domain signal. Semantics-bearing markers
/// (`@throws`, `sealed`, `implicit`, `override`) are intentionally kept.
/// Whole-line matches only.
fn strip_scala_markers(text: &str) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        if is_scala_compiler_hint(line.trim()) {
            continue;
        }
        out.push(line.to_string());
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn is_scala_compiler_hint(line: &str) -> bool {
    const BARE: &[&str] = &["@inline", "@specialized", "@transient", "@volatile"];
    if BARE.contains(&line) {
        return true;
    }
    line == "@SerialVersionUID" || line.starts_with("@SerialVersionUID(")
}

/// Strip Dart override markers and lint-suppression comments.
///
/// `@override` mirrors the Java rule (the annotated member keeps its
/// text); `// ignore:` directives mirror the Ruby magic-comment rule.
/// Whole-line matches only, so trailing comments keep their line.
fn strip_dart_markers(text: &str) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "@override" {
            continue;
        }
        if is_dart_ignore_comment(trimmed) {
            continue;
        }
        out.push(line.to_string());
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn is_dart_ignore_comment(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("// ignore_for_file:") || lower.starts_with("// ignore:")
}

/// Weaken Lua module return-table wrappers while keeping return names.
fn weaken_lua_return(text: &str) -> String {
    let mut out = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        let trimmed = line.trim();
        if is_lua_empty_table_assign(trimmed) {
            continue;
        }
        if let Some(inner) = strip_lua_return_table(trimmed) {
            if inner.trim().is_empty() {
                continue;
            }
            out.push(inner);
            continue;
        }
        if is_lua_bare_return(trimmed) {
            continue;
        }
        out.push(line.to_string());
    }
    let joined = out.join("\n");
    if joined.trim().is_empty() && !text.trim().is_empty() {
        return text.to_string();
    }
    joined
}

fn is_lua_empty_table_assign(line: &str) -> bool {
    let t = line.trim().trim_end_matches(';').trim();
    let Some(rest) = t.strip_prefix("local ") else {
        return false;
    };
    let Some(eq_pos) = rest.find('=') else {
        return false;
    };
    let rhs = rest[eq_pos + 1..].trim();
    rhs == "{}"
}

fn is_lua_bare_return(line: &str) -> bool {
    let t = line.trim().trim_end_matches(';').trim();
    let Some(rest) = t.strip_prefix("return ") else {
        return false;
    };
    let name = rest.trim();
    !name.is_empty()
        && !name.contains('{')
        && !name.contains('}')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

fn strip_lua_return_table(line: &str) -> Option<String> {
    let t = line.trim();
    let rest = t.strip_prefix("return")?;
    if rest.is_empty() {
        return None;
    }
    let next = rest.chars().next().unwrap_or(' ');
    if !next.is_whitespace() {
        return None;
    }
    let rest = rest.trim_start();
    let inner = rest.strip_prefix('{')?.strip_suffix('}')?;
    let inner = inner.trim().trim_end_matches(';').trim();
    Some(inner.to_string())
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

    #[test]
    fn strip_c_guards_denoise() {
        let source = "#ifndef FOO_H\n#define FOO_H\nint value;\n#endif\n#pragma once";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::C));
        assert!(cleaned.contains("int value;"), "got: {cleaned}");
        assert!(!cleaned.contains("#ifndef"), "got: {cleaned}");
        assert!(!cleaned.contains("pragma once"), "got: {cleaned}");
    }

    #[test]
    fn strip_c_guards_keep_content_for_cpp() {
        let source = "#ifdef __cplusplus\nextern \"C\" {\nvoid foo();\n}\n#endif";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Cpp));
        assert!(cleaned.contains("void foo();"), "got: {cleaned}");
        assert!(!cleaned.contains("extern"), "got: {cleaned}");
        assert!(!cleaned.contains("__cplusplus"), "got: {cleaned}");
    }

    #[test]
    fn strip_c_guards_reverse_keeps_real_defines() {
        let source = "#define MAX_SIZE 1024\nint buf[MAX_SIZE];";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::C));
        assert!(cleaned.contains("MAX_SIZE"), "got: {cleaned}");
        assert!(cleaned.contains("int buf"), "got: {cleaned}");
    }

    #[test]
    fn strip_python_boilerplate_denoise() {
        let source =
            "# -*- coding: utf-8 -*-\nfrom __future__ import annotations\ndef foo():\n    pass";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Python));
        assert!(cleaned.contains("def foo():"), "got: {cleaned}");
        assert!(!cleaned.contains("coding"), "got: {cleaned}");
        assert!(!cleaned.contains("__future__"), "got: {cleaned}");
    }

    #[test]
    fn strip_python_main_guard_denoise() {
        let source = "if __name__ == \"__main__\":\n    main()\n    setup()";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Python));
        assert!(cleaned.contains("main()"), "got: {cleaned}");
        assert!(!cleaned.contains("__main__"), "got: {cleaned}");
    }

    #[test]
    fn strip_python_reverse_keeps_real_code() {
        let source = "x = 1\n# useful comment\nprint(x)";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Python));
        assert!(cleaned.contains("x = 1"), "got: {cleaned}");
        assert!(cleaned.contains("print(x)"), "got: {cleaned}");
    }

    #[test]
    fn strip_js_wrapper_denoise() {
        let source = "\"use strict\";\nexport default function foo() {\nreturn 1;\n}";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::JavaScript));
        assert!(!cleaned.contains("use strict"), "got: {cleaned}");
        assert!(!cleaned.contains("export default"), "got: {cleaned}");
        assert!(cleaned.contains("function foo()"), "got: {cleaned}");
    }

    #[test]
    fn strip_js_wrapper_reverse_keeps_real_export() {
        let source = "export const value = 42;\nfunction helper() {}";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::TypeScript));
        assert!(cleaned.contains("export const value"), "got: {cleaned}");
        assert!(cleaned.contains("helper"), "got: {cleaned}");
    }

    #[test]
    fn strip_go_boilerplate_denoise() {
        let source = "_ = x\nif err != nil {\nreturn err\n}\nprocess()";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Go));
        assert!(cleaned.contains("process()"), "got: {cleaned}");
        assert!(!cleaned.contains("_ = x"), "got: {cleaned}");
        assert!(!cleaned.contains("if err != nil"), "got: {cleaned}");
    }

    #[test]
    fn strip_go_reverse_keeps_real_logic() {
        let source = "result := compute(input)\nif result > 0 {\nemit(result)\n}";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Go));
        assert!(cleaned.contains("compute(input)"), "got: {cleaned}");
        assert!(cleaned.contains("emit(result)"), "got: {cleaned}");
    }

    #[test]
    fn strip_bash_header_denoise() {
        let source = "#!/bin/bash\nset -euo pipefail\necho hello";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Bash));
        assert!(cleaned.contains("echo hello"), "got: {cleaned}");
        assert!(!cleaned.contains("#!/bin/bash"), "got: {cleaned}");
        assert!(!cleaned.contains("set -euo"), "got: {cleaned}");
    }

    #[test]
    fn strip_bash_reverse_keeps_real_set() {
        let source = "set foo=bar\necho done";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Bash));
        assert!(cleaned.contains("set foo=bar"), "got: {cleaned}");
    }

    #[test]
    fn strip_java_boilerplate_denoise() {
        let source = "@Override\npublic String getName() {\nreturn this.name;\n}";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Java));
        assert!(!cleaned.contains("@Override"), "got: {cleaned}");
        assert!(cleaned.contains("getName"), "got: {cleaned}");
    }

    #[test]
    fn strip_java_reverse_keeps_real_body() {
        let source = "public int compute(int x) {\nint y = x * 2;\nreturn y + 1;\n}";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Java));
        assert!(cleaned.contains("compute"), "got: {cleaned}");
        assert!(cleaned.contains("x * 2"), "got: {cleaned}");
    }

    #[test]
    fn strip_csharp_markers_denoise() {
        let source =
            "#region Helpers\nvoid foo() {}\n#endregion\n[assembly: AssemblyVersion(\"1.0\")]";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::CSharp));
        assert!(cleaned.contains("void foo()"), "got: {cleaned}");
        assert!(!cleaned.contains("#region"), "got: {cleaned}");
        assert!(!cleaned.contains("[assembly:"), "got: {cleaned}");
    }

    #[test]
    fn strip_csharp_reverse_keeps_real_attr() {
        let source = "[Test]\nvoid test_foo() {}";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::CSharp));
        assert!(cleaned.contains("[Test]"), "got: {cleaned}");
    }

    #[test]
    fn strip_php_markers_denoise() {
        let source = "<?php\nnamespace App\\Controller;\nfunction index() {}";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Php));
        assert!(cleaned.contains("function index()"), "got: {cleaned}");
        assert!(!cleaned.contains("<?php"), "got: {cleaned}");
        assert!(cleaned.contains("App\\Controller"), "got: {cleaned}");
        assert!(!cleaned.contains("namespace App"), "got: {cleaned}");
    }

    #[test]
    fn strip_php_reverse_keeps_real_code() {
        let source = "function render($x) {\nreturn htmlspecialchars($x);\n}";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Php));
        assert!(cleaned.contains("htmlspecialchars"), "got: {cleaned}");
    }

    #[test]
    fn strip_ruby_markers_denoise() {
        let source = "# frozen_string_literal: true\nclass Foo\nprivate\ndef bar; end\nend";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Ruby));
        assert!(cleaned.contains("def bar"), "got: {cleaned}");
        assert!(!cleaned.contains("frozen_string_literal"), "got: {cleaned}");
        assert!(!cleaned.contains("\nprivate\n"), "got: {cleaned}");
    }

    #[test]
    fn strip_ruby_reverse_keeps_real_comment() {
        let source = "# calculates total\ndef total(xs)\nxs.sum\nend";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Ruby));
        assert!(cleaned.contains("calculates total"), "got: {cleaned}");
        assert!(cleaned.contains("xs.sum"), "got: {cleaned}");
    }

    #[test]
    fn strip_lua_return_denoise() {
        let source = "local M = {}\nfunction M.foo() end\nreturn M";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Lua));
        assert!(cleaned.contains("M.foo"), "got: {cleaned}");
        assert!(!cleaned.contains("local M = {}"), "got: {cleaned}");
        assert!(!cleaned.contains("return M"), "got: {cleaned}");
    }

    #[test]
    fn strip_lua_table_unwraps_inner() {
        let source = "return { foo = foo, bar = bar }";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Lua));
        assert!(cleaned.contains("foo = foo"), "got: {cleaned}");
        assert!(!cleaned.contains("return {"), "got: {cleaned}");
    }

    #[test]
    fn strip_lua_reverse_keeps_real_return() {
        let source = "function compute(x)\nreturn x * 2\nend";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Lua));
        assert!(cleaned.contains("return x * 2"), "got: {cleaned}");
    }

    #[test]
    fn strip_kotlin_markers_denoise() {
        let source = "@JvmStatic\nfun create(name: String): Widget {\nreturn Widget(name)\n}";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Kotlin));
        assert!(!cleaned.contains("@JvmStatic"), "got: {cleaned}");
        assert!(cleaned.contains("fun create"), "got: {cleaned}");
        assert!(cleaned.contains("Widget(name)"), "got: {cleaned}");
    }

    #[test]
    fn strip_kotlin_reverse_keeps_same_line_marker() {
        // A marker sharing its line with a declaration is never touched.
        let source = "@JvmStatic fun create(): Widget";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Kotlin));
        assert!(cleaned.contains("@JvmStatic"), "got: {cleaned}");
    }

    #[test]
    fn strip_scala_markers_denoise() {
        let source = "@inline\ndef hash(s: String): Int {\nvar h = 0\nh\n}";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Scala));
        assert!(!cleaned.contains("@inline"), "got: {cleaned}");
        assert!(cleaned.contains("def hash"), "got: {cleaned}");
        assert!(cleaned.contains("var h = 0"), "got: {cleaned}");
    }

    #[test]
    fn strip_scala_reverse_keeps_semantic_markers() {
        // Exception contracts stay: only compiler and storage hints strip.
        let source = "@throws[IOException]\ndef load(): Data";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Scala));
        assert!(cleaned.contains("@throws"), "got: {cleaned}");
        assert!(cleaned.contains("def load"), "got: {cleaned}");
    }

    #[test]
    fn strip_dart_markers_denoise() {
        let source = "@override\nString describe() {\nreturn name;\n}\n// ignore: unused_element";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Dart));
        assert!(!cleaned.contains("@override"), "got: {cleaned}");
        assert!(!cleaned.contains("ignore:"), "got: {cleaned}");
        assert!(cleaned.contains("describe()"), "got: {cleaned}");
        assert!(cleaned.contains("return name;"), "got: {cleaned}");
    }

    #[test]
    fn strip_dart_reverse_keeps_trailing_comment() {
        // A marker with trailing content is not a pure marker line.
        let source = "@override // reason\nString describe()";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Dart));
        assert!(cleaned.contains("@override"), "got: {cleaned}");
        assert!(cleaned.contains("describe()"), "got: {cleaned}");
    }

    #[test]
    fn strip_empty_guard_keeps_original() {
        let source = "#!/bin/bash";
        let cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Bash));
        assert!(!cleaned.trim().is_empty(), "got: {cleaned}");
        assert!(cleaned.contains("#!/bin/bash"), "got: {cleaned}");
    }

    /// Sync guard: the noise profile is an explicit expectation table, not
    /// a silent default. Every parsable language must have a conscious entry.
    #[test]
    fn noise_profile_covers_all_parsable_languages() {
        use std::collections::HashSet;
        let enabled: HashSet<Language> = [
            Language::Rust,
            Language::C,
            Language::Cpp,
            Language::Python,
            Language::JavaScript,
            Language::TypeScript,
            Language::Jsx,
            Language::Tsx,
            Language::Go,
            Language::Bash,
            Language::Java,
            Language::CSharp,
            Language::Php,
            Language::Ruby,
            Language::Lua,
            Language::Kotlin,
            Language::Scala,
            Language::Dart,
        ]
        .into_iter()
        .collect();
        let all_off = [
            Language::Html,
            Language::Css,
            Language::Vue,
            Language::Svelte,
            Language::Ejs,
        ];
        for language in Language::ast_parsable_languages() {
            let profile = NoiseProfile::for_language(*language);
            if enabled.contains(language) {
                assert!(
                    profile.has_language_rules(),
                    "{language:?} listed as enabled but has no rules"
                );
            } else {
                assert!(
                    all_off.contains(language),
                    "{language:?} is parsable but missing from the noise expectation: \
                     add it to enabled or all_off explicitly"
                );
                assert_eq!(
                    profile,
                    NoiseProfile::none(),
                    "{language:?} must stay all-off by design"
                );
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
            "strip-remap-guard",
            &["striprg".to_string()],
            &Language::Rust.to_string(),
        );
        let native = register_plugin_language("strip-native-guard", &["stripng".to_string()]);
        let source = "unsafe { &mut *self.inner.get() }.as_mut()";
        let remapped_cleaned = strip_syntax_noise(
            source,
            NoiseProfile::for_language(Language::Custom(remapped)),
        );
        let rust_cleaned = strip_syntax_noise(source, NoiseProfile::for_language(Language::Rust));
        assert_eq!(
            remapped_cleaned, rust_cleaned,
            "remapped language must inherit host stripping"
        );
        let native_cleaned =
            strip_syntax_noise(source, NoiseProfile::for_language(Language::Custom(native)));
        assert!(
            native_cleaned.contains("unsafe"),
            "native plugin language must stay conservative, got: {native_cleaned}"
        );
        clear_plugin_languages();
    }
}
