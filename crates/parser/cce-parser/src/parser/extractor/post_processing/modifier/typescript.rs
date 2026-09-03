use crate::parser::extractor::capture;
use crate::tree_sitter_query::executor::QueryMatch;
use cce_types::entity::Entity;

pub fn extract_typescript_modifiers(mat: &QueryMatch, entity: &mut Entity) {
    let main_capture = match capture::parser::find_main_capture(mat) {
        Some(c) => c,
        None => return,
    };
    let text = &main_capture.text;

    let mut found: Vec<String> = Vec::new();

    let lower_text = text.to_lowercase();
    let name_lower = entity.name.to_lowercase();
    let has_hash_private = lower_text.contains(&format!("#{}", name_lower))
        || lower_text.contains('#')
            && (entity.name.starts_with('#')
                || lower_text.contains("#private")
                || lower_text.contains("# protected"));

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

    for tok in &tokens {
        let t = tok.trim();
        if t == "public" || t == "private" || t == "protected" || t == "readonly" {
            if !found.contains(&t.to_string()) {
                found.push(t.to_string());
            }
        } else if t.starts_with('#') && !found.contains(&"private".to_string()) {
            found.push("private".to_string());
        }
    }

    if has_hash_private && !found.contains(&"private".to_string()) && text.contains('#') {
        found.push("private".to_string());
    }

    if entity.name.starts_with('#') && !found.contains(&"private".to_string()) {
        found.push("private".to_string());
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
    fn ts_extract_private() {
        let mut e = make_entity("foo");
        let m = make_match("private foo() {}");
        extract_typescript_modifiers(&m, &mut e);
        assert!(e.modifiers.contains(&"private".to_string()));
    }

    #[test]
    fn ts_extract_hash_private() {
        let mut e = make_entity("#field");
        let m = make_match("#field = 1");
        extract_typescript_modifiers(&m, &mut e);
        assert!(e.modifiers.contains(&"private".to_string()));
    }

    #[test]
    fn ts_extract_public() {
        let mut e = make_entity("bar");
        let m = make_match("public bar() {}");
        extract_typescript_modifiers(&m, &mut e);
        assert!(e.modifiers.contains(&"public".to_string()));
    }
}
