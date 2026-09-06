//! Per-language literal type vocabulary.
//!
//! Literal expressions (`42`, `"hi"`, `true`) carry no explicit type, so the
//! extractor and the inference engine synthesize one. The synthesized name
//! must use the target language's own vocabulary: `number` is only valid
//! for JavaScript/TypeScript, `int` for C-like languages, `i32` for Rust,
//! and so on. Unknown or plugin languages fall back to the previous generic
//! vocabulary to stay compatible.

use super::language::Language;

/// Shape of a literal expression, without language-specific naming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralKind {
    /// Integer literal (`42`, `0xFF`, `1_000`, `10u32`).
    Integer,
    /// Floating-point literal (`3.14`, `1e3`, `2.0f64`).
    Float,
    /// String literal (`"hi"`, `'hi'`, `` `hi` ``, `r"hi"`).
    String,
    /// Single-character literal (`'a'` in C-like languages).
    Char,
    /// Boolean literal (`true` / `false`).
    Boolean,
    /// Null-like literal (`null`, `None`, `nil`).
    Null,
    /// Array/list literal (`[1, 2]`).
    Array,
    /// Object/map literal (`{...}`).
    Object,
}

/// Return the idiomatic type name for a literal kind in a language.
///
/// The mapping mirrors what each language's own inference path already
/// produces for annotated code, so synthesized literal types agree with
/// declared ones (e.g. C++ `auto count = 42` yields `int`, matching
/// `int explicit_val = 10`).
pub fn literal_type_name(language: &Language, kind: LiteralKind) -> &'static str {
    let js_family = matches!(
        language,
        Language::JavaScript | Language::TypeScript | Language::Tsx | Language::Jsx
    );
    match kind {
        LiteralKind::Integer => {
            if js_family {
                return "number";
            }
            match language {
                Language::Rust => "i32",
                Language::Kotlin | Language::Scala => "Int",
                Language::Ruby => "Integer",
                Language::Lua => "number",
                _ => "int",
            }
        }
        LiteralKind::Float => {
            if js_family {
                return "number";
            }
            match language {
                Language::Rust => "f64",
                Language::Go => "float64",
                Language::Java | Language::CSharp | Language::C | Language::Cpp => "double",
                Language::Kotlin | Language::Scala => "Double",
                Language::Ruby => "Float",
                Language::Dart => "double",
                Language::Lua => "number",
                _ => "float",
            }
        }
        LiteralKind::String => match language {
            Language::Python => "str",
            Language::Rust
            | Language::Java
            | Language::CSharp
            | Language::Kotlin
            | Language::Scala
            | Language::Dart
            | Language::Ruby => "String",
            _ => "string",
        },
        LiteralKind::Char => {
            if js_family {
                return "string";
            }
            match language {
                Language::Kotlin | Language::Scala => "Char",
                Language::Python => "str",
                Language::Dart | Language::Ruby => "String",
                Language::Bash | Language::Lua | Language::Php => "string",
                _ => "char",
            }
        }
        LiteralKind::Boolean => {
            if js_family {
                return "boolean";
            }
            match language {
                Language::Java => "boolean",
                Language::Kotlin | Language::Scala => "Boolean",
                Language::Ruby | Language::Lua => "boolean",
                // Bare `true`/`false` in shell are builtin commands whose
                // results are consumed as strings.
                Language::Bash => "string",
                _ => "bool",
            }
        }
        LiteralKind::Null => match language {
            Language::Python => "None",
            Language::Lua | Language::Go => "nil",
            // Shell has no null value; `null`/`None` assignments are strings.
            Language::Bash => "string",
            _ => "null",
        },
        LiteralKind::Array => match language {
            Language::Python => "list",
            Language::Ruby => "Array",
            Language::Go => "slice",
            _ => "array",
        },
        LiteralKind::Object => match language {
            Language::Python => "dict",
            Language::Ruby => "Hash",
            Language::Lua => "table",
            Language::Go => "map",
            _ => "object",
        },
    }
}

/// Classify a numeric literal as integer or float.
///
/// Accepts digit separators (`1_000`), base prefixes (`0xFF`, `0o17`,
/// `0b11`) and Rust-style type suffixes (`10u32`, `2.0f64`) so they still
/// resolve instead of yielding no binding. Returns `None` for
/// non-numeric text.
pub fn classify_numeric_literal(trimmed: &str) -> Option<LiteralKind> {
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["0x", "0o", "0b"] {
        if lower.starts_with(prefix)
            && lower[prefix.len()..]
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == '_')
        {
            return Some(LiteralKind::Integer);
        }
    }
    // Strip a trailing Rust-style numeric suffix before parsing.
    let mut core = trimmed;
    let mut float_suffix = false;
    for suffix in [
        "f32", "f64", "i32", "i64", "u32", "u64", "i8", "u8", "i16", "u16", "isize", "usize", "f16",
    ] {
        if lower.ends_with(suffix) && trimmed.len() > suffix.len() {
            core = &trimmed[..trimmed.len() - suffix.len()];
            float_suffix = suffix.starts_with('f');
            break;
        }
    }
    let digits: String = core.chars().filter(|c| *c != '_').collect();
    if digits.is_empty() || digits.parse::<f64>().is_err() {
        return None;
    }
    if float_suffix || digits.contains('.') || digits.contains('e') || digits.contains('E') {
        Some(LiteralKind::Float)
    } else {
        Some(LiteralKind::Integer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_languages_do_not_leak_js_vocabulary() {
        for language in [
            Language::C,
            Language::Cpp,
            Language::CSharp,
            Language::Rust,
            Language::Go,
            Language::Java,
            Language::Kotlin,
            Language::Scala,
            Language::Python,
            Language::Ruby,
            Language::Php,
            Language::Dart,
        ] {
            assert_ne!(
                literal_type_name(&language, LiteralKind::Integer),
                "number",
                "{language:?} integer"
            );
            assert_ne!(
                literal_type_name(&language, LiteralKind::Float),
                "number",
                "{language:?} float"
            );
        }
    }

    #[test]
    fn test_js_family_keeps_number_vocabulary() {
        for language in [
            Language::JavaScript,
            Language::TypeScript,
            Language::Tsx,
            Language::Jsx,
        ] {
            assert_eq!(literal_type_name(&language, LiteralKind::Integer), "number");
            assert_eq!(literal_type_name(&language, LiteralKind::Float), "number");
            assert_eq!(
                literal_type_name(&language, LiteralKind::Boolean),
                "boolean"
            );
        }
    }

    #[test]
    fn test_classify_numeric_literal() {
        assert_eq!(classify_numeric_literal("42"), Some(LiteralKind::Integer));
        assert_eq!(classify_numeric_literal("3.14"), Some(LiteralKind::Float));
        assert_eq!(
            classify_numeric_literal("1_000"),
            Some(LiteralKind::Integer)
        );
        assert_eq!(classify_numeric_literal("0xFF"), Some(LiteralKind::Integer));
        assert_eq!(
            classify_numeric_literal("10u32"),
            Some(LiteralKind::Integer)
        );
        assert_eq!(classify_numeric_literal("2.0f64"), Some(LiteralKind::Float));
        assert_eq!(classify_numeric_literal("1e3"), Some(LiteralKind::Float));
        assert_eq!(classify_numeric_literal("hello"), None);
        assert_eq!(classify_numeric_literal(""), None);
    }

    #[test]
    fn test_idiomatic_names_per_language() {
        assert_eq!(
            literal_type_name(&Language::Cpp, LiteralKind::Integer),
            "int"
        );
        assert_eq!(
            literal_type_name(&Language::Rust, LiteralKind::Integer),
            "i32"
        );
        assert_eq!(
            literal_type_name(&Language::Kotlin, LiteralKind::Integer),
            "Int"
        );
        assert_eq!(
            literal_type_name(&Language::Python, LiteralKind::String),
            "str"
        );
        assert_eq!(
            literal_type_name(&Language::Java, LiteralKind::String),
            "String"
        );
        assert_eq!(
            literal_type_name(&Language::Python, LiteralKind::Null),
            "None"
        );
        assert_eq!(literal_type_name(&Language::Lua, LiteralKind::Null), "nil");
        assert_eq!(
            literal_type_name(&Language::Python, LiteralKind::Array),
            "list"
        );
    }
}
