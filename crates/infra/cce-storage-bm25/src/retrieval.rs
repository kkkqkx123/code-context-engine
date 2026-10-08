//! BM25 retrieval (storage-layer read path)

use std::collections::HashMap;

use crate::Bm25Error;
use cce_text::MixedToken;
use tantivy::schema::Value;
use tantivy::tokenizer::{TextAnalyzer, Token, TokenStream};

use crate::manager::IndexManager;
use crate::schema::IndexSchema;
use cce_storage_common::{FulltextHit, FulltextSearchOptions, TermOperator};

/// BM25 retrieval handler — stateless read path over a Tantivy index.
#[derive(Debug, Clone, Default)]
pub struct Bm25Retrieval;

/// Upper bound for a single retrieval window (`limit + offset`).
///
/// Per-hit document materialization grows linearly with the window, so an
/// unbounded caller would reintroduce the linear amplification previously
/// observed with per-hit work. Requests beyond the window are truncated.
const MAX_RETRIEVAL_WINDOW: usize = 200;

/// Wrap a filter clause so it constrains the matched set without
/// contributing a BM25 score.
///
/// Tantivy has no `Filter` occurrence: every `Must` clause adds its term
/// score to each matched document. A tenant/epoch/category filter would
/// otherwise inflate all scores by a term-dependent constant (e.g. the
/// project term alone adds `ln(1 + 0.5/(N + 0.5))` per doc), breaking score
/// parity with the offline benchmark scorer and polluting cross-epoch
/// ranking. `BoostQuery` with a zero boost keeps the constraint while
/// zeroing the contribution.
fn no_score(query: Box<dyn tantivy::query::Query>) -> Box<dyn tantivy::query::Query> {
    Box::new(tantivy::query::BoostQuery::new(query, 0.0))
}

impl Bm25Retrieval {
    /// Create a new BM25 retrieval instance
    pub fn new() -> Self {
        Self
    }

    /// Search documents in the index
    pub fn search(
        &self,
        manager: &IndexManager,
        schema: &IndexSchema,
        query_text: &str,
        options: &FulltextSearchOptions,
    ) -> Result<Vec<FulltextHit>, Bm25Error> {
        let reader = manager.reader()?;
        let searcher = reader.searcher();

        let mut query = Self::parse_query(
            query_text,
            schema,
            manager.index(),
            &searcher,
            &options.field_weights,
            options.term_operator,
        )?;

        // Apply the epoch-view filter
        if !options.epochs.is_empty() {
            let epoch_clauses: Vec<(tantivy::query::Occur, Box<dyn tantivy::query::Query>)> =
                options
                    .epochs
                    .iter()
                    .map(|epoch| {
                        let epoch_term = tantivy::Term::from_field_i64(schema.epoch, *epoch);
                        let clause: Box<dyn tantivy::query::Query> =
                            Box::new(tantivy::query::TermQuery::new(
                                epoch_term,
                                tantivy::schema::IndexRecordOption::Basic,
                            ));
                        (tantivy::query::Occur::Should, clause)
                    })
                    .collect();
            let mut epoch_filter = tantivy::query::BooleanQuery::new(epoch_clauses);
            if options.epochs.len() > 1 {
                epoch_filter.set_minimum_number_should_match(1);
            }

            query = Box::new(tantivy::query::BooleanQuery::new(vec![
                (tantivy::query::Occur::Must, query),
                (
                    tantivy::query::Occur::Must,
                    no_score(Box::new(epoch_filter)),
                ),
            ]));

            // Parent-generation rows of overridden files stay hidden
            if options.epochs.len() > 1
                && let Some(excluded_files) = options
                    .excluded_files
                    .as_ref()
                    .filter(|files| !files.is_empty())
            {
                let parent_term = tantivy::Term::from_field_i64(schema.epoch, options.epochs[0]);
                let mut exclusion_clauses: Vec<(
                    tantivy::query::Occur,
                    Box<dyn tantivy::query::Query>,
                )> = vec![(
                    tantivy::query::Occur::Must,
                    Box::new(tantivy::query::TermQuery::new(
                        parent_term,
                        tantivy::schema::IndexRecordOption::Basic,
                    )),
                )];
                for path in excluded_files {
                    let path_term = tantivy::Term::from_field_text(schema.file_path, path);
                    exclusion_clauses.push((
                        tantivy::query::Occur::Should,
                        Box::new(tantivy::query::TermQuery::new(
                            path_term,
                            tantivy::schema::IndexRecordOption::Basic,
                        )),
                    ));
                }
                let mut exclusion = tantivy::query::BooleanQuery::new(exclusion_clauses);
                exclusion.set_minimum_number_should_match(1);

                query = Box::new(tantivy::query::BooleanQuery::new(vec![
                    (tantivy::query::Occur::Must, query),
                    (tantivy::query::Occur::MustNot, Box::new(exclusion)),
                ]));
            }
        }

        // Apply project_id filter
        {
            let project_id_str = options.project_id.to_string();
            let project_term = tantivy::Term::from_field_text(schema.project_id, &project_id_str);
            let project_filter: Box<dyn tantivy::query::Query> =
                Box::new(tantivy::query::TermQuery::new(
                    project_term,
                    tantivy::schema::IndexRecordOption::Basic,
                ));

            query = Box::new(tantivy::query::BooleanQuery::new(vec![
                (tantivy::query::Occur::Must, query),
                (tantivy::query::Occur::Must, no_score(project_filter)),
            ]));
        }

        // Exclude test chunks when requested
        if options.exclude_test {
            let test_term = tantivy::Term::from_field_u64(schema.test, 1);
            let test_filter: Box<dyn tantivy::query::Query> =
                Box::new(tantivy::query::TermQuery::new(
                    test_term,
                    tantivy::schema::IndexRecordOption::Basic,
                ));

            query = Box::new(tantivy::query::BooleanQuery::new(vec![
                (tantivy::query::Occur::Must, query),
                (tantivy::query::Occur::MustNot, test_filter),
            ]));
        }

        // Include only specific categories
        if !options.include_categories.is_empty() {
            let clauses: Vec<(tantivy::query::Occur, Box<dyn tantivy::query::Query>)> = options
                .include_categories
                .iter()
                .map(|cat| {
                    let term = tantivy::Term::from_field_u64(schema.category, cat.as_u8() as u64);
                    let clause: Box<dyn tantivy::query::Query> =
                        Box::new(tantivy::query::TermQuery::new(
                            term,
                            tantivy::schema::IndexRecordOption::Basic,
                        ));
                    (tantivy::query::Occur::Should, clause)
                })
                .collect();
            let mut category_filter = tantivy::query::BooleanQuery::new(clauses);
            category_filter.set_minimum_number_should_match(1);

            query = Box::new(tantivy::query::BooleanQuery::new(vec![
                (tantivy::query::Occur::Must, query),
                (
                    tantivy::query::Occur::Must,
                    no_score(Box::new(category_filter)),
                ),
            ]));
        }

        // Exclude specific categories
        if !options.exclude_categories.is_empty() {
            for cat in &options.exclude_categories {
                let term = tantivy::Term::from_field_u64(schema.category, cat.as_u8() as u64);
                let category_filter: Box<dyn tantivy::query::Query> = Box::new(
                    tantivy::query::TermQuery::new(term, tantivy::schema::IndexRecordOption::Basic),
                );

                query = Box::new(tantivy::query::BooleanQuery::new(vec![
                    (tantivy::query::Occur::Must, query),
                    (tantivy::query::Occur::MustNot, category_filter),
                ]));
            }
        }

        let limit = (options.limit + options.offset).min(MAX_RETRIEVAL_WINDOW);
        let top_docs = tantivy::collector::TopDocs::with_limit(limit).order_by_score();

        let results: Vec<(f32, tantivy::DocAddress)> = searcher.search(&query, &top_docs)?;

        let mut search_results = Vec::with_capacity(
            results
                .len()
                .saturating_sub(options.offset.min(results.len())),
        );

        for (score, doc_address) in results.into_iter().skip(options.offset) {
            let doc = searcher.doc(doc_address)?;
            let document_id = Self::extract_field_value(&doc, schema.document_id);
            let chunk_id = Self::extract_field_value(&doc, schema.chunk_id);
            let file_path = Self::extract_field_value(&doc, schema.file_path);
            let title_value = Self::extract_field_value(&doc, schema.title);

            let mut fields = HashMap::with_capacity(7);
            if !chunk_id.is_empty() {
                fields.insert("chunk_id".to_string(), chunk_id);
            }
            if !file_path.is_empty() {
                fields.insert("file_path".to_string(), file_path);
            }
            fields.insert("title".to_string(), title_value);
            let test_value = Self::extract_field_value(&doc, schema.test);
            if !test_value.is_empty() {
                fields.insert("test".to_string(), test_value);
            }
            let category_value = Self::extract_field_value(&doc, schema.category);
            if !category_value.is_empty() {
                fields.insert("category".to_string(), category_value);
            }
            let entity_ids: Vec<String> = doc
                .get_all(schema.entity_id)
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect();
            if !entity_ids.is_empty() {
                fields.insert("entity_id".to_string(), entity_ids.join(","));
            }
            let segment_id = Self::extract_field_value(&doc, schema.segment_id);
            if !segment_id.is_empty() {
                fields.insert("segment_id".to_string(), segment_id);
            }

            search_results.push(FulltextHit {
                document_id,
                score,
                fields,
            });
        }

        Ok(search_results)
    }

    /// Parse query text into a single-form tantivy query.
    ///
    /// Field weights come straight from the search options: with the default
    /// cleaner configuration the raw and cleaned query forms are nearly
    /// identical, so merging both only double-counts the same terms.
    fn parse_query(
        query_text: &str,
        schema: &IndexSchema,
        index: &tantivy::Index,
        searcher: &tantivy::Searcher,
        field_weights: &HashMap<String, f32>,
        operator: TermOperator,
    ) -> Result<Box<dyn tantivy::query::Query>, Bm25Error> {
        if query_text.trim().is_empty() {
            return Ok(Box::new(tantivy::query::EmptyQuery {}));
        }

        let title_weight = field_weights.get("title").copied().unwrap_or(2.0);
        let content_weight = field_weights.get("content").copied().unwrap_or(1.0);
        let keywords_weight = field_weights.get("keywords").copied().unwrap_or(2.0);

        let tokenizer = index
            .tokenizers()
            .get("mixed")
            .ok_or_else(|| Bm25Error::Search("mixed tokenizer not registered".to_string()))?;

        Ok(Self::build_query(
            query_text,
            schema,
            tokenizer,
            searcher,
            title_weight,
            content_weight,
            keywords_weight,
            operator,
        ))
    }

    /// Build a single query form from `query_text`.
    #[allow(clippy::too_many_arguments)]
    fn build_query(
        query_text: &str,
        schema: &IndexSchema,
        mut tokenizer: TextAnalyzer,
        searcher: &tantivy::Searcher,
        title_weight: f32,
        content_weight: f32,
        keywords_weight: f32,
        operator: TermOperator,
    ) -> Box<dyn tantivy::query::Query> {
        let (phrase_segments, remaining) = extract_phrases(query_text);
        let mut clauses: Vec<(tantivy::query::Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

        for phrase in phrase_segments {
            let phrase_query = build_phrase_query(&phrase, schema, &mut tokenizer, searcher);
            if let Some(q) = phrase_query {
                clauses.push((occur_for(operator), q));
            }
        }

        let tokens = collect_tokens(&mut tokenizer, &remaining);
        for (token, scale) in
            expand_query_tokens(&tokens, |text| field_doc_freq(searcher, schema, text))
        {
            let Some(clause) = build_token_query_with_scale(
                token,
                schema,
                title_weight,
                content_weight,
                keywords_weight,
                scale,
            ) else {
                continue;
            };
            clauses.push((occur_for(operator), clause));
        }

        if clauses.is_empty() {
            Box::new(tantivy::query::EmptyQuery {})
        } else if clauses.len() == 1 {
            clauses
                .into_iter()
                .next()
                .map(|(_, q)| q)
                .unwrap_or_else(|| {
                    Box::new(tantivy::query::EmptyQuery {}) as Box<dyn tantivy::query::Query>
                })
        } else {
            Box::new(tantivy::query::BooleanQuery::new(clauses))
        }
    }

    fn extract_field_value(
        doc: &tantivy::schema::TantivyDocument,
        field: tantivy::schema::Field,
    ) -> String {
        doc.get_first(field)
            .map(|v| Self::compact_value_to_string(&v))
            .unwrap_or_default()
    }

    fn compact_value_to_string(value: &tantivy::schema::document::CompactDocValue) -> String {
        if let Some(s) = value.as_str() {
            s.to_string()
        } else if let Some(n) = value.as_u64() {
            n.to_string()
        } else {
            String::new()
        }
    }
}

/// Map a [`TermOperator`] to a tantivy [`Occur`].
fn occur_for(operator: TermOperator) -> tantivy::query::Occur {
    match operator {
        TermOperator::Or => tantivy::query::Occur::Should,
        TermOperator::And => tantivy::query::Occur::Must,
    }
}

/// Tokenize `text` with the analyzer and collect token metadata.
fn collect_tokens(tokenizer: &mut TextAnalyzer, text: &str) -> Vec<MixedToken> {
    let mut stream = tokenizer.token_stream(text);
    let mut tokens = Vec::new();
    stream.process(&mut |token: &Token| {
        tokens.push(MixedToken {
            text: token.text.clone(),
            offset_from: token.offset_from,
            offset_to: token.offset_to,
            position: token.position as u32,
            position_length: token.position_length as u32,
        });
    });
    tokens
}

/// Extract quoted segments from `text`.
fn extract_phrases(text: &str) -> (Vec<String>, String) {
    let mut phrases = Vec::new();
    let mut remainder = String::new();
    let mut in_quote = false;
    let mut current = String::new();

    for ch in text.chars() {
        if ch == '"' {
            if in_quote {
                if !current.trim().is_empty() {
                    phrases.push(current.trim().to_string());
                }
                current.clear();
                in_quote = false;
            } else {
                in_quote = true;
            }
        } else if in_quote {
            current.push(ch);
        } else {
            remainder.push(ch);
        }
    }

    if in_quote && !current.trim().is_empty() {
        phrases.push(current.trim().to_string());
    }

    (phrases, remainder)
}

/// Build a phrase query for a quoted segment.
///
/// Applies the same stopword and zero-document-frequency expansion as plain
/// terms so quoted and unquoted queries behave consistently.
fn build_phrase_query(
    phrase: &str,
    schema: &IndexSchema,
    tokenizer: &mut TextAnalyzer,
    searcher: &tantivy::Searcher,
) -> Option<Box<dyn tantivy::query::Query>> {
    let tokens = collect_tokens(tokenizer, phrase);
    let expanded_terms: Vec<String> =
        expand_query_tokens(&tokens, |text| field_doc_freq(searcher, schema, text))
            .into_iter()
            .map(|(token, _)| token.text)
            .collect();

    if expanded_terms.is_empty() {
        return None;
    }

    let mut field_queries: Vec<(tantivy::query::Occur, Box<dyn tantivy::query::Query>)> =
        Vec::new();
    for field in [schema.title, schema.content, schema.keywords] {
        let terms: Vec<tantivy::Term> = expanded_terms
            .iter()
            .map(|t| tantivy::Term::from_field_text(field, t))
            .collect();
        let phrase: Box<dyn tantivy::query::Query> =
            Box::new(tantivy::query::PhraseQuery::new(terms));
        field_queries.push((tantivy::query::Occur::Should, phrase));
    }

    Some(Box::new(tantivy::query::BooleanQuery::new(field_queries)))
}

fn build_token_query_with_scale(
    token: MixedToken,
    schema: &IndexSchema,
    title_weight: f32,
    content_weight: f32,
    keywords_weight: f32,
    scale: f32,
) -> Option<Box<dyn tantivy::query::Query>> {
    if token.text.is_empty() {
        return None;
    }

    let mut clauses: Vec<(tantivy::query::Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    let title_term = tantivy::Term::from_field_text(schema.title, &token.text);
    clauses.push((
        tantivy::query::Occur::Should,
        Box::new(tantivy::query::BoostQuery::new(
            Box::new(tantivy::query::TermQuery::new(
                title_term,
                tantivy::schema::IndexRecordOption::WithFreqsAndPositions,
            )),
            title_weight * scale,
        )),
    ));

    let content_term = tantivy::Term::from_field_text(schema.content, &token.text);
    clauses.push((
        tantivy::query::Occur::Should,
        Box::new(tantivy::query::BoostQuery::new(
            Box::new(tantivy::query::TermQuery::new(
                content_term,
                tantivy::schema::IndexRecordOption::WithFreqsAndPositions,
            )),
            content_weight * scale,
        )),
    ));

    let keywords_term = tantivy::Term::from_field_text(schema.keywords, &token.text);
    clauses.push((
        tantivy::query::Occur::Should,
        Box::new(tantivy::query::BoostQuery::new(
            Box::new(tantivy::query::TermQuery::new(
                keywords_term,
                tantivy::schema::IndexRecordOption::WithFreqsAndPositions,
            )),
            keywords_weight * scale,
        )),
    ));

    Some(Box::new(tantivy::query::BooleanQuery::new(clauses)))
}

fn is_query_stopword(text: &str) -> bool {
    matches!(text, "method" | "on" | "in" | "constructor" | "function")
}

fn field_doc_freq(searcher: &tantivy::Searcher, schema: &IndexSchema, text: &str) -> u64 {
    let mut total = 0u64;
    for field in [schema.title, schema.content, schema.keywords] {
        let term = tantivy::Term::from_field_text(field, text);
        total += searcher.doc_freq(&term).unwrap_or(0);
    }
    total
}

fn whole_term_doc_freq<D>(doc_freq: &mut D, cache: &mut HashMap<String, u64>, text: &str) -> u64
where
    D: FnMut(&str) -> u64,
{
    if let Some(&cached) = cache.get(text) {
        return cached;
    }
    let total = doc_freq(text);
    cache.insert(text.to_string(), total);
    total
}

/// Expand tokenizer output into production BM25 query clauses.
///
/// The caller supplies a document-frequency callback for the indexed BM25
/// text fields. Split tokens are auxiliary matches: they are down-weighted when
/// the whole token exists, promoted when only the split parts are indexed, and
/// emitted at half weight when no whole token is present.
pub fn expand_query_tokens<D>(tokens: &[MixedToken], mut doc_freq: D) -> Vec<(MixedToken, f32)>
where
    D: FnMut(&str) -> u64,
{
    use std::collections::BTreeMap;

    let mut by_position: BTreeMap<u32, Vec<&MixedToken>> = BTreeMap::new();
    for token in tokens {
        if token.text.is_empty() {
            continue;
        }
        by_position.entry(token.position).or_default().push(token);
    }

    let mut expanded = Vec::new();
    let mut cache: HashMap<String, u64> = HashMap::new();
    for (_, group) in by_position {
        let whole = group.iter().find(|t| t.position_length == 1).copied();
        let splits: Vec<&MixedToken> = group
            .iter()
            .filter(|t| t.position_length == 0)
            .copied()
            .collect();

        if let Some(whole_token) = whole {
            if is_query_stopword(whole_token.text.as_str()) {
                continue;
            }
            if whole_term_doc_freq(&mut doc_freq, &mut cache, whole_token.text.as_str()) == 0
                && !splits.is_empty()
            {
                for split in splits {
                    if split.text.is_empty() || is_query_stopword(split.text.as_str()) {
                        continue;
                    }
                    expanded.push(((*split).clone(), 1.0));
                }
                continue;
            }
            expanded.push(((*whole_token).clone(), 1.0));
            for split in splits {
                if split.text.is_empty() || is_query_stopword(split.text.as_str()) {
                    continue;
                }
                expanded.push(((*split).clone(), 0.5));
            }
        } else {
            for split in splits {
                if split.text.is_empty() || is_query_stopword(split.text.as_str()) {
                    continue;
                }
                expanded.push(((*split).clone(), 0.5));
            }
        }
    }
    expanded
}
