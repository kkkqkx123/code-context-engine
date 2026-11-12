//! Plain-text surgery for code fragments.
//!
//! Pure string functions with no language branches: condition extraction,
//! parenthesis handling, assignment splitting and truncation. Language
//! differences are handled by the shape of the input (a Go guard simply has
//! no parenthesis to balance, so it falls through to the legacy path),
//! never by switching on a language inside these functions.

use super::vocab::MAX_OBJECT_CHARS;

/// Extract the discriminant between the leading keyword and the body brace.
///
/// Braceless guards first try a balanced parenthesis right after the
/// keyword, so a C null check still yields its condition instead of silence.
/// Guards without a parenthesis (Go) or without braces and parentheses
/// fall back to the first line with the trailing statement cut away.
pub(super) fn discriminant_before_brace(text: &str) -> Option<String> {
    if let Some(brace) = text.find('{') {
        let head = text[..brace].trim();
        let rest = strip_leading_keywords(head);
        let rest = unwrap_parens(rest.trim());
        let rest = collapse_whitespace(rest);
        if rest.trim().is_empty() {
            return None;
        }
        return Some(rest);
    }
    let line = first_meaningful_line(text)?;
    if let Some(cond) = balanced_paren_condition(&line) {
        return Some(cond);
    }
    let rest = strip_leading_keywords(&line);
    let rest = rest.trim().trim_end_matches(';').trim();
    let rest = cut_trailing_statement(rest);
    let rest = unwrap_parens(rest.trim());
    let rest = collapse_whitespace(rest);
    if rest.trim().is_empty() {
        return None;
    }
    Some(rest)
}

/// Extract the balanced parenthesis right after the leading keyword.
///
/// Returns the collapsed inner text when the keyword is immediately
/// followed by a balanced group, `None` otherwise. Guards whose condition
/// has no parenthesis (Go `if x > 0`) or no balanced group fall through to
/// the legacy first-line path in the caller.
pub(super) fn balanced_paren_condition(line: &str) -> Option<String> {
    let rest = strip_leading_keywords(line).trim();
    if !rest.starts_with('(') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth -= 1;
            if depth == 0 {
                let inner = rest[1..i].trim();
                let inner = unwrap_parens(inner).trim();
                let inner = collapse_whitespace(inner);
                if inner.is_empty() {
                    return None;
                }
                return Some(inner);
            }
        }
    }
    None
}

/// Strip statement keywords from the head of a fact fragment.
pub(super) fn strip_leading_keywords(mut head: &str) -> &str {
    for _ in 0..3 {
        let word_end = head
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(head.len());
        let (word, _) = head.split_at(word_end);
        if matches!(
            word,
            "if" | "else" | "switch" | "match" | "for" | "while" | "loop"
        ) {
            head = head[word_end..].trim_start();
        } else {
            break;
        }
    }
    head
}

/// Strip one leading keyword such as `return` from a fragment.
pub(super) fn leading_keyword_stripped(text: &str, keywords: &[&str]) -> Option<String> {
    let flat = collapse_whitespace(text);
    let word_end = flat
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(flat.len());
    let (word, _) = flat.split_at(word_end);
    if keywords.contains(&word) {
        Some(flat[word_end..].trim_start().to_string())
    } else {
        None
    }
}

/// Cut a trailing single-statement tail from a braceless guard head.
pub(super) fn cut_trailing_statement(head: &str) -> &str {
    for keyword in [" return ", " break", " continue "] {
        if let Some(pos) = head.find(keyword) {
            let before = head[..pos].trim_end();
            if !before.is_empty() {
                return before;
            }
        }
    }
    head
}

/// Remove one pair of parentheses wrapping the whole string.
pub(super) fn unwrap_parens(s: &str) -> &str {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('(') && t.ends_with(')') {
        let bytes = t.as_bytes();
        let mut depth = 0i32;
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth -= 1;
                if depth == 0 && i != bytes.len() - 1 {
                    return t;
                }
            }
        }
        if depth == 0 {
            return t[1..t.len() - 1].trim();
        }
    }
    t
}

/// Whether a call-like shape holds: an opening paren glued to a name.
pub(super) fn looks_like_call(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' && i > 0 {
            let prev = bytes[i - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b']' || prev == b')' {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Whether a fragment opens with a control keyword.
///
/// Such fragments are control statements the extractor expanded around a
/// match, not the data operation the kind label claims.
pub(super) fn starts_with_control_keyword(text: &str) -> bool {
    let flat = collapse_whitespace(text);
    let word_end = flat
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(flat.len());
    matches!(
        &flat[..word_end],
        "if" | "else" | "for" | "switch" | "match" | "while" | "loop"
    )
}

/// Strip declaration prefixes so the bound name leads the sentence.
pub(super) fn strip_binding_prefix(lhs: &str) -> &str {
    let mut rest = lhs.trim();
    for _ in 0..3 {
        let word_end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if matches!(&rest[..word_end], "let" | "mut" | "var" | "const")
            && rest[word_end..].starts_with(char::is_whitespace)
        {
            rest = rest[word_end..].trim_start();
        } else {
            break;
        }
    }
    rest
}

/// Split an assignment into its left- and right-hand sides.
pub(super) fn split_assignment(text: &str) -> Option<(&str, &str)> {
    if let Some(pos) = text.find(":=") {
        return Some((text[..pos].trim(), text[pos + 2..].trim()));
    }
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' {
            let prev = if i > 0 { bytes[i - 1] } else { b' ' };
            let next = bytes.get(i + 1).copied().unwrap_or(b' ');
            let is_comparison = matches!(
                prev,
                b'=' | b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^'
            ) || matches!(next, b'=' | b'>');
            if !is_comparison {
                return Some((text[..i].trim(), text[i + 1..].trim()));
            }
        }
        i += 1;
    }
    None
}

/// First non-empty line of a fact fragment.
pub(super) fn first_meaningful_line(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Collapse whitespace runs so multi-line fragments fit one sentence.
pub(super) fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Keep the head of an object, cutting at a word boundary when possible.
pub(super) fn truncate_object(text: &str) -> String {
    let flat = collapse_whitespace(text);
    let flat = flat.trim();
    if flat.len() <= MAX_OBJECT_CHARS {
        return flat.to_string();
    }
    let mut end = MAX_OBJECT_CHARS;
    while end > 0 && !flat.is_char_boundary(end) {
        end -= 1;
    }
    let cut = flat[..end].trim_end();
    match cut.rfind(' ') {
        Some(pos) if pos > 0 => cut[..pos].to_string(),
        _ => cut.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn braceless_call_body_yields_condition_only() {
        let cond =
            discriminant_before_brace("if (period->event_streams) free(period->event_streams);");
        assert_eq!(cond.as_deref(), Some("period->event_streams"));
    }

    #[test]
    fn balanced_parens_keep_nested_groups() {
        let cond = balanced_paren_condition("if (a && (b || c)) x();");
        assert_eq!(cond.as_deref(), Some("a && (b || c)"));
    }

    #[test]
    fn unbalanced_parens_fall_back_to_none() {
        assert_eq!(balanced_paren_condition("if (a && (b) x();"), None);
    }

    #[test]
    fn parenless_guards_fall_back_to_none() {
        assert_eq!(balanced_paren_condition("if x > 0 {"), None);
    }

    #[test]
    fn parenless_guard_keeps_legacy_path() {
        let cond = discriminant_before_brace("if x > 0 {\nreturn x\n}");
        assert_eq!(cond.as_deref(), Some("x > 0"));
    }
}
