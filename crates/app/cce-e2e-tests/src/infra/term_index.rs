use std::collections::HashMap;

use cce_storage_bm25::expand_query_tokens;
use cce_storage_common::FulltextDocument;
use cce_text::MixedTokenizer;

const FIELD_NAMES: [&str; 3] = ["title", "keywords", "content"];

/// BM25 document fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    Title = 0,
    Keywords = 1,
    Content = 2,
}

impl Field {
    pub const ALL: [Field; 3] = [Field::Title, Field::Keywords, Field::Content];

    pub fn from_index(idx: usize) -> Option<Field> {
        match idx {
            0 => Some(Field::Title),
            1 => Some(Field::Keywords),
            2 => Some(Field::Content),
            _ => None,
        }
    }

    pub fn index(&self) -> usize {
        *self as usize
    }
}

/// In-memory term-document index built once and reused for all parameter sets.
///
/// All field texts are tokenized during construction. The resulting
/// postings and statistics are invariant under k1, b, and field-weight changes.
#[derive(Debug, Clone)]
pub struct InMemoryTermIndex {
    /// term → (doc_id, field_index, term_frequency)
    pub postings: HashMap<String, Vec<(u32, u8, u32)>>,
    /// term → [df_title, df_keywords, df_content]
    pub term_field_df: HashMap<String, [u32; 3]>,
    /// per-doc field lengths: [title_len, keywords_len, content_len]
    pub doc_lengths: Vec<[u32; 3]>,
    /// number of docs with non-empty content per field
    pub field_doc_count: [u32; 3],
    /// total tokens per field across all docs
    pub field_total_tokens: [u64; 3],
    pub n_docs: u32,
}

impl InMemoryTermIndex {
    /// Average raw token count of a field across ALL documents.
    ///
    /// Mirrors tantivy's `average_fieldnorm = total_num_tokens / num_docs`
    /// (the denominator is the full segment doc count, not the docs that
    /// carry the field — empty-field docs contribute a zero fieldnorm).
    pub fn avg_field_length(&self, field: Field) -> f64 {
        if self.n_docs == 0 {
            return 0.0;
        }
        self.field_total_tokens[field.index()] as f64 / self.n_docs as f64
    }

    pub fn term_doc_freq(&self, term: &str) -> u64 {
        self.term_field_df
            .get(term)
            .map(|df| df.iter().map(|count| u64::from(*count)).sum())
            .unwrap_or(0)
    }
}

/// Build a term index from BM25 documents.
///
/// Tokenises each field (title, keywords, content) independently,
/// counts term frequencies per (doc, field), and records per-field
/// document frequencies.
pub fn build_term_index(documents: &[FulltextDocument]) -> InMemoryTermIndex {
    let n_docs = documents.len();
    let mut postings: HashMap<String, Vec<(u32, u8, u32)>> = HashMap::new();
    let mut term_field_df: HashMap<String, [u32; 3]> = HashMap::new();
    let mut doc_lengths: Vec<[u32; 3]> = Vec::with_capacity(n_docs);
    let mut field_doc_count = [0u32; 3];
    let mut field_total_tokens = [0u64; 3];

    for (doc_id, doc) in documents.iter().enumerate() {
        let mut lengths = [0u32; 3];

        for (field_idx, field_name) in FIELD_NAMES.iter().enumerate() {
            let text = doc.get_field(field_name).map(|s| s.as_str()).unwrap_or("");
            let tokens = tokenize_text(text);
            lengths[field_idx] = tokens.len() as u32;

            if tokens.is_empty() {
                continue;
            }
            field_doc_count[field_idx] += 1;
            field_total_tokens[field_idx] += tokens.len() as u64;

            let mut tf_counter: HashMap<String, u32> = HashMap::new();
            for token in &tokens {
                *tf_counter.entry(token.clone()).or_insert(0) += 1;
            }

            for (term, tf) in tf_counter {
                postings.entry(term.clone()).or_default().push((
                    doc_id as u32,
                    field_idx as u8,
                    tf,
                ));

                let df_entry = term_field_df.entry(term).or_insert([0u32; 3]);
                df_entry[field_idx] += 1;
            }
        }

        doc_lengths.push(lengths);
    }

    InMemoryTermIndex {
        postings,
        term_field_df,
        doc_lengths,
        field_doc_count,
        field_total_tokens,
        n_docs: n_docs as u32,
    }
}

/// Tokenize text for BM25 scoring.
///
/// Uses the shared production `MixedTokenizer` so benchmark term statistics
/// mirror the real tantivy indexing behavior (identifier splitting, CJK
/// segmentation, dual-form tokens). The index stores attested word forms
/// only; depluralization applies on the query side (`expand_query`), so
/// index statistics stay untouched and existing deployments need no
/// reindexing for the query-side gain.
pub fn tokenize_text(text: &str) -> Vec<String> {
    MixedTokenizer::new().tokenize(text)
}

/// A single production-expanded query term.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryTerm {
    pub text: String,
    pub scale: f64,
}

/// A query produced by the production BM25 query-expansion pipeline.
#[derive(Debug, Clone, Default)]
pub struct ExpandedQuery {
    pub terms: Vec<QueryTerm>,
}

impl ExpandedQuery {
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

/// Build query terms exactly as production `Bm25Retrieval::parse_query` does:
/// tokenize the query with the production tokenizer, then expand whole/split
/// forms using the corpus document-frequency table.
pub fn expand_query(query_text: &str, index: &InMemoryTermIndex) -> ExpandedQuery {
    // Query-side plural variants (shared implementation with production
    // retrieval): a plural query term also tries its singular form, so
    // `files` reaches docs that only spell `file`. Index statistics are
    // untouched, so this cannot dilute attested-term weights.
    let tokens =
        cce_text::expand_with_plural_variants(MixedTokenizer::new().tokenize_offsets(query_text));
    let terms = expand_query_tokens(&tokens, |text| index.term_doc_freq(text))
        .into_iter()
        .map(|(token, scale)| QueryTerm {
            text: token.text,
            scale: scale as f64,
        })
        .collect();
    ExpandedQuery { terms }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize_text("Hello World");
        assert_eq!(tokens, vec!["hello", "world"]);
    }

    #[test]
    fn test_tokenize_punctuation() {
        let tokens = tokenize_text("foo.bar()");
        // MixedTokenizer emits the original form plus split forms.
        assert!(tokens.contains(&"foo.bar".to_string()));
        assert!(tokens.contains(&"foo".to_string()));
        assert!(tokens.contains(&"bar".to_string()));
    }

    #[test]
    fn test_tokenize_underscore() {
        let tokens = tokenize_text("foo_bar");
        // MixedTokenizer emits the original form plus split forms.
        assert!(tokens.contains(&"foo_bar".to_string()));
        assert!(tokens.contains(&"foo".to_string()));
        assert!(tokens.contains(&"bar".to_string()));
    }

    #[test]
    fn test_build_term_index_single_doc() {
        let doc = FulltextDocument::new("test:1")
            .with_field("title", "Test Function")
            .with_field("content", "This is a test function for testing")
            .with_field("keywords", "test func");
        let index = build_term_index(&[doc]);

        assert_eq!(index.n_docs, 1);
        assert!(index.postings.contains_key("test"));
        assert!(index.postings.contains_key("function"));
        assert!(index.postings.contains_key("func"));
        assert_eq!(index.doc_lengths[0], [2, 2, 7]);
    }

    fn index_with_text(text: &str) -> InMemoryTermIndex {
        let doc = FulltextDocument::new("test:0")
            .with_field("title", text)
            .with_field("keywords", "")
            .with_field("content", "");
        build_term_index(&[doc])
    }

    #[test]
    fn test_query_forms_identifier_expands_whole_and_split_tokens() {
        let index = index_with_text("OnceCell::get_or_init");
        let forms = expand_query("OnceCell::get_or_init", &index);
        assert!(
            forms
                .terms
                .iter()
                .any(|t| t.text == "oncecell::get_or_init" && t.scale == 1.0)
        );
        for split in ["once", "cell", "get", "or", "init"] {
            assert!(
                forms
                    .terms
                    .iter()
                    .any(|t| t.text == split && t.scale == 0.5),
                "expected split token {split}"
            );
        }
    }

    #[test]
    fn test_query_forms_whole_term_df_zero_falls_back_to_split_tokens() {
        let index = index_with_text("once cell get or init");
        let forms = expand_query("OnceCell::get_or_init", &index);
        assert!(
            !forms
                .terms
                .iter()
                .any(|t| t.text == "oncecell::get_or_init")
        );
        for split in ["once", "cell", "get", "or", "init"] {
            assert!(
                forms
                    .terms
                    .iter()
                    .any(|t| t.text == split && t.scale == 1.0),
                "expected split token {split}"
            );
        }
    }

    #[test]
    fn test_query_forms_drops_stopwords() {
        let index = index_with_text("read_file function");
        let forms = expand_query("read_file function", &index);
        assert!(
            forms
                .terms
                .iter()
                .any(|t| t.text == "read_file" && t.scale == 1.0)
        );
        assert!(!forms.terms.iter().any(|t| t.text == "function"));
    }

    #[test]
    fn test_query_forms_plain_query_keeps_whole_tokens() {
        let index = index_with_text("plain query");
        let forms = expand_query("plain query", &index);
        assert_eq!(
            forms
                .terms
                .iter()
                .map(|t| (t.text.as_str(), t.scale))
                .collect::<Vec<_>>(),
            vec![("plain", 1.0), ("query", 1.0)]
        );
    }
}
