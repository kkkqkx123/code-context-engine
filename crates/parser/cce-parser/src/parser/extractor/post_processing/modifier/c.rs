//! C/C++ declaration specifier extraction.
//!
//! Scans the leading declaration specifiers (`static`, `extern`, `virtual`,
//! ...) and trailing member-function qualifiers (`const`, `override`,
//! `final`). Only reserved keywords are recognized, so identifiers can
//! never be misread as modifiers. `override`/`final` are contextual in
//! C++ and are therefore only accepted in trailing position.

use crate::parser::extractor::capture;
use crate::tree_sitter_query::executor::QueryMatch;
use cce_types::entity::Entity;

/// Leading specifiers that act as modifiers rather than type names.
const LEADING_SPECIFIERS: &[&str] = &[
    "static",
    "extern",
    "inline",
    "constexpr",
    "const",
    "volatile",
    "register",
    "virtual",
    "explicit",
    "friend",
    "mutable",
    "typedef",
];

/// Trailing member-function qualifiers (C++).
const TRAILING_QUALIFIERS: &[&str] = &["override", "final", "const", "volatile"];

/// Strip pointer/reference/punctuation adornments for keyword comparison.
fn clean_token(token: &str) -> String {
    token
        .trim_matches(|c: char| {
            c == '*' || c == '&' || c == ',' || c == ';' || c == '(' || c == ')' || c == '{'
        })
        .trim_end_matches(':')
        .to_lowercase()
}

pub fn extract_c_modifiers(mat: &QueryMatch, entity: &mut Entity) {
    let main_capture = match capture::parser::find_main_capture(mat) {
        Some(c) => c,
        None => return,
    };
    let text = main_capture.text.trim();
    if text.is_empty() {
        return;
    }

    let mut found: Vec<String> = Vec::new();

    // Leading declaration specifiers, in source order.
    for token in text.split_whitespace() {
        let cleaned = clean_token(token);
        if cleaned.is_empty() {
            continue;
        }
        if LEADING_SPECIFIERS.contains(&cleaned.as_str()) {
            if !found.contains(&cleaned) {
                found.push(cleaned);
            }
        } else {
            break;
        }
    }

    // Trailing member-function qualifiers: scan the declarator tail
    // (before the body or terminator) from the end.
    let head = match text.find('{') {
        Some(pos) => &text[..pos],
        None => text.trim_end_matches(';'),
    };
    let mut trailing: Vec<String> = Vec::new();
    for token in head.split_whitespace().rev() {
        let cleaned = clean_token(token);
        if cleaned.is_empty() {
            continue;
        }
        if TRAILING_QUALIFIERS.contains(&cleaned.as_str()) {
            if !found.contains(&cleaned) && !trailing.contains(&cleaned) {
                trailing.push(cleaned);
            }
        } else {
            break;
        }
    }
    trailing.reverse();
    found.extend(trailing);

    entity.modifiers = found;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_sitter_query::executor::Capture;

    fn test_match(text: &str) -> QueryMatch {
        QueryMatch {
            captures: vec![Capture {
                name: "entity.function".to_string(),
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

    fn modifiers_for(text: &str) -> Vec<String> {
        let mat = test_match(text);
        let mut entity = Entity::new(
            cce_types::entity::EntityId(1),
            cce_types::entity::EntityKind::Function,
            "f".to_string(),
            cce_types::Span::default(),
        );
        extract_c_modifiers(&mat, &mut entity);
        entity.modifiers
    }

    #[test]
    fn test_static_function() {
        assert_eq!(modifiers_for("static int add(int a) {"), vec!["static"]);
    }

    #[test]
    fn test_multiple_leading_specifiers() {
        assert_eq!(
            modifiers_for("static inline int add(int a) {"),
            vec!["static", "inline"]
        );
    }

    #[test]
    fn test_extern_const_variable() {
        assert_eq!(
            modifiers_for("extern const char *name;"),
            vec!["extern", "const"]
        );
    }

    #[test]
    fn test_plain_function_has_no_modifiers() {
        assert!(modifiers_for("int add(int a, int b) {").is_empty());
    }

    #[test]
    fn test_type_keywords_are_not_modifiers() {
        assert!(modifiers_for("unsigned long count;").is_empty());
    }

    #[test]
    fn test_virtual_method() {
        assert_eq!(modifiers_for("virtual int draw();"), vec!["virtual"]);
    }

    #[test]
    fn test_const_override_trailing() {
        assert_eq!(
            modifiers_for("int draw() const override {"),
            vec!["const", "override"]
        );
    }

    #[test]
    fn test_trailing_final() {
        assert_eq!(modifiers_for("void run() final;"), vec!["final"]);
    }

    #[test]
    fn test_pure_virtual_terminator_not_modifier() {
        assert_eq!(modifiers_for("virtual void run() = 0;"), vec!["virtual"]);
    }
}
