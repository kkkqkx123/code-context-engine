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

        // Three-character operators must precede their two-character
        // prefixes: `!==` contains `==`, so matching `==` first would
        // mis-split `a !== b` into `a !` / `b`.
        for op in &["===", "!==", "==", "!=", ">=", "<=", ">", "<"] {
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

    /// Split a condition on top-level `&&` conjunctions.
    ///
    /// Each conjunct of `A && B` holds in the then-branch, so narrowing can
    /// process them independently. Without this,
    /// `typeof a === "number" && typeof b === "number"` parses the right
    /// side as the pseudo-type `number" && typeof b === "number"`.
    /// `||` is deliberately not split: no single disjunct holds alone.
    /// Splitting respects nesting (`()[]{}<>`) and string literals.
    pub fn split_top_level_conjuncts(text: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let bytes = text.as_bytes();
        let mut depth = 0i32;
        let mut quote: Option<u8> = None;
        let mut start = 0usize;
        let mut i = 0usize;
        while i < bytes.len() {
            let b = bytes[i];
            if let Some(q) = quote {
                if b == b'\\' {
                    i += 2;
                    continue;
                }
                if b == q {
                    quote = None;
                }
                i += 1;
                continue;
            }
            match b {
                b'"' | b'\'' | b'`' => {
                    quote = Some(b);
                    i += 1;
                }
                b'(' | b'[' | b'{' | b'<' => {
                    depth += 1;
                    i += 1;
                }
                b')' | b']' | b'}' | b'>' => {
                    depth -= 1;
                    i += 1;
                }
                b'&' if depth == 0 && bytes.get(i + 1) == Some(&b'&') => {
                    parts.push(text[start..i].trim());
                    i += 2;
                    start = i;
                }
                _ => {
                    i += 1;
                }
            }
        }
        parts.push(text[start..].trim());
        parts.retain(|p| !p.is_empty());
        parts
    }

    /// Split a condition on top-level word conjunctions (`and`).
    ///
    /// Same contract as [`split_top_level_conjuncts`] for Python-style
    /// `A and B` guards.
    pub fn split_top_level_word_conjuncts<'a>(text: &'a str, word: &str) -> Vec<&'a str> {
        let mut parts = Vec::new();
        let bytes = text.as_bytes();
        let w = word.as_bytes();
        let mut depth = 0i32;
        let mut quote: Option<u8> = None;
        let mut start = 0usize;
        let mut i = 0usize;
        while i < bytes.len() {
            let b = bytes[i];
            if let Some(q) = quote {
                if b == b'\\' {
                    i += 2;
                    continue;
                }
                if b == q {
                    quote = None;
                }
                i += 1;
                continue;
            }
            match b {
                b'"' | b'\'' | b'`' => {
                    quote = Some(b);
                    i += 1;
                }
                b'(' | b'[' | b'{' | b'<' => {
                    depth += 1;
                    i += 1;
                }
                b')' | b']' | b'}' | b'>' => {
                    depth -= 1;
                    i += 1;
                }
                _ if depth == 0 && text[i..].starts_with(word) => {
                    let before = i.checked_sub(1).map(|p| bytes[p]);
                    let after = bytes.get(i + w.len()).copied();
                    let boundary_before =
                        before.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == b'_'));
                    let boundary_after =
                        after.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == b'_'));
                    if boundary_before && boundary_after {
                        parts.push(text[start..i].trim());
                        i += w.len();
                        start = i;
                        continue;
                    }
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }
        parts.push(text[start..].trim());
        parts.retain(|p| !p.is_empty());
        parts
    }

    /// Whether an `if` fact carries an `else` continuation.
    ///
    /// Delegates to the shared fact-text scan so recorded ranges and
    /// text-level detection never disagree.
    pub fn has_else_branch(text: &str) -> bool {
        cce_types::has_outer_else_branch(text)
    }

    /// Byte offset of the outer `else` keyword within the fact text.
    ///
    /// Delegates to the shared fact-text scan so recorded ranges and
    /// text-level detection never disagree. Returns `None` when no outer
    /// `else` continuation exists.
    pub fn find_else_offset(text: &str) -> Option<usize> {
        cce_types::find_outer_else_offset(text)
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
    fn test_split_top_level_conjuncts() {
        assert_eq!(
            split_top_level_conjuncts("typeof a === \"number\" && typeof b === \"number\""),
            vec!["typeof a === \"number\"", "typeof b === \"number\""]
        );
        assert_eq!(split_top_level_conjuncts("x != null"), vec!["x != null"]);
        // `||` is not split; nested `&&` stays intact.
        assert_eq!(split_top_level_conjuncts("a || b"), vec!["a || b"]);
        assert_eq!(
            split_top_level_conjuncts("f(a && b) && c"),
            vec!["f(a && b)", "c"]
        );
        // `&&` inside strings is not a separator.
        assert_eq!(
            split_top_level_conjuncts("x === \"a && b\" && y"),
            vec!["x === \"a && b\"", "y"]
        );
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

    #[test]
    fn test_has_else_branch_with_block() {
        assert!(has_else_branch(
            "if (x instanceof String) { use(x); } else { other(x); }"
        ));
    }

    #[test]
    fn test_has_else_branch_braceless() {
        assert!(has_else_branch(
            "if (x != null) return x; else return null;"
        ));
    }

    #[test]
    fn test_has_else_branch_without_else() {
        assert!(!has_else_branch("if (x instanceof String) { use(x); }"));
    }

    #[test]
    fn test_has_else_branch_nested_else_does_not_count() {
        assert!(!has_else_branch("if (a) { if (b) { x(); } else { y(); } }"));
    }

    #[test]
    fn test_has_else_branch_string_literal_does_not_count() {
        assert!(!has_else_branch("if (a) { log(\"else\"); }"));
    }

    #[test]
    fn test_has_else_branch_else_if_chain_counts() {
        assert!(has_else_branch("if (a) { x(); } else if (b) { y(); }"));
    }

    #[test]
    fn test_find_else_offset_points_at_else_keyword() {
        let text = "if (x instanceof String) { use(x); } else { other(x); }";
        let offset = find_else_offset(text).expect("else offset exists");
        assert_eq!(&text[offset..offset + 4], "else");
        assert!(has_else_branch(text));
    }

    #[test]
    fn test_find_else_offset_without_else_is_none() {
        assert!(find_else_offset("if (x instanceof String) { use(x); }").is_none());
    }
}
