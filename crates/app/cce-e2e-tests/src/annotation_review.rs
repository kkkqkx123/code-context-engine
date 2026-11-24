//! Offline annotation-review export for the RelationAnnotator.
//!
//! For every query in the `full_pipeline` benchmark snapshot this job replays
//! embedding recall offline (cosine over the stored chunk/query vectors, no
//! Qdrant, no network) and renders one self-contained markdown file per query
//! under:
//!
//! ```text
//! outputs/scenarios/{lang}/annotation/{project}/
//! ├── index.md
//! └── {query_id}.md
//! ```
//!
//! Each `{query_id}.md` holds the plain top-K recall hits (control group)
//! side by side with the same hits passed through
//! `RelationAnnotator::annotate_single` (annotation enabled), plus the
//! `AnnotationMetadata` so reviewers can tell "no effect" apart from
//! "annotation switched off". When expansion is enabled, one call-graph hop
//! (callees first, then callers) is resolved from the benchmark's
//! `call_edges` sidecar, hard-filtered by the job's path policy
//! (`ReviewFilterOptions`, applied before the annotator's expansion budget
//! cap), and attached to every hit. See
//! `docs/plan/tests/annotation-review-export-design.md`.
//!
//! Only `full_pipeline` snapshots are consumed: the other baselines carry
//! raw source slices with no entity structure and no call edges, so
//! annotation is meaningless on them. The baseline therefore appears nowhere
//! in the output paths.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use cce_orchestrator::query::annotation::{
    AnnotatedResult, ExpandedUnit, ExpansionOrigin, RelationAnnotationConfig, RelationAnnotator,
    SearchResultInput,
};
use cce_types::{EntityId, RelationType};

use crate::bench_data::{BenchmarkData, ChunkData, cosine_similarity, load_benchmark_data};
use crate::bench_gen::fixture_source_path;
use crate::judgments::evaluate::BenchmarkPaths;
use crate::review_filter::ReviewFilterOptions;
use crate::{FixtureSpec, OutputCategory, OutputManager};

/// Which text the annotator consumes for a hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentMode {
    /// Use the benchmark chunk text (`extract_unit_from_content` path).
    /// Fully offline and reproducible.
    Chunk,
    /// Read the real source file under the fixture root and slice the
    /// chunk's absolute line range (same extractor entry point, richer unit).
    Source,
}

impl ContentMode {
    fn label(self) -> &'static str {
        match self {
            Self::Chunk => "chunk",
            Self::Source => "source",
        }
    }
}

/// Which recall path feeds the review. First cut implements embedding only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallMode {
    Emb,
}

impl RecallMode {
    fn label(self) -> &'static str {
        match self {
            Self::Emb => "emb",
        }
    }
}

/// Benchmark snapshot consumed by the review. Fixed: only `full_pipeline`
/// carries NL-converted chunk text with entity structure; the other
/// baselines hold raw source slices where annotation is meaningless.
const BASELINE: &str = "full_pipeline";

/// One annotation-review export job.
pub struct AnnotationReviewConfig {
    /// Benchmark project name, also the rkyv directory and output fixture
    /// name (e.g. `"once_cell"`, `"flask"`).
    pub project: &'static str,
    /// Language directory under `outputs/scenarios/` (e.g. `"rust"`).
    pub language: &'static str,
    /// Fixture used to resolve real source files in `ContentMode::Source`.
    pub spec: FixtureSpec,
    /// Number of recall hits rendered per query.
    pub top_k: usize,
    pub content_mode: ContentMode,
    pub recall: RecallMode,
    /// Attach one call-graph hop (callees + callers) from the benchmark's
    /// `call_edges` sidecar to every hit. Requires a snapshot generated
    /// with the call-edge sidecar.
    pub expansion: bool,
    /// Path policy applied to expansion neighbors before they reach the
    /// annotator's expansion budget cap. Mirrors the main query path's file
    /// filters; default (`ReviewFilterOptions::default()`) keeps the review
    /// scope that includes `tests/` hits.
    pub filter: ReviewFilterOptions,
}

/// Call-graph adjacency over the benchmark entity-id space (`i64` mirrors
/// `ChunkData::entity_ids`).
struct CallGraph {
    /// caller entity id -> edge indices (forward: who calls whom)
    forward: HashMap<i64, Vec<usize>>,
    /// callee entity id -> edge indices (backward: who is called by whom)
    backward: HashMap<i64, Vec<usize>>,
    /// entity id -> embedding chunk index; primary entity of a chunk wins
    chunk_of_entity: HashMap<i64, usize>,
}

impl CallGraph {
    fn build(bench: &BenchmarkData) -> Self {
        let mut forward: HashMap<i64, Vec<usize>> = HashMap::new();
        let mut backward: HashMap<i64, Vec<usize>> = HashMap::new();
        for (idx, edge) in bench.call_edges.iter().enumerate() {
            forward.entry(edge.caller_entity_id).or_default().push(idx);
            backward.entry(edge.callee_entity_id).or_default().push(idx);
        }
        let mut chunk_of_entity: HashMap<i64, usize> = HashMap::new();
        for (idx, chunk) in bench.embedding.chunks.iter().enumerate() {
            if let Some(&primary) = chunk.entity_ids.first() {
                chunk_of_entity.entry(primary).or_insert(idx);
            }
        }
        for (idx, chunk) in bench.embedding.chunks.iter().enumerate() {
            for &id in chunk.entity_ids.iter().skip(1) {
                chunk_of_entity.entry(id).or_insert(idx);
            }
        }
        Self {
            forward,
            backward,
            chunk_of_entity,
        }
    }
}

/// One scored recall hit: embedding chunk index plus cosine score.
struct ScoredHit {
    chunk_idx: usize,
    score: f64,
}

/// Soft score multiplier applied to test-code hits so source-oriented queries
/// surface `src/` first without hard-dropping tests (still valid evidence).
const TEST_SCORE_FACTOR: f64 = 0.85;

/// Soft score multiplier for chunks with no content entity ids (class-shell /
/// segment-only text) so thin high-similarity shells do not occupy top slots.
const EMPTY_ENTITY_SCORE_FACTOR: f64 = 0.90;

/// Minimum same-file `source_ranges` overlap (vs. the smaller range set) for a
/// later hit to be treated as a near-duplicate of an earlier one.
const SPAN_OVERLAP_THRESHOLD: f64 = 0.5;

/// Total covered lines across a chunk's precise source ranges.
fn covered_lines(ranges: &[crate::bench_data::ChunkSourceRange]) -> usize {
    ranges
        .iter()
        .filter(|r| r.end_line >= r.start_line)
        .map(|r| r.end_line - r.start_line + 1)
        .sum()
}

/// Overlap ratio of `candidate` against `selected` in the same file: shared
/// lines divided by the smaller range set. Empty ranges fall back to the
/// navigation span so chunks without precise coverage still dedup.
fn span_overlap_ratio(a: &ChunkData, b: &ChunkData) -> f64 {
    if a.file_path != b.file_path {
        return 0.0;
    }
    let nav = |c: &ChunkData| -> Vec<crate::bench_data::ChunkSourceRange> {
        if c.start_line == 0 {
            Vec::new()
        } else {
            vec![crate::bench_data::ChunkSourceRange {
                start_line: c.start_line,
                end_line: c.end_line,
            }]
        }
    };
    let a_ranges: Vec<_> = if a.source_ranges.is_empty() {
        nav(a)
    } else {
        a.source_ranges.clone()
    };
    let b_ranges: Vec<_> = if b.source_ranges.is_empty() {
        nav(b)
    } else {
        b.source_ranges.clone()
    };
    if a_ranges.is_empty() || b_ranges.is_empty() {
        return 0.0;
    }
    let shared: usize = a_ranges
        .iter()
        .flat_map(|ar| {
            b_ranges
                .iter()
                .filter(move |br| ar.start_line <= br.end_line && br.start_line <= ar.end_line)
                .map(move |br| {
                    let lo = ar.start_line.max(br.start_line);
                    let hi = ar.end_line.min(br.end_line);
                    hi.saturating_sub(lo) + 1
                })
        })
        .sum();
    let denom = covered_lines(&a_ranges).min(covered_lines(&b_ranges));
    if denom == 0 {
        0.0
    } else {
        shared as f64 / denom as f64
    }
}

/// Apply soft penalties for test chunks and empty-entity shells, then collapse
/// same-file near-duplicate spans by demoting later hits among the head of the
/// ranking (where top-K is drawn), and re-sort.
fn apply_recall_penalties(bench: &BenchmarkData, ranked: &mut [Vec<ScoredHit>], top_k: usize) {
    // Only the head of the ranking can occupy top-K; pairwise overlap there is
    // enough and keeps the pass near-linear in the working set.
    let head = (top_k.saturating_mul(4)).max(top_k).min(64);
    for hits in ranked.iter_mut() {
        for hit in hits.iter_mut() {
            let chunk = &bench.embedding.chunks[hit.chunk_idx];
            if chunk.test_info.is_test() {
                hit.score *= TEST_SCORE_FACTOR;
            }
            if chunk.entity_ids.is_empty() {
                hit.score *= EMPTY_ENTITY_SCORE_FACTOR;
            }
        }
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));

        let n = hits.len().min(head);
        for i in 0..n {
            for j in (i + 1)..n {
                let (earlier, later) = (hits[i].chunk_idx, hits[j].chunk_idx);
                let ratio = span_overlap_ratio(
                    &bench.embedding.chunks[earlier],
                    &bench.embedding.chunks[later],
                );
                if ratio >= SPAN_OVERLAP_THRESHOLD {
                    hits[j].score *= 0.75;
                }
            }
        }
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    }
}

/// Rank every query against the embedding chunks with cosine similarity.
///
/// Mirrors the embedding half of `retrieval_method::rank_recall` without the
/// BM25 side (review needs no BM25 scoring yet).
fn rank_embedding(bench: &BenchmarkData) -> Vec<Vec<ScoredHit>> {
    let dim = bench.embedding.dimension as usize;
    let n_chunks = if dim > 0 {
        bench
            .embedding
            .vectors
            .len()
            .checked_div(dim)
            .unwrap_or(0)
            .min(bench.embedding.chunks.len())
            .min(bench.embedding.texts.len())
    } else {
        0
    };
    (0..bench.queries.len())
        .map(|q_idx| {
            let Some(q_vec) = bench
                .embedding
                .query_vectors
                .get(q_idx * dim..(q_idx + 1) * dim)
                .filter(|_| dim > 0)
            else {
                return Vec::new();
            };
            let mut ranked: Vec<ScoredHit> = (0..n_chunks)
                .map(|c_idx| {
                    let c_vec = &bench.embedding.vectors[c_idx * dim..(c_idx + 1) * dim];
                    ScoredHit {
                        chunk_idx: c_idx,
                        score: cosine_similarity(c_vec, q_vec),
                    }
                })
                .collect();
            ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
            ranked
        })
        .collect()
}

/// Annotator content plus the line window and display path for one hit.
struct HitContent {
    content: String,
    start_line: u32,
    end_line: u32,
    /// Path handed to the annotator. Absolute in source mode so the
    /// file-coverage replacement can read the file; the benchmark-relative
    /// path is always used for display.
    annotation_path: String,
}

/// Resolve the text the annotator consumes for a hit.
fn resolve_hit_content(
    chunk: &ChunkData,
    chunk_text: &str,
    mode: ContentMode,
    fixture_root: &Path,
) -> Result<HitContent, String> {
    match mode {
        ContentMode::Chunk => {
            let line_count = chunk_text.lines().count() as u32;
            if line_count == 0 {
                return Err("chunk text is empty".to_string());
            }
            Ok(HitContent {
                content: chunk_text.to_string(),
                start_line: 1,
                end_line: line_count,
                annotation_path: chunk.file_path.clone(),
            })
        }
        ContentMode::Source => {
            let abs = fixture_root.join(&chunk.file_path);
            let content = std::fs::read_to_string(&abs)
                .map_err(|e| format!("read {}: {e}", abs.display()))?;
            let line_count = content.lines().count() as u32;
            if line_count == 0 {
                return Err(format!("source file {} is empty", chunk.file_path));
            }
            let start = chunk.start_line as u32;
            let end = chunk.end_line as u32;
            if start == 0 || end == 0 || start > end || end > line_count {
                return Err(format!(
                    "chunk range {start}-{end} outside {} ({} lines)",
                    chunk.file_path, line_count
                ));
            }
            Ok(HitContent {
                content,
                start_line: start,
                end_line: end,
                annotation_path: abs.to_string_lossy().replace('\\', "/"),
            })
        }
    }
}

/// Build the annotator input for one hit.
fn build_annotation_input(chunk: &ChunkData, hit: &HitContent, score: f64) -> SearchResultInput {
    SearchResultInput {
        id: chunk.chunk_id.clone(),
        entity_id: chunk.entity_ids.first().map(|&id| EntityId(id as u64)),
        name: chunk.entity_name.clone(),
        kind: String::new(),
        file_path: hit.annotation_path.clone(),
        start_line: hit.start_line,
        end_line: hit.end_line,
        content: hit.content.clone(),
        score: score as f32,
    }
}

/// Resolve one call-graph neighbour as an expansion unit.
///
/// The neighbour is rendered from its own embedding chunk: NL text with
/// 1-based remapped lines in chunk mode, absolute source lines in source
/// mode. Neighbours without a chunk, excluded by the job's path filter, or
/// already present in the query's top-K hits are skipped (the annotator
/// dedups and caps the rest). The path filter runs here so excluded
/// neighbours never occupy an expansion budget slot.
///
/// The unit carries the primary hit score (ties keep caller order under the
/// annotator's stable score sort), the benchmark edge's relation type parsed
/// to a call-domain default, and explicit non-stdlib / non-external marks so
/// the annotator's noise filters treat benchmark neighbours as workspace
/// calls.
#[allow(clippy::too_many_arguments)]
fn neighbor_unit(
    bench: &BenchmarkData,
    graph: &CallGraph,
    source_cache: &mut HashMap<String, Option<Vec<String>>>,
    neighbor_id: i64,
    origin: ExpansionOrigin,
    edge_label: &str,
    relation_type: &str,
    score: f32,
    top_chunk_ids: &HashSet<String>,
    mode: ContentMode,
    fixture_root: &Path,
    filter: &ReviewFilterOptions,
) -> Option<ExpandedUnit> {
    let chunk_idx = *graph.chunk_of_entity.get(&neighbor_id)?;
    let chunk = &bench.embedding.chunks[chunk_idx];
    if filter.is_excluded_path(&chunk.file_path) {
        return None;
    }
    if top_chunk_ids.contains(&chunk.chunk_id) {
        return None;
    }
    let (code, start_line, end_line, file_path) = match mode {
        ContentMode::Chunk => {
            let text = bench.embedding.texts.get(chunk_idx).map_or("", |t| t);
            let line_count = text.lines().count() as u32;
            if line_count == 0 {
                return None;
            }
            (text.to_string(), 1, line_count, chunk.file_path.clone())
        }
        ContentMode::Source => {
            let lines = source_cache
                .entry(chunk.file_path.clone())
                .or_insert_with(|| {
                    std::fs::read_to_string(fixture_root.join(&chunk.file_path))
                        .map(|content| content.lines().map(String::from).collect())
                        .ok()
                })
                .as_ref()?;
            let start = chunk.start_line.max(1);
            let end = chunk.end_line.min(lines.len());
            if end < start {
                return None;
            }
            let code = lines[start - 1..end].join("\n");
            let abs = fixture_root
                .join(&chunk.file_path)
                .to_string_lossy()
                .replace('\\', "/");
            (code, start as u32, end as u32, abs)
        }
    };
    let name = display_name(chunk).to_string();
    let relation = relation_type
        .parse::<RelationType>()
        .unwrap_or(RelationType::DirectCall);
    Some(
        ExpandedUnit::new(code, file_path, start_line, end_line, name)
            .with_entity_id(EntityId(neighbor_id as u64))
            .with_expansion(origin, edge_label)
            .with_relation_type(relation)
            .with_score(score)
            .with_stdlib(false)
            .with_external(false),
    )
}

/// Resolve the one-hop forward (callee) and backward (caller) expansion
/// units for one hit chunk.
///
/// The path filter is applied per neighbour before the units are returned,
/// i.e. before `RelationAnnotator` applies its expansion budget cap.
/// Neighbours inherit the primary hit score.
#[allow(clippy::too_many_arguments)]
fn resolve_expansion(
    bench: &BenchmarkData,
    graph: &CallGraph,
    chunk: &ChunkData,
    primary_score: f32,
    top_chunk_ids: &HashSet<String>,
    mode: ContentMode,
    fixture_root: &Path,
    source_cache: &mut HashMap<String, Option<Vec<String>>>,
    filter: &ReviewFilterOptions,
) -> (Vec<ExpandedUnit>, Vec<ExpandedUnit>) {
    let Some(&primary_id) = chunk.entity_ids.first() else {
        return (Vec::new(), Vec::new());
    };
    let mut forward = Vec::new();
    let mut backward = Vec::new();
    if let Some(edges) = graph.forward.get(&primary_id) {
        for &edge_idx in edges {
            let edge = &bench.call_edges[edge_idx];
            if let Some(unit) = neighbor_unit(
                bench,
                graph,
                source_cache,
                edge.callee_entity_id,
                ExpansionOrigin::Forward,
                "calls",
                &edge.relation_type,
                primary_score,
                top_chunk_ids,
                mode,
                fixture_root,
                filter,
            ) {
                forward.push(unit);
            }
        }
    }
    if let Some(edges) = graph.backward.get(&primary_id) {
        for &edge_idx in edges {
            let edge = &bench.call_edges[edge_idx];
            if let Some(unit) = neighbor_unit(
                bench,
                graph,
                source_cache,
                edge.caller_entity_id,
                ExpansionOrigin::Backward,
                "called by",
                &edge.relation_type,
                primary_score,
                top_chunk_ids,
                mode,
                fixture_root,
                filter,
            ) {
                backward.push(unit);
            }
        }
    }
    (forward, backward)
}

/// Rendered outcome for one hit in the `annotated/` tree.
enum AnnotatedOutcome {
    Annotated(Box<AnnotatedResult>),
    Fallback { note: String, raw: String },
}

fn sanitize_query_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn entity_ids_list(chunk: &ChunkData) -> String {
    chunk
        .entity_ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn display_name(chunk: &ChunkData) -> &str {
    if chunk.entity_name.is_empty() {
        &chunk.chunk_id
    } else {
        &chunk.entity_name
    }
}

fn render_hit_header(rank: usize, chunk: &ChunkData, score: f64) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "### {}. {}  score={:.4}\n\n",
        rank + 1,
        display_name(chunk),
        score
    ));
    out.push_str(&format!(
        "- file: `{}:{}-{}`\n",
        chunk.file_path, chunk.start_line, chunk.end_line
    ));
    out.push_str(&format!(
        "- entity_ids: [{}], segment_id: {}\n",
        entity_ids_list(chunk),
        if chunk.segment_id.is_empty() {
            "(none)"
        } else {
            &chunk.segment_id
        }
    ));
    out.push('\n');
    out
}

fn render_query_header(
    query_id: &str,
    query_text: &str,
    query_type: &str,
    mode_label: &str,
    content_label: &str,
) -> String {
    format!(
        "# Query: {query_id}\n\n- **Query**: {query_text}\n- **Type**: {query_type}\n- **Mode**: {mode_label}\n- **Content**: {content_label}\n\n## Results\n\n"
    )
}

/// Run the review: offline recall, per-query annotation, markdown export.
pub async fn run_annotation_review(config: AnnotationReviewConfig) -> anyhow::Result<()> {
    let paths = BenchmarkPaths::new(config.project);
    let data_path = paths.data_dir(BASELINE);
    if !data_path.exists() {
        anyhow::bail!(
            "benchmark data not found at {}; run the matching gen_bench example first",
            data_path.display()
        );
    }
    let bench = load_benchmark_data(&data_path).map_err(anyhow::Error::msg)?;
    println!(
        "Loaded {} queries, {} embedding chunks from {}",
        bench.queries.len(),
        bench.embedding.chunks.len(),
        data_path.display()
    );

    // Annotation must be enabled: the default config takes the verbatim
    // shortcut, which would make every annotated file identical to raw.
    // The workspace root enables existence checks against the fixture tree
    // so vanished files render as references instead of ghost source.
    let fixture_root = fixture_source_path(config.spec.clone());
    let mut annotation_config = RelationAnnotationConfig::new()
        .enable(true)
        .with_workspace_root(fixture_root.clone());
    if config.expansion {
        annotation_config = annotation_config.with_expansion(true);
    }
    let annotator = RelationAnnotator::new(annotation_config);
    let graph = CallGraph::build(&bench);
    if config.expansion && bench.call_edges.is_empty() {
        println!(
            "warning: expansion requested but {BASELINE}/{} snapshot carries no call edges; \
             regenerate the benchmark data with the current gen_bench example",
            config.project
        );
    }
    let mut ranked = rank_embedding(&bench);
    apply_recall_penalties(&bench, &mut ranked, config.top_k);
    let mut source_cache: HashMap<String, Option<Vec<String>>> = HashMap::new();

    let base = OutputManager::builder()
        .category(OutputCategory::Scenarios)
        .language(config.language)
        .scenario(format!("annotation/{}", config.project))
        .build();
    let base_dir = base.ensure_output_dir()?;

    let mode_label = format!("{}_top{}", config.recall.label(), config.top_k);
    let mut index_rows = Vec::new();

    for (q_idx, query) in bench.queries.iter().enumerate() {
        let top: Vec<&ScoredHit> = ranked
            .get(q_idx)
            .map(|hits| hits.iter().take(config.top_k).collect())
            .unwrap_or_default();
        let top_chunk_ids: HashSet<String> = top
            .iter()
            .map(|hit| bench.embedding.chunks[hit.chunk_idx].chunk_id.clone())
            .collect();

        let mut annotated_md = render_query_header(
            &query.id,
            &query.text,
            &query.query_type.to_string(),
            &mode_label,
            config.content_mode.label(),
        );
        annotated_md.push_str(
            "> Scope: primary recall includes `tests/` hits (soft score demotion only, \
             `export_py` exclude_tests does not apply); expansion neighbors are \
             hard-filtered by the job's path filter (default: none).\n\n",
        );

        for (rank, hit) in top.iter().enumerate() {
            let chunk = &bench.embedding.chunks[hit.chunk_idx];
            let chunk_text = bench.embedding.texts.get(hit.chunk_idx).map_or("", |t| t);

            annotated_md.push_str(&render_hit_header(rank, chunk, hit.score));
            annotated_md.push_str("#### Content (raw)\n\n");
            annotated_md.push_str(chunk_text);
            annotated_md.push_str("\n\n#### Content (annotated)\n\n");

            let outcome =
                match resolve_hit_content(chunk, chunk_text, config.content_mode, &fixture_root) {
                    Ok(resolved) => {
                        let input = build_annotation_input(chunk, &resolved, hit.score);
                        let (forward, backward) = if config.expansion {
                            resolve_expansion(
                                &bench,
                                &graph,
                                chunk,
                                hit.score as f32,
                                &top_chunk_ids,
                                config.content_mode,
                                &fixture_root,
                                &mut source_cache,
                                &config.filter,
                            )
                        } else {
                            (Vec::new(), Vec::new())
                        };
                        match annotator.annotate_single(input, forward, backward).await {
                            Ok(result) => AnnotatedOutcome::Annotated(Box::new(result)),
                            Err(e) => AnnotatedOutcome::Fallback {
                                note: format!("annotation failed ({e}); content identical to raw."),
                                raw: chunk_text.to_string(),
                            },
                        }
                    }
                    Err(note) => AnnotatedOutcome::Fallback {
                        note: format!("{note}; content identical to raw."),
                        raw: chunk_text.to_string(),
                    },
                };
            match outcome {
                AnnotatedOutcome::Annotated(result) => {
                    let meta = &result.metadata;
                    annotated_md.push_str(&format!(
                        "- annotation: expanded={}, expanded_nodes={} (fwd={}, bwd={}), files={}, original_length={}, annotated_length={}, truncated={}\n",
                        meta.expanded,
                        meta.expanded_nodes,
                        meta.forward_nodes,
                        meta.backward_nodes,
                        meta.file_count,
                        meta.original_length,
                        meta.annotated_length,
                        meta.truncated,
                    ));
                    if !meta.expanded {
                        annotated_md.push_str("> not expanded — content identical to raw.\n");
                    }
                    annotated_md.push('\n');
                    annotated_md.push_str(&result.annotated_content);
                    annotated_md.push_str("\n\n");
                }
                AnnotatedOutcome::Fallback { note, raw } => {
                    annotated_md.push_str(&format!("> {note}\n\n{raw}\n\n"));
                }
            }
        }

        let file_name = format!("{}.md", sanitize_query_id(&query.id));
        std::fs::write(base_dir.join(&file_name), &annotated_md)?;

        let top1 = top.first().map(|hit| {
            let chunk = &bench.embedding.chunks[hit.chunk_idx];
            format!(
                "{}:{}-{} ({:.4})",
                chunk.file_path, chunk.start_line, chunk.end_line, hit.score
            )
        });
        index_rows.push((query, top1));
    }

    let mut index = format!(
        "# Annotation review: {} ({}, {}, content={}, expansion={})\n\n\
         > Scope: primary recall includes `tests/` hits; the `exclude_tests` filter in \
         `export_py` applies only to written summary/chunk/structured reports, not to \
         annotation review recall. Expansion neighbors are hard-filtered by the job's \
         path filter before the annotator budget cap (default: none).\n\n\
         | query_id | type | query | top-1 hit |\n|---|---|---|---|\n",
        config.project,
        BASELINE,
        mode_label,
        config.content_mode.label(),
        if config.expansion { "on" } else { "off" }
    );
    for (query, top1) in &index_rows {
        index.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            query.id,
            query.query_type,
            query.text.replace('|', "\\|"),
            top1.as_deref().unwrap_or("(no hits)")
        ));
    }
    std::fs::write(base_dir.join("index.md"), &index)?;

    println!(
        "Annotation review written for {} queries -> {}",
        bench.queries.len(),
        base_dir.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench_data::{CallEdgeData, ChunkSourceRange, RetrieverDataset};
    use cce_types::TestInfo;

    fn chunk(chunk_id: &str, file_path: &str, entity_id: i64) -> ChunkData {
        ChunkData {
            chunk_id: chunk_id.to_string(),
            entity_name: chunk_id.to_string(),
            file_path: file_path.to_string(),
            start_line: 1,
            end_line: 3,
            source_ranges: Vec::<ChunkSourceRange>::new(),
            source_span_kind: String::new(),
            test_info: TestInfo::unknown(),
            language: None,
            entity_ids: vec![entity_id],
            segment_id: String::new(),
        }
    }

    fn empty_dataset() -> RetrieverDataset {
        RetrieverDataset {
            chunks: Vec::new(),
            texts: Vec::new(),
            vectors: Vec::new(),
            query_vectors: Vec::new(),
            dimension: 0,
        }
    }

    /// Primary entity 1 in `src/lib.rs` calls entity 2 in `tests/lib_test.rs`.
    fn bench_with_test_callee() -> BenchmarkData {
        BenchmarkData {
            queries: Vec::new(),
            query_texts: Vec::new(),
            embedding: RetrieverDataset {
                chunks: vec![
                    chunk("c0", "src/lib.rs", 1),
                    chunk("c1", "tests/lib_test.rs", 2),
                ],
                texts: vec![
                    "fn primary() {}".to_string(),
                    "fn neighbor() {}".to_string(),
                ],
                vectors: Vec::new(),
                query_vectors: Vec::new(),
                dimension: 0,
            },
            bm25: empty_dataset(),
            bm25_documents: Vec::new(),
            call_edges: vec![CallEdgeData {
                caller_entity_id: 1,
                callee_entity_id: 2,
                callee_name: "neighbor".to_string(),
                callee_file: "tests/lib_test.rs".to_string(),
                callee_start_line: 1,
                callee_end_line: 3,
                relation_type: "call.direct".to_string(),
            }],
        }
    }

    fn forward_neighbors(filter: &ReviewFilterOptions) -> Vec<ExpandedUnit> {
        let bench = bench_with_test_callee();
        let graph = CallGraph::build(&bench);
        let primary = &bench.embedding.chunks[0];
        let top_chunk_ids = HashSet::from([primary.chunk_id.clone()]);
        let mut source_cache = HashMap::new();
        let (forward, _) = resolve_expansion(
            &bench,
            &graph,
            primary,
            0.9,
            &top_chunk_ids,
            ContentMode::Chunk,
            Path::new("."),
            &mut source_cache,
            filter,
        );
        forward
    }

    #[test]
    fn default_filter_keeps_test_neighbor() {
        let forward = forward_neighbors(&ReviewFilterOptions::default());
        assert_eq!(forward.len(), 1);
        assert_eq!(forward[0].name, "c1");
        // Review neighbours satisfy the annotator's noise-filter semantics.
        assert!((forward[0].score - 0.9).abs() < f32::EPSILON);
        assert!(forward[0].is_call_domain());
        assert!(!forward[0].is_stdlib);
        assert!(!forward[0].is_external);
    }

    #[test]
    fn exclude_tests_drops_test_neighbor_before_budget_cap() {
        let filter = ReviewFilterOptions::default().with_exclude_tests(true);
        assert!(forward_neighbors(&filter).is_empty());
    }

    #[test]
    fn excluded_files_match_neighbor_path_exactly() {
        let filter = ReviewFilterOptions::default()
            .with_excluded_files(vec!["tests/lib_test.rs".to_string()]);
        assert!(forward_neighbors(&filter).is_empty());

        let other =
            ReviewFilterOptions::default().with_excluded_files(vec!["src/other.rs".to_string()]);
        assert_eq!(forward_neighbors(&other).len(), 1);
    }
}
