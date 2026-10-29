//! Text utilities for common string operations

/// Collapse all whitespace into single spaces and trim
///
/// Replaces all sequences of whitespace characters (spaces, tabs, newlines)
/// with a single space, then trims leading and trailing whitespace.
///
/// # Example
///
/// ```
/// use cce_utils::text::normalize_whitespace;
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
/// use cce_utils::text::normalize_whitespace_preserving_newlines;
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

/// Move a byte index down to the nearest UTF-8 char boundary.
///
/// Rust `str` slicing panics when an index lands inside a multi-byte char
/// (`¡`, CJK, emoji). Any byte arithmetic (`len * ratio`, `pos - window`,
/// `max - 3`, fixed caps like `[..256]`) can produce such an index, so every
/// computed index must pass through here before slicing. Out-of-range input
/// is clamped to `text.len()`.
pub fn floor_char_boundary(text: &str, position: usize) -> usize {
    let mut position = position.min(text.len());
    while position > 0 && !text.is_char_boundary(position) {
        position -= 1;
    }
    position
}

/// Move a byte index up to the nearest UTF-8 char boundary.
///
/// Complement to [`floor_char_boundary`] for split loops that must make
/// forward progress (a floored cut equal to the chunk start would stall).
/// Out-of-range input is clamped to `text.len()`.
pub fn ceil_char_boundary(text: &str, position: usize) -> usize {
    let mut position = position.min(text.len());
    while position < text.len() && !text.is_char_boundary(position) {
        position += 1;
    }
    position
}

/// Find `needle` in `haystack` without case sensitivity.
///
/// Returns `(start, length)` in bytes of the original `haystack`. Lowercasing
/// can change the byte length of a character (for example `İ` lowercases to
/// `i` plus a combining dot), so the offsets of a lowercased copy can never
/// be reused on the original text. The comparison is done character by
/// character and the returned length covers the original characters that
/// matched.
pub fn find_case_insensitive(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return Some((0, 0));
    }

    let needle_lower: Vec<char> = needle
        .chars()
        .flat_map(|character| character.to_lowercase())
        .collect();

    for (start, _) in haystack.char_indices() {
        let mut matched = 0usize;
        for (relative, character) in haystack[start..].char_indices() {
            let lowered: Vec<char> = character.to_lowercase().collect();
            let next = matched + lowered.len();
            if next > needle_lower.len() || needle_lower[matched..next] != lowered[..] {
                break;
            }
            matched = next;
            let end = start + relative + character.len_utf8();
            if matched == needle_lower.len() {
                return Some((start, end - start));
            }
        }
    }
    None
}

/// Truncate `text` to at most `max_bytes` bytes without splitting a char.
///
/// Returns the longest char-boundary-aligned prefix within the budget.
pub fn truncate_to_byte_len(text: &str, max_bytes: usize) -> &str {
    &text[..floor_char_boundary(text, max_bytes)]
}

/// Truncate a `String` in place without splitting a char.
///
/// Centralizes the `floor` + `String::truncate` pair so callers never
/// pass a raw computed index to `truncate` directly.
pub fn truncate_string_to_byte_len(text: &mut String, max_bytes: usize) {
    let boundary = floor_char_boundary(text, max_bytes);
    text.truncate(boundary);
}

/// Move a byte index down to the previous UTF-8 char boundary.
pub fn prev_char_boundary(text: &str, position: usize) -> usize {
    floor_char_boundary(text, position)
}

/// Move a byte index up to the next UTF-8 char boundary.
pub fn next_char_boundary(text: &str, position: usize) -> usize {
    ceil_char_boundary(text, position)
}

/// The character starting at `position`, when `position` is a valid
/// UTF-8 char boundary inside `text`.
pub fn char_at(text: &str, position: usize) -> Option<char> {
    if !text.is_char_boundary(position) {
        return None;
    }
    text[position..].chars().next()
}

/// Slice `text` between two byte offsets without ever splitting a char.
pub fn slice_clamped(text: &str, start: usize, end: usize) -> &str {
    let start = floor_char_boundary(text, start);
    let end = floor_char_boundary(text, end.max(start));
    &text[start..end]
}

/// Strict slice between two byte offsets.
pub fn slice_exact(text: &str, start: usize, end: usize) -> Option<&str> {
    if start > end
        || end > text.len()
        || !text.is_char_boundary(start)
        || !text.is_char_boundary(end)
    {
        return None;
    }
    Some(&text[start..end])
}

/// Check if text is empty or contains only whitespace
///
/// # Example
///
/// ```
/// use cce_utils::text::is_blank;
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
/// use cce_utils::text::split_camel_case;
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
/// use cce_utils::text::remove_quotes;
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
/// use cce_utils::text::split_identifier;
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
/// use cce_utils::text::split_camel_case_words;
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

    #[test]
    fn test_floor_char_boundary_never_splits_multibyte() {
        let text = format!("{}¡{}世界🌍", "a".repeat(10), "b".repeat(10));
        for i in 0..=text.len() {
            let floored = floor_char_boundary(&text, i);
            assert!(text.is_char_boundary(floored));
            assert!(floored <= i.min(text.len()));
            assert_eq!(&text[..floored], truncate_to_byte_len(&text, i));
        }
        // '¡' is 2 bytes: flooring the middle byte steps back one.
        let inverted = "a¡b";
        assert_eq!(floor_char_boundary(inverted, 2), 1);
        assert_eq!(truncate_to_byte_len(inverted, 2), "a");
        // Out-of-range input clamps instead of panicking.
        assert_eq!(floor_char_boundary(inverted, 100), inverted.len());
    }

    #[test]
    fn test_find_case_insensitive_returns_original_offsets() {
        assert_eq!(find_case_insensitive("Hello World", "world"), Some((6, 5)));
        assert_eq!(find_case_insensitive("前缀 Hello", "hello"), Some((7, 5)));
        assert_eq!(find_case_insensitive("abc", "z"), None);
        assert_eq!(find_case_insensitive("abc", ""), Some((0, 0)));
    }

    #[test]
    fn test_find_case_insensitive_handles_length_changing_lowercase() {
        // `İ` is two bytes, while its lowercase form is three bytes
        // (`i` plus a combining dot). The returned length must still cover
        // the original character.
        let haystack = "İstanbul";
        let needle = "i\u{307}stanbul";
        assert_eq!(
            find_case_insensitive(haystack, needle),
            Some((0, haystack.len()))
        );
    }

    #[test]
    fn test_ceil_moves_forward_to_boundary() {
        let inverted = "a¡b";
        assert_eq!(ceil_char_boundary(inverted, 2), 3);
        assert_eq!(ceil_char_boundary(inverted, 100), inverted.len());
        let mut owned = inverted.to_string();
        truncate_string_to_byte_len(&mut owned, 2);
        assert_eq!(owned, "a");
    }
}

#[cfg(test)]
mod unified_boundary_helper_tests {
    use super::*;

    #[test]
    fn unified_boundary_helpers_never_split_multibyte() {
        let text = format!("{}¡{}世界🌍", "a".repeat(10), "b".repeat(10));
        for i in 0..=text.len() {
            assert!(text.is_char_boundary(prev_char_boundary(&text, i)));
            assert!(text.is_char_boundary(next_char_boundary(&text, i)));
            assert_eq!(prev_char_boundary(&text, i), floor_char_boundary(&text, i));
            assert_eq!(next_char_boundary(&text, i), ceil_char_boundary(&text, i));
        }
        assert_eq!(char_at(&text, 0), Some('a'));
        assert!(slice_exact(&text, 0, 1).is_some());
        let sliced = slice_clamped(&text, 1, 2);
        assert!(text.is_char_boundary(sliced.as_ptr() as usize - text.as_ptr() as usize));
    }
}
