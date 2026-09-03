//! Highlight utilities for BM25 search results

use aho_corasick::AhoCorasick;
use cce_text::MixedTokenizer;
use std::collections::HashMap;

/// Matched term information (re-exported for convenience)
pub use crate::types::MatchedTerm;

/// Internal match information used during highlight processing
#[derive(Debug, Clone)]
struct InternalMatch {
    start: usize,
    end: usize,
}

/// Extract matched terms from query and the stored title field
pub fn extract_matched_terms(query_text: &str, title_value: &str) -> Vec<MatchedTerm> {
    let mut matched_terms = Vec::new();

    let query_terms = tokenize_terms(query_text);
    if query_terms.is_empty() {
        return matched_terms;
    }

    let field_terms = tokenize_terms(title_value);
    for term in &query_terms {
        let count = field_terms.iter().filter(|t| *t == term).count();
        if count > 0 {
            matched_terms.push(MatchedTerm {
                term: term.clone(),
                field: "title".to_string(),
                count,
            });
        }
    }

    matched_terms
}

/// Tokenize text into lowercase terms using the shared `MixedTokenizer`.
fn tokenize_terms(text: &str) -> Vec<String> {
    let tokenizer = MixedTokenizer::new();
    tokenizer.tokenize(text)
}

/// Check if a character is a word character (alphanumeric or underscore)
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Check if a match at the given byte positions has whole-word boundaries
fn is_word_match(text: &str, start: usize, end: usize) -> bool {
    if start > 0 {
        if let Some(c) = text[..start].chars().last() {
            if is_word_char(c) {
                return false;
            }
        }
    }

    if end < text.len() {
        if let Some(c) = text[end..].chars().next() {
            if is_word_char(c) {
                return false;
            }
        }
    }

    true
}

/// Find all matching term positions in the text using Aho-Corasick
fn find_all_matches(text: &str, query_terms: &[String]) -> Vec<InternalMatch> {
    if text.is_empty() || query_terms.is_empty() {
        return Vec::new();
    }

    let Ok(ac) = AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .build(query_terms)
    else {
        return Vec::new();
    };

    let mut matches = Vec::new();

    for m in ac.find_iter(text) {
        let start = m.start();
        let end = m.end();

        if is_word_match(text, start, end) {
            matches.push(InternalMatch { start, end });
        }
    }

    matches
}

/// Find tokenizer-based matches
fn find_token_matches(text: &str, query_terms: &[String]) -> Vec<InternalMatch> {
    if text.is_empty() || query_terms.is_empty() {
        return Vec::new();
    }

    let tokenizer = MixedTokenizer::new();
    let query_tokens: std::collections::HashSet<String> = query_terms.iter().cloned().collect();

    let mut matches = Vec::new();
    for token in tokenizer.tokenize_offsets(text) {
        if query_tokens.contains(&token.text) && token.offset_from < token.offset_to {
            matches.push(InternalMatch {
                start: token.offset_from,
                end: token.offset_to,
            });
        }
    }

    matches
}

/// Merge overlapping matches into combined spans
fn merge_overlaps(raw_matches: Vec<InternalMatch>) -> Vec<InternalMatch> {
    if raw_matches.is_empty() {
        return raw_matches;
    }

    let mut sorted = raw_matches;
    sorted.sort_by_key(|m| m.start);

    let mut merged: Vec<InternalMatch> = Vec::with_capacity(sorted.len());

    for m in sorted {
        if let Some(last) = merged.last_mut() {
            if m.start <= last.end {
                last.end = last.end.max(m.end);
                continue;
            }
        }
        merged.push(m);
    }

    merged
}

/// Count occurrences of a term in text (whole word match)
pub fn count_term_occurrences(text: &str, term: &str) -> usize {
    if term.is_empty() {
        return 0;
    }

    let query_terms = vec![term.to_string()];
    let matches = find_all_matches(text, &query_terms);
    matches.len()
}

/// Generate highlighted snippets for query terms in the stored title field
pub fn generate_highlights(query_text: &str, title_value: &str) -> HashMap<String, String> {
    let mut highlights = HashMap::new();

    let query_terms = tokenize_terms(query_text);

    if query_terms.is_empty() {
        return highlights;
    }

    if let Some(title_highlight) = highlight_text(title_value, &query_terms) {
        highlights.insert("title".to_string(), title_highlight);
    }

    highlights
}

/// Highlight text by wrapping matching terms with `<mark>` tags
pub fn highlight_text(text: &str, query_terms: &[String]) -> Option<String> {
    if text.is_empty() || query_terms.is_empty() {
        return None;
    }

    let token_matches = find_token_matches(text, query_terms);
    let word_matches = find_all_matches(text, query_terms);

    let mut raw_matches = token_matches;
    raw_matches.extend(word_matches);

    if raw_matches.is_empty() {
        return None;
    }

    let matches = merge_overlaps(raw_matches);

    let mut result = String::new();
    let mut pos = 0;

    for m in &matches {
        result.push_str(&text[pos..m.start]);
        result.push_str("<mark>");
        result.push_str(&text[m.start..m.end]);
        result.push_str("</mark>");
        pos = m.end;
    }

    result.push_str(&text[pos..]);

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_text_basic() {
        let terms = vec!["rust".to_string()];
        let result = highlight_text("Learn Rust programming", &terms);
        assert_eq!(
            result,
            Some("Learn <mark>Rust</mark> programming".to_string())
        );
    }

    #[test]
    fn test_highlight_text_multiple_terms() {
        let terms = vec!["async".to_string(), "await".to_string()];
        let result = highlight_text("async fn foo() -> impl Future { await }", &terms);
        let r = result.unwrap();
        assert!(r.contains("<mark>async</mark>"));
        assert!(r.contains("<mark>await</mark>"));
    }

    #[test]
    fn test_highlight_text_no_match() {
        let terms = vec!["python".to_string()];
        let result = highlight_text("Learn Rust programming", &terms);
        assert_eq!(result, None);
    }

    #[test]
    fn test_highlight_text_empty() {
        let terms = vec!["rust".to_string()];
        assert_eq!(highlight_text("", &terms), None);
        assert_eq!(highlight_text("hello", &[] as &[String]), None);
    }

    #[test]
    fn test_highlight_text_word_boundary() {
        let terms = vec!["rust".to_string()];
        let result = highlight_text("I am a rustacean", &terms);
        assert_eq!(result, None);
        let result = highlight_text("I love Rust", &terms);
        assert_eq!(result, Some("I love <mark>Rust</mark>".to_string()));
    }

    #[test]
    fn test_count_term_occurrences() {
        let text = "the quick brown fox jumps over the lazy dog";
        assert_eq!(count_term_occurrences(text, "the"), 2);
        assert_eq!(count_term_occurrences(text, "fox"), 1);
        assert_eq!(count_term_occurrences(text, "cat"), 0);
    }

    #[test]
    fn test_count_term_occurrences_word_boundary() {
        let text = "async fn async_foo()";
        assert_eq!(count_term_occurrences(text, "async"), 1);
    }

    #[test]
    fn test_extract_matched_terms() {
        let terms = extract_matched_terms("rust async", "Rust function");
        assert_eq!(terms.len(), 1);

        let title_term = terms.iter().find(|t| t.field == "title").unwrap();
        assert_eq!(title_term.term, "rust");
        assert_eq!(title_term.count, 1);
    }

    #[test]
    fn test_generate_highlights() {
        let highlights = generate_highlights("rust async", "Rust module");
        assert!(highlights.contains_key("title"));
    }

    #[test]
    fn test_highlight_text_case_insensitive() {
        let terms = vec!["rust".to_string()];
        let result = highlight_text("I love RUST and Rust and rust", &terms);
        assert_eq!(
            result,
            Some(
                "I love <mark>RUST</mark> and <mark>Rust</mark> and <mark>rust</mark>".to_string()
            )
        );
    }
}
