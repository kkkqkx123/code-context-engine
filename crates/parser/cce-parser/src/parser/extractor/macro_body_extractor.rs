//! Macro body extractor
//!
//! Walks the AST for `macro_rules!` definitions and stores the raw `token_tree`
//! bodies as behavior facts on the owning macro entities.
//!
//! Tree-sitter AST shape for Rust macros:
//!   macro_definition
//!     macro_rules!
//!     identifier (name)
//!     {
//!     macro_rule*
//!       token_tree_pattern (left/matcher)
//!       =>
//!       token_tree (right/body)
//!     ;
//!     ...
//!     }
//!     ;

use tree_sitter::Tree;

use crate::parser::extractor::utils::extract_text_without_comments;
use crate::tree_sitter_query::error::QueryError;
use cce_types::language::Language;
use cce_types::{BehaviorFact, BehaviorFactKind, BehaviorStore, Entity, EntityId, EntityKind};

/// Macro body extractor
pub struct MacroBodyExtractor;

impl MacroBodyExtractor {
    /// Create a new macro body extractor
    pub fn new() -> Self {
        Self
    }

    /// Check if this language currently supports macro body extraction.
    pub fn supports_language(language: &Language) -> bool {
        matches!(language, Language::Rust)
    }

    /// Extract macro bodies and attach them to the behavior sidecar.
    pub fn extract(
        &self,
        tree: &Tree,
        source: &str,
        language: &Language,
        entities: &[Entity],
        behavior: &mut BehaviorStore,
    ) -> Result<(), QueryError> {
        if !Self::supports_language(language) {
            return Ok(());
        }

        let entity_index = EntityIndex::new(entities);
        let root = tree.root_node();

        // Walk all macro_definition nodes
        let mut cursor = root.walk();
        for node in root.children(&mut cursor) {
            if node.kind() != "macro_definition" {
                continue;
            }

            // Find the macro name from the identifier child
            let macro_name = find_macro_name(node, source);
            if macro_name.is_none() {
                continue;
            }
            let macro_name = macro_name.unwrap();

            // Find the macro entity by name
            let entity_id = entity_index.find_by_name(&macro_name);
            if entity_id.is_none() {
                continue;
            }
            let entity_id = entity_id.unwrap();

            // Iterate over children to find all macro_rule nodes
            let mut rule_cursor = node.walk();
            for child in node.children(&mut rule_cursor) {
                if child.kind() != "macro_rule" {
                    continue;
                }

                // Find the token_tree child with field "right" (the macro body)
                for i in 0..child.child_count() {
                    if let Some(grandchild) = child.child(i as u32) {
                        if grandchild.kind() == "token_tree"
                            && child.field_name_for_child(i as u32) == Some("right")
                        {
                            // Extract the macro body text
                            let body_text = extract_text_without_comments(
                                tree,
                                source,
                                grandchild.start_byte(),
                                grandchild.end_byte(),
                            );
                            if body_text.trim().is_empty() {
                                continue;
                            }

                            let fact = BehaviorFact::new(
                                BehaviorFactKind::MacroBody,
                                body_text,
                                grandchild.start_byte(),
                                grandchild.end_byte(),
                            );

                            behavior.push_fact(entity_id, fact);
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for MacroBodyExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn find_macro_name(macro_node: tree_sitter::Node, source: &str) -> Option<String> {
    let mut cursor = macro_node.walk();
    for child in macro_node.children(&mut cursor) {
        if child.kind() == "identifier" {
            for i in 0..macro_node.child_count() {
                if let Some(sibling) = macro_node.child(i as u32) {
                    if sibling == child && macro_node.field_name_for_child(i as u32) == Some("name")
                    {
                        let start = child.start_byte();
                        let end = child.end_byte();
                        if start < source.len() && end <= source.len() {
                            return Some(source[start..end].to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Sorted entity index for locating macro entities by name
struct EntityIndex {
    macros: Vec<(String, EntityId)>,
}

impl EntityIndex {
    fn new(entities: &[Entity]) -> Self {
        let mut macros: Vec<_> = entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::Macro)
            .map(|entity| (entity.name.clone(), entity.id))
            .collect();

        macros.sort_by(|a, b| a.0.cmp(&b.0));

        Self { macros }
    }

    fn find_by_name(&self, name: &str) -> Option<EntityId> {
        // Binary search for exact match
        match self
            .macros
            .binary_search_by(|probe| probe.0.as_str().cmp(name))
        {
            Ok(i) => Some(self.macros[i].1),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast_parser::AstParser;
    use cce_types::EntityKind;

    #[test]
    fn test_macro_body_extractor_simple_macro() {
        let mut parser = AstParser::new();
        let code = r#"macro_rules! vec {
    ($x:expr) => {
        {
            let mut v = Vec::new();
            v.push($x);
            v
        }
    };
}"#;

        let (tree, _) = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse Rust code");

        let entities = crate::parser::extractor::EntityExtractor::new()
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract entities");

        let macro_entity = entities
            .iter()
            .find(|e| e.name == "vec" && e.kind == EntityKind::Macro)
            .expect("vec macro entity should exist");

        let mut behavior = BehaviorStore::default();
        let extractor = MacroBodyExtractor::new();
        extractor
            .extract(&tree, code, &Language::Rust, &entities, &mut behavior)
            .expect("extract should succeed");

        let macro_body = behavior
            .get(macro_entity.id)
            .expect("macro behavior entry should exist");

        assert_eq!(macro_body.facts.len(), 1);
        assert_eq!(macro_body.facts[0].kind, BehaviorFactKind::MacroBody);
        assert!(macro_body.facts[0].text.contains("let mut v"));
        assert!(macro_body.facts[0].text.contains("v.push"));
    }

    #[test]
    fn test_macro_body_extractor_multi_rule_macro() {
        let mut parser = AstParser::new();
        let code = r#"macro_rules! write {
    ($dst:expr, $($arg:tt)*) => {
        $dst.write_fmt(format_args!($($arg)*))
    };
    ($dst:expr, $($arg:tt)*) => {
        writeln!($dst, $($arg)*)
    };
}"#;

        let (tree, _) = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse Rust code");

        let entities = crate::parser::extractor::EntityExtractor::new()
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract entities");

        let macro_entity = entities
            .iter()
            .find(|e| e.name == "write" && e.kind == EntityKind::Macro)
            .expect("write macro entity should exist");

        let mut behavior = BehaviorStore::default();
        let extractor = MacroBodyExtractor::new();
        extractor
            .extract(&tree, code, &Language::Rust, &entities, &mut behavior)
            .expect("extract should succeed");

        let macro_body = behavior
            .get(macro_entity.id)
            .expect("macro behavior entry should exist");

        assert_eq!(macro_body.facts.len(), 2, "should extract both macro rules");
        assert!(macro_body.facts[0].text.contains("write_fmt"));
        assert!(macro_body.facts[1].text.contains("writeln!"));
    }

    #[test]
    fn test_macro_body_extractor_message_macro() {
        let mut parser = AstParser::new();
        let code = r#"macro_rules! message {
    ($($tt:tt)*) => {
        if crate::messages::messages() {
            eprintln_locked!($($tt)*);
        }
    };
}"#;

        let (tree, _) = parser
            .parse_with_tree(code, &Language::Rust)
            .expect("Failed to parse Rust code");

        let entities = crate::parser::extractor::EntityExtractor::new()
            .extract(&tree, code, &Language::Rust)
            .expect("Failed to extract entities");

        let macro_entity = entities
            .iter()
            .find(|e| e.name == "message" && e.kind == EntityKind::Macro)
            .expect("message macro entity should exist");

        let mut behavior = BehaviorStore::default();
        let extractor = MacroBodyExtractor::new();
        extractor
            .extract(&tree, code, &Language::Rust, &entities, &mut behavior)
            .expect("extract should succeed");

        let macro_body = behavior
            .get(macro_entity.id)
            .expect("macro behavior entry should exist");

        assert_eq!(macro_body.facts.len(), 1);
        assert_eq!(macro_body.facts[0].kind, BehaviorFactKind::MacroBody);
        assert!(
            macro_body.facts[0]
                .text
                .contains("if crate::messages::messages()")
        );
        assert!(macro_body.facts[0].text.contains("eprintln_locked!"));
    }

    #[test]
    fn test_macro_body_extractor_real_messages_file() {
        let fixture_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/messages.rs");
        let src = std::fs::read_to_string(&fixture_path).expect("messages.rs fixture should exist");

        let mut coordinator = crate::parser::coordinator::ParseCoordinator::new();
        let parsed = coordinator
            .parse("messages.rs", &src)
            .expect("parse should succeed");

        // All four macro definitions should have bodies extracted
        for name in [
            "message",
            "err_message",
            "ignore_message",
            "eprintln_locked",
        ] {
            let found = parsed
                .entities
                .iter()
                .filter(|e| e.name == name && e.kind == EntityKind::Macro)
                .any(|e| {
                    parsed.behavior.get(e.id).is_some_and(|b| {
                        b.facts
                            .iter()
                            .any(|f| f.kind == BehaviorFactKind::MacroBody)
                    })
                });
            assert!(found, "macro {} should have a MacroBody fact", name);
        }
    }
}
