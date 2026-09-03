//! Shared utilities for control flow type narrowing.
//!
//! Language-specific narrowing logic has been moved to the per-language
//! type inferers (python.rs, typescript.rs, rust.rs, go.rs). This module
//! retains only the shared helper functions used by those implementations.

/// Shared helper functions for control flow narrowing.
pub mod shared {
    /// Strip outer parentheses from a condition: `(...)` → `...`.
    pub fn strip_outer_parens(text: &str) -> &str {
        let text = text.trim();
        if text.starts_with('(') && text.ends_with(')') {
            &text[1..text.len() - 1]
        } else {
            text
        }
    }

    /// Extract arguments from a function call: `funcname(arg1, arg2)` → `"arg1, arg2"`.
    pub fn extract_call_args<'a>(text: &'a str, func_name: &str) -> Option<&'a str> {
        let text = text.trim();
        let prefix = format!("{func_name}(");
        let rest = text.strip_prefix(&prefix)?;
        let mut depth = 1;
        for (i, ch) in rest.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&rest[..i]);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Split two comma-separated arguments, respecting nested parens.
    pub fn split_two_args(args: &str) -> Option<(&str, &str)> {
        let mut depth = 0;
        for (i, ch) in args.char_indices() {
            match ch {
                '(' | '[' | '<' => depth += 1,
                ')' | ']' | '>' => depth -= 1,
                ',' if depth == 0 => {
                    return Some((&args[..i], &args[i + 1..]));
                }
                _ => {}
            }
        }
        None
    }

    /// Parse a type argument (single type or tuple of types).
    /// Returns the type name, using `|` as separator for tuples.
    pub fn parse_type_arg(arg: &str) -> Option<String> {
        let arg = arg.trim();

        if !arg.starts_with('(') {
            return Some(arg.to_string());
        }

        let inner = &arg[1..arg.len() - 1];
        let types: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
        if types.is_empty() {
            return None;
        }
        Some(types.join(" | "))
    }

    /// Extract balanced parentheses content starting from the first char.
    pub fn extract_balanced_parens(text: &str) -> Option<&str> {
        let text = text.trim();
        if !text.starts_with('(') {
            return None;
        }
        let mut depth = 0;
        for (i, ch) in text.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&text[1..i]);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Split a comparison expression: `a === b` → (a, "===", b).
    pub fn split_comparison(text: &str) -> Option<(String, String, String)> {
        let text = text.trim();

        for op in &["===", "==", "!==", "!=", ">=", "<=", ">", "<"] {
            if let Some(pos) = text.find(op) {
                let left = text[..pos].trim().to_string();
                let right = text[pos + op.len()..].trim().to_string();
                if !left.is_empty() && !right.is_empty() {
                    return Some((left, op.to_string(), right));
                }
            }
        }
        None
    }

    /// Parse a string literal: `"hello"` → `hello`.
    pub fn parse_string_literal(text: &str) -> Option<String> {
        let text = text.trim();
        if text.len() >= 2
            && ((text.starts_with('"') && text.ends_with('"'))
                || (text.starts_with('\'') && text.ends_with('\'')))
        {
            Some(text[1..text.len() - 1].to_string())
        } else {
            None
        }
    }

    /// Check if a string is a valid identifier.
    pub fn is_valid_ident(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        let mut chars = s.chars();
        if let Some(first) = chars.next() {
            if !first.is_alphabetic() && first != '_' {
                return false;
            }
        }
        chars.all(|c| c.is_alphanumeric() || c == '_')
    }
}

#[cfg(test)]
mod tests {
    use super::shared::*;

    #[test]
    fn test_strip_outer_parens() {
        assert_eq!(strip_outer_parens("(x)"), "x");
        assert_eq!(strip_outer_parens("x"), "x");
        assert_eq!(strip_outer_parens("((x))"), "(x)");
    }

    #[test]
    fn test_extract_balanced_parens() {
        assert_eq!(extract_balanced_parens("(val)"), Some("val"));
        assert_eq!(extract_balanced_parens("(a, b)"), Some("a, b"));
        assert_eq!(extract_balanced_parens("(a, (b, c))"), Some("a, (b, c)"));
    }

    #[test]
    fn test_parse_type_arg_single() {
        assert_eq!(parse_type_arg("MyClass"), Some("MyClass".to_string()));
    }

    #[test]
    fn test_parse_type_arg_tuple() {
        assert_eq!(
            parse_type_arg("(int, float)"),
            Some("int | float".to_string())
        );
    }

    #[test]
    fn test_split_two_args() {
        let (a, b) = split_two_args("x, MyClass").unwrap();
        assert_eq!(a.trim(), "x");
        assert_eq!(b.trim(), "MyClass");
    }

    #[test]
    fn test_split_two_args_nested_parens() {
        let (a, b) = split_two_args("x, (int, float)").unwrap();
        assert_eq!(a.trim(), "x");
        assert_eq!(b.trim(), "(int, float)");
    }

    #[test]
    fn test_is_valid_ident() {
        assert!(is_valid_ident("x"));
        assert!(is_valid_ident("my_var"));
        assert!(is_valid_ident("_private"));
        assert!(is_valid_ident("var123"));
        assert!(!is_valid_ident(""));
        assert!(!is_valid_ident("123abc"));
        assert!(!is_valid_ident("my-var"));
    }

    #[test]
    fn test_parse_string_literal() {
        assert_eq!(
            parse_string_literal("\"string\""),
            Some("string".to_string())
        );
        assert_eq!(parse_string_literal("'number'"), Some("number".to_string()));
        assert_eq!(parse_string_literal("string"), None);
    }

    #[test]
    fn test_split_comparison() {
        let (left, op, right) = split_comparison("x === \"string\"").unwrap();
        assert_eq!(left, "x");
        assert_eq!(op, "===");
        assert_eq!(right, "\"string\"");
    }

    // ==================== Additional tests ====================

    #[test]
    fn test_strip_outer_parens_empty() {
        assert_eq!(strip_outer_parens(""), "");
        assert_eq!(strip_outer_parens("  "), "");
    }

    #[test]
    fn test_strip_outer_parens_no_parens() {
        assert_eq!(strip_outer_parens("hello"), "hello");
        assert_eq!(strip_outer_parens("a + b"), "a + b");
    }

    #[test]
    fn test_extract_call_args_isinstance() {
        assert_eq!(
            extract_call_args("isinstance(x, str)", "isinstance"),
            Some("x, str")
        );
    }

    #[test]
    fn test_extract_call_args_no_match() {
        assert_eq!(extract_call_args("x + y", "isinstance"), None);
    }

    #[test]
    fn test_extract_call_args_nested_parens() {
        assert_eq!(
            extract_call_args("func(a, (b, c))", "func"),
            Some("a, (b, c)")
        );
    }

    #[test]
    fn test_split_comparison_eq() {
        let (left, op, right) = split_comparison("x == y").unwrap();
        assert_eq!(left, "x");
        assert_eq!(op, "==");
        assert_eq!(right, "y");
    }

    #[test]
    fn test_split_comparison_not_equal() {
        let (left, op, right) = split_comparison("x != y").unwrap();
        assert_eq!(left, "x");
        assert_eq!(op, "!=");
        assert_eq!(right, "y");
    }

    #[test]
    fn test_split_comparison_gt() {
        let (left, op, right) = split_comparison("x > 0").unwrap();
        assert_eq!(left, "x");
        assert_eq!(op, ">");
        assert_eq!(right, "0");
    }

    #[test]
    fn test_split_comparison_gte() {
        let (left, op, right) = split_comparison("x >= 0").unwrap();
        assert_eq!(left, "x");
        assert_eq!(op, ">=");
        assert_eq!(right, "0");
    }

    #[test]
    fn test_split_comparison_lt() {
        let (left, op, right) = split_comparison("x < 10").unwrap();
        assert_eq!(left, "x");
        assert_eq!(op, "<");
        assert_eq!(right, "10");
    }

    #[test]
    fn test_split_comparison_lte() {
        let (left, op, right) = split_comparison("x <= 10").unwrap();
        assert_eq!(left, "x");
        assert_eq!(op, "<=");
        assert_eq!(right, "10");
    }

    #[test]
    fn test_split_comparison_no_operator() {
        assert!(split_comparison("xyz").is_none());
    }

    #[test]
    fn test_extract_balanced_parens_unbalanced() {
        assert!(extract_balanced_parens("(abc").is_none());
    }

    #[test]
    fn test_extract_balanced_parens_not_starting_with_paren() {
        assert!(extract_balanced_parens("abc)").is_none());
    }

    #[test]
    fn test_parse_type_arg_empty_tuple() {
        assert_eq!(parse_type_arg("()"), Some("".to_string()));
    }

    #[test]
    fn test_split_two_args_single_arg() {
        assert!(split_two_args("x").is_none());
    }

    #[test]
    fn test_is_valid_ident_underscore_only() {
        assert!(is_valid_ident("_"));
    }

    #[test]
    fn test_parse_string_literal_empty() {
        assert_eq!(parse_string_literal("\"\""), Some("".to_string()));
        assert_eq!(parse_string_literal("''"), Some("".to_string()));
    }

    #[test]
    fn test_parse_string_literal_mismatched() {
        assert_eq!(parse_string_literal("\"hello'"), None);
        assert_eq!(parse_string_literal("'hello\""), None);
    }

    #[test]
    fn test_split_two_args_nested_brackets() {
        let (a, b) = split_two_args("x[0], y").unwrap();
        assert_eq!(a.trim(), "x[0]");
        assert_eq!(b.trim(), "y");
    }

    #[test]
    fn test_split_two_args_nested_angle() {
        let (a, b) = split_two_args("List<int>, String").unwrap();
        assert_eq!(a.trim(), "List<int>");
        assert_eq!(b.trim(), "String");
    }
}
