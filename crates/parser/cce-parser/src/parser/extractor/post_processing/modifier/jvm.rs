use crate::parser::extractor::capture;
use crate::tree_sitter_query::executor::QueryMatch;
use cce_types::entity::Entity;

#[allow(clippy::collapsible_match)]
pub fn extract_jvm_modifiers(mat: &QueryMatch, entity: &mut Entity) {
    let main_capture = match capture::parser::find_main_capture(mat) {
        Some(c) => c,
        None => return,
    };
    let text = &main_capture.text;
    let tokens: Vec<String> = text
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| {
                c == ',' || c == ';' || c == '{' || c == '(' || c == ')' || c == '[' || c == ']'
            })
            .trim_end_matches(&[',', ';', '{', '(', '='][..])
            .to_lowercase()
        })
        .collect();

    let mut found: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < tokens.len() {
        let tok = tokens[i].trim().to_lowercase();
        if tok == "protected" && i + 1 < tokens.len() {
            let next = tokens[i + 1].trim().to_lowercase();
            if next == "internal" {
                if !found.contains(&"protected internal".to_string()) {
                    found.push("protected internal".to_string());
                }
                i += 2;
                continue;
            }
        }
        if tok == "private" && i + 1 < tokens.len() {
            let next = tokens[i + 1].trim().to_lowercase();
            if next == "protected" {
                if !found.contains(&"private protected".to_string()) {
                    found.push("private protected".to_string());
                }
                i += 2;
                continue;
            }
        }
        match tok.as_str() {
            "public" | "private" | "protected" | "internal" => {
                if !found.contains(&tok) {
                    found.push(tok.clone());
                }
            }
            _ => {}
        }
        i += 1;
    }

    entity.modifiers = found;
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

    fn make_match(text: &str) -> QueryMatch {
        QueryMatch {
            captures: vec![make_capture("entity.function", text)],
            pattern_index: 0,
            index: 0,
        }
    }

    #[test]
    fn jvm_extract_public() {
        let mut e = make_entity("foo");
        let m = make_match("public static void foo() {");
        extract_jvm_modifiers(&m, &mut e);
        assert!(e.modifiers.contains(&"public".to_string()));
    }

    #[test]
    fn jvm_extract_protected_internal() {
        let mut e = make_entity("foo");
        let m = make_match("protected internal void foo() {");
        extract_jvm_modifiers(&m, &mut e);
        assert!(e.modifiers.contains(&"protected internal".to_string()));
    }

    #[test]
    fn jvm_extract_private_protected() {
        let mut e = make_entity("foo");
        let m = make_match("private protected void foo() {");
        extract_jvm_modifiers(&m, &mut e);
        assert!(e.modifiers.contains(&"private protected".to_string()));
    }
}
