//! Keyword extractor for BM25 indexing
//!
//! Extracts keywords from code entities for BM25 indexing.
//! Produces two kinds of keywords:
//! 1. Original form (lowered): `get_or_init`, `once_cell`
//! 2. Type words: parameter/return type tokens (`gitignore`, `builder`)
//!
//! Split forms (`get`, `or`, `init`) are deliberately NOT stored here: the
//! `title` and `content` fields already produce them via the shared tokenizer,
//! so re-storing them in `keywords` would double/triple-count their tf across
//! fields. `keywords` is now a "exact name + type word" complement to `title`.

use cce_types::GroupedEntity;

/// Keyword extractor for BM25 indexing
pub struct KeywordExtractor;

impl KeywordExtractor {
    /// Create a new keyword extractor
    pub fn new() -> Self {
        Self
    }

    /// Extract keywords from an entity name and its type information.
    ///
    /// Produces two kinds of bounded keywords:
    /// - Original form: the entity name as-is, lowered
    /// - Type words: tokens from parameter/return types (structured AST fields)
    ///
    /// Split forms are intentionally excluded (the tokenizer already emits them
    /// for `title`/`content`).
    pub fn extract(&self, entity: &GroupedEntity) -> Vec<String> {
        let name = &entity.name;
        if name.is_empty() {
            return vec![];
        }

        let lowered = name.to_lowercase();
        let mut keywords = Vec::new();

        // 1. Original form (lowered)
        keywords.push(lowered.clone());

        // 2. Parameter type keywords (structured AST field, not string-matched)
        for (_, param_type) in &entity.parameters {
            if let Some(ty) = param_type {
                for keyword in Self::extract_type_keywords(ty) {
                    keywords.push(keyword);
                }
            }
        }

        // 3. Return type keywords (structured AST field)
        if let Some(return_type) = &entity.return_type {
            for keyword in Self::extract_type_keywords(return_type) {
                keywords.push(keyword);
            }
        }

        self.deduplicate(keywords)
    }

    /// Split a name into component words at `_`, `-`, and camelCase boundaries.
    ///
    /// Examples:
    /// - `get_or_init` → `["get", "or", "init"]`
    /// - `OnceCell` → `["once", "cell"]`
    /// - `XMLParser` → `["xml", "parser"]`
    /// - `calculate_total_price` → `["calculate", "total", "price"]`
    fn split_name_parts(ident: &str) -> Vec<String> {
        cce_utils::text::split_identifier(ident)
    }

    /// Split a word on camelCase/PascalCase boundaries.
    /// Words are output in lowered form.
    /// Extract keywords from a type annotation string.
    ///
    /// Splits on generic/tuple/pointer separators and extracts meaningful words.
    /// E.g., `Option<Vec<PathBuf>>` → `["option", "vec", "pathbuf"]`
    fn extract_type_keywords(type_text: &str) -> Vec<String> {
        let mut keywords = Vec::new();
        for segment in type_text
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|s| !s.is_empty())
        {
            keywords.push(segment.to_lowercase());
            for word in Self::split_name_parts(segment) {
                let lower = word.to_lowercase();
                if lower != segment.to_lowercase() && lower.len() >= 2 {
                    keywords.push(lower);
                }
            }
        }
        keywords
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

        // Parameter types are now extracted (structured AST fields)
        assert!(
            keywords.contains(&"f64".to_string()),
            "f64 from parameter type"
        );
        assert!(
            keywords.contains(&"i32".to_string()),
            "i32 from parameter type"
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
    fn test_extract_type_keywords_simple() {
        let keywords = KeywordExtractor::extract_type_keywords("GitignoreBuilder");
        assert!(keywords.iter().any(|k| k == "gitignorebuilder"));
        assert!(keywords.iter().any(|k| k == "gitignore"));
        assert!(keywords.iter().any(|k| k == "builder"));
    }

    #[test]
    fn test_extract_type_keywords_generic() {
        let keywords = KeywordExtractor::extract_type_keywords("Option<Vec<PathBuf>>");
        assert!(keywords.iter().any(|k| k == "option"));
        assert!(keywords.iter().any(|k| k == "vec"));
        assert!(keywords.iter().any(|k| k == "pathbuf"));
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
        assert!(keywords.iter().any(|k| k == "pathbuf"), "from param type");
        assert!(keywords.iter().any(|k| k == "str"), "line param type");
        assert!(keywords.iter().any(|k| k == "result"), "return type");
        assert!(
            keywords.iter().any(|k| k == "gitignorebuilder"),
            "return type inner"
        );
        assert!(
            keywords.iter().any(|k| k == "gitignore"),
            "return type split"
        );
        assert!(keywords.iter().any(|k| k == "builder"), "return type split");
    }
}
