//! Query intent auto-detection
//!
//! Provides lightweight rule-based classification of query intent
//! when the caller does not explicitly set `QueryIntent`.

use super::query_options::QueryIntent;

/// Detect query intent from the query text using heuristic rules.
///
/// Rules (evaluated in order, first match wins):
/// 1. Entity: contains code-like identifiers (CamelCase, snake_case with `::`,
///    file extensions, or `fn`/`struct`/`class` keywords)
/// 2. Semantic: contains natural-language question words (what, how, why, etc.)
/// 3. Keyword: contains operators, punctuation, or short precise terms
/// 4. Default: Hybrid
pub fn detect_intent(query: &str) -> QueryIntent {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return QueryIntent::Hybrid;
    }

    if looks_like_entity_query(trimmed) {
        return QueryIntent::Entity;
    }

    if looks_like_semantic_query(trimmed) {
        return QueryIntent::Semantic;
    }

    if looks_like_keyword_query(trimmed) {
        return QueryIntent::Keyword;
    }

    QueryIntent::Hybrid
}

fn looks_like_entity_query(query: &str) -> bool {
    let lower = query.to_lowercase();

    // Code keywords that indicate entity lookup
    let code_keywords = [
        "fn ",
        "func ",
        "function ",
        "struct ",
        "class ",
        "trait ",
        "impl ",
        "enum ",
        "type ",
        "const ",
        "static ",
        "mod ",
        "use ",
        "pub ",
    ];
    for kw in &code_keywords {
        if lower.contains(kw) {
            return true;
        }
    }

    // Path-like patterns (module::submodule)
    if query.contains("::") {
        return true;
    }

    // File extensions commonly searched
    let extensions = [".rs", ".py", ".ts", ".js", ".go", ".java", ".cpp", ".h"];
    for ext in &extensions {
        if query.contains(ext) {
            return true;
        }
    }

    // CamelCase identifier (at least one uppercase after lowercase)
    has_camel_case(query)
}

fn looks_like_semantic_query(query: &str) -> bool {
    let lower = query.to_lowercase();

    // Question words indicating natural-language intent
    let question_words = [
        "what ",
        "how ",
        "why ",
        "when ",
        "where ",
        "which ",
        "who ",
        "explain ",
        "describe ",
        "understand ",
        "difference between ",
    ];
    for qw in &question_words {
        if lower.starts_with(qw) || lower.contains(&format!(" {qw}")) {
            return true;
        }
    }

    // Long natural-language queries (more than 6 words) tend to be semantic
    let word_count = query.split_whitespace().count();
    word_count > 6 && !has_camel_case(query)
}

fn looks_like_keyword_query(query: &str) -> bool {
    // Operators and special characters that indicate keyword intent
    let operators = [
        '(', ')', '{', '}', '=', '>', '<', '+', '-', '*', '/', '!', '?', ':', ';', ',', '.', '|',
        '&', '%', '$', '@', '#', '~', '^',
    ];
    if query.chars().any(|c| operators.contains(&c)) {
        return true;
    }

    // snake_case identifiers (contain underscore)
    if query.contains('_') {
        return true;
    }

    false
}

fn has_camel_case(query: &str) -> bool {
    let mut prev_lowercase = false;
    for ch in query.chars() {
        if ch.is_ascii_lowercase() {
            prev_lowercase = true;
        } else if ch.is_ascii_uppercase() && prev_lowercase {
            return true;
        } else if !ch.is_ascii_alphanumeric() {
            prev_lowercase = false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_entity_intent() {
        assert_eq!(detect_intent("fn parse_query"), QueryIntent::Entity);
        assert_eq!(detect_intent("struct SearchConfig"), QueryIntent::Entity);
        assert_eq!(detect_intent("QueryCoordinator"), QueryIntent::Entity);
        assert_eq!(detect_intent("module::submodule"), QueryIntent::Entity);
        assert_eq!(detect_intent("search.rs"), QueryIntent::Entity);
    }

    #[test]
    fn test_detect_semantic_intent() {
        assert_eq!(
            detect_intent("how does the authentication flow work"),
            QueryIntent::Semantic
        );
        assert_eq!(
            detect_intent("what is the difference between vector and bm25"),
            QueryIntent::Semantic
        );
        assert_eq!(
            detect_intent("explain the search pipeline architecture"),
            QueryIntent::Semantic
        );
    }

    #[test]
    fn test_detect_keyword_intent() {
        assert_eq!(detect_intent("parse_query"), QueryIntent::Keyword);
        assert_eq!(detect_intent("score > 0.5"), QueryIntent::Keyword);
        assert_eq!(detect_intent("a+b"), QueryIntent::Keyword);
    }

    #[test]
    fn test_detect_hybrid_default() {
        assert_eq!(detect_intent("search config"), QueryIntent::Hybrid);
        assert_eq!(detect_intent(""), QueryIntent::Hybrid);
        assert_eq!(detect_intent("authentication"), QueryIntent::Hybrid);
    }

    #[test]
    fn test_camel_case_detection() {
        assert!(has_camel_case("SearchConfig"));
        assert!(has_camel_case("parseQuery"));
        assert!(!has_camel_case("search_config"));
        assert!(!has_camel_case("search"));
    }
}
