use cce_storage_common::FulltextDocument;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use cce_orchestrator::index::{FileProcessor, build_bm25_documents};
use cce_parser::ast_to_nl::chunker::ChunkedResult;
use cce_parser::grouper::GroupType;
use cce_scanner::FileEntry;
use cce_types::OutputMode;

use crate::FixtureSpec;
use crate::bench_data::{ChunkData, RelevanceJudgment};
use crate::bench_gen::{chunk_data_from_result, scan_fixture};
use crate::infra::{
    Bm25Config, ExpandedQuery, InMemoryTermIndex, build_term_index, expand_query, score_all,
};
use crate::range_evaluator::{evaluate_ranked_chunks_range_based_cutoffs, is_relevant_to_query};
use cce_storage_bm25::TermOperator;

const PROJECT_ID: i64 = 1;
const EPOCH: i64 = 0;
const TOP_K: usize = 20;

/// Stable, hashable, sortable parameter set key using fixed-precision integers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParameterSetKey {
    pub k1_pct: u16,
    pub b_pct: u16,
    pub title_w_pct: u16,
    pub keywords_w_pct: u16,
    pub content_w_pct: u16,
}

impl ParameterSetKey {
    fn from_floats(k1: f32, b: f32, title_w: f32, keywords_w: f32, content_w: f32) -> Self {
        Self {
            k1_pct: (k1 * 100.0).round() as u16,
            b_pct: (b * 100.0).round() as u16,
            title_w_pct: (title_w * 100.0).round() as u16,
            keywords_w_pct: (keywords_w * 100.0).round() as u16,
            content_w_pct: (content_w * 100.0).round() as u16,
        }
    }

    fn k1(&self) -> f32 {
        self.k1_pct as f32 / 100.0
    }
    fn b(&self) -> f32 {
        self.b_pct as f32 / 100.0
    }
    fn title_w(&self) -> f32 {
        self.title_w_pct as f32 / 100.0
    }
    fn keywords_w(&self) -> f32 {
        self.keywords_w_pct as f32 / 100.0
    }
    fn content_w(&self) -> f32 {
        self.content_w_pct as f32 / 100.0
    }

    pub fn display_name(&self) -> String {
        format!(
            "k1_{:.2}_b_{:.2}_t_{:.2}_kw_{:.2}_c_{:.2}",
            self.k1(),
            self.b(),
            self.title_w(),
            self.keywords_w(),
            self.content_w(),
        )
    }

    fn to_bm25_config(&self) -> Bm25Config {
        Bm25Config {
            k1: self.k1() as f64,
            b: self.b() as f64,
            title_weight: self.title_w() as f64,
            keywords_weight: self.keywords_w() as f64,
            content_weight: self.content_w() as f64,
        }
    }
}

/// Phase describes when a parameter set was first introduced in the sweep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Algorithm,
    FieldWeights,
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Phase::Algorithm => write!(f, "algorithm"),
            Phase::FieldWeights => write!(f, "field_weights"),
        }
    }
}

/// Single-query observation: the only source of truth in the experiment.
#[derive(Debug, Clone)]
pub struct QueryObservation {
    pub parameter_key: ParameterSetKey,
    pub phase: Phase,
    pub query_id: String,
    pub query_type: String,
    pub query_text: String,
    pub first_strong_rank: Option<usize>,
    pub strong_f1_at_10: f64,
    pub recall_any_at_20: f64,
}

/// Aggregate metrics derived from query observations.
#[derive(Debug, Clone)]
pub struct ParameterSetAggregate {
    pub parameter_key: ParameterSetKey,
    pub phase: Phase,
    pub query_count: usize,
    pub strong_mrr_at_10: f64,
    pub strong_f1_at_10: f64,
    pub recall_any_at_20: f64,
}

/// Aggregate metrics broken down by query type.
#[derive(Debug, Clone)]
pub struct QueryTypeAggregate {
    pub parameter_key: ParameterSetKey,
    pub phase: Phase,
    pub query_type: String,
    pub query_count: usize,
    pub strong_mrr_at_10: f64,
    pub strong_f1_at_10: f64,
    pub recall_any_at_20: f64,
}

struct SweepTiming {
    corpus_build: std::time::Duration,
    index_build: std::time::Duration,
    query_and_eval: std::time::Duration,
    report_generation: std::time::Duration,
}

struct Manifest {
    fixture: String,
    judgment_count: usize,
    document_count: usize,
    tokenizer: String,
    stage_one_parameter_count: usize,
    stage_two_parameter_count: usize,
    unique_parameter_count: usize,
    index_build_count: usize,
    timing: SweepTiming,
}

struct PreparedQuery<'a> {
    judgment: &'a RelevanceJudgment,
    query_text: String,
    query_forms: ExpandedQuery,
}

struct EvaluationContext<'a> {
    chunks: &'a [ChunkData],
    queries: Vec<PreparedQuery<'a>>,
}

pub async fn run_project_bm25_parameter_sweep(
    project: crate::benchmark_projects::BenchmarkProject,
    output_dir: &Path,
) -> Result<Vec<QueryObservation>> {
    run_bm25_parameter_sweep(
        &project.fixture_label(),
        project.target_spec(),
        project.distractor_spec(),
        project.judgments(),
        output_dir,
    )
    .await
}

async fn run_bm25_parameter_sweep(
    fixture_name: &str,
    target_spec: FixtureSpec,
    distractor_spec: FixtureSpec,
    judgments: Vec<RelevanceJudgment>,
    output_dir: &Path,
) -> Result<Vec<QueryObservation>> {
    let sweep_start = Instant::now();

    eprintln!("Building full-pipeline BM25 documents for {fixture_name}...");
    let corpus_start = Instant::now();
    let (chunks, documents) = build_full_pipeline_documents(target_spec, distractor_spec).await?;
    let corpus_build = corpus_start.elapsed();
    eprintln!("Built {} BM25 documents.", documents.len());

    if chunks.len() != documents.len() {
        anyhow::bail!(
            "BM25 benchmark document/chunk mismatch: {} documents, {} chunks",
            documents.len(),
            chunks.len()
        );
    }

    let index_start = Instant::now();
    let term_index = build_term_index(&documents);
    let index_build = index_start.elapsed();
    eprintln!(
        "In-memory term index built: {} terms, {} docs",
        term_index.postings.len(),
        term_index.n_docs
    );

    let context = build_evaluation_context(&chunks, &judgments, &term_index);

    let mut all_observations: Vec<QueryObservation> = Vec::new();

    // Stage 1: 9 algorithm parameter sets with fixed field weights 4/2/1
    let stage_one_sets = stage_one_parameter_sets();
    eprintln!(
        "Stage 1: evaluating {} parameter sets",
        stage_one_sets.len()
    );

    for param in &stage_one_sets {
        let observations = evaluate_parameter_set(&term_index, param, Phase::Algorithm, &context)?;
        all_observations.extend(observations);
    }

    // Select finalists from Qualified query type aggregates
    let by_qt = aggregate_by_query_type(&all_observations);
    let finalists = select_finalists(&by_qt);

    // Stage 2: field-weight presets for each finalist, deduplicated against stage 1
    let evaluated_keys: HashSet<ParameterSetKey> = all_observations
        .iter()
        .map(|obs| obs.parameter_key.clone())
        .collect();
    let stage_two_sets = stage_two_parameter_sets(&finalists, &evaluated_keys);
    eprintln!(
        "Stage 2: {} parameter sets ({} new after dedup against stage 1, {} finalists)",
        stage_two_sets.len() + finalists.len() * 9,
        stage_two_sets.len(),
        finalists.len(),
    );

    for param in &stage_two_sets {
        let observations =
            evaluate_parameter_set(&term_index, param, Phase::FieldWeights, &context)?;
        all_observations.extend(observations);
    }

    let query_and_eval = sweep_start.elapsed() - corpus_build - index_build;

    let aggregate = aggregate_by_parameter_set(&all_observations);
    let aggregate_by_query_type = aggregate_by_query_type(&all_observations);

    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create {}", output_dir.display()))?;

    let report_start = Instant::now();
    write_aggregate_metrics(output_dir.join("aggregate_metrics.md"), &aggregate)?;
    write_aggregate_metrics_by_query_type(
        output_dir.join("aggregate_metrics_by_query_type.md"),
        &aggregate_by_query_type,
    )?;
    write_per_query_metrics(output_dir.join("per_query_metrics.md"), &all_observations)?;
    let report_generation = report_start.elapsed();

    let manifest = Manifest {
        fixture: fixture_name.to_string(),
        judgment_count: judgments.len(),
        document_count: documents.len(),
        tokenizer: "MixedTokenizer (production query expansion)".to_string(),
        stage_one_parameter_count: stage_one_sets.len(),
        stage_two_parameter_count: stage_two_sets.len(),
        unique_parameter_count: aggregate.len(),
        index_build_count: 1,
        timing: SweepTiming {
            corpus_build,
            index_build,
            query_and_eval,
            report_generation,
        },
    };
    write_manifest(output_dir.join("manifest.md"), &manifest)?;

    let total_elapsed = sweep_start.elapsed();
    eprintln!(
        "Sweep complete: {} unique parameter sets, {:.2}s total (build {:.2}s + query {:.2}s)",
        aggregate.len(),
        total_elapsed.as_secs_f64(),
        index_build.as_secs_f64(),
        query_and_eval.as_secs_f64(),
    );

    Ok(all_observations)
}

fn stage_one_parameter_sets() -> Vec<ParameterSetKey> {
    [1.2, 1.5, 1.8]
        .into_iter()
        .flat_map(|k1| {
            [0.4, 0.6, 0.75]
                .into_iter()
                .map(move |b| ParameterSetKey::from_floats(k1, b, 4.0, 2.0, 1.0))
        })
        .collect()
}

fn select_finalists(aggregate: &[QueryTypeAggregate]) -> Vec<(u16, u16)> {
    let mut ranked: Vec<_> = aggregate
        .iter()
        .filter(|metric| metric.query_type == "qualified")
        .collect();
    ranked.sort_by(|left, right| {
        right
            .strong_mrr_at_10
            .total_cmp(&left.strong_mrr_at_10)
            .then_with(|| right.strong_f1_at_10.total_cmp(&left.strong_f1_at_10))
            .then_with(|| right.recall_any_at_20.total_cmp(&left.recall_any_at_20))
            .then_with(|| {
                left.parameter_key
                    .display_name()
                    .cmp(&right.parameter_key.display_name())
            })
    });
    ranked
        .into_iter()
        .take(2)
        .map(|metric| (metric.parameter_key.k1_pct, metric.parameter_key.b_pct))
        .collect()
}

fn stage_two_parameter_sets(
    finalists: &[(u16, u16)],
    evaluated: &HashSet<ParameterSetKey>,
) -> Vec<ParameterSetKey> {
    let mut sets = Vec::new();
    for &(k1_pct, b_pct) in finalists {
        let k1 = k1_pct as f32 / 100.0;
        let b = b_pct as f32 / 100.0;
        for title_w in [2.0, 4.0, 6.0] {
            for keywords_w in [1.0, 2.0, 4.0] {
                let key = ParameterSetKey::from_floats(k1, b, title_w, keywords_w, 1.0);
                if !evaluated.contains(&key) {
                    sets.push(key);
                }
            }
        }
    }
    sets
}

fn build_evaluation_context<'a>(
    chunks: &'a [ChunkData],
    judgments: &'a [RelevanceJudgment],
    term_index: &InMemoryTermIndex,
) -> EvaluationContext<'a> {
    let queries = judgments
        .iter()
        .map(|judgment| {
            let query_forms = expand_query(&judgment.query_text, term_index);
            PreparedQuery {
                judgment,
                query_text: judgment.query_text.clone(),
                query_forms,
            }
        })
        .collect();

    EvaluationContext { chunks, queries }
}

fn evaluate_parameter_set(
    term_index: &InMemoryTermIndex,
    key: &ParameterSetKey,
    phase: Phase,
    context: &EvaluationContext<'_>,
) -> Result<Vec<QueryObservation>> {
    let config = key.to_bm25_config();
    let query_forms_list: Vec<ExpandedQuery> = context
        .queries
        .iter()
        .map(|pq| pq.query_forms.clone())
        .collect();

    // Production query semantics: whole/split token expansion, field weights,
    // Or operator (production default).
    let all_ranked = score_all(
        term_index,
        &query_forms_list,
        &config,
        TermOperator::Or,
        TOP_K,
    );

    let mut observations = Vec::with_capacity(context.queries.len());

    for (query_idx, prepared_query) in context.queries.iter().enumerate() {
        let judgment = prepared_query.judgment;
        let ranked = &all_ranked[query_idx];

        let mut ranked_indices = Vec::with_capacity(ranked.len());
        let mut first_strong_rank = None;

        for (rank, &(doc_idx, _score)) in ranked.iter().enumerate() {
            ranked_indices.push(doc_idx);
            if first_strong_rank.is_none()
                && is_relevant_to_query(&context.chunks[doc_idx], judgment)
                    .is_some_and(|level| level == crate::bench_data::RelevanceLevel::Strong)
            {
                first_strong_rank = Some(rank + 1);
            }
        }

        let scores = evaluate_ranked_chunks_range_based_cutoffs(
            judgment,
            context.chunks,
            &ranked_indices,
            &[10, TOP_K],
        );
        let at_ten = scores[0].clone();
        let at_twenty = scores[1].clone();

        observations.push(QueryObservation {
            parameter_key: key.clone(),
            phase: phase.clone(),
            query_id: judgment.id.clone(),
            query_type: judgment.query_type.to_string(),
            query_text: prepared_query.query_text.clone(),
            first_strong_rank,
            strong_f1_at_10: at_ten.f1_strong,
            recall_any_at_20: at_twenty.recall_any,
        });
    }

    Ok(observations)
}

pub fn aggregate_by_parameter_set(observations: &[QueryObservation]) -> Vec<ParameterSetAggregate> {
    let mut map: BTreeMap<ParameterSetKey, (Phase, Vec<&QueryObservation>)> = BTreeMap::new();
    for obs in observations {
        let entry = map
            .entry(obs.parameter_key.clone())
            .or_insert_with(|| (obs.phase.clone(), Vec::new()));
        entry.1.push(obs);
    }

    map.into_iter()
        .map(|(key, (phase, group))| {
            let count = group.len();
            let mrr_total: f64 = group
                .iter()
                .map(|obs| obs.first_strong_rank.map_or(0.0, |r| 1.0 / r as f64))
                .sum();
            let f1_total: f64 = group.iter().map(|obs| obs.strong_f1_at_10).sum();
            let recall_total: f64 = group.iter().map(|obs| obs.recall_any_at_20).sum();
            ParameterSetAggregate {
                parameter_key: key,
                phase,
                query_count: count,
                strong_mrr_at_10: average(mrr_total, count),
                strong_f1_at_10: average(f1_total, count),
                recall_any_at_20: average(recall_total, count),
            }
        })
        .collect()
}

pub fn aggregate_by_query_type(observations: &[QueryObservation]) -> Vec<QueryTypeAggregate> {
    let mut map: BTreeMap<(ParameterSetKey, String), (Phase, Vec<&QueryObservation>)> =
        BTreeMap::new();
    for obs in observations {
        let entry = map
            .entry((obs.parameter_key.clone(), obs.query_type.clone()))
            .or_insert_with(|| (obs.phase.clone(), Vec::new()));
        entry.1.push(obs);
    }

    map.into_iter()
        .map(|((key, qt), (phase, group))| {
            let count = group.len();
            let mrr_total: f64 = group
                .iter()
                .map(|obs| obs.first_strong_rank.map_or(0.0, |r| 1.0 / r as f64))
                .sum();
            let f1_total: f64 = group.iter().map(|obs| obs.strong_f1_at_10).sum();
            let recall_total: f64 = group.iter().map(|obs| obs.recall_any_at_20).sum();
            QueryTypeAggregate {
                parameter_key: key,
                phase,
                query_type: qt,
                query_count: count,
                strong_mrr_at_10: average(mrr_total, count),
                strong_f1_at_10: average(f1_total, count),
                recall_any_at_20: average(recall_total, count),
            }
        })
        .collect()
}

async fn build_full_pipeline_documents(
    target_spec: FixtureSpec,
    distractor_spec: FixtureSpec,
) -> Result<(Vec<ChunkData>, Vec<FulltextDocument>)> {
    let target_entries = scan_fixture(target_spec)?;
    let distractor_entries = scan_fixture(distractor_spec)?;
    let chunks =
        process_bm25_chunks(target_entries.iter().chain(distractor_entries.iter())).await?;
    let chunk_refs: Vec<_> = chunks.iter().collect();
    let documents = build_bm25_documents(&chunk_refs, PROJECT_ID, EPOCH);
    let chunk_data = chunks.iter().map(chunk_data_from_result).collect();
    Ok((chunk_data, documents))
}

async fn process_bm25_chunks<'a>(
    entries: impl Iterator<Item = &'a FileEntry>,
) -> Result<Vec<ChunkedResult>> {
    let mut processor = FileProcessor::new();
    let mut chunks = Vec::new();
    for entry in entries {
        let result = processor
            .process_file_complete(entry, OutputMode::Bm25)
            .await
            .with_context(|| format!("Failed to process {}", entry.path.display()))?;
        chunks.extend(result.chunks.into_iter().filter(|chunk| {
            chunk.group_type != GroupType::FileDocumentation && !chunk.text.is_empty()
        }));
    }
    Ok(chunks)
}

fn average(total: f64, count: usize) -> f64 {
    if count == 0 {
        0.0
    } else {
        total / count as f64
    }
}

// ---- Report writers ----

fn write_manifest(path: std::path::PathBuf, manifest: &Manifest) -> Result<()> {
    let duration_secs = |d: std::time::Duration| format!("{:.2}s", d.as_secs_f64());
    let rows = vec![
        vec!["fixture".to_string(), manifest.fixture.clone()],
        vec![
            "judgment_count".to_string(),
            manifest.judgment_count.to_string(),
        ],
        vec![
            "document_count".to_string(),
            manifest.document_count.to_string(),
        ],
        vec!["tokenizer".to_string(), manifest.tokenizer.clone()],
        vec![
            "stage_one_parameter_count".to_string(),
            manifest.stage_one_parameter_count.to_string(),
        ],
        vec![
            "stage_two_parameter_count".to_string(),
            manifest.stage_two_parameter_count.to_string(),
        ],
        vec![
            "unique_parameter_count".to_string(),
            manifest.unique_parameter_count.to_string(),
        ],
        vec![
            "index_build_count".to_string(),
            manifest.index_build_count.to_string(),
        ],
        vec![
            "corpus_build_time".to_string(),
            duration_secs(manifest.timing.corpus_build),
        ],
        vec![
            "index_build_time".to_string(),
            duration_secs(manifest.timing.index_build),
        ],
        vec![
            "query_and_eval_time".to_string(),
            duration_secs(manifest.timing.query_and_eval),
        ],
        vec![
            "report_generation_time".to_string(),
            duration_secs(manifest.timing.report_generation),
        ],
    ];
    write_markdown_table(path, &["key", "value"], &rows)
}

fn write_aggregate_metrics(
    path: std::path::PathBuf,
    metrics: &[ParameterSetAggregate],
) -> Result<()> {
    let rows = metrics
        .iter()
        .map(|m| {
            vec![
                m.parameter_key.display_name(),
                m.phase.to_string(),
                format!("{:.2}", m.parameter_key.k1()),
                format!("{:.2}", m.parameter_key.b()),
                format!("{:.2}", m.parameter_key.title_w()),
                format!("{:.2}", m.parameter_key.keywords_w()),
                format!("{:.2}", m.parameter_key.content_w()),
                m.query_count.to_string(),
                format!("{:.4}", m.strong_mrr_at_10),
                format!("{:.4}", m.strong_f1_at_10),
                format!("{:.4}", m.recall_any_at_20),
            ]
        })
        .collect::<Vec<_>>();
    write_markdown_table(
        path,
        &[
            "preset",
            "phase",
            "k1",
            "b",
            "title_w",
            "keywords_w",
            "content_w",
            "n",
            "MRR@10",
            "F1@10",
            "Recall@20",
        ],
        &rows,
    )
}

fn write_aggregate_metrics_by_query_type(
    path: std::path::PathBuf,
    metrics: &[QueryTypeAggregate],
) -> Result<()> {
    let rows = metrics
        .iter()
        .map(|m| {
            vec![
                m.parameter_key.display_name(),
                m.phase.to_string(),
                m.query_type.clone(),
                format!("{:.2}", m.parameter_key.k1()),
                format!("{:.2}", m.parameter_key.b()),
                format!("{:.2}", m.parameter_key.title_w()),
                format!("{:.2}", m.parameter_key.keywords_w()),
                format!("{:.2}", m.parameter_key.content_w()),
                m.query_count.to_string(),
                format!("{:.4}", m.strong_mrr_at_10),
                format!("{:.4}", m.strong_f1_at_10),
                format!("{:.4}", m.recall_any_at_20),
            ]
        })
        .collect::<Vec<_>>();
    write_markdown_table(
        path,
        &[
            "preset",
            "phase",
            "query_type",
            "k1",
            "b",
            "title_w",
            "keywords_w",
            "content_w",
            "n",
            "MRR@10",
            "F1@10",
            "Recall@20",
        ],
        &rows,
    )
}

fn write_per_query_metrics(
    path: std::path::PathBuf,
    observations: &[QueryObservation],
) -> Result<()> {
    let rows = observations
        .iter()
        .map(|obs| {
            vec![
                obs.parameter_key.display_name(),
                obs.phase.to_string(),
                obs.query_id.clone(),
                obs.query_type.clone(),
                markdown_cell(&obs.query_text),
                obs.first_strong_rank
                    .map_or("none".to_string(), |r| r.to_string()),
                format!("{:.4}", obs.strong_f1_at_10),
                format!("{:.4}", obs.recall_any_at_20),
            ]
        })
        .collect::<Vec<_>>();
    write_markdown_table(
        path,
        &[
            "preset",
            "phase",
            "query_id",
            "query_type",
            "query_text",
            "first_strong_rank",
            "F1@10",
            "Recall@20",
        ],
        &rows,
    )
}

fn write_markdown_table(
    path: std::path::PathBuf,
    headers: &[&str],
    rows: &[Vec<String>],
) -> Result<()> {
    let mut output = String::new();
    output.push_str(
        &headers
            .iter()
            .map(|header| format!("| {header} "))
            .collect::<String>(),
    );
    output.push_str("|\n");
    output.push_str(&headers.iter().map(|_| "|---").collect::<String>());
    output.push_str("|\n");
    for row in rows {
        output.push_str(
            &row.iter()
                .map(|value| format!("| {value} "))
                .collect::<String>(),
        );
        output.push_str("|\n");
    }
    std::fs::write(&path, output).with_context(|| format!("Failed to write {}", path.display()))
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}
