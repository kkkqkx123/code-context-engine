use std::fmt;

use cce_types::TestInfo;
use cce_types::language::Language;
use rkyv::{Archive, Deserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// Query type classification.
///
/// The four retrieval tiers sit on two perturbation axes (lexical x structural):
/// - `Qualified`: symbol tokens preserved, wrapped in natural-language structure
///   (e.g. `find_at in RegexMatcher`). Between fuzzy and exact matching.
/// - `Fuzzy`: symbol tokens artificially perturbed (case, affix, synonym,
///   paraphrase, abbreviation), structure preserved.
/// - `Semantic`: behavior-level description, no lexical overlap required.
/// - `CrossLang`: non-English query text describing the same entities
///   (verification only, kept in once_cell).
#[derive(
    Archive,
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    SerdeSerialize,
    SerdeDeserialize,
)]
pub enum QueryType {
    #[serde(rename = "qualified")]
    Qualified,
    #[serde(rename = "fuzzy")]
    Fuzzy,
    #[serde(rename = "semantic")]
    Semantic,
    #[serde(rename = "cross_lang")]
    CrossLang,
    #[serde(rename = "file_documentation")]
    FileDocumentation,
}

/// Evaluation scope configuration
#[derive(
    Archive,
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    SerdeSerialize,
    SerdeDeserialize,
)]
pub enum EvaluationScope {
    #[serde(rename = "core_retrieval")]
    CoreRetrieval,
    #[serde(rename = "diagnostic_cross_lang")]
    DiagnosticCrossLang,
    #[serde(rename = "all")]
    All,
}

impl EvaluationScope {
    /// Returns whether the given query type should be included in this scope
    pub fn includes_query_type(&self, query_type: QueryType) -> bool {
        match self {
            EvaluationScope::CoreRetrieval => matches!(
                query_type,
                QueryType::Qualified | QueryType::Fuzzy | QueryType::Semantic
            ),
            EvaluationScope::DiagnosticCrossLang => query_type == QueryType::CrossLang,
            EvaluationScope::All => true,
        }
    }

    /// Returns a human-readable description of the scope
    pub fn description(&self) -> &'static str {
        match self {
            EvaluationScope::CoreRetrieval => {
                "Qualified, Fuzzy and Semantic queries only (core retrieval evaluation)"
            }
            EvaluationScope::DiagnosticCrossLang => "Cross-language queries only (diagnostic)",
            EvaluationScope::All => "All query types",
        }
    }

    /// Returns query types included in this scope
    pub fn included_query_types(&self) -> Vec<QueryType> {
        match self {
            EvaluationScope::CoreRetrieval => {
                vec![QueryType::Qualified, QueryType::Fuzzy, QueryType::Semantic]
            }
            EvaluationScope::DiagnosticCrossLang => vec![QueryType::CrossLang],
            EvaluationScope::All => vec![
                QueryType::Qualified,
                QueryType::Fuzzy,
                QueryType::Semantic,
                QueryType::CrossLang,
                QueryType::FileDocumentation,
            ],
        }
    }
}

impl fmt::Display for EvaluationScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvaluationScope::CoreRetrieval => write!(f, "core_retrieval"),
            EvaluationScope::DiagnosticCrossLang => write!(f, "diagnostic_cross_lang"),
            EvaluationScope::All => write!(f, "all"),
        }
    }
}

impl fmt::Display for QueryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryType::Qualified => write!(f, "qualified"),
            QueryType::Fuzzy => write!(f, "fuzzy"),
            QueryType::Semantic => write!(f, "semantic"),
            QueryType::CrossLang => write!(f, "cross_lang"),
            QueryType::FileDocumentation => write!(f, "file_documentation"),
        }
    }
}

/// Relevance level for a source code snippet
#[derive(
    Archive,
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    SerdeSerialize,
    SerdeDeserialize,
    PartialOrd,
    Ord,
)]
#[repr(u8)]
pub enum RelevanceLevel {
    /// Irrelevant
    Irrelevant = 0,
    /// Related (implements or contains related logic)
    Related = 1,
    /// Strongly relevant (direct definition or core implementation)
    Strong = 2,
}

impl fmt::Display for RelevanceLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelevanceLevel::Irrelevant => write!(f, "irrelevant"),
            RelevanceLevel::Related => write!(f, "related"),
            RelevanceLevel::Strong => write!(f, "strong"),
        }
    }
}

/// Source code location range (stable across chunking changes)
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct SourceRange {
    /// File path (relative to fixture root)
    pub file: String,
    /// Start line (1-indexed)
    pub start_line: usize,
    /// End line (1-indexed, inclusive)
    pub end_line: usize,
}

/// Relevance judgment for a query: which source ranges are relevant
///
/// Strategy:
/// - Strong/Related: Hardcoded line ranges for precise, core functionality
/// - Unmatched: Implicitly Irrelevant (no need to list distractors)
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct RelevanceJudgment {
    pub id: String,
    pub query_text: String,
    pub query_type: QueryType,
    /// Perturbation subtype for fuzzy queries (naming_case, naming_affix,
    /// synonym, paraphrase, abbrev_expand). `None` for other query types.
    pub fuzzy_subtype: Option<String>,
    /// Source code ranges with Strong/Related levels (hardcoded, precise)
    pub relevant_ranges: Vec<(SourceRange, RelevanceLevel)>,
}

/// A query judgment with relevance labels (legacy, for compatibility)
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct QueryJudgment {
    pub id: String,
    pub query_text: String,
    pub query_type: QueryType,
    /// Names of chunks expected to match (positive samples)
    pub relevant_names: Vec<String>,
    /// Names of distractor chunks that should NOT match (negative samples)
    pub irrelevant_names: Vec<String>,
}

/// Get the reference query set for once_cell benchmark (legacy, name-based).
pub fn once_cell_queries() -> Vec<QueryJudgment> {
    vec![
        QueryJudgment {
            id: "G1Q1".into(),
            query_text: "get_or_init in OnceCell".into(),
            query_type: QueryType::Qualified,
            relevant_names: vec!["get_or_init".into()],
            irrelevant_names: vec![
                "bubble_sort".into(),
                "quick_sort".into(),
                "Matrix::new".into(),
            ],
        },
        QueryJudgment {
            id: "G1Q2".into(),
            query_text: "set method on OnceCell".into(),
            query_type: QueryType::Qualified,
            relevant_names: vec!["set".into()],
            irrelevant_names: vec![
                "bubble_sort".into(),
                "merge_sort".into(),
                "determinant".into(),
            ],
        },
        QueryJudgment {
            id: "G1Q3".into(),
            query_text: "new constructor in OnceCell".into(),
            query_type: QueryType::Qualified,
            relevant_names: vec!["new".into()],
            irrelevant_names: vec!["Matrix::identity".into(), "bubble_sort".into()],
        },
        QueryJudgment {
            id: "G2Q1".into(),
            query_text: "initialize a value only once".into(),
            query_type: QueryType::Semantic,
            relevant_names: vec![
                "get_or_init".into(),
                "get_or_try_init".into(),
                "set".into(),
                "try_insert".into(),
            ],
            irrelevant_names: vec!["bubble_sort".into(), "quick_sort".into()],
        },
        QueryJudgment {
            id: "G2Q2".into(),
            query_text: "read the stored value from cell".into(),
            query_type: QueryType::Semantic,
            relevant_names: vec!["get".into(), "get_mut".into()],
            irrelevant_names: vec!["transpose".into(), "determinant".into()],
        },
        QueryJudgment {
            id: "G2Q3".into(),
            query_text: "safely retrieve optional value".into(),
            query_type: QueryType::Semantic,
            relevant_names: vec!["get".into(), "get_mut".into(), "into_inner".into()],
            irrelevant_names: vec!["merge_sort".into(), "Matrix::zero".into()],
        },
        QueryJudgment {
            id: "G3Q1".into(),
            query_text: "lazy evaluation delayed computation".into(),
            query_type: QueryType::Semantic,
            relevant_names: vec!["into_value".into(), "force".into(), "force_mut".into()],
            irrelevant_names: vec!["bubble_sort".into(), "matrix_multiply".into()],
        },
        QueryJudgment {
            id: "G3Q2".into(),
            query_text: "consume and extract inner value".into(),
            query_type: QueryType::Semantic,
            relevant_names: vec!["into_inner".into(), "take".into()],
            irrelevant_names: vec!["quick_sort".into(), "transpose".into()],
        },
        QueryJudgment {
            id: "G4Q1".into(),
            query_text: "懒加载的全局变量".into(),
            query_type: QueryType::CrossLang,
            relevant_names: vec![
                "Lazy::new".into(),
                "Lazy::force".into(),
                "once_cell_lib".into(),
            ],
            irrelevant_names: vec!["bubble_sort".into(), "Matrix::new".into()],
        },
        QueryJudgment {
            id: "G4Q2".into(),
            query_text: "线程安全的一次初始化".into(),
            query_type: QueryType::CrossLang,
            relevant_names: vec![
                "sync::OnceCell".into(),
                "sync::Lazy".into(),
                "get_or_init".into(),
            ],
            irrelevant_names: vec!["quick_sort".into(), "merge_sort".into()],
        },
        QueryJudgment {
            id: "G4Q3".into(),
            query_text: "获取可能未初始化的值".into(),
            query_type: QueryType::CrossLang,
            relevant_names: vec!["get".into(), "get_mut".into()],
            irrelevant_names: vec!["determinant".into(), "Matrix::identity".into()],
        },
        QueryJudgment {
            id: "G4Q4".into(),
            query_text: "延迟初始化的高性能实现".into(),
            query_type: QueryType::CrossLang,
            relevant_names: vec!["Lazy".into(), "once_cell_lib".into(), "sync::Lazy".into()],
            irrelevant_names: vec!["bubble_sort".into(), "transpose".into()],
        },
        QueryJudgment {
            id: "G5Q1".into(),
            query_text: "thread synchronization blocking wait once cell".into(),
            query_type: QueryType::Semantic,
            relevant_names: vec![
                "OnceCell".into(),
                "initialize_or_wait".into(),
                "wait".into(),
                "Guard".into(),
            ],
            irrelevant_names: vec!["add".into()],
        },
        QueryJudgment {
            id: "G5Q2".into(),
            query_text: "parking lot based once cell implementation".into(),
            query_type: QueryType::Semantic,
            relevant_names: vec!["OnceCell".into(), "initialize_inner".into(), "Guard".into()],
            irrelevant_names: vec!["add".into()],
        },
        QueryJudgment {
            id: "G5Q3".into(),
            query_text: "no standard library once cell critical section".into(),
            query_type: QueryType::Semantic,
            relevant_names: vec!["OnceCell".into()],
            irrelevant_names: vec!["add".into()],
        },
        QueryJudgment {
            id: "G5Q4".into(),
            query_text: "lock-free non-blocking once cell atomic".into(),
            query_type: QueryType::Semantic,
            relevant_names: vec![
                "OnceNonZeroUsize".into(),
                "OnceBool".into(),
                "OnceRef".into(),
                "OnceBox".into(),
            ],
            irrelevant_names: vec!["add".into()],
        },
    ]
}

/// A single chunk's metadata (shared across retriever paths)
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct ChunkData {
    pub chunk_id: String,
    pub entity_name: String,
    pub file_path: String,
    /// Start line in source file (1-indexed, 0 if unknown)
    pub start_line: usize,
    /// End line in source file (1-indexed, 0 if unknown)
    pub end_line: usize,
    /// Precise source coverage. Unlike the enclosing start/end navigation
    /// span, these ranges do not include repeated impl or module headers.
    pub source_ranges: Vec<ChunkSourceRange>,
    /// Serialized `SourceSpanKind` from the chunker.
    pub source_span_kind: String,
    /// End-to-end test marker (AST detection + file-path rules) inherited
    /// from the chunker.
    pub test_info: TestInfo,
    /// Source language of the chunk (None for document/config/plain-text).
    /// Used as the Unknown-fallback: per-language path rules.
    pub language: Option<Language>,
    /// Project-scoped entity IDs for entity-level cross-path alignment,
    /// mirroring the production payload/index `entity_ids`. Empty for
    /// document/plain-text chunks.
    pub entity_ids: Vec<i64>,
    /// Segment ID for cross-path alignment, mirroring `ChunkMetadata.segment_id`.
    /// Document/plain-text chunks carry the logical section group id here;
    /// entity-level alignment takes priority when `entity_ids` is non-empty.
    pub segment_id: String,
}

/// A single source-covered line interval within a benchmark chunk.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct ChunkSourceRange {
    /// Start line in source file (1-indexed, 0 if unavailable).
    pub start_line: usize,
    /// End line in source file (1-indexed, 0 if unavailable).
    pub end_line: usize,
}

/// A query with its relevance judgments
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct QueryData {
    pub id: String,
    pub text: String,
    pub query_type: QueryType,
    /// Positive sample entity names
    pub relevant_names: Vec<String>,
    /// Negative sample entity names (from distractor fixture)
    pub irrelevant_names: Vec<String>,
}

/// Data for a single retriever path (Embedding or BM25).
/// Each path has its own chunks (potentially different counts and texts).
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct RetrieverDataset {
    pub chunks: Vec<ChunkData>,
    /// Chunk texts for this retriever path
    pub texts: Vec<String>,
    /// Flat chunk vectors (empty for BM25 path)
    pub vectors: Vec<f32>,
    /// Flat query vectors (empty for BM25 path)
    pub query_vectors: Vec<f32>,
    /// Embedding dimension (0 for BM25 path)
    pub dimension: u32,
}

/// Full benchmark dataset with separate data per retriever path.
/// BM25 and Embedding paths each have their own chunking results
/// (different texts, potentially different counts).
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct BenchmarkData {
    pub queries: Vec<QueryData>,
    /// Query texts used to build embedding query vectors.
    pub query_texts: Vec<String>,
    /// Embedding path dataset
    pub embedding: RetrieverDataset,
    /// BM25 path dataset
    pub bm25: RetrieverDataset,
    /// Per-chunk BM25 documents (title/content/keywords), index-aligned with
    /// `bm25.chunks` and `bm25.texts`. The offline scorer consumes these so
    /// its three-field weighted model mirrors production indexing; query
    /// terms are NOT materialized here — the evaluator derives the raw and
    /// cleaned query forms with the production tokenizer at evaluation time
    /// (single source of truth, zero tokenizer drift).
    pub bm25_documents: Vec<Bm25DocRecord>,
    /// In-project direct call edges (full_pipeline only), used by the
    /// offline annotation-review example to drive relation expansion. Empty
    /// for baselines that do not build a relation index.
    #[serde(default)]
    pub call_edges: Vec<CallEdgeData>,
}

/// One resolved in-project call edge for offline annotation expansion.
///
/// Keyed by the project-scoped `EntityId`s that also appear in
/// `ChunkData::entity_ids`, so the review harness can map a recalled chunk
/// to its callees/callers and back to the chunk that carries the neighbour.
#[derive(
    Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize, PartialEq, Eq,
)]
pub struct CallEdgeData {
    /// Caller entity id
    pub caller_entity_id: i64,
    /// Callee entity id
    pub callee_entity_id: i64,
    /// Callee scoped name (presentation only)
    pub callee_name: String,
    /// Callee file path (relative to fixture root, forward slashes)
    pub callee_file: String,
    /// Callee start line (0 when unresolved)
    pub callee_start_line: u32,
    /// Callee end line (0 when unresolved)
    pub callee_end_line: u32,
    /// Relation type label (e.g. "call.direct", "call.method")
    pub relation_type: String,
}

/// BM25 document fields consumed by the offline scorer, mirroring the
/// production `FulltextDocument` shape (title/content/keywords).
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct Bm25DocRecord {
    pub title: String,
    pub keywords: String,
    pub content: String,
}

/// Cosine similarity between two f32 vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum();
    let norm_b: f32 = b.iter().map(|x| x * x).sum();
    let mag = (norm_a * norm_b).sqrt();
    if mag == 0.0 { 0.0 } else { (dot / mag) as f64 }
}

/// Production BM25 configuration for offline scoring.
///
/// k1/b come from `Bm25AlgorithmConfig::default` and field weights from
/// `Bm25RetrievalConfig::default` — the same defaults the running server uses.
pub fn production_bm25_config() -> crate::infra::Bm25Config {
    let algorithm = cce_config::modules::Bm25AlgorithmConfig::default();
    let retrieval = cce_config::modules::search::Bm25RetrievalConfig::default();
    crate::infra::Bm25Config {
        k1: algorithm.k1 as f64,
        b: algorithm.b as f64,
        title_weight: retrieval.field_weights.get("title").copied().unwrap_or(2.0) as f64,
        keywords_weight: retrieval
            .field_weights
            .get("keywords")
            .copied()
            .unwrap_or(2.0) as f64,
        content_weight: retrieval
            .field_weights
            .get("content")
            .copied()
            .unwrap_or(1.0) as f64,
    }
}

/// Score every query against every BM25 document with production semantics.
///
/// Builds the in-memory three-field term index from the documents, expands each
/// query with production BM25 query semantics, then scores it with
/// `infra::score_all` (whole/split token expansion, field weights,
/// `Or`/`And` operator). Returns `scores[query_idx][doc_idx]`, index-aligned
/// with the input documents.
pub fn compute_bm25_scores(
    documents: &[Bm25DocRecord],
    query_texts: &[String],
    operator: cce_storage_bm25::TermOperator,
) -> Vec<Vec<f64>> {
    let bm25_docs: Vec<cce_storage_common::FulltextDocument> = documents
        .iter()
        .enumerate()
        .map(|(i, record)| {
            cce_storage_common::FulltextDocument::new(format!("doc:{i}"))
                .with_field("title", &record.title)
                .with_field("keywords", &record.keywords)
                .with_field("content", &record.content)
        })
        .collect();
    let term_index = crate::infra::build_term_index(&bm25_docs);
    let queries: Vec<crate::infra::ExpandedQuery> = query_texts
        .iter()
        .map(|text| crate::infra::expand_query(text, &term_index))
        .collect();
    let config = production_bm25_config();
    let ranked = crate::infra::score_all(&term_index, &queries, &config, operator, documents.len());
    ranked
        .into_iter()
        .map(|ranked| {
            let mut scores = vec![0.0_f64; documents.len()];
            for (doc_idx, score) in ranked {
                scores[doc_idx] = score;
            }
            scores
        })
        .collect()
}

/// Load benchmark data from rkyv file
pub fn load_benchmark_data(path: &std::path::Path) -> Result<BenchmarkData, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let data = rkyv::from_bytes::<BenchmarkData, rkyv::rancor::Error>(&bytes)
        .map_err(|e| format!("rkyv deserialize failed: {:?}", e))?;
    Ok(data)
}

/// Normalize a file path for judgment matching.
///
/// Converts backslashes to forward slashes while preserving the full relative path.
/// For workspace projects (e.g. ripgrep), `crates/<name>/src/...` and `src/...`
/// are distinct files that must not collapse to the same path.
pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// Check if a single source range is valid.
///
/// A valid range has:
/// - start_line > 0
/// - end_line >= start_line
pub fn is_valid_source_range(range: &ChunkSourceRange) -> bool {
    range.start_line > 0 && range.end_line >= range.start_line
}

/// Validate all source_ranges in a chunk.
///
/// Returns true if all ranges are valid or if the chunk has no ranges
/// (which is allowed for chunks with Unavailable source_span_kind).
pub fn validate_chunk_source_ranges(chunk: &ChunkData) -> bool {
    chunk.source_ranges.iter().all(is_valid_source_range)
}

/// Validate source_ranges across multiple chunks (e.g., from different baselines).
///
/// Returns a list of (chunk_index, error_message) for invalid chunks.
pub fn validate_source_ranges_consistency(
    chunks_list: &[&[ChunkData]],
    baseline_names: &[&str],
) -> Vec<(usize, String)> {
    let mut errors = Vec::new();

    for (chunks, baseline_name) in chunks_list.iter().zip(baseline_names.iter()) {
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            if !validate_chunk_source_ranges(chunk) {
                errors.push((
                    chunk_idx,
                    format!(
                        "invalid source_ranges in {} / {}",
                        baseline_name, chunk.chunk_id
                    ),
                ));
            }
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bm25_scores_use_production_tokenizer() {
        let q = "TypesBuilder";
        let queries = vec![q.to_string()];
        let documents = vec![
            Bm25DocRecord {
                title: "TypesBuilder".into(),
                keywords: "builder".into(),
                content: "Function TypesBuilder creates a matcher.".into(),
            },
            Bm25DocRecord {
                title: "Matrix".into(),
                keywords: "matrix".into(),
                content: "Function Matrix creates a matrix.".into(),
            },
        ];

        let scores = compute_bm25_scores(&documents, &queries, cce_storage_bm25::TermOperator::Or);

        // Exact identifier match (original token) must beat the doc whose
        // content only contains the split words.
        assert!(scores[0][0] > scores[0][1]);
    }

    #[test]
    fn qualified_identifier_and_plain_words_produce_different_expansions() {
        let docs = vec![
            cce_storage_common::FulltextDocument::new("d:0")
                .with_field("title", "RegexMatcher::find_at")
                .with_field("keywords", "regex matcher find at")
                .with_field("content", "RegexMatcher::find_at"),
        ];
        let index = crate::infra::build_term_index(&docs);
        let raw = crate::infra::expand_query("RegexMatcher::find_at", &index);
        let plain = crate::infra::expand_query("regex matcher find_at", &index);

        assert_ne!(raw.terms, plain.terms);
        assert!(
            raw.terms
                .iter()
                .any(|t| t.text == "regexmatcher::find_at" && t.scale == 1.0)
        );
        assert!(
            !plain
                .terms
                .iter()
                .any(|t| t.text == "regexmatcher::find_at")
        );
    }

    #[test]
    fn test_normalize_path_preserves_fixture_structure() {
        assert_eq!(normalize_path("src/lib.rs "), "src/lib.rs ");
        assert_eq!(
            normalize_path("crates/ignore/src/walk.rs "),
            "crates/ignore/src/walk.rs "
        );
        assert_eq!(
            normalize_path("fixtures/rust/review/ripgrep/src/search/regex/matcher.rs "),
            "fixtures/rust/review/ripgrep/src/search/regex/matcher.rs "
        );
    }

    #[test]
    fn test_normalize_path_passes_through() {
        assert_eq!(normalize_path("x/y/z "), "x/y/z ");
    }

    #[test]
    fn debug_scanner_vs_processed() {
        // Use scan_fixture to see what the scanner returns
        let spec = crate::FixtureSpec::rust_ripgrep();
        let entries = crate::bench_gen::scan_fixture(spec).unwrap();
        let mut src_files: Vec<_> = entries
            .iter()
            .filter(|e| e.relative_path.starts_with("src/"))
            .map(|e| e.relative_path.to_string_lossy().to_string())
            .collect();
        src_files.sort();
        eprintln!("=== Scanner found {} src/ files ===", src_files.len());
        for f in &src_files {
            eprintln!("  {f}");
        }
        eprintln!("\n=== Total scanner entries: {} ===", entries.len());

        // Now load benchmark data. Skip when the artifact is missing or was
        // generated under an older schema (stale data is not committed).
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data/benchmark/full_pipeline/ripgrep/bge-m3/bench_data.rkyv");
        if !base.exists() {
            eprintln!("\nSKIP: benchmark data not generated; run gen_bench_ripgrep first");
            return;
        }
        let bench = match load_benchmark_data(&base) {
            Ok(bench) => bench,
            Err(e) => {
                eprintln!(
                    "\nSKIP: benchmark data stale or unreadable ({e}); regenerate with gen_bench_ripgrep"
                );
                return;
            }
        };
        let chunk_src: std::collections::BTreeSet<String> = bench
            .embedding
            .chunks
            .iter()
            .chain(bench.bm25.chunks.iter())
            .filter(|c| c.file_path.starts_with("src/"))
            .map(|c| c.file_path.clone())
            .collect();
        eprintln!(
            "\n=== Chunk files starting with 'src/' ({}): ===",
            chunk_src.len()
        );
        for f in &chunk_src {
            eprintln!("  {f}");
        }

        // Check which scanner src files didn't make it to chunks
        let missing: Vec<_> = src_files
            .iter()
            .filter(|sf| !chunk_src.contains(*sf))
            .collect();
        eprintln!(
            "\n=== Scanner src/ files NOT in chunks ({}): ===",
            missing.len()
        );
        for f in &missing {
            eprintln!("  MISSING: {f}");
        }
    }

    #[test]
    fn test_source_range_validity() {
        let valid_range = ChunkSourceRange {
            start_line: 5,
            end_line: 10,
        };
        assert!(is_valid_source_range(&valid_range));

        let invalid_start = ChunkSourceRange {
            start_line: 0,
            end_line: 10,
        };
        assert!(!is_valid_source_range(&invalid_start));

        let inverted = ChunkSourceRange {
            start_line: 10,
            end_line: 5,
        };
        assert!(!is_valid_source_range(&inverted));
    }
}
