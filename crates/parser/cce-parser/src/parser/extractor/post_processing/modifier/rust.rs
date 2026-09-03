use crate::parser::extractor::capture;
use crate::tree_sitter_query::executor::QueryMatch;
use cce_types::entity::Entity;

pub fn parse_rust_visibility(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let bytes = lower.as_bytes();
    let mut search_start = 0usize;
    while let Some(rel) = lower[search_start..].find("pub") {
        let abs = search_start + rel;
        if abs > 0 {
            let prev = bytes[abs - 1] as char;
            if prev.is_alphanumeric() || prev == '_' {
                search_start = abs + 3;
                continue;
            }
        }
        let rest = &lower[abs + 3..];
        let trimmed = rest.trim_start();
        if trimmed.starts_with('(') {
            if let Some(close_idx) = trimmed.find(')') {
                let inside = &trimmed[1..close_idx];
                let inside_trim = inside.trim();
                if inside_trim == "crate" {
                    return Some("pub(crate)".to_string());
                } else if inside_trim == "super" {
                    return Some("pub(super)".to_string());
                } else if inside_trim == "self" {
                    return Some("pub(self)".to_string());
                } else if let Some(after) = inside_trim.strip_prefix("in") {
                    let after_in = after.trim();
                    if after_in.is_empty() {
                        return Some("pub(in)".to_string());
                    } else {
                        return Some(format!("pub(in {})", after_in));
                    }
                } else {
                    search_start = abs + 3;
                    continue;
                }
            } else {
                return Some("pub".to_string());
            }
        } else {
            return Some("pub".to_string());
        }
    }
    None
}

pub fn extract_rust_modifiers(mat: &QueryMatch, entity: &mut Entity) {
    let main_capture = match capture::parser::find_main_capture(mat) {
        Some(c) => c,
        None => return,
    };

    let text = &main_capture.text;
    let category = main_capture.name.split('.').nth(1).unwrap_or("");

    let item_keywords: &[&str] = &[
        "fn", "struct", "enum", "trait", "impl", "type", "union", "mod", "use", "extern",
    ];

    let mut found_modifiers: Vec<String> = Vec::new();

    if let Some(vis) = parse_rust_visibility(text) {
        found_modifiers.push(vis);
    }

    let tokens: Vec<&str> = text.split_whitespace().collect();
    let has_pub = found_modifiers.iter().any(|m| m.starts_with("pub"));

    match category {
        "static" => {
            if !found_modifiers.iter().any(|m| m == "static") {
                found_modifiers.insert(0, "static".to_string());
            }
            for token in &tokens {
                let trimmed = token
                    .trim_end_matches(&[',', ';', '{', '(', '='][..])
                    .to_lowercase();
                let t = trimmed.trim();
                if !has_pub
                    && (t == "pub"
                        || t == "pub(crate)"
                        || t == "pub(super)"
                        || t == "pub(self)"
                        || t.starts_with("pub(in"))
                {
                    if !found_modifiers.iter().any(|m| m == t) {
                        found_modifiers.push(t.to_string());
                    }
                } else if t == "mut" {
                    if !found_modifiers.contains(&"mut".to_string()) {
                        found_modifiers.push("mut".to_string());
                    }
                } else if item_keywords.contains(&t) {
                    continue;
                }
            }
        }
        "function" => {
            for token in &tokens {
                let trimmed = token
                    .trim_end_matches(&[',', ';', '{', '(', '='][..])
                    .to_lowercase();
                let t = trimmed.trim();
                if t.starts_with("pub") {
                    continue;
                }
                if t == "unsafe" || t == "async" || t == "default" || t == "const" {
                    if !found_modifiers.contains(&t.to_string()) {
                        found_modifiers.push(t.to_string());
                    }
                } else if t == "fn" || item_keywords.contains(&t) {
                    break;
                }
            }
        }
        "struct" | "enum" | "trait" | "union" | "type" | "const" | "constant" => {
            for token in &tokens {
                let trimmed = token
                    .trim_end_matches(&[',', ';', '{', '(', '='][..])
                    .to_lowercase();
                let t = trimmed.trim();
                if t.starts_with("pub") {
                    continue;
                }
                if t == "unsafe" {
                    if !found_modifiers.contains(&"unsafe".to_string()) {
                        found_modifiers.push("unsafe".to_string());
                    }
                } else if item_keywords.contains(&t) || t == "impl" {
                    break;
                } else if !matches!(t, "unsafe") {
                    // leading modifier scan placeholder
                }
            }
        }
        "variable" => {
            if let Some(token) = tokens.first() {
                let trimmed = token
                    .trim_end_matches(&[',', ';', '{', '(', '='][..])
                    .to_lowercase();
                if trimmed.trim() == "mut" && !found_modifiers.contains(&"mut".to_string()) {
                    found_modifiers.push("mut".to_string());
                }
            }
        }
        "impl" => {
            for token in &tokens {
                let trimmed = token
                    .trim_end_matches(&[',', ';', '{', '(', '='][..])
                    .to_lowercase();
                let t = trimmed.trim();
                if t.starts_with("pub") {
                    continue;
                }
                if t == "unsafe" || t == "default" {
                    if !found_modifiers.contains(&t.to_string()) {
                        found_modifiers.push(t.to_string());
                    }
                } else if t == "impl" || item_keywords.contains(&t) {
                    break;
                }
            }
        }
        _ => {}
    }

    entity.modifiers = found_modifiers;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_sitter_query::executor::{Capture, QueryMatch};
    use cce_types::Span;
    use cce_types::entity::{Entity, EntityId, EntityKind};
    use std::collections::HashMap;

    fn make_entity(name: &str) -> Entity {
        Entity {
            id: EntityId(1),
            kind: EntityKind::Function,
            name: name.to_string(),
            signature: String::new(),
            parameters: Vec::new(),
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            children: Vec::new(),
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            metadata: HashMap::new(),
            is_stdlib: false,
            subtype: None,
            stdlib_category: None,
        }
    }

    fn make_capture(name: &str, text: &str) -> Capture {
        Capture {
            name: name.to_string(),
            text: text.to_string(),
            start_byte: 0,
            end_byte: text.len(),
            start_point: (0, 0),
            end_point: (0, 0),
        }
    }

    fn make_match(text: &str, capture_name: &str) -> QueryMatch {
        QueryMatch {
            captures: vec![make_capture(capture_name, text)],
            pattern_index: 0,
            index: 0,
        }
    }

    #[test]
    fn rust_visibility_pub() {
        assert_eq!(
            parse_rust_visibility("pub fn foo()"),
            Some("pub".to_string())
        );
        assert_eq!(
            parse_rust_visibility("pub(crate) struct S"),
            Some("pub(crate)".to_string())
        );
        assert_eq!(
            parse_rust_visibility("pub(in crate::a::b) fn f()"),
            Some("pub(in crate::a::b)".to_string())
        );
        assert_eq!(parse_rust_visibility("fn foo()"), None);
    }

    #[test]
    fn rust_extract_function_pub() {
        let mut entity = make_entity("foo");
        let mat = make_match("pub unsafe fn foo()", "entity.function");
        extract_rust_modifiers(&mat, &mut entity);
        assert!(entity.modifiers.contains(&"pub".to_string()));
        assert!(entity.modifiers.contains(&"unsafe".to_string()));
    }
}
