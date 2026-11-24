//! Rerank benchmark: recall + cross-encoder rerank comparison.
//!
//! Reuses `data/benchmark/{baseline}/{fixture}/bge-m3/bench_data.rkyv`
//! (never modified) and evaluates the embedding recall path with and without
//! cross-encoder reranking. Rerank scores are
//! generated once per (baseline, method) by calling a `RerankProvider` and
//! stored as sidecar files `rerank_{model}_{method}_depth{depth}.rkyv` next to
//! `bench_data.rkyv`; scoring is fully offline from the sidecars.
//!
//! Measurement semantics: for every (baseline, method, query) the control row
//! and the reranked row share the exact same candidate list and evaluation
//! mapping — the only difference is the rerank reorder.
//!
//! Fusion semantics: sidecars store raw rerank scores only, so the final-score
//! fusion is a pure scoring-time choice. Raw recall scores carry incompatible
//! scales, so `RerankScoring::normalize_initial` optionally min-max normalizes
//! the initial scores per query before blending, making `alpha` comparable
//! across methods. Each (fusion, normalization) variant writes its own output
//! directory, so variants never overwrite each other.
//!
//! Sub-modules:
//! - root: sidecar types, candidate construction, score generation, scoring
//! - `report`: markdown report writers

pub mod report;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use rkyv::{Archive, Deserialize, Serialize};
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

use crate::bench_data::{BenchmarkData, ChunkData, QueryType, RelevanceJudgment, RelevanceLevel};
use crate::judgments::evaluate::{BASELINES, BenchmarkPaths, DEFAULT_TOP_K, TOP_K_VALUES};
use crate::range_evaluator::RankedScan;
use crate::retrieval_method::{RecallRankings, rank_recall};
use cce_config::modules::search::RerankFusionStrategy;

/// Registry key of the rerank model under `[llm.rerank_models]`.
pub const RERANK_MODEL_KEY: &str = "bge-reranker";
/// Fixed rerank candidate depth: head of each recall ranking.
pub const RERANK_CANDIDATE_DEPTH: usize = 50;
/// Recall methods covered by the rerank matrix.
///
/// Only the embedding path participates: reranking is a semantic re-scoring
/// step, so it is defined against the semantic recall path. The BM25 lexical
/// path keeps its own lexical ordering (cross-encoder scoring over the hybrid
/// expanded text measured strictly worse than recall order), and the fusion
/// baseline is covered by the retrieval-method benchmark instead.
pub const RERANK_METHODS: &[&str] = &["emb"];
/// Documented candidate text rule (also recorded in every sidecar header).
pub const TEXT_SOURCE_RULE: &str = "emb: embedding path text";
/// Candidate truncation applied by the provider request (mirrors production).
pub const RERANK_TRUNCATE_CHARS: usize = 500;

/// Method label for the reranked variant of a recall method.
pub fn reranked_method(method: &str) -> String {
    format!("{method}+rerank")
}

/// Ordered method labels used across all reports (control + reranked pairs).
pub fn ordered_methods() -> Vec<String> {
    let mut methods = Vec::with_capacity(RERANK_METHODS.len() * 2);
    for method in RERANK_METHODS {
        methods.push((*method).to_string());
        methods.push(reranked_method(method));
    }
    methods
}

/// Deterministic 64-bit FNV-1a hash (stable across processes, unlike SipHash).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// One reranked candidate stored in the sidecar.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct RerankScoredCandidate {
    pub chunk_id: String,
    pub initial_score: f64,
    pub recall_rank: usize,
    pub rerank_score: f32,
}

/// Per-query rerank outcome stored in the sidecar.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct RerankQueryOutcome {
    pub query_id: String,
    pub query_text: String,
    pub candidates: Vec<RerankScoredCandidate>,
    pub elapsed_ms: u64,
    pub failed: bool,
}

/// Sidecar header: generation contract for staleness validation.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct RerankSidecarHeader {
    pub rerank_model_key: String,
    pub model_name: String,
    pub mode: String,
    pub retrieval_method: String,
    pub candidate_depth: usize,
    pub text_source: String,
    pub text_source_rule: String,
    pub truncate_chars: usize,
    pub source_hash: u64,
    pub generated_at: String,
}

/// Precomputed rerank scores for one (baseline, method) pair.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, SerdeSerialize, SerdeDeserialize)]
pub struct RerankSidecar {
    pub header: RerankSidecarHeader,
    pub queries: Vec<RerankQueryOutcome>,
}

/// Encode a sidecar to rkyv bytes.
pub fn encode_sidecar(sidecar: &RerankSidecar) -> Result<Vec<u8>, String> {
    rkyv::to_bytes::<rkyv::rancor::Error>(sidecar)
        .map(|bytes| bytes.to_vec())
        .map_err(|e| format!("rerank sidecar encode failed: {e:?}"))
}

/// Decode a sidecar from rkyv bytes.
pub fn decode_sidecar(bytes: &[u8]) -> Result<RerankSidecar, String> {
    rkyv::from_bytes::<RerankSidecar, rkyv::rancor::Error>(bytes)
        .map_err(|e| format!("rerank sidecar decode failed: {e:?}"))
}

/// Persist a sidecar next to its `bench_data.rkyv`.
pub fn save_sidecar(
    path: &Path,
    sidecar: &RerankSidecar,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(
        path,
        encode_sidecar(sidecar)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
    )?;
    Ok(())
}

/// Load a sidecar from disk.
pub fn load_sidecar(path: &Path) -> Result<RerankSidecar, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    Ok(decode_sidecar(&bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?)
}

/// Candidate text fed to the rerank model.
///
/// The BM25 hybrid-enhanced text is deliberately absent: its token-expanded
/// repetition stream is tuned for lexical matching, not language-model
/// consumption, and measured strictly worse than recall order under a
/// cross-encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerdeSerialize, SerdeDeserialize)]
pub enum RerankTextSource {
    /// The embedding path's NL summary text (what the embedder consumed).
    EmbText,
    /// The chunk's raw source code, resolved from the fixture via chunk line
    /// spans. Contrast baseline against `EmbText`.
    RawCode,
}

impl RerankTextSource {
    /// Registry label used in sidecar filenames and headers.
    pub fn label(&self) -> &'static str {
        match self {
            Self::EmbText => "emb-text",
            Self::RawCode => "raw-code",
        }
    }

    /// Resolve the candidate text for one embedding chunk.
    ///
    /// `EmbText` reads the stored embedding text; `RawCode` slices the
    /// fixture source file by the chunk's navigation span (1-indexed,
    /// inclusive). Missing files or spans fall back to the embedding text.
    pub fn resolve(&self, bench: &BenchmarkData, chunk_idx: usize) -> Option<String> {
        let chunk = bench.embedding.chunks.get(chunk_idx)?;
        let emb_text = bench.embedding.texts.get(chunk_idx)?;
        match self {
            Self::EmbText => Some(emb_text.clone()),
            Self::RawCode => {
                if chunk.start_line == 0 || chunk.end_line < chunk.start_line {
                    return Some(emb_text.clone());
                }
                let source = fixture_source(&chunk.file_path)?;
                let selected: Vec<&str> = source
                    .lines()
                    .skip(chunk.start_line - 1)
                    .take(chunk.end_line - chunk.start_line + 1)
                    .collect();
                if selected.is_empty() {
                    Some(emb_text.clone())
                } else {
                    Some(selected.join("\n"))
                }
            }
        }
    }
}

/// Load one fixture source file by its repo-relative path.
///
/// Fixture roots live under `crates/app/cce-e2e-tests/fixtures/<category>/...`
/// and chunk `file_path` values are relative to the fixture root, so the
/// lookup scans the fixture tree for the first matching suffix.
fn fixture_source(file_path: &str) -> Option<String> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let relative = Path::new(file_path);
    for dir in std::fs::read_dir(&base).ok()?.flatten() {
        let candidate = dir.path().join(relative);
        if let Ok(source) = std::fs::read_to_string(&candidate) {
            return Some(source);
        }
    }
    // Fall back to a suffix walk for paths with fixture-specific prefixes.
    let mut stack = vec![base];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.ends_with(relative) {
                return std::fs::read_to_string(&path).ok();
            }
        }
    }
    None
}

/// Sidecar path for one (baseline, method, text-source) triple, next to
/// `bench_data.rkyv`.
pub fn sidecar_path(
    project: &'static str,
    baseline: &str,
    method: &str,
    text_source: RerankTextSource,
    model_key: &str,
    depth: usize,
) -> PathBuf {
    let bench_path = BenchmarkPaths::new(project).data_dir(baseline);
    let dir = bench_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join(format!(
        "rerank_{model_key}_{method}_{}_depth{depth}.rkyv",
        text_source.label()
    ))
}

/// Runtime parameters for rerank score generation and offline scoring.
#[derive(Debug, Clone)]
pub struct RerankRuntime {
    /// Registry key under `[llm.rerank_models]`.
    pub model_key: String,
    /// Real model name sent to the provider.
    pub model_name: String,
    /// Execution mode label (`cross_encoder` / `generative` / test fakes).
    pub mode: String,
    /// Candidate depth (fixed by the matrix).
    pub depth: usize,
    /// Final-score fusion applied offline at scoring time.
    pub fusion: RerankFusionStrategy,
    /// Per-call timeout in milliseconds.
    pub timeout_ms: u64,
    /// Candidate text fed to the rerank model.
    pub text_source: RerankTextSource,
}

impl RerankRuntime {
    /// Human-readable fusion label for manifests.
    pub fn fusion_label(&self) -> String {
        match self.fusion {
            RerankFusionStrategy::RerankOnly => "rerank_only".to_string(),
            RerankFusionStrategy::LinearWeighted { alpha } => {
                format!("linear_weighted(alpha={alpha})")
            }
            RerankFusionStrategy::Multiplicative => "multiplicative".to_string(),
            RerankFusionStrategy::ReciprocalRankFusion { k } => {
                format!("reciprocal_rank_fusion(k={k})")
            }
        }
    }
}

/// Scoring-time parameters: which sidecars to consume and how to fuse.
#[derive(Debug, Clone)]
pub struct RerankScoring {
    /// Registry key embedded in the sidecar filenames/headers.
    pub model_key: String,
    /// Candidate depth embedded in the sidecar filenames/headers.
    pub depth: usize,
    /// Final-score fusion applied offline (re-runnable without new calls).
    pub fusion: RerankFusionStrategy,
    /// Min-max normalize initial scores per query to [0, 1] before blending,
    /// so `alpha` weighs comparable scales across recall methods.
    pub normalize_initial: bool,
    /// Candidate text source recorded in the sidecar header.
    pub text_source: RerankTextSource,
}

impl RerankScoring {
    /// Human-readable fusion label for manifests and console output.
    pub fn fusion_label(&self) -> String {
        let base = match self.fusion {
            RerankFusionStrategy::RerankOnly => "rerank_only".to_string(),
            RerankFusionStrategy::LinearWeighted { alpha } => {
                format!("linear_weighted(alpha={alpha})")
            }
            RerankFusionStrategy::Multiplicative => "multiplicative".to_string(),
            RerankFusionStrategy::ReciprocalRankFusion { k } => {
                format!("reciprocal_rank_fusion(k={k})")
            }
        };
        if self.normalize_initial {
            format!("{base}+norm_init")
        } else {
            base
        }
    }

    /// Filesystem-safe output directory variant for this scoring.
    pub fn variant_dir(&self) -> String {
        let base = match self.fusion {
            RerankFusionStrategy::RerankOnly => "rerank_only".to_string(),
            RerankFusionStrategy::LinearWeighted { alpha } => {
                format!("linear-weighted-alpha{alpha}")
            }
            RerankFusionStrategy::Multiplicative => "multiplicative".to_string(),
            RerankFusionStrategy::ReciprocalRankFusion { k } => {
                format!("reciprocal-rank-fusion-k{k}")
            }
        };
        let mut dir = format!("{base}_{}", self.text_source.label());
        if self.normalize_initial {
            dir.push_str("_norm-init");
        }
        dir
    }
}

/// Min-max normalize one query's initial scores to [0, 1].
///
/// Mirrors the production `minmax_normalize` contract: an empty candidate
/// list yields an empty vector, and a degenerate (all-equal) list yields all
/// 1.0 so every candidate keeps full initial weight instead of collapsing
/// to zero.
pub fn normalize_initial_scores(candidates: &[CandidateRef]) -> Vec<f32> {
    let scores: Vec<f32> = candidates
        .iter()
        .map(|candidate| candidate.initial_score as f32)
        .collect();
    if scores.is_empty() {
        return Vec::new();
    }
    let min = scores.iter().copied().fold(f32::MAX, f32::min);
    let max = scores.iter().copied().fold(f32::MIN, f32::max);
    if max - min <= f32::EPSILON {
        return vec![1.0; scores.len()];
    }
    scores
        .iter()
        .map(|score| (score - min) / (max - min))
        .collect()
}

/// One rerank candidate with its evaluation chunk resolved.
#[derive(Debug, Clone)]
pub struct CandidateRef {
    pub chunk_id: String,
    pub eval_chunk: ChunkData,
    pub text: String,
    pub file_path: String,
    pub initial_score: f64,
    pub recall_rank: usize,
}

/// Build the rerank candidate list for one query: the head of the method's
/// recall ranking (recall order, 1-based ranks), with candidate text resolved
/// through the requested text source.
pub fn build_candidates(
    bench: &BenchmarkData,
    recall: &RecallRankings,
    method: &str,
    text_source: RerankTextSource,
    query_idx: usize,
    depth: usize,
) -> Vec<CandidateRef> {
    match method {
        "emb" => recall
            .emb_ranked
            .get(query_idx)
            .map(|ranked| {
                ranked
                    .iter()
                    .take(depth)
                    .enumerate()
                    .filter_map(|(position, (chunk_idx, score))| {
                        let chunk = bench.embedding.chunks.get(*chunk_idx)?;
                        let text = text_source.resolve(bench, *chunk_idx)?;
                        Some(CandidateRef {
                            chunk_id: chunk.chunk_id.clone(),
                            eval_chunk: chunk.clone(),
                            text,
                            file_path: chunk.file_path.clone(),
                            initial_score: *score,
                            recall_rank: position + 1,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Score one query through any `RerankProvider` (real model or test fake).
///
/// One attempt plus one retry; a second failure (or timeout, or an incomplete
/// score mapping) records a failed sample instead of blocking the batch.
pub async fn score_query_outcome<P: cce_llm::RerankProvider>(
    provider: &P,
    query_id: &str,
    query_text: &str,
    candidates: &[CandidateRef],
    runtime: &RerankRuntime,
) -> RerankQueryOutcome {
    use cce_llm::rerank::RerankCandidate;
    use cce_llm::{RerankRequest, RerankRuntimeConfig};

    let failed = || RerankQueryOutcome {
        query_id: query_id.to_string(),
        query_text: query_text.to_string(),
        candidates: Vec::new(),
        elapsed_ms: 0,
        failed: true,
    };
    if candidates.is_empty() {
        return RerankQueryOutcome {
            failed: false,
            candidates: Vec::new(),
            elapsed_ms: 0,
            query_id: query_id.to_string(),
            query_text: query_text.to_string(),
        };
    }

    let request = RerankRequest {
        query: query_text.to_string(),
        candidates: candidates
            .iter()
            .map(|candidate| RerankCandidate {
                id: candidate.chunk_id.clone(),
                content: candidate.text.clone(),
                file_path: candidate.file_path.clone(),
                initial_score: candidate.initial_score as f32,
                entity_type: None,
                metadata: HashMap::new(),
            })
            .collect(),
        config: RerankRuntimeConfig {
            max_candidates: runtime.depth,
            temperature: 0.0,
            return_reasoning: false,
            score_fusion_strategy: runtime.fusion,
            timeout_ms: runtime.timeout_ms,
        },
    };

    for attempt in 0..2 {
        let call = tokio::time::timeout(
            std::time::Duration::from_millis(runtime.timeout_ms),
            provider.rerank(&request),
        )
        .await;
        let result = match call {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                eprintln!("rerank call failed for {query_id} (attempt {attempt}): {error:?}");
                continue;
            }
            Err(_) => {
                eprintln!("rerank call timed out for {query_id} (attempt {attempt})");
                continue;
            }
        };
        let scores: HashMap<&str, f32> = result
            .reranked_candidates
            .iter()
            .map(|reranked| (reranked.id.as_str(), reranked.rerank_score))
            .collect();
        if candidates
            .iter()
            .all(|candidate| scores.contains_key(candidate.chunk_id.as_str()))
        {
            return RerankQueryOutcome {
                query_id: query_id.to_string(),
                query_text: query_text.to_string(),
                candidates: candidates
                    .iter()
                    .map(|candidate| RerankScoredCandidate {
                        chunk_id: candidate.chunk_id.clone(),
                        initial_score: candidate.initial_score,
                        recall_rank: candidate.recall_rank,
                        rerank_score: scores[candidate.chunk_id.as_str()],
                    })
                    .collect(),
                elapsed_ms: result.elapsed_ms,
                failed: false,
            };
        }
        eprintln!("rerank response missed candidates for {query_id} (attempt {attempt})");
    }
    failed()
}

/// Generate the sidecar for one (baseline, method) pair: serial `/rerank`
/// calls over every bench query (serial pacing avoids timing noise and
/// provider rate limits).
pub async fn generate_sidecar<P: cce_llm::RerankProvider>(
    provider: &P,
    bench: &BenchmarkData,
    source_hash: u64,
    method: &str,
    runtime: &RerankRuntime,
) -> RerankSidecar {
    let recall = rank_recall(bench);
    let mut queries = Vec::with_capacity(bench.queries.len());
    for (query_idx, query) in bench.queries.iter().enumerate() {
        let candidates = build_candidates(
            bench,
            &recall,
            method,
            runtime.text_source,
            query_idx,
            runtime.depth,
        );
        if candidates.is_empty() {
            eprintln!("  query {} has no {method} candidates; skipped", query.id);
            continue;
        }
        let query_text = bench
            .query_texts
            .get(query_idx)
            .map(String::as_str)
            .unwrap_or(query.text.as_str());
        let outcome =
            score_query_outcome(provider, &query.id, query_text, &candidates, runtime).await;
        if outcome.failed {
            eprintln!("  query {} recorded as failed sample", query.id);
        }
        queries.push(outcome);
    }
    RerankSidecar {
        header: RerankSidecarHeader {
            rerank_model_key: runtime.model_key.clone(),
            model_name: runtime.model_name.clone(),
            mode: runtime.mode.clone(),
            retrieval_method: method.to_string(),
            candidate_depth: runtime.depth,
            text_source: runtime.text_source.label().to_string(),
            text_source_rule: TEXT_SOURCE_RULE.to_string(),
            truncate_chars: RERANK_TRUNCATE_CHARS,
            source_hash,
            generated_at: chrono::Utc::now().to_rfc3339(),
        },
        queries,
    }
}

/// Generate sidecars for every baseline and every matrix method.
///
/// Reads only `bench_data.rkyv`; never modifies it. Returns written paths.
pub async fn generate_all_sidecars<P: cce_llm::RerankProvider>(
    project: &'static str,
    provider: &P,
    runtime: &RerankRuntime,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let paths = BenchmarkPaths::new(project);
    let mut written = Vec::new();
    for baseline in BASELINES {
        let data_path = paths.data_dir(baseline);
        if !data_path.exists() {
            eprintln!(
                "WARNING: {baseline} data not found at {}",
                data_path.display()
            );
            continue;
        }
        println!(
            "Loading baseline '{baseline}' from: {}",
            data_path.display()
        );
        let bytes = std::fs::read(&data_path)?;
        let source_hash = fnv1a64(&bytes);
        let bench = rkyv::from_bytes::<BenchmarkData, rkyv::rancor::Error>(&bytes)
            .map_err(|e| format!("rkyv deserialize failed: {e:?}"))?;
        for method in RERANK_METHODS {
            println!("  scoring method '{method}' (depth {})", runtime.depth);
            let sidecar = generate_sidecar(provider, &bench, source_hash, method, runtime).await;
            let failed = sidecar.queries.iter().filter(|q| q.failed).count();
            let out = sidecar_path(
                project,
                baseline,
                method,
                runtime.text_source,
                &runtime.model_key,
                runtime.depth,
            );
            save_sidecar(&out, &sidecar)?;
            println!(
                "  wrote {} ({} queries, {} failed)",
                out.display(),
                sidecar.queries.len(),
                failed
            );
            written.push(out);
        }
    }
    if written.is_empty() {
        eprintln!("No baseline data loaded. Run `gen_bench_{project}` first.");
    }
    Ok(written)
}

/// One (baseline, method, query, top_k) score row.
pub struct RerankRow {
    pub baseline: String,
    pub method: String,
    pub query_id: String,
    pub query_type: QueryType,
    pub top_k: usize,
    pub range: crate::range_evaluator::RangeBasedPerQueryScore,
    pub avg_first_hit_rank: Option<f64>,
    pub redundant_coverage: usize,
}

/// Top-k relevance detail for one (baseline, method, query).
pub struct RerankRelevance {
    pub baseline: String,
    pub method: String,
    pub query_id: String,
    pub strong: Vec<(usize, String, RelevanceLevel)>,
    pub related: Vec<(usize, String, RelevanceLevel)>,
}

/// Latency/cost accounting for one (baseline, method).
pub struct RerankLatency {
    pub baseline: String,
    pub method: String,
    pub scored_queries: usize,
    pub failed_queries: usize,
    pub elapsed_ms: Vec<u64>,
    pub candidate_counts: Vec<usize>,
}

/// Per-baseline manifest entry for the run report.
pub struct BaselineManifest {
    pub baseline: String,
    pub source_hash: u64,
    pub emb_chunks: usize,
    pub bm25_chunks: usize,
    pub methods: Vec<String>,
}

/// Aggregated result of a rerank benchmark run.
pub struct RerankBenchmarkRun {
    pub output_dir: PathBuf,
    pub rows: Vec<RerankRow>,
    pub relevance: Vec<RerankRelevance>,
    pub latency: Vec<RerankLatency>,
    pub methods: Vec<String>,
    pub project: String,
    pub model_key: String,
    pub model_name: String,
    pub mode: String,
    pub depth: usize,
    pub fusion: String,
    pub judgment_count: usize,
    pub baselines: Vec<BaselineManifest>,
}

/// Per-query evaluation output (control + reranked rows).
pub struct QueryRerankEval {
    pub rows: Vec<RerankRow>,
    pub relevance: Vec<RerankRelevance>,
}

/// Score one query offline: control order plus the fused rerank order.
///
/// The stored scores are aligned to freshly rebuilt candidates by chunk id;
/// any mismatch (failed sample, stale sidecar) yields no rows.
pub fn evaluate_query_rerank(
    baseline: &str,
    method: &str,
    query_id: &str,
    judgment: &RelevanceJudgment,
    candidates: &[CandidateRef],
    outcome: &RerankQueryOutcome,
    scoring: &RerankScoring,
) -> QueryRerankEval {
    let mut eval = QueryRerankEval {
        rows: Vec::new(),
        relevance: Vec::new(),
    };
    if outcome.failed {
        return eval;
    }
    let scores: HashMap<&str, f32> = outcome
        .candidates
        .iter()
        .map(|candidate| (candidate.chunk_id.as_str(), candidate.rerank_score))
        .collect();
    if candidates
        .iter()
        .any(|candidate| !scores.contains_key(candidate.chunk_id.as_str()))
    {
        return eval;
    }
    let initials = if scoring.normalize_initial {
        normalize_initial_scores(candidates)
    } else {
        candidates
            .iter()
            .map(|candidate| candidate.initial_score as f32)
            .collect()
    };
    let mut reranked: Vec<(&CandidateRef, f32)> = {
        // Rerank order is descending rerank score with a stable tiebreak, so
        // rank-fusion variants observe the same ordering as production.
        let mut order: Vec<usize> = (0..candidates.len()).collect();
        order.sort_by(|&left, &right| {
            scores[candidates[right].chunk_id.as_str()]
                .total_cmp(&scores[candidates[left].chunk_id.as_str()])
        });
        let mut rerank_rank = vec![0usize; candidates.len()];
        for (rank, index) in order.iter().enumerate() {
            rerank_rank[*index] = rank;
        }
        candidates
            .iter()
            .zip(initials)
            .enumerate()
            .map(|(initial_rank, (candidate, initial))| {
                let final_score = scoring.fusion.calculate(
                    scores[candidate.chunk_id.as_str()],
                    initial,
                    rerank_rank[initial_rank],
                    initial_rank,
                );
                (candidate, final_score)
            })
            .collect()
    };
    // Stable sort: score ties keep recall order, mirroring production merge.
    reranked.sort_by(|left, right| right.1.total_cmp(&left.1));

    let control: Vec<&CandidateRef> = candidates.iter().collect();
    score_order(baseline, method, query_id, judgment, &control, &mut eval);
    let order: Vec<&CandidateRef> = reranked.iter().map(|(candidate, _)| *candidate).collect();
    score_order(
        baseline,
        &reranked_method(method),
        query_id,
        judgment,
        &order,
        &mut eval,
    );
    eval
}

/// Single-scan scoring of one candidate order across all top-k cutoffs.
fn score_order(
    baseline: &str,
    method_label: &str,
    query_id: &str,
    judgment: &RelevanceJudgment,
    ordered: &[&CandidateRef],
    eval: &mut QueryRerankEval,
) {
    let max_cutoff = TOP_K_VALUES.iter().copied().max().unwrap_or(0);
    let mut scan = RankedScan::new(judgment, TOP_K_VALUES, ordered.len());
    let mut strong = Vec::new();
    let mut related = Vec::new();
    for (position, candidate) in ordered.iter().take(max_cutoff).enumerate() {
        let relevance = scan.advance(&candidate.eval_chunk);
        if position < DEFAULT_TOP_K {
            if let Some(level) = relevance.highest_level() {
                let location = format!(
                    "{}:{}-{}",
                    candidate.eval_chunk.file_path,
                    candidate.eval_chunk.start_line,
                    candidate.eval_chunk.end_line
                );
                match level {
                    RelevanceLevel::Strong => strong.push((position + 1, location, level)),
                    RelevanceLevel::Related => related.push((position + 1, location, level)),
                    _ => {}
                }
            }
        }
    }

    for (&top_k, (range, diagnostics)) in TOP_K_VALUES.iter().zip(scan.finish()) {
        eval.rows.push(RerankRow {
            baseline: baseline.to_string(),
            method: method_label.to_string(),
            query_id: query_id.to_string(),
            query_type: judgment.query_type,
            top_k,
            range,
            avg_first_hit_rank: diagnostics.avg_first_hit_rank,
            redundant_coverage: diagnostics.redundant_coverage,
        });
    }

    eval.relevance.push(RerankRelevance {
        baseline: baseline.to_string(),
        method: method_label.to_string(),
        query_id: query_id.to_string(),
        strong,
        related,
    });
}

/// Run the offline rerank benchmark for a project and write all reports.
///
/// Reads `bench_data.rkyv` plus the generated sidecars; makes no model calls.
/// A stale sidecar (source hash, method, depth, candidate identity, or model
/// mismatch) aborts the run instead of producing misleading numbers.
pub fn run_rerank_benchmark(
    project: &'static str,
    judgments: &[RelevanceJudgment],
    scoring: &RerankScoring,
) -> Result<RerankBenchmarkRun, Box<dyn std::error::Error>> {
    let paths = BenchmarkPaths::new(project);
    let mut rows = Vec::new();
    let mut relevance = Vec::new();
    let mut latency = Vec::new();
    let mut baselines = Vec::new();
    let mut model_name = String::new();
    let mut mode = String::new();

    for baseline in BASELINES {
        let data_path = paths.data_dir(baseline);
        if !data_path.exists() {
            eprintln!(
                "WARNING: {baseline} data not found at {}",
                data_path.display()
            );
            continue;
        }
        println!(
            "Loading baseline '{baseline}' from: {}",
            data_path.display()
        );
        let bytes = std::fs::read(&data_path)?;
        let source_hash = fnv1a64(&bytes);
        let bench: BenchmarkData = rkyv::from_bytes::<BenchmarkData, rkyv::rancor::Error>(&bytes)
            .map_err(|e| format!("rkyv deserialize failed: {e:?}"))?;
        let recall = rank_recall(&bench);
        let mut ok_methods = Vec::new();

        for method in RERANK_METHODS {
            let sidecar_file = sidecar_path(
                project,
                baseline,
                method,
                scoring.text_source,
                &scoring.model_key,
                scoring.depth,
            );
            if !sidecar_file.exists() {
                eprintln!(
                    "WARNING: {method} sidecar not found at {}; run the generation step first",
                    sidecar_file.display()
                );
                continue;
            }
            let sidecar = load_sidecar(&sidecar_file)?;
            validate_sidecar(&sidecar, method, source_hash, scoring)?;
            if model_name.is_empty() {
                model_name = sidecar.header.model_name.clone();
                mode = sidecar.header.mode.clone();
            } else if model_name != sidecar.header.model_name || mode != sidecar.header.mode {
                return Err(format!(
                    "sidecar model mismatch in {}: expected {model_name}/{mode}, found {}/{}",
                    sidecar_file.display(),
                    sidecar.header.model_name,
                    sidecar.header.mode
                )
                .into());
            }

            let mut scored = 0;
            let mut failed = 0;
            let mut elapsed = Vec::new();
            let mut candidate_counts = Vec::new();
            for outcome in &sidecar.queries {
                let Some(judgment) = judgments.iter().find(|j| j.id == outcome.query_id) else {
                    continue;
                };
                let Some(query_idx) = bench.queries.iter().position(|q| q.id == outcome.query_id)
                else {
                    return Err(format!(
                        "sidecar query {} missing from {baseline} bench data",
                        outcome.query_id
                    )
                    .into());
                };
                if outcome.failed {
                    failed += 1;
                    continue;
                }
                let candidates = build_candidates(
                    &bench,
                    &recall,
                    method,
                    scoring.text_source,
                    query_idx,
                    scoring.depth,
                );
                let regenerated: Vec<&str> = candidates
                    .iter()
                    .map(|candidate| candidate.chunk_id.as_str())
                    .collect();
                let stored: Vec<&str> = outcome
                    .candidates
                    .iter()
                    .map(|candidate| candidate.chunk_id.as_str())
                    .collect();
                if regenerated != stored {
                    return Err(format!(
                        "sidecar candidate drift for {baseline}/{method}/{}: regenerate the sidecar",
                        outcome.query_id
                    )
                    .into());
                }
                let eval = evaluate_query_rerank(
                    baseline,
                    method,
                    &outcome.query_id,
                    judgment,
                    &candidates,
                    outcome,
                    scoring,
                );
                if eval.rows.is_empty() {
                    failed += 1;
                    continue;
                }
                scored += 1;
                elapsed.push(outcome.elapsed_ms);
                candidate_counts.push(outcome.candidates.len());
                rows.extend(eval.rows);
                relevance.extend(eval.relevance);
            }
            latency.push(RerankLatency {
                baseline: (*baseline).to_string(),
                method: (*method).to_string(),
                scored_queries: scored,
                failed_queries: failed,
                elapsed_ms: elapsed,
                candidate_counts,
            });
            ok_methods.push((*method).to_string());
        }

        baselines.push(BaselineManifest {
            baseline: (*baseline).to_string(),
            source_hash,
            emb_chunks: bench.embedding.chunks.len(),
            bm25_chunks: bench.bm25.chunks.len(),
            methods: ok_methods,
        });
    }

    if rows.is_empty() {
        eprintln!("No rerank data scored. Generate sidecars first.");
    }

    let fusion_label = scoring.fusion_label();
    let out = paths
        .output_dir()
        .join(format!("rerank_{}", scoring.variant_dir()));
    std::fs::create_dir_all(&out)?;
    let run = RerankBenchmarkRun {
        output_dir: out,
        rows,
        relevance,
        latency,
        methods: ordered_methods(),
        project: project.to_string(),
        model_key: scoring.model_key.clone(),
        model_name,
        mode,
        depth: scoring.depth,
        fusion: fusion_label,
        judgment_count: judgments.len(),
        baselines,
    };
    report::write_all(&run)?;
    println!(
        "\n✓ Rerank evaluation files written to: {}",
        run.output_dir.display()
    );
    print_summary(&run.rows);

    Ok(run)
}

/// Reject a stale sidecar instead of scoring misleading numbers.
fn validate_sidecar(
    sidecar: &RerankSidecar,
    method: &str,
    source_hash: u64,
    scoring: &RerankScoring,
) -> Result<(), Box<dyn std::error::Error>> {
    let header = &sidecar.header;
    if header.retrieval_method != method {
        return Err(format!(
            "sidecar method mismatch: expected {method}, found {}",
            header.retrieval_method
        )
        .into());
    }
    if header.rerank_model_key != scoring.model_key {
        return Err(format!(
            "sidecar model mismatch: expected {}, found {}",
            scoring.model_key, header.rerank_model_key
        )
        .into());
    }
    if header.candidate_depth != scoring.depth {
        return Err(format!(
            "sidecar depth mismatch: expected {}, found {}",
            scoring.depth, header.candidate_depth
        )
        .into());
    }
    if header.source_hash != source_hash {
        return Err(format!(
            "sidecar stale for {method}: bench_data.rkyv changed since generation; regenerate the sidecar"
        )
        .into());
    }
    Ok(())
}

/// Print a compact top-5 console summary grouped by (baseline, method).
fn print_summary(rows: &[RerankRow]) {
    println!("\n=== Rerank Summary (top-5, F1_any) ===");
    let top5: Vec<_> = rows.iter().filter(|row| row.top_k == 5).collect();
    let mut map: BTreeMap<(&str, &str), Vec<&RerankRow>> = BTreeMap::new();
    for row in &top5 {
        map.entry((row.baseline.as_str(), row.method.as_str()))
            .or_default()
            .push(row);
    }
    for ((baseline, method), group) in &map {
        let count = group.len() as f64;
        let recall = group.iter().map(|row| row.range.recall_any).sum::<f64>() / count;
        let f1 = group.iter().map(|row| row.range.f1_any).sum::<f64>() / count;
        println!(
            "  {baseline:22} {method:16}  R={recall:.3} F1={f1:.3} (n={})",
            group.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_hash_is_deterministic() {
        assert_eq!(fnv1a64(b"bench"), fnv1a64(b"bench"));
        assert_ne!(fnv1a64(b"bench-a"), fnv1a64(b"bench-b"));
    }

    #[test]
    fn sidecar_round_trip_preserves_scores() {
        let sidecar = RerankSidecar {
            header: RerankSidecarHeader {
                rerank_model_key: RERANK_MODEL_KEY.to_string(),
                model_name: "test-model".to_string(),
                mode: "test".to_string(),
                retrieval_method: "emb".to_string(),
                candidate_depth: RERANK_CANDIDATE_DEPTH,
                text_source: RerankTextSource::EmbText.label().to_string(),
                text_source_rule: TEXT_SOURCE_RULE.to_string(),
                truncate_chars: RERANK_TRUNCATE_CHARS,
                source_hash: 42,
                generated_at: "test".to_string(),
            },
            queries: vec![RerankQueryOutcome {
                query_id: "TQ1".to_string(),
                query_text: "alpha beta".to_string(),
                candidates: vec![RerankScoredCandidate {
                    chunk_id: "c0".to_string(),
                    initial_score: 0.9,
                    recall_rank: 1,
                    rerank_score: 0.2,
                }],
                elapsed_ms: 7,
                failed: false,
            }],
        };
        let bytes = encode_sidecar(&sidecar).expect("encode must succeed");
        let decoded = decode_sidecar(&bytes).expect("decode must succeed");
        assert_eq!(decoded.queries.len(), 1);
        assert_eq!(decoded.header.source_hash, 42);
        assert!((decoded.queries[0].candidates[0].rerank_score - 0.2).abs() < f32::EPSILON);
    }

    #[test]
    fn stale_sidecar_is_rejected() {
        let sidecar = RerankSidecar {
            header: RerankSidecarHeader {
                rerank_model_key: RERANK_MODEL_KEY.to_string(),
                model_name: "m".to_string(),
                mode: "cross_encoder".to_string(),
                retrieval_method: "emb".to_string(),
                candidate_depth: RERANK_CANDIDATE_DEPTH,
                text_source: RerankTextSource::EmbText.label().to_string(),
                text_source_rule: TEXT_SOURCE_RULE.to_string(),
                truncate_chars: RERANK_TRUNCATE_CHARS,
                source_hash: 1,
                generated_at: "test".to_string(),
            },
            queries: Vec::new(),
        };
        let scoring = RerankScoring {
            model_key: RERANK_MODEL_KEY.to_string(),
            depth: RERANK_CANDIDATE_DEPTH,
            fusion: RerankFusionStrategy::RerankOnly,
            normalize_initial: false,
            text_source: RerankTextSource::EmbText,
        };
        assert!(validate_sidecar(&sidecar, "emb", 2, &scoring).is_err());
        assert!(validate_sidecar(&sidecar, "emb", 1, &scoring).is_ok());
    }

    #[test]
    fn normalize_initial_scores_maps_min_to_zero_and_max_to_one() {
        let candidates = vec![
            test_candidate("c0", 12.5),
            test_candidate("c1", 1.75),
            test_candidate("c2", 7.0),
        ];
        let normalized = normalize_initial_scores(&candidates);
        assert_eq!(normalized.len(), 3);
        assert!((normalized[0] - 1.0).abs() < 1e-6);
        assert!((normalized[1] - 0.0).abs() < 1e-6);
        assert!((normalized[2] - (7.0 - 1.75) as f32 / (12.5 - 1.75) as f32).abs() < 1e-6);
    }

    #[test]
    fn normalize_initial_scores_degenerate_list_keeps_full_weight() {
        let normalized =
            normalize_initial_scores(&[test_candidate("c0", 0.0), test_candidate("c1", 0.0)]);
        assert_eq!(normalized, vec![1.0, 1.0]);
        assert!(normalize_initial_scores(&[]).is_empty());
    }

    fn test_candidate(id: &str, initial_score: f64) -> CandidateRef {
        CandidateRef {
            chunk_id: id.to_string(),
            eval_chunk: crate::bench_data::ChunkData {
                chunk_id: id.to_string(),
                entity_name: String::new(),
                file_path: "src/lib.rs".to_string(),
                start_line: 1,
                end_line: 2,
                source_ranges: Vec::new(),
                source_span_kind: String::new(),
                test_info: cce_types::TestInfo::unknown(),
                language: None,
                entity_ids: Vec::new(),
                segment_id: String::new(),
            },
            text: String::new(),
            file_path: "src/lib.rs".to_string(),
            initial_score,
            recall_rank: 1,
        }
    }

    #[test]
    fn scoring_variant_dir_is_filesystem_safe() {
        let scoring = RerankScoring {
            model_key: RERANK_MODEL_KEY.to_string(),
            depth: RERANK_CANDIDATE_DEPTH,
            fusion: RerankFusionStrategy::LinearWeighted { alpha: 0.7 },
            normalize_initial: true,
            text_source: RerankTextSource::EmbText,
        };
        assert_eq!(
            scoring.variant_dir(),
            "linear-weighted-alpha0.7_emb-text_norm-init"
        );
        assert!(scoring.fusion_label().contains("+norm_init"));
    }
}
