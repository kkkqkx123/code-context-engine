//! Relation-graph visualization export for the real relation pipeline.
//!
//! Seeds are resolved deterministically against the indexed snapshot: every
//! judgment's first strong expectation range contributes its host entity
//! (via line-range lookup) and the caller-supplied hub names contribute
//! `get_function_ids_by_name` matches. For every seed this job runs the
//! production relation and graph queries (`get_callees`, `get_callers`,
//! forward/backward call chains, ego graph) and renders one self-contained
//! markdown file plus a machine-readable graph snapshot under:
//!
//! ```text
//! outputs/scenarios/{lang}/relation_review/{project}/
//! ├── run_manifest.txt
//! ├── index.md
//! ├── entity/{seed_slug}.md
//! └── graph/{seed_slug}.json
//! ```
//!
//! Relation queries need no vector backend, so this export runs fully
//! offline on BM25-only sources. Outputs are for manual review only and
//! never participate in assertions.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Context;
use cce_codegraph::index::snapshot_query::SnapshotSymbolQueryOps;
use cce_codegraph::index::{SnapshotEntityQueryOps, SnapshotRelationQueryOps};
use cce_llm_client::OpenAICompatibleProvider;
use cce_orchestrator::SearchSources;
use cce_orchestrator::query::ExcludableContentType;
use cce_orchestrator::query::graph::{GraphDirection, GraphEdge, GraphService, SubGraph};
use cce_orchestrator::query::relation_searcher::RelationQueryOptions;
use cce_orchestrator::query::types::query_options::path_excluded_by_types;
use cce_types::{EntityId, ResolvedRelation};

use crate::bench_data::{QueryType, RelevanceJudgment, RelevanceLevel};
use crate::embedding::EmbeddingConfig;
use crate::fixture::{FixtureAccess, FixtureSpec, TestFixture};
use crate::mock_embedding_server::{MockEmbeddingServer, mock_embedding_config};
use crate::output_manager::{OutputBuilder, OutputCategory};
use crate::query_test::QueryWorkflowTest;

/// One relation-review export job over a single fixture.
pub struct RelationReviewConfig {
    /// Benchmark project name, also the output directory name.
    pub project: &'static str,
    /// Language directory under `outputs/scenarios/`.
    pub language: &'static str,
    /// Fixture holding the indexed corpus.
    pub spec: FixtureSpec,
    /// Judgment set used to derive seed entities from expectation ranges.
    pub judgments: Vec<RelevanceJudgment>,
    /// Hub entity names resolved via the snapshot function index.
    pub hub_names: Vec<&'static str>,
    /// Maximum number of seeds rendered.
    pub max_seeds: usize,
    /// Traversal depth for call chains and ego graphs.
    pub depth: usize,
    /// Per-direction neighbor limit.
    pub limit: usize,
}

/// A resolved seed entity with its provenance.
struct Seed {
    id: EntityId,
    name: String,
    provenance: String,
    /// Cross-run join key; `None` when the entity has no registered symbol key.
    stable_id: Option<String>,
}

/// Rendered per-seed outcome kept for the index page.
struct SeedOutcome {
    slug: String,
    name: String,
    provenance: String,
    callees: usize,
    callers: usize,
    forward: usize,
    backward: usize,
    ego_nodes: usize,
    ego_edges: usize,
}

/// Run the full relation-review export.
pub async fn run_relation_review(config: RelationReviewConfig) -> anyhow::Result<()> {
    let manager = OutputBuilder::new()
        .category(OutputCategory::Scenarios)
        .language(config.language)
        .scenario(format!("relation_review/{}", config.project))
        .build();
    manager.clean().ok();

    let server = MockEmbeddingServer::start().await;
    let embedder = Arc::new(
        OpenAICompatibleProvider::from_model(&mock_embedding_config(&server.base_url), "mock")
            .context("mock embedder")?,
    );

    let fixture = TestFixture::load(config.spec.clone()).context("load review fixture")?;
    let mut query_test = QueryWorkflowTest::new(fixture, EmbeddingConfig::mock())
        .with_embedder(embedder)
        .with_sources(SearchSources::none().with_bm25());
    let index_result = query_test.index().await.context("index fixture")?.clone();
    query_test
        .ensure_coordinator()
        .await
        .context("init query coordinator")?;

    let (seeds, unprocessed_hubs) = resolve_seeds(&query_test, &config);
    let include_tests = std::env::var("CCE_RELATION_REVIEW_INCLUDE_TESTS")
        .is_ok_and(|value| !value.is_empty() && value != "0");
    let relation_options = RelationQueryOptions {
        max_depth: config.depth,
        limit: config.limit,
        exclude_content_types: if include_tests {
            Vec::new()
        } else {
            vec![ExcludableContentType::Test]
        },
        ..RelationQueryOptions::default()
    };

    let mut outcomes = Vec::new();
    let mut skipped = Vec::new();
    for seed in seeds {
        let coordinator = query_test.coordinator();
        let seed_name = seed.name.clone();
        let describe = |id: EntityId| describe_entity(coordinator, id);

        let callees: Vec<ResolvedRelation> = coordinator
            .get_callees(seed.id)
            .unwrap_or_default()
            .into_iter()
            .filter(is_call_view_edge)
            .collect();
        let caller_relations: Vec<ResolvedRelation> = coordinator
            .get_callers_paginated(seed.id, &relation_options)
            .unwrap_or_default()
            .into_iter()
            .filter(is_call_view_edge)
            .collect();
        let forward: Vec<cce_codegraph::CallChainNode> = coordinator
            .query_forward(seed.id, &relation_options)
            .unwrap_or_default()
            .into_iter()
            .filter(|node| node.relation_type != cce_types::RelationType::FunctionReference)
            .collect();
        let backward: Vec<cce_codegraph::CallChainNode> = coordinator
            .query_backward(seed.id, &relation_options)
            .unwrap_or_default()
            .into_iter()
            .filter(|node| node.relation_type != cce_types::RelationType::FunctionReference)
            .collect();
        let ego = GraphService::new(coordinator.relation_searcher_arc())
            .ego_graph(seed.id, config.depth, GraphDirection::Both)
            .unwrap_or_default();
        let ego = if include_tests {
            ego
        } else {
            exclude_test_nodes(ego)
        };
        let incoming = coordinator
            .relation_searcher()
            .query()
            .index()
            .get_relations_to_entity(seed.id);
        let references: Vec<ResolvedRelation> = incoming
            .into_iter()
            .filter(|relation| !relation.relation_type.is_call())
            .collect();
        let file_referrers = coordinator
            .relation_searcher()
            .query()
            .get_file_callers_of(seed.id);
        let seed_file = coordinator
            .relation_searcher()
            .query()
            .index()
            .get_file_path_by_entity(seed.id)
            .unwrap_or_default();

        let slug = seed_slug(&seed_name, seed.stable_id.as_deref());
        let folded_ego = fold_ego_edges(&ego.edges);
        let nested = nested_closure_entities(query_test.coordinator(), seed.id, &seed_file);
        let page = render_seed_page(
            &seed,
            &SeedPageData {
                seed_file: &seed_file,
                callees: &callees,
                callers: &caller_relations,
                references: &references,
                file_referrers: &file_referrers,
                forward: &forward,
                backward: &backward,
                ego: &ego,
                folded_ego: &folded_ego,
                nested: &nested,
            },
            &describe,
        );
        manager.write(&format!("entity/{slug}.md"), &page)?;
        manager.write(
            &format!("graph/{slug}.json"),
            &serde_json::to_string_pretty(&SeedGraphJson::from_ego(&seed, &ego))
                .context("serialize seed graph")?,
        )?;
        outcomes.push(SeedOutcome {
            slug,
            name: seed_name,
            provenance: seed.provenance.clone(),
            callees: callees.len(),
            callers: caller_relations.len(),
            forward: forward.len(),
            backward: backward.len(),
            ego_nodes: ego.nodes.len(),
            ego_edges: folded_ego.len(),
        });
    }
    for hub in &config.hub_names {
        if unprocessed_hubs.iter().any(|name| name == hub) {
            skipped.push(format!(
                "hub `{hub}`: unprocessed (seed budget of {} exhausted)",
                config.max_seeds
            ));
        } else if !hub_resolved(&outcomes, hub) {
            skipped.push(format!("hub `{hub}`: no entity matched"));
        }
    }

    let export = GraphService::new(query_test.coordinator().relation_searcher_arc())
        .export_full(500)
        .unwrap_or_default();
    manager.write(
        "run_manifest.txt",
        &render_manifest(
            &config,
            &outcomes,
            &skipped,
            export.nodes.len(),
            export.edges.len(),
            &index_result,
            include_tests,
        ),
    )?;
    manager.write(
        "index.md",
        &render_index(&config, &outcomes, &skipped, include_tests),
    )?;

    println!(
        "Relation review written to {}",
        manager.output_dir().display()
    );
    query_test.cleanup().await;
    server.stop().await;
    Ok(())
}

/// Resolve seed entities from judgment ranges first, then hub names.
///
/// Returns the seeds plus the hub names never attempted because the seed
/// budget was already exhausted, so callers can tell `unprocessed` apart
/// from `no entity matched`.
fn resolve_seeds<F>(
    query_test: &QueryWorkflowTest<F>,
    config: &RelationReviewConfig,
) -> (Vec<Seed>, Vec<String>)
where
    F: FixtureAccess,
{
    let mut seeds = Vec::new();
    let mut dedup = SeedDedup::default();
    let mut unprocessed_hubs = Vec::new();

    let snapshot = query_test.coordinator().relation_searcher().query().index();
    for judgment in &config.judgments {
        if seeds.len() >= config.max_seeds {
            break;
        }
        if judgment.query_type != QueryType::Qualified {
            continue;
        }
        try_seed_judgment(snapshot, judgment, &mut dedup, &mut seeds);
    }

    for hub in &config.hub_names {
        if seeds.len() >= config.max_seeds {
            unprocessed_hubs.push(hub.to_string());
            continue;
        }
        // A hub whose name is already under review contributes nothing new:
        // expanding it would spend budget on a same-name duplicate instead
        // of an uncovered hub, so mark it covered and leave the slot free.
        if seeds.iter().any(|seed| seed.name.eq_ignore_ascii_case(hub)) {
            continue;
        }
        let mut ids: Vec<EntityId> = snapshot
            .get_function_ids_by_name(hub)
            .into_iter()
            .filter(|id| hub_candidate_ok(snapshot, *id))
            .collect();
        ids.sort_by(|a, b| {
            hub_degree(snapshot, *b)
                .cmp(&hub_degree(snapshot, *a))
                .then_with(|| a.0.cmp(&b.0))
        });
        for id in ids.into_iter().take(2) {
            if seeds.len() >= config.max_seeds {
                break;
            }
            let name = snapshot
                .get_function_by_entity_id(id)
                .map(|entity| entity.name.clone())
                .unwrap_or_else(|| hub.to_string());
            push_seed(
                snapshot,
                &mut dedup,
                &mut seeds,
                id,
                name,
                format!("hub `{hub}`"),
                stable_id_of(snapshot, id),
            );
        }
    }
    if seeds.len() < config.max_seeds {
        for judgment in &config.judgments {
            if seeds.len() >= config.max_seeds {
                break;
            }
            if judgment.query_type == QueryType::Qualified {
                continue;
            }
            try_seed_judgment(snapshot, judgment, &mut dedup, &mut seeds);
        }
    }
    (seeds, unprocessed_hubs)
}

/// Deduplication state for seed selection: selected entity ids plus the
/// exact file spans already covered (alias entities such as
/// `req.get = req.header` share one span and collapse to a single seed).
#[derive(Default)]
struct SeedDedup {
    seen: HashSet<u64>,
    seen_spans: HashSet<(String, u64, u64)>,
}

fn try_seed_judgment(
    snapshot: &cce_codegraph::index::snapshot_index::LayeredSnapshotIndex,
    judgment: &RelevanceJudgment,
    dedup: &mut SeedDedup,
    seeds: &mut Vec<Seed>,
) {
    let Some((range, _)) = judgment
        .relevant_ranges
        .iter()
        .find(|(_, level)| *level == RelevanceLevel::Strong)
        .or_else(|| judgment.relevant_ranges.first())
    else {
        return;
    };
    let query_start = range.start_line.saturating_sub(1);
    let query_end = range.end_line.saturating_sub(1);
    let ids = snapshot.get_entities_in_line_range(&range.file, query_start, query_end);
    if let Some(id) = pick_most_specific_entity(snapshot, &ids, range.start_line, range.end_line) {
        let name = snapshot
            .get_function_by_entity_id(id)
            .map(|entity| entity.name.clone())
            .unwrap_or_else(|| format!("entity-{}", id.0));
        push_seed(
            snapshot,
            dedup,
            seeds,
            id,
            name,
            format!(
                "judgment {} range {}:{}-{}",
                judgment.id, range.file, range.start_line, range.end_line
            ),
            stable_id_of(snapshot, id),
        );
    }
}

/// Ordering key for judgment-seed selection: covered rows first, then less
/// span excess, smaller size, and lower entity id for determinism.
type CoverageKey = (
    usize,
    core::cmp::Reverse<u64>,
    core::cmp::Reverse<u64>,
    core::cmp::Reverse<u64>,
);

/// Pick the entity covering the most judgment rows.
///
/// Judgment ranges name an implementation span, so the seed is the entity
/// covering the largest share of the range, usually the range's exact owner.
/// The previous smallest-inside rule resolved `app.use` to its nested
/// `mounted_app` closure and `createApplication` to its inner `app`
/// closure, hiding the judged methods from review. Ties break toward less
/// span excess, then smaller size, then lower entity id for determinism.
/// Function-like entities still win over type definitions so a range
/// straddling overloads resolves to a method, with a class chosen only
/// when no callable candidate exists.
fn pick_most_specific_entity(
    snapshot: &cce_codegraph::index::snapshot_index::LayeredSnapshotIndex,
    ids: &[EntityId],
    start_line: usize,
    end_line: usize,
) -> Option<EntityId> {
    let first_row = start_line.min(end_line).saturating_sub(1);
    let last_row = start_line.max(end_line).saturating_sub(1);
    for function_like in [true, false] {
        let mut best: Option<(CoverageKey, EntityId)> = None;
        for id in ids.iter().copied() {
            let Some(entity) = snapshot.get_function_by_entity_id(id) else {
                continue;
            };
            let is_fn = entity.kind.is_function_like();
            if is_fn != function_like {
                continue;
            }
            if !(is_fn || entity.kind.is_type_definition()) {
                continue;
            }
            let start = entity.span.start_position.row;
            let end = entity.span.end_position.row.max(start);
            if end < first_row || start > last_row {
                continue;
            }
            let overlap = end.min(last_row) - start.max(first_row) + 1;
            let excess = (end - start + 1 - overlap) as u64;
            let byte_len = entity.span.end_byte.saturating_sub(entity.span.start_byte) as u64;
            let key = (
                overlap,
                core::cmp::Reverse(excess),
                core::cmp::Reverse(byte_len),
                core::cmp::Reverse(id.0),
            );
            let replace = match &best {
                None => true,
                Some((current, _)) => key > *current,
            };
            if replace {
                best = Some((key, id));
            }
        }
        if let Some((_, id)) = best {
            return Some(id);
        }
    }
    None
}

/// Whether a hub-name match may become a review seed.
///
/// The name index also holds non-callable entities (an `examples/` variable
/// shadowing a router method, an import binding), which previously became
/// junk seeds. Hubs additionally stay within production paths, matching the
/// scope used for chains and ego graphs.
fn hub_candidate_ok(
    snapshot: &cce_codegraph::index::snapshot_index::LayeredSnapshotIndex,
    id: EntityId,
) -> bool {
    let Some(entity) = snapshot.get_function_by_entity_id(id) else {
        return false;
    };
    if !(entity.kind.is_function_like() || entity.kind.is_type_definition()) {
        return false;
    }
    let path = snapshot.get_file_path_by_entity(id).unwrap_or_default();
    if path.is_empty() {
        return false;
    }
    if path_excluded_by_types(&path, &[ExcludableContentType::Test]) {
        return false;
    }
    if path
        .split('/')
        .any(|segment| segment.eq_ignore_ascii_case("examples"))
    {
        return false;
    }
    true
}

/// Insert a seed unless its entity or its exact file span was selected.
///
/// Same-span deduplication collapses alias entities of one function (such as
/// `req.get = req.header`) into a single seed so redundant pages do not
/// consume the seed budget.
fn push_seed(
    snapshot: &cce_codegraph::index::snapshot_index::LayeredSnapshotIndex,
    dedup: &mut SeedDedup,
    seeds: &mut Vec<Seed>,
    id: EntityId,
    name: String,
    provenance: String,
    stable_id: Option<String>,
) {
    if !dedup.seen.insert(id.0) {
        return;
    }
    if let Some(entity) = snapshot.get_function_by_entity_id(id) {
        let file = snapshot.get_file_path_by_entity(id).unwrap_or_default();
        let span_key = (
            file,
            entity.span.start_byte as u64,
            entity.span.end_byte as u64,
        );
        if !dedup.seen_spans.insert(span_key) {
            return;
        }
    }
    seeds.push(Seed {
        id,
        name,
        provenance,
        stable_id,
    });
}

/// Hub candidate connectivity used to surface the most-linked entities.
fn hub_degree(
    snapshot: &cce_codegraph::index::snapshot_index::LayeredSnapshotIndex,
    id: EntityId,
) -> usize {
    let outgoing = snapshot
        .get_resolved_relations_by_caller(id)
        .map(|relations| relations.len())
        .unwrap_or(0);
    outgoing + snapshot.get_callers_by_callee_entity(id).len()
}

/// Stable symbol id for cross-run joins, when the entity has one.
fn stable_id_of(
    snapshot: &cce_codegraph::index::snapshot_index::LayeredSnapshotIndex,
    id: EntityId,
) -> Option<String> {
    snapshot
        .get_symbol_key_by_entity_id(id)
        .map(|key| key.stable_id().0)
}

/// Whether a hub name contributed at least one rendered seed.
///
/// Provenance is recorded at seed time, so the check reads it directly
/// instead of reverse-looking-up the slug suffix. A same-named judgment
/// seed also counts: the entity the hub would have contributed is already
/// under review, so the hub must not be reported as unmatched or starved.
fn hub_resolved(outcomes: &[SeedOutcome], hub: &str) -> bool {
    let wanted = format!("hub `{hub}`");
    outcomes
        .iter()
        .any(|outcome| outcome.provenance == wanted || outcome.name.eq_ignore_ascii_case(hub))
}

/// One-line human description of an entity id for neighbor tables.
fn describe_entity(
    coordinator: &cce_orchestrator::QueryCoordinator,
    id: EntityId,
) -> (String, String, String) {
    let snapshot = coordinator.relation_searcher().query().index();
    match snapshot.get_function_by_entity_id(id) {
        Some(entity) => {
            let file = snapshot
                .get_file_path_by_entity(id)
                .unwrap_or_else(|| "(unknown file)".to_string());
            (
                entity.name,
                format!("{:?}", entity.kind).to_lowercase(),
                format!(
                    "{file}:{}-{}",
                    entity.span.start_position.row + 1,
                    entity.span.end_position.row + 1
                ),
            )
        }
        None => (
            format!("entity-{}", id.0),
            "unknown".to_string(),
            "(unknown)".to_string(),
        ),
    }
}

/// Neighbor data rendered on one seed page.
struct SeedPageData<'a> {
    seed_file: &'a str,
    callees: &'a [ResolvedRelation],
    callers: &'a [ResolvedRelation],
    references: &'a [ResolvedRelation],
    file_referrers: &'a [String],
    forward: &'a [cce_codegraph::CallChainNode],
    backward: &'a [cce_codegraph::CallChainNode],
    ego: &'a SubGraph,
    folded_ego: &'a [FoldedEdge],
    nested: &'a [(String, String, String)],
}

/// One ego edge group sharing (source, relation, target).
///
/// The traversal emits one edge per call site, so hot pairs repeat once
/// per call. Review tables count and render folded groups; the raw
/// per-callsite edges stay in the machine-readable graph JSON.
struct FoldedEdge {
    source: String,
    target: String,
    relation: String,
    callsites: Vec<String>,
}

/// Fold per-callsite ego edges by (source, relation, target), preserving
/// first-seen order.
fn fold_ego_edges(edges: &[GraphEdge]) -> Vec<FoldedEdge> {
    let mut folded: Vec<FoldedEdge> = Vec::new();
    let mut index: HashMap<(String, String, String), usize> = HashMap::new();
    for edge in edges {
        let key = (
            edge.source.clone(),
            edge.target.clone(),
            edge.relation.clone(),
        );
        if let Some(&pos) = index.get(&key) {
            if let Some(site) = edge.callsite.as_deref() {
                folded[pos].callsites.push(site.to_string());
            }
            continue;
        }
        index.insert(key.clone(), folded.len());
        folded.push(FoldedEdge {
            source: key.0,
            target: key.1,
            relation: key.2,
            callsites: edge.callsite.iter().cloned().collect(),
        });
    }
    folded
}

/// Compact callsite list for one folded edge group.
fn folded_callsites_label(callsites: &[String]) -> String {
    const SHOWN: usize = 3;
    let mut shown: Vec<&str> = callsites.iter().take(SHOWN).map(String::as_str).collect();
    if callsites.len() > SHOWN {
        shown.push("\u{2026}");
    }
    if shown.is_empty() {
        return "-".to_string();
    }
    let mut label = shown.join(", ");
    if callsites.len() > SHOWN {
        label.push_str(&format!(" (+{} more)", callsites.len() - SHOWN));
    }
    label
}

/// Function-like entities strictly inside the seed span.
///
/// Calls inside them are attributed to the closure rather than the seed,
/// so a seed delegating to callbacks (such as `app.use` to its `forEach`
/// callback) would otherwise look behaviorless. Listed for orientation;
/// their own edges stay on their own pages.
fn nested_closure_entities(
    coordinator: &cce_orchestrator::QueryCoordinator,
    seed: EntityId,
    seed_file: &str,
) -> Vec<(String, String, String)> {
    let snapshot = coordinator.relation_searcher().query().index();
    let Some(seed_entity) = snapshot.get_function_by_entity_id(seed) else {
        return Vec::new();
    };
    let start = seed_entity.span.start_position.row;
    let end = seed_entity.span.end_position.row;
    snapshot
        .get_entities_in_line_range(seed_file, start, end)
        .into_iter()
        .filter(|id| *id != seed)
        .filter_map(|id| snapshot.get_function_by_entity_id(id))
        .filter(|entity| entity.kind.is_function_like())
        .filter(|entity| {
            let inner_start = entity.span.start_position.row;
            let inner_end = entity.span.end_position.row;
            inner_start >= start && inner_end <= end && (inner_start != start || inner_end != end)
        })
        .take(8)
        .map(|entity| {
            (
                entity.name.clone(),
                format!("{:?}", entity.kind).to_lowercase(),
                format!(
                    "{}:{}-{}",
                    seed_file,
                    entity.span.start_position.row + 1,
                    entity.span.end_position.row + 1
                ),
            )
        })
        .collect()
}

/// Render one self-contained per-seed markdown page.
fn render_seed_page(
    seed: &Seed,
    data: &SeedPageData<'_>,
    describe: &dyn Fn(EntityId) -> (String, String, String),
) -> String {
    let SeedPageData {
        seed_file,
        callees,
        callers,
        references,
        file_referrers,
        forward,
        backward,
        ego,
        folded_ego,
        nested,
    } = *data;
    let (name, kind, location) = describe(seed.id);
    let mut page = format!("# Seed: {name} (`{kind}`)\n\n");
    page.push_str(&format!(
        "- Stable id: `{}`\n",
        seed.stable_id.as_deref().unwrap_or("(none)")
    ));
    page.push_str(&format!(
        "- Epoch entity id: `{}` (parse-order ordinal, run-local; not a cross-run identity)\n",
        seed.id.0
    ));
    page.push_str(&format!("- Location: `{location}`\n"));
    page.push_str(&format!("- Provenance: {}\n", seed.provenance));
    if !nested.is_empty() {
        let items: Vec<String> = nested
            .iter()
            .map(|(name, kind, location)| format!("`{name}` (`{kind}`, `{location}`)"))
            .collect();
        page.push_str(&format!(
            "- Nested closures: {}. Calls inside them are attributed to the closure, not to this seed.\n",
            items.join(", ")
        ));
    }

    page.push_str(&format!("\n## Callees ({})\n\n", callees.len()));
    if callees.is_empty() {
        page.push_str("(none)\n");
    } else {
        page.push_str("| callee | relation | location | callsite |\n|---|---|---|---|\n");
        for relation in callees {
            let (target, _, target_location) =
                relation.callee_id.map(describe).unwrap_or_else(|| {
                    (
                        relation.callee_name.clone(),
                        "external".to_string(),
                        "(external)".to_string(),
                    )
                });
            page.push_str(&format!(
                "| {} | {:?} | `{}` | `{}` |\n",
                cell(&target),
                relation.relation_type,
                cell(&target_location),
                cell(&callsite_label(seed_file, relation)),
            ));
        }
    }

    page.push_str(&format!("\n## Callers ({})\n\n", callers.len()));
    if callers.is_empty() {
        page.push_str("(none)\n");
    } else {
        page.push_str("| caller | relation | location | callsite |\n|---|---|---|---|\n");
        for relation in callers {
            let (caller_name, _, caller_location) = describe(relation.caller);
            let caller_file = caller_location.split(':').next().unwrap_or("");
            page.push_str(&format!(
                "| {} | {:?} | `{}` | `{}` |\n",
                cell(&caller_name),
                relation.relation_type,
                cell(&caller_location),
                cell(&callsite_label(caller_file, relation)),
            ));
        }
    }

    page.push_str(&format!("\n## Forward chain ({})\n\n", forward.len()));
    page.push_str(
        "Internal traversal only; external callees appear in Callees and the ego graph.\n",
    );
    page.push_str(&render_chain(forward, describe));
    page.push_str(&format!(
        "\n## Incoming references ({})\n\n",
        references.len() + file_referrers.len()
    ));
    page.push_str(
        "Non-call incoming edges (table entries, assignments, function-pointer passing). Excluded from Callers by design; shown here so table-driven dispatch is not mistaken for dead code.\n",
    );
    if references.is_empty() && file_referrers.is_empty() {
        page.push_str("(none)\n");
    } else {
        page.push_str("| source | relation | callsite |\n|---|---|---|\n");
        for relation in references {
            let (source, _, source_location) = describe(relation.caller);
            let caller_file = source_location.split(':').next().unwrap_or("");
            page.push_str(&format!(
                "| {} | {:?} | `{}` |\n",
                cell(&source),
                relation.relation_type,
                cell(&callsite_label(caller_file, relation)),
            ));
        }
        for file in file_referrers {
            page.push_str(&format!("| {} | file_reference | `-` |\n", cell(file)));
        }
    }
    page.push_str(&format!("\n## Backward chain ({})\n\n", backward.len()));
    page.push_str(&render_chain(backward, describe));

    page.push_str(&format!(
        "\n## Ego graph ({} nodes, {} edges)\n\n",
        ego.nodes.len(),
        folded_ego.len()
    ));
    page.push_str(
        "Edges folded by (source, relation, target); repeated calls share one row with their callsites listed.\n",
    );
    if ego.nodes.is_empty() {
        page.push_str("(empty)\n");
    } else {
        page.push_str("| node | kind | source |\n|---|---|---|\n");
        for node in &ego.nodes {
            page.push_str(&format!(
                "| {} | {} | `{}` |\n",
                cell(&node.label),
                cell(&node.kind),
                cell(&node_source(node)),
            ));
        }
        page.push_str("\n| from | relation | to | callsites |\n|---|---|---|---:|\n");
        for edge in folded_ego {
            page.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                cell(&edge.source),
                cell(&edge.relation),
                cell(&edge.target),
                cell(&folded_callsites_label(&edge.callsites))
            ));
        }
        page.push_str("\n```mermaid\n");
        page.push_str(&render_mermaid(&ego.nodes, folded_ego));
        page.push_str("```\n");
    }
    page
}

/// Indented call-chain listing.
fn render_chain(
    nodes: &[cce_codegraph::CallChainNode],
    describe: &dyn Fn(EntityId) -> (String, String, String),
) -> String {
    if nodes.is_empty() {
        return String::from("(none)\n");
    }
    let mut out = String::new();
    for node in nodes {
        let (name, _, _) = describe(node.function_id);
        out.push_str(&format!(
            "{}- {} `{}` depth={} relation={:?} call_line={}\n",
            "  ".repeat(node.depth.min(8)),
            cell(&name),
            cell(&node.file_path),
            node.depth,
            node.relation_type,
            node.call_line
                .map(|line| line.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ));
    }
    out
}

/// Whether a relation belongs in the call-semantics views (Callees, Callers,
/// forward/backward chains).
///
/// Function-reference edges record that a function value is used, not that
/// it is invoked. They surface only in the Incoming references section;
/// letting them into the call views would inflate caller counts and chains.
fn is_call_view_edge(relation: &ResolvedRelation) -> bool {
    relation.relation_type != cce_types::RelationType::FunctionReference
}

/// Callsite label for a per-callsite edge: caller file plus 1-based line.
fn callsite_label(caller_file: &str, relation: &ResolvedRelation) -> String {
    match relation.span.line_range_opt() {
        Some((line, _)) if !caller_file.is_empty() => format!("{caller_file}:{line}"),
        _ => "-".to_string(),
    }
}

/// Drop ego nodes under excluded test paths plus their incident edges.
fn exclude_test_nodes(ego: SubGraph) -> SubGraph {
    let excluded_types = [ExcludableContentType::Test];
    let kept: HashSet<String> = ego
        .nodes
        .iter()
        .filter(|node| {
            node.source_file.is_empty()
                || !path_excluded_by_types(&node.source_file, &excluded_types)
        })
        .map(|node| node.id.clone())
        .collect();
    let nodes: Vec<_> = ego
        .nodes
        .into_iter()
        .filter(|node| kept.contains(node.id.as_str()))
        .collect();
    let edges: Vec<_> = ego
        .edges
        .into_iter()
        .filter(|edge| kept.contains(edge.source.as_str()) && kept.contains(edge.target.as_str()))
        .collect();
    SubGraph { nodes, edges }
}

/// Mermaid flowchart for the folded ego graph.
fn render_mermaid(
    nodes: &[cce_orchestrator::query::graph::GraphNode],
    edges: &[FoldedEdge],
) -> String {
    let mut out = String::from("flowchart TD\n");
    for node in nodes {
        out.push_str(&format!(
            "    {}[\"{}\"]\n",
            mermaid_id(&node.id),
            mermaid_label(&node.label),
        ));
    }
    for edge in edges {
        out.push_str(&format!(
            "    {} -->|{}| {}\n",
            mermaid_id(&edge.source),
            mermaid_label(&edge.relation),
            mermaid_id(&edge.target),
        ));
    }
    out
}

/// Manifest describing how the export was produced.
fn render_manifest(
    config: &RelationReviewConfig,
    outcomes: &[SeedOutcome],
    skipped: &[String],
    export_nodes: usize,
    export_edges: usize,
    index_result: &cce_orchestrator::IndexResult,
    include_tests: bool,
) -> String {
    let mut manifest = format!("project: {}\n", config.project);
    manifest.push_str(&format!("language: {}\n", config.language));
    manifest.push_str(&format!(
        "seeds: {} (max {})\n",
        outcomes.len(),
        config.max_seeds
    ));
    manifest.push_str(&format!(
        "depth: {} | limit: {}\n",
        config.depth, config.limit
    ));
    manifest.push_str(&format!("hubs: {}\n", config.hub_names.join(", ")));
    manifest.push_str("backend: embedded snapshot (no vector service needed)\n");
    manifest.push_str(&format!(
        "index: files={} entities={} relations={} (relations: per-callsite edges; call-class edges carry their span, non-call edges collapse per entity pair)\n",
        index_result.total_files, index_result.total_entities, index_result.total_relations,
    ));
    manifest.push_str(&format!(
        "project_graph: nodes={export_nodes} edges={export_edges} (callsite-deduped edges over the hub set, capped at 500 nodes)\n",
    ));
    manifest.push_str(
        "caliber: ego `nodes/edges` counts callsite-deduped graph edges, not index relations; compare only within one caliber\n",
    );
    manifest.push_str(&format!(
        "scope: {} (set CCE_RELATION_REVIEW_INCLUDE_TESTS=1 to include test files)\n",
        if include_tests {
            "production + test files"
        } else {
            "production files only, test paths excluded"
        }
    ));
    if skipped.is_empty() {
        manifest.push_str("skipped: none\n");
    } else {
        manifest.push_str("skipped:\n");
        for entry in skipped {
            manifest.push_str(&format!("- {entry}\n"));
        }
    }
    manifest
}

/// Entry page linking every seed file with degree counts.
fn render_index(
    config: &RelationReviewConfig,
    outcomes: &[SeedOutcome],
    skipped: &[String],
    include_tests: bool,
) -> String {
    let scope_note = if include_tests {
        "Chains and ego include test files (CCE_RELATION_REVIEW_INCLUDE_TESTS=1)."
    } else {
        "Chains and ego default to production files only; test paths are excluded."
    };
    let mut page = format!("# Relation Review: {}\n\n", config.project);
    page.push_str(&format!(
        "{} seeds (depth {}, limit {}). {} Open each file for chains and the ego graph.\n\n",
        outcomes.len(),
        config.depth,
        config.limit,
        scope_note
    ));
    page.push_str("| seed | provenance | callees | callers | forward | backward | ego nodes/edges |\n|---|---|---:|---:|---:|---:|---|\n");
    for outcome in outcomes {
        page.push_str(&format!(
            "| [{}](./entity/{}.md) | {} | {} | {} | {} | {} | {}/{} |\n",
            cell(&outcome.name),
            outcome.slug,
            cell(&outcome.provenance),
            outcome.callees,
            outcome.callers,
            outcome.forward,
            outcome.backward,
            outcome.ego_nodes,
            outcome.ego_edges,
        ));
    }
    if !skipped.is_empty() {
        page.push_str("\n## Skipped\n\n");
        for entry in skipped {
            page.push_str(&format!("- {entry}\n"));
        }
    }
    page
}

/// Filesystem-safe slug for a seed entity.
///
/// Anchored on the stable symbol id's short prefix, not the run-local entity
/// ordinal: parse-order ids shift whenever the file set changes, which used to
/// rename every artifact between runs and made outputs un-diffable. Seeds
/// without a symbol key fall back to a fixed marker.
fn seed_slug(name: &str, stable_id: Option<&str>) -> String {
    let safe: String = name
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() || char == '-' || char == '_' {
                char
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = safe.trim_matches('_');
    let stem = if trimmed.is_empty() {
        "entity"
    } else {
        trimmed
    };
    let suffix = stable_id
        .and_then(|id| id.strip_prefix("sym_"))
        .filter(|hex| hex.len() >= 8)
        .map(|hex| hex[..8].to_string())
        .unwrap_or_else(|| "nosym".to_string());
    format!("{stem}-{suffix}")
}

/// Keep markdown table cells on one line.
fn cell(text: &str) -> String {
    text.replace('|', "/").replace('\n', " ")
}

/// Node source rendering for the ego node table.
fn node_source(node: &cce_orchestrator::query::graph::GraphNode) -> String {
    if node.source_file.is_empty() {
        node.source_location.clone()
    } else if node.source_location.is_empty() {
        node.source_file.clone()
    } else {
        format!("{} {}", node.source_file, node.source_location)
    }
}

/// Mermaid-safe node identifier.
fn mermaid_id(id: &str) -> String {
    let safe: String = id
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() {
                char
            } else {
                '_'
            }
        })
        .collect();
    format!("n{safe}")
}

/// Mermaid-safe label text.
fn mermaid_label(label: &str) -> String {
    label
        .replace('"', "'")
        .replace(['<', '>', '|', '{', '}', '[', ']'], " ")
}

/// Machine-readable snapshot of one seed ego graph.
#[derive(serde::Serialize)]
struct SeedGraphJson {
    seed_id: u64,
    seed_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seed_stable_id: Option<String>,
    nodes: Vec<cce_orchestrator::query::graph::GraphNode>,
    edges: Vec<cce_orchestrator::query::graph::GraphEdge>,
}

impl SeedGraphJson {
    fn from_ego(seed: &Seed, ego: &SubGraph) -> Self {
        Self {
            seed_id: seed.id.0,
            seed_name: seed.name.clone(),
            seed_stable_id: seed.stable_id.clone(),
            nodes: ego.nodes.clone(),
            edges: ego.edges.clone(),
        }
    }
}
