//! Keyword extractor for BM25 indexing
//!
//! Extracts keywords from code entities for BM25 indexing.
//! Produces the exact declaration name only:
//! 1. Original form (lowered): `get_or_init`, `once_cell`
//!
//! Split forms (`get`, `or`, `init`) are deliberately NOT stored here: the
//! `title` and `content` fields already produce them via the shared tokenizer,
//! so re-storing them in `keywords` would double/triple-count their tf across
//! fields. Parameter types, return types, and call targets live in `content`
//! only; promoting them to the boosted `keywords` field lets callers outrank
//! callees and dilutes the identity signal.

use cce_types::GroupedEntity;

/// Keyword extractor for BM25 indexing
pub struct KeywordExtractor;

impl KeywordExtractor {
    /// Create a new keyword extractor
    pub fn new() -> Self {
        Self
    }

    /// Extract keywords from an entity name.
    ///
    /// Returns the lowered declaration name only. Type information and call
    /// targets stay in `content` where the tokenizer splits them.
    ///
    /// Split forms are intentionally excluded (the tokenizer already emits them
    /// for `title`/`content`).
    pub fn extract(&self, entity: &GroupedEntity) -> Vec<String> {
        let name = &entity.name;
        if name.is_empty() {
            return vec![];
        }

        let lowered = name.to_lowercase();
        self.deduplicate(vec![lowered])
    }

    /// Split a name into component words at `_`, `-`, and camelCase boundaries.
    ///
    /// Examples:
    /// - `get_or_init` → `["get", "or", "init"]`
    /// - `OnceCell` → `["once", "cell"]`
    /// - `XMLParser` → `["xml", "parser"]`
    /// - `calculate_total_price` → `["calculate", "total", "price"]`
    #[allow(dead_code)]
    fn split_name_parts(ident: &str) -> Vec<String> {
        cce_utils::text::split_identifier(ident)
    }

    /// Deduplicate keywords while preserving order
    fn deduplicate(&self, keywords: Vec<String>) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();

        for keyword in keywords {
            let lower_key = keyword.to_lowercase();

            if lower_key.is_empty() {
                continue;
            }

            if lower_key.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }

            if lower_key.len() < 2 {
                continue;
            }

            if seen.insert(lower_key.clone()) {
                result.push(lower_key);
            }
        }

        result
    }
}

impl Default for KeywordExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use cce_types::{EntityId, EntityKind};

    fn create_test_function() -> GroupedEntity {
        GroupedEntity {
            id: EntityId(1),
            kind: EntityKind::Function,
            name: "calculate_total_price".to_string(),
            signature: "fn calculate_total_price(price: f64, quantity: i32) -> f64".to_string(),
            parameters: smallvec::smallvec![
                ("price".into(), Some("f64".into())),
                ("quantity".into(), Some("i32".into())),
            ],
            return_type: Some("f64".to_string()),
            doc_comment: Some("/// Calculates the total price.".to_string()),
            modifiers: Vec::new(),
            attributes: HashMap::new(),
            subtype: None,
            is_stdlib: false,
            stdlib_category: None,
            metadata: Default::default(),
        }
    }

    #[test]
    fn test_extract_keywords_original_form() {
        let extractor = KeywordExtractor::new();
        let entity = create_test_function();
        let keywords = extractor.extract(&entity);

        // Should contain the original name (lowered)
        assert!(keywords.contains(&"calculate_total_price".to_string()));

        // Split words from the entity name are NOT stored (tokenizer emits them
        // for title/content, so keywords no longer duplicates them).
        assert!(
            !keywords.contains(&"calculate".to_string()),
            "split word should not be in keywords: {:?}",
            keywords
        );

        // Parameter and return types stay in content only.
        assert!(
            !keywords.contains(&"f64".to_string()),
            "f64 must not be a keyword: {:?}",
            keywords
        );
        assert!(
            !keywords.contains(&"i32".to_string()),
            "i32 must not be a keyword: {:?}",
            keywords
        );

        // Docstring words should NOT be keywords
        assert!(!keywords.contains(&"calculates".to_string()));
    }

    #[test]
    fn test_extract_keywords_split_form() {
        let extractor = KeywordExtractor::new();

        // snake_case: "calculate_total_price" → original form only, no splits
        let entity = GroupedEntity {
            name: "calculate_total_price".to_string(),
            ..Default::default()
        };
        let keywords = extractor.extract(&entity);
        assert!(
            !keywords.contains(&"calculate".to_string()),
            "should NOT contain split 'calculate', got: {:?}",
            keywords
        );
        assert!(
            !keywords.contains(&"total".to_string()),
            "should NOT contain split 'total'"
        );
        assert!(
            !keywords.contains(&"price".to_string()),
            "should NOT contain split 'price'"
        );
        assert!(
            keywords.contains(&"calculate_total_price".to_string()),
            "should contain original form"
        );
    }

    #[test]
    fn test_extract_keywords_camel_case() {
        let extractor = KeywordExtractor::new();

        let entity = GroupedEntity {
            name: "OnceCell".to_string(),
            ..Default::default()
        };
        let keywords = extractor.extract(&entity);
        assert!(
            !keywords.contains(&"once".to_string()),
            "should NOT contain split 'once', got: {:?}",
            keywords
        );
        assert!(
            !keywords.contains(&"cell".to_string()),
            "should NOT contain split 'cell'"
        );
        assert!(
            keywords.contains(&"oncecell".to_string()),
            "should contain original form 'oncecell'"
        );
    }

    #[test]
    fn test_extract_keywords_no_compact_form() {
        let extractor = KeywordExtractor::new();

        let entity = GroupedEntity {
            name: "get_or_init".to_string(),
            ..Default::default()
        };
        let keywords = extractor.extract(&entity);
        assert!(
            !keywords.contains(&"getorinit".to_string()),
            "should NOT contain compact 'getorinit', got: {:?}",
            keywords
        );
        assert!(
            keywords.contains(&"get_or_init".to_string()),
            "should contain original form"
        );
        assert!(!keywords.contains(&"get".to_string()));
        assert!(!keywords.contains(&"or".to_string()));
        assert!(!keywords.contains(&"init".to_string()));
    }

    #[test]
    fn test_extract_keywords_parking_lot_core() {
        let extractor = KeywordExtractor::new();

        let entity = GroupedEntity {
            name: "parking_lot_core".to_string(),
            ..Default::default()
        };
        let keywords = extractor.extract(&entity);
        assert!(
            !keywords.contains(&"parking".to_string()),
            "should NOT contain 'parking'"
        );
        assert!(
            !keywords.contains(&"lot".to_string()),
            "should NOT contain 'lot'"
        );
        assert!(
            !keywords.contains(&"core".to_string()),
            "should NOT contain 'core'"
        );
        assert!(
            keywords.contains(&"parking_lot_core".to_string()),
            "should contain original"
        );
        assert!(
            !keywords.contains(&"parkinglotcore".to_string()),
            "should NOT contain compact 'parkinglotcore'"
        );
    }

    #[test]
    fn test_extract_keywords_no_duplicates() {
        let extractor = KeywordExtractor::new();
        let entity = GroupedEntity {
            name: "test_test".to_string(),
            ..Default::default()
        };
        let keywords = extractor.extract(&entity);

        let test_count = keywords.iter().filter(|k| *k == "test").count();
        assert!(
            test_count <= 1,
            "no duplicate 'test', keywords: {:?}",
            keywords
        );
    }

    #[test]
    fn test_split_name_parts_snake_case() {
        let result = KeywordExtractor::split_name_parts("get_or_init");
        assert_eq!(result, vec!["get", "or", "init"]);
    }

    #[test]
    fn test_split_name_parts_camel_case() {
        let result = KeywordExtractor::split_name_parts("OnceCell");
        assert_eq!(result, vec!["once", "cell"]);
    }

    #[test]
    fn test_split_name_parts_mixed() {
        let result = KeywordExtractor::split_name_parts("processUserData");
        assert_eq!(result, vec!["process", "user", "data"]);
    }

    #[test]
    fn test_split_name_parts_with_acronym() {
        let result = KeywordExtractor::split_name_parts("XMLParser");
        assert_eq!(result, vec!["xml", "parser"]);
    }

    #[test]
    fn test_deduplicate_removes_single_char() {
        let extractor = KeywordExtractor::new();
        let keywords = vec!["a".to_string(), "valid".to_string()];
        let result = extractor.deduplicate(keywords);
        assert!(!result.contains(&"a".to_string()));
        assert!(result.contains(&"valid".to_string()));
    }

    #[test]
    fn test_deduplicate_removes_digits() {
        let extractor = KeywordExtractor::new();
        let keywords = vec!["123".to_string(), "abc".to_string()];
        let result = extractor.deduplicate(keywords);
        assert!(!result.contains(&"123".to_string()));
        assert!(result.contains(&"abc".to_string()));
    }

    #[test]
    fn test_extract_type_keywords_from_entity() {
        let extractor = KeywordExtractor::new();
        let entity = GroupedEntity {
            name: "add_line".to_string(),
            parameters: smallvec::smallvec![
                ("from".into(), Some("PathBuf".into())),
                ("line".into(), Some("str".into())),
            ],
            return_type: Some("Result<GitignoreBuilder>".to_string()),
            ..Default::default()
        };
        let keywords = extractor.extract(&entity);
        assert!(keywords.iter().any(|k| k == "add_line"), "entity name");
        assert!(
            !keywords.iter().any(|k| k == "add"),
            "split word should not be in keywords"
        );
        assert!(
            !keywords.iter().any(|k| k == "line"),
            "split word should not be in keywords"
        );
        assert!(
            !keywords.iter().any(|k| k == "pathbuf"),
            "param types stay in content"
        );
        assert!(
            !keywords.iter().any(|k| k == "str"),
            "param types stay in content"
        );
        assert!(
            !keywords.iter().any(|k| k == "result"),
            "return types stay in content"
        );
        assert!(
            !keywords.iter().any(|k| k == "gitignorebuilder"),
            "return types stay in content"
        );
    }
}
