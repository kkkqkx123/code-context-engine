//! Python modifier extraction.
//!
//! Python functions carry a single declaration modifier: `async`. It is
//! reported either through the dedicated `@entity.function.async` capture
//! or, as a fallback, through the leading keyword of the declaration text.

use crate::parser::extractor::capture;
use crate::tree_sitter_query::executor::QueryMatch;
use cce_types::entity::Entity;

pub fn extract_python_modifiers(mat: &QueryMatch, entity: &mut Entity) {
    let main_capture = match capture::parser::find_main_capture(mat) {
        Some(c) => c,
        None => return,
    };

    let mut found: Vec<String> = Vec::new();
    if main_capture.name.contains(".async")
        || main_capture
            .text
            .split_whitespace()
            .next()
            .is_some_and(|first| first == "async")
    {
        found.push("async".to_string());
    }

    entity.modifiers = found;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_sitter_query::executor::Capture;

    fn test_match(name: &str, text: &str) -> QueryMatch {
        QueryMatch {
            captures: vec![Capture {
                name: name.to_string(),
                text: text.to_string(),
                start_byte: 0,
                end_byte: text.len(),
                start_point: (0, 0),
                end_point: (0, 0),
            }],
            pattern_index: 0,
            index: 0,
        }
    }

    fn modifiers_for(name: &str, text: &str) -> Vec<String> {
        let mat = test_match(name, text);
        let mut entity = Entity::new(
            cce_types::entity::EntityId(1),
            cce_types::entity::EntityKind::Function,
            "f".to_string(),
            cce_types::Span::default(),
        );
        extract_python_modifiers(&mat, &mut entity);
        entity.modifiers
    }

    #[test]
    fn test_async_capture_name() {
        assert_eq!(
            modifiers_for("entity.function.async", "async def fetch():"),
            vec!["async"]
        );
    }

    #[test]
    fn test_async_leading_keyword() {
        assert_eq!(
            modifiers_for("entity.function", "async def fetch():"),
            vec!["async"]
        );
    }

    #[test]
    fn test_plain_function_has_no_modifiers() {
        assert!(modifiers_for("entity.function", "def process(x):").is_empty());
    }
}
