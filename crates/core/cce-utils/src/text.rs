//! Text utilities for common string operations

/// Collapse all whitespace into single spaces and trim
///
/// Replaces all sequences of whitespace characters (spaces, tabs, newlines)
/// with a single space, then trims leading and trailing whitespace.
///
/// # Example
///
/// ```
/// use cce_core::utils::text::normalize_whitespace;
///
/// let text = "  hello   world  \n\t  test  ";
/// assert_eq!(normalize_whitespace(text), "hello world test");
/// ```
pub fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Normalize a code fragment into a more natural-language-friendly form.
///
/// This helper keeps code-like identifiers visible while softening common
/// syntax into tokens that are easier to read in index-oriented text.
pub fn normalize_code_fragment(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut result = text.to_string();
    result = result.replace("::", ":");
    result = result.replace("<<=", " shift left assign ");
    result = result.replace(">>=", " shift right assign ");
    result = result.replace("<<", " shift left ");
    result = result.replace(">>", " shift right ");
    result = result.replace("=>", " maps to ");
    result = result.replace(['`', '"'], "");
    normalize_whitespace(&result)
}

/// Collapse whitespace within lines, preserving line breaks
///
/// Useful for docstrings where line structure should be maintained
/// but excess indentation or spacing should be cleaned up.
///
/// # Example
///
/// ```
/// use cce_core::utils::text::normalize_whitespace_preserving_newlines;
///
/// let text = "  line  one  \n  line   two  ";
/// let result = normalize_whitespace_preserving_newlines(text);
/// assert_eq!(result, "line one\nline two");
/// ```
pub fn normalize_whitespace_preserving_newlines(text: &str) -> String {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Check if text is empty or contains only whitespace
///
/// # Example
///
/// ```
/// use cce_core::utils::text::is_blank;
///
/// assert!(is_blank(""));
/// assert!(is_blank("   "));
/// assert!(is_blank("\n\t"));
/// assert!(!is_blank("hello"));
/// ```
pub fn is_blank(text: &str) -> bool {
    text.trim().is_empty()
}

/// Split camelCase / PascalCase identifiers into separate words
///
/// Handles consecutive uppercase letters (e.g., "XMLParser" -> "XML Parser").
///
/// # Arguments
///
/// * `text` - The identifier to split
///
/// # Example
///
/// ```
/// use cce_core::utils::text::split_camel_case;
///
/// assert_eq!(split_camel_case("calculateTotal"), "calculate Total");
/// assert_eq!(split_camel_case("XMLParser"), "XML Parser");
/// assert_eq!(split_camel_case("calculate"), "calculate");
/// ```
pub fn split_camel_case(text: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();

    for (i, c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            if !result.is_empty() {
                let prev_is_lower = i > 0 && chars[i - 1].is_lowercase();
                let next_is_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();

                if prev_is_lower || next_is_lower {
                    result.push(' ');
                }
            }
            result.push(*c);
        } else {
            result.push(*c);
        }
    }
    result
}

/// Remove single/double quotes and backticks from text
///
/// # Example
///
/// ```
/// use cce_core::utils::text::remove_quotes;
///
/// assert_eq!(remove_quotes(r#"hello "world" `test`"#), "hello world test");
/// assert_eq!(remove_quotes("no quotes"), "no quotes");
/// ```
pub fn remove_quotes(text: &str) -> String {
    text.chars()
        .filter(|&c| !matches!(c, '\'' | '"' | '`'))
        .collect()
}

/// Split an identifier into component words.
///
/// Splits at `_`, `-`, `.`, `/`, `::`, `:` and camelCase/PascalCase boundaries.
/// Returns lowercased words of length >= 2 that are not all digits.
///
/// # Example
///
/// ```
/// use cce_core::utils::text::split_identifier;
///
/// assert_eq!(split_identifier("get_or_init"), vec!["get", "or", "init"]);
/// assert_eq!(split_identifier("OnceCell"), vec!["once", "cell"]);
/// assert_eq!(split_identifier("XMLParser"), vec!["xml", "parser"]);
/// assert_eq!(split_identifier("std::path::Path"), vec!["std", "path", "path"]);
/// assert_eq!(split_identifier("utf-8"), vec!["utf"]);
/// ```
pub fn split_identifier(ident: &str) -> Vec<String> {
    if ident.is_empty() {
        return vec![];
    }

    let mut result = Vec::new();

    for segment in ident.split(['_', '-', '.', '/']) {
        if segment.is_empty() {
            continue;
        }
        for sub in segment.split("::") {
            if sub.is_empty() {
                continue;
            }
            for sub2 in sub.split(':') {
                if sub2.is_empty() {
                    continue;
                }
                result.extend(split_camel_case_words(sub2));
            }
        }
    }

    result
        .into_iter()
        .filter(|s| s.len() >= 2 && !s.chars().all(|c| c.is_ascii_digit()))
        .collect()
}

/// Split a camelCase/PascalCase word into individual words.
///
/// Output is lowercased. Handles consecutive uppercase (acronyms),
/// digit-letter transitions and `_`/`-` separators.
///
/// # Example
///
/// ```
/// use cce_core::utils::text::split_camel_case_words;
///
/// assert_eq!(split_camel_case_words("getUserById"), vec!["get", "user", "by", "id"]);
/// assert_eq!(split_camel_case_words("XMLParser"), vec!["xml", "parser"]);
/// assert_eq!(split_camel_case_words("v1_2_3"), vec!["v", "1", "2", "3"]);
/// ```
pub fn split_camel_case_words(word: &str) -> Vec<String> {
    if word.is_empty() {
        return vec![];
    }

    let mut result = Vec::new();

    // `_`/`-` are hard separators; empty segments are skipped so repeated
    // separators (e.g. `a__b`) do not produce empty words.
    for segment in word.split(['_', '-']) {
        if segment.is_empty() {
            continue;
        }

        let chars: Vec<char> = segment.chars().collect();
        let mut current = String::new();

        for i in 0..chars.len() {
            let c = chars[i];

            if c.is_uppercase() && !current.is_empty() {
                let prev_is_lower = i > 0 && chars[i - 1].is_lowercase();
                let next_is_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();

                if prev_is_lower || next_is_lower {
                    result.push(current.to_lowercase());
                    current.clear();
                }
            } else if !current.is_empty() {
                let last = current.chars().last().expect("current is non-empty");
                if (c.is_ascii_digit() && !last.is_ascii_digit())
                    || (c.is_alphabetic() && last.is_ascii_digit())
                {
                    result.push(current.to_lowercase());
                    current.clear();
                }
            }

            current.push(c);
        }

        if !current.is_empty() {
            result.push(current.to_lowercase());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_camel_case_words() {
        assert_eq!(
            split_camel_case_words("getUserById"),
            vec!["get", "user", "by", "id"]
        );
        assert_eq!(split_camel_case_words("XMLParser"), vec!["xml", "parser"]);
        assert_eq!(split_camel_case_words("v1_2_3"), vec!["v", "1", "2", "3"]);
        assert_eq!(split_camel_case_words("parse-xml"), vec!["parse", "xml"]);
        assert_eq!(split_camel_case_words("a__b"), vec!["a", "b"]);
        assert_eq!(split_camel_case_words(""), Vec::<String>::new());
    }

    #[test]
    fn test_normalize_whitespace_basic() {
        assert_eq!(normalize_whitespace("hello world"), "hello world");
        assert_eq!(normalize_whitespace("  hello   world  "), "hello world");
        assert_eq!(normalize_whitespace("hello\nworld"), "hello world");
        assert_eq!(normalize_whitespace("hello\tworld"), "hello world");
        assert_eq!(normalize_whitespace(""), "");
    }

    #[test]
    fn test_normalize_whitespace_mixed() {
        let text = "  hello   world  \n\t  test  ";
        assert_eq!(normalize_whitespace(text), "hello world test");
    }

    #[test]
    fn test_normalize_code_fragment() {
        let text = "std::result::Result::<usize, std::io::Error>::Ok(buffer.len())? { 1 << 2 }";
        let normalized = normalize_code_fragment(text);

        assert!(normalized.contains("std:result:Result"));
        assert!(normalized.contains("shift left"));
        assert!(normalized.contains("{ 1 shift left 2 }"));
    }

    #[test]
    fn test_normalize_code_fragment_shift_assignments() {
        let text = "value <<= 2; other >>= 1; plain >> 3;";
        let normalized = normalize_code_fragment(text);

        assert!(normalized.contains("shift left assign"));
        assert!(normalized.contains("shift right assign"));
        assert!(normalized.contains("shift right 3"));
    }

    #[test]
    fn test_normalize_whitespace_preserving_newlines() {
        assert_eq!(
            normalize_whitespace_preserving_newlines("line one\nline two"),
            "line one\nline two"
        );
        assert_eq!(
            normalize_whitespace_preserving_newlines("  line  one  \n  line   two  "),
            "line one\nline two"
        );
        assert_eq!(
            normalize_whitespace_preserving_newlines("  hello   "),
            "hello"
        );
    }

    #[test]
    fn test_is_blank() {
        assert!(is_blank(""));
        assert!(is_blank("   "));
        assert!(is_blank("\n\t"));
        assert!(!is_blank("hello"));
        assert!(!is_blank(" a "));
    }
}
