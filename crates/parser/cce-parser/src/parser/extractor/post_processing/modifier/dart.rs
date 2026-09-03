use crate::parser::extractor::capture;
use crate::tree_sitter_query::executor::QueryMatch;
use cce_types::entity::Entity;

pub fn extract_dart_modifiers(mat: &QueryMatch, entity: &mut Entity) {
    let main_capture = match capture::parser::find_main_capture(mat) {
        Some(c) => c,
        None => return,
    };
    let text = &main_capture.text;
    let _ = text;
    entity.modifiers = Vec::new();
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

    #[test]
    fn dart_no_modifiers() {
        let mut e = make_entity("_private");
        let m = QueryMatch {
            captures: vec![make_capture("entity.function", "void _private() {}")],
            pattern_index: 0,
            index: 0,
        };
        extract_dart_modifiers(&m, &mut e);
        assert!(e.modifiers.is_empty());
    }
}
