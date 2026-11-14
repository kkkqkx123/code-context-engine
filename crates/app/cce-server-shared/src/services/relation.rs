//! Relation domain service shared by the HTTP and MCP surfaces.
//!
//! The service owns symbol seed resolution, relation filtering, pagination and
//! every call/class query. Transports only decode their wire parameters and map
//! the neutral results below onto their own response shapes.
//!
//! Availability is part of the error contract: an unloaded, loading or
//! unavailable relation runtime yields [`RelationServiceError::Unavailable`]
//! instead of an empty success payload.

use std::collections::HashMap;
use std::sync::Arc;

use cce_codegraph::index::snapshot_query::{
    SnapshotEntityQueryOps, SnapshotFileQueryOps, SnapshotRelationQueryOps, SnapshotSymbolQueryOps,
};
use cce_config::RelationConfig;
use cce_orchestrator::query::{PathQueryOptions, RelationQueryOptions, RelationSearcher};
use cce_types::{EntityId, ExternalCallType, ResolvedRelation};

use crate::context::SharedContext;
use crate::engine::EngineError;
use crate::runtime::{PublishedSnapshot, RelationRuntime, RelationRuntimeState};

/// Upper bound on candidates reported for an ambiguous seed.
pub const MAX_SEED_CANDIDATES: usize = 50;

/// Failure of the shared relation service.
///
/// Transports map input-side variants onto their invalid-argument shape and
/// availability onto their service-unavailable shape.
#[derive(Debug, thiserror::Error)]
pub enum RelationServiceError {
    /// A wire value or query parameter is not acceptable.
    #[error("invalid request: {0}")]
    InvalidInput(String),
    /// The project could not be loaded from the registry.
    #[error("failed to load project: {0}")]
    Project(String),
    /// The seed matched no entity.
    #[error("unknown symbol seed: {0}")]
    UnknownSeed(String),
    /// The seed matched several entities.
    #[error("symbol seed '{seed}' is ambiguous; pass one of the candidate stable IDs")]
    AmbiguousSeed {
        seed: String,
        candidates: Vec<SymbolCandidate>,
    },
    /// The relation runtime cannot serve queries in its current state.
    #[error("relation index not available: {state:?}, epoch: {epoch}")]
    Unavailable {
        state: RelationRuntimeState,
        epoch: i64,
    },
    /// The underlying relation query failed.
    #[error("{0}")]
    Execution(String),
}

impl From<EngineError> for RelationServiceError {
    fn from(error: EngineError) -> Self {
        Self::Execution(error.to_string())
    }
}

/// A candidate shown to the caller when a seed is ambiguous. Carries the stable
/// id the client must retry with.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SymbolCandidate {
    pub stable_id: String,
    pub file_path: String,
    pub scoped_name: String,
    pub kind: String,
}

/// A runtime entity described by its transport-neutral identity.
///
/// `stable_id` falls back to `entity:<runtime id>` for entities the symbol
/// table does not know, keeping the field present on every payload while
/// never pretending the value is a durable symbol id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRef {
    pub stable_id: String,
    pub name: String,
    pub file_path: String,
}

/// Epoch and staleness information stamped onto relation answers.
#[derive(Debug, Clone)]
pub struct RelationStamp {
    pub relation_epoch: i64,
    /// Capability map when the served snapshot lags the latest publication.
    pub stale: Option<serde_json::Value>,
}

/// One call edge described by resolved endpoints.
#[derive(Debug, Clone)]
pub struct CallEdge {
    pub caller: EntityRef,
    pub callee: Option<EntityRef>,
    /// Raw resolved target label: the callee symbol name for internal edges,
    /// the external target string otherwise.
    pub target_name: String,
    pub relation_type: String,
    pub is_external: bool,
    /// Call site row.
    pub start_line: u32,
}

/// A filtered relation page: the total is measured before pagination.
#[derive(Debug, Clone)]
pub struct CallEdges {
    /// Entity the listing was requested for.
    pub subject: EntityRef,
    pub total: usize,
    pub items: Vec<CallEdge>,
    pub stamp: RelationStamp,
}

/// One node of a call chain or path.
#[derive(Debug, Clone)]
pub struct ChainNode {
    pub entity: EntityRef,
    pub depth: usize,
    pub relation_type: String,
    pub call_line: Option<usize>,
}

/// A call-chain traversal result for one queried entity.
#[derive(Debug, Clone)]
pub struct CallChainPage {
    /// Traversed entity.
    pub function: EntityRef,
    /// Depth of the filtered chain before pagination.
    pub total: usize,
    pub items: Vec<ChainNode>,
    pub stamp: RelationStamp,
}

/// A shortest call path between two entities.
#[derive(Debug, Clone)]
pub struct CallPath {
    pub path: Option<Vec<ChainNode>>,
    pub stamp: RelationStamp,
}

/// Ancestors and descendants of a class, both as transitive closures.
#[derive(Debug, Clone)]
pub struct ClassHierarchy {
    pub class: EntityRef,
    pub ancestors: Vec<(EntityRef, usize)>,
    pub descendants: Vec<(EntityRef, usize)>,
    pub stamp: RelationStamp,
}

/// Interface implementation relations of a class.
#[derive(Debug, Clone)]
pub struct ClassImplementations {
    pub class: EntityRef,
    pub interfaces: Vec<EntityRef>,
    pub implementors: Vec<EntityRef>,
    pub stamp: RelationStamp,
}

/// Everything a transport needs to render a function detail.
#[derive(Debug, Clone)]
pub struct FunctionDetail {
    pub entity: EntityRef,
    pub signature: String,
    pub start_line: u32,
    pub end_line: u32,
    pub doc_comment: Option<String>,
    pub stamp: RelationStamp,
}

/// External call classification accepted on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassificationCategory {
    Stdlib,
    External,
    Dev,
    Local,
    Unknown,
}

impl ClassificationCategory {
    /// Parse a classification label, case-insensitively.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "stdlib" | "std" => Some(Self::Stdlib),
            "external" | "ext" | "third_party" => Some(Self::External),
            "dev" | "development" => Some(Self::Dev),
            "local" | "path" => Some(Self::Local),
            "unknown" | "other" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// Canonical label of the category.
    pub fn label(self) -> &'static str {
        match self {
            Self::Stdlib => "stdlib",
            Self::External => "external",
            Self::Dev => "dev",
            Self::Local => "local",
            Self::Unknown => "unknown",
        }
    }

    fn into_probe(self) -> ExternalCallType {
        match self {
            Self::Stdlib => ExternalCallType::StandardLibrary {
                library: String::new(),
            },
            Self::External => ExternalCallType::ExternalLibrary {
                package: String::new(),
            },
            Self::Dev => ExternalCallType::DevDependency {
                package: String::new(),
            },
            Self::Local => ExternalCallType::LocalDependency {
                package: String::new(),
            },
            Self::Unknown => ExternalCallType::Unknown {
                raw_target: String::new(),
            },
        }
    }
}

/// Counts of resolved relations grouped by classification label.
#[derive(Debug, Clone)]
pub struct ClassificationStats {
    pub stats: HashMap<String, usize>,
    pub total: usize,
}

/// Traversal direction of a call-chain query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallDirection {
    /// Caller to callees.
    Forward,
    /// Callee to callers.
    Backward,
}

impl CallDirection {
    /// Accept every spelling both surfaces have historically used.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "down" | "forward" | "callees" => Some(Self::Forward),
            "up" | "backward" | "callers" => Some(Self::Backward),
            _ => None,
        }
    }

    /// Canonical label of the direction.
    pub fn label(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Backward => "backward",
        }
    }
}

/// Filter options for the call-semantics tools: only call-domain edges between
/// resolvable project entities qualify.
pub fn call_domain_options() -> RelationQueryOptions {
    RelationQueryOptions::new()
        .with_relation_domains(vec!["call".to_string()])
        .with_include_external(false)
}

/// Wire-independent relation filter described by its raw field values.
///
/// Comma-separated list decoding lives here so both surfaces interpret the
/// same input identically.
#[derive(Debug, Clone, Default)]
pub struct RelationFilterSpec {
    pub exclude_tests: bool,
    pub directory_prefix: Option<String>,
    /// Comma-separated exact file paths to exclude.
    pub excluded_files: Option<String>,
    /// Comma-separated coarse relation domains to keep.
    pub domains: Option<String>,
    pub include_external: bool,
}

impl RelationFilterSpec {
    /// Apply the filter onto relation query options.
    pub fn apply(self, options: RelationQueryOptions) -> RelationQueryOptions {
        let mut options = options.with_exclude_tests(self.exclude_tests);
        if let Some(prefix) = self.directory_prefix {
            options = options.with_directory_prefix(prefix);
        }
        if let Some(files) = self.excluded_files {
            options = options.with_excluded_files(parse_list(&files));
        }
        if let Some(domains) = self.domains {
            options = options.with_relation_domains(
                parse_list(&domains)
                    .into_iter()
                    .map(|domain| domain.to_lowercase())
                    .collect(),
            );
        }
        options.with_include_external(self.include_external)
    }

    /// Build relation query options from wire pagination and filter values.
    ///
    /// Transports pass decoded parameters straight through so neither
    /// surface names the orchestration query type.
    pub fn into_options(
        self,
        max_depth: usize,
        offset: usize,
        limit: usize,
    ) -> RelationQueryOptions {
        self.apply(
            RelationQueryOptions::new()
                .with_max_depth(max_depth)
                .with_offset(offset)
                .with_limit(limit),
        )
    }
}

/// Split a comma-separated parameter value into trimmed non-empty items.
fn parse_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// Snapshot, searcher and runtime of one relation query.
pub(crate) struct RelationAccess {
    pub snapshot: Arc<PublishedSnapshot>,
    pub searcher: Arc<RelationSearcher>,
    pub runtime: Arc<RelationRuntime>,
}

impl RelationAccess {
    /// Stamp the answer with epoch and staleness.
    pub async fn stamp(&self) -> RelationStamp {
        let info = self.runtime.get_capability_info().await;
        RelationStamp {
            relation_epoch: self.snapshot.relation_epoch,
            stale: info.stale.then(|| info.to_json_map()),
        }
    }
}

/// Resolve the snapshot, searcher and runtime, rejecting projects whose
/// relation runtime cannot serve queries.
pub(crate) async fn relation_access(
    state: &SharedContext,
    project_id: i64,
) -> Result<RelationAccess, RelationServiceError> {
    if project_id <= 0 {
        return Err(RelationServiceError::InvalidInput(
            "project_id must be positive".to_string(),
        ));
    }
    let runtime = state
        .engine
        .get_relation_runtime(project_id)
        .await
        .map_err(|error| RelationServiceError::Execution(error.to_string()))?;
    let state_code = runtime.get_state().await;
    if !state_code.can_serve_queries() {
        return Err(RelationServiceError::Unavailable {
            state: state_code,
            epoch: runtime.get_capability_info().await.relation_epoch,
        });
    }
    let Some(snapshot) = runtime.get_snapshot().await else {
        return Err(RelationServiceError::Unavailable {
            state: state_code,
            epoch: runtime.get_capability_info().await.relation_epoch,
        });
    };
    let searcher = state.get_relation_searcher(project_id).await?;
    Ok(RelationAccess {
        snapshot,
        searcher,
        runtime,
    })
}

/// Resolve a symbol seed to a runtime entity id inside one snapshot.
pub(crate) fn resolve_seed_in(
    snapshot: &PublishedSnapshot,
    seed: &str,
) -> Result<EntityId, RelationServiceError> {
    let (ids, candidates) = collect_candidates(snapshot.index.as_ref(), seed);
    match ids.len() {
        1 => Ok(ids[0]),
        0 => Err(RelationServiceError::UnknownSeed(seed.to_string())),
        _ => Err(RelationServiceError::AmbiguousSeed {
            seed: seed.to_string(),
            candidates,
        }),
    }
}

/// Transport-neutral description of an entity.
pub(crate) fn entity_ref(snapshot: &PublishedSnapshot, id: EntityId) -> EntityRef {
    let index = snapshot.index.as_ref();
    EntityRef {
        stable_id: stable_id(index, id).unwrap_or_else(|| format!("entity:{}", id.0)),
        name: index
            .get_function_by_entity_id(id)
            .map(|entity| entity.name.clone())
            .unwrap_or_else(|| "Unknown".to_string()),
        file_path: index
            .get_file_path_by_entity(id)
            .unwrap_or_else(|| "Unknown".to_string()),
    }
}

/// Stable id of an entity, or `None` when the entity has no symbol key.
pub(crate) fn stable_id<I: SnapshotSymbolQueryOps>(
    index: &I,
    entity_id: EntityId,
) -> Option<String> {
    index
        .get_symbol_key_by_entity_id(entity_id)
        .map(|key| key.stable_id().0)
}

/// Load the effective relation configuration of a project.
pub(crate) async fn relation_config(
    state: &SharedContext,
    project_id: i64,
) -> Result<RelationConfig, RelationServiceError> {
    state
        .engine
        .project_registry()
        .get_or_load(project_id)
        .await
        .map(|entry| entry.config.relation.clone())
        .map_err(|error| RelationServiceError::Project(error.to_string()))
}

/// Resolve a symbol seed against the project's currently served snapshot.
pub async fn resolve_entity(
    state: &SharedContext,
    project_id: i64,
    seed: &str,
) -> Result<(EntityId, EntityRef), RelationServiceError> {
    let access = relation_access(state, project_id).await?;
    let id = resolve_seed_in(&access.snapshot, seed)?;
    Ok((id, entity_ref(&access.snapshot, id)))
}

/// Find the call edges leaving one entity.
pub async fn callees(
    state: &SharedContext,
    project_id: i64,
    seed: &str,
    options: &RelationQueryOptions,
) -> Result<CallEdges, RelationServiceError> {
    let access = relation_access(state, project_id).await?;
    let entity_id = resolve_seed_in(&access.snapshot, seed)?;
    let filtered = access.searcher.filter_callees(entity_id, options);
    let total = filtered.len();
    let items = filtered
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .map(|relation| describe_edge(&access.snapshot, relation))
        .collect();
    Ok(CallEdges {
        subject: entity_ref(&access.snapshot, entity_id),
        total,
        items,
        stamp: access.stamp().await,
    })
}

/// Find the call edges entering one entity.
pub async fn callers(
    state: &SharedContext,
    project_id: i64,
    seed: &str,
    options: &RelationQueryOptions,
) -> Result<CallEdges, RelationServiceError> {
    let access = relation_access(state, project_id).await?;
    let entity_id = resolve_seed_in(&access.snapshot, seed)?;
    let filtered = access.searcher.filter_callers(entity_id, options);
    let total = filtered.len();
    let items = filtered
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .map(|relation| describe_edge(&access.snapshot, relation))
        .collect();
    Ok(CallEdges {
        subject: entity_ref(&access.snapshot, entity_id),
        total,
        items,
        stamp: access.stamp().await,
    })
}

/// Traverse the call chain of one entity in the requested direction.
///
/// The project relation configuration gates the query and clamps the requested
/// depth, so both transports observe the same limits.
pub async fn call_chain(
    state: &SharedContext,
    project_id: i64,
    seed: &str,
    direction: CallDirection,
    options: RelationQueryOptions,
) -> Result<CallChainPage, RelationServiceError> {
    let access = relation_access(state, project_id).await?;
    let config = relation_config(state, project_id).await?;
    if !config.index.resolve_call_chains {
        return Err(RelationServiceError::Unavailable {
            state: RelationRuntimeState::Disabled,
            epoch: access.snapshot.relation_epoch,
        });
    }
    let entity_id = resolve_seed_in(&access.snapshot, seed)?;
    let clamped = options.max_depth.min(config.max_call_depth);
    let options = options.with_max_depth(clamped);
    let nodes = match direction {
        CallDirection::Forward => access.searcher.query_forward(entity_id, &options),
        CallDirection::Backward => access.searcher.query_backward(entity_id, &options),
    }
    .map_err(|error| RelationServiceError::Execution(error.to_string()))?;
    let total = nodes.len();
    let items = nodes
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .map(|node| ChainNode {
            entity: entity_ref(&access.snapshot, node.function_id),
            depth: node.depth,
            relation_type: node.relation_type.to_string(),
            call_line: node.call_line,
        })
        .collect();
    Ok(CallChainPage {
        function: entity_ref(&access.snapshot, entity_id),
        total,
        items,
        stamp: access.stamp().await,
    })
}

/// Find the shortest call path between two seeds.
pub async fn call_path(
    state: &SharedContext,
    project_id: i64,
    start_seed: &str,
    end_seed: &str,
    max_depth: usize,
) -> Result<CallPath, RelationServiceError> {
    let access = relation_access(state, project_id).await?;
    let config = relation_config(state, project_id).await?;
    if !config.index.resolve_call_chains {
        return Err(RelationServiceError::Unavailable {
            state: RelationRuntimeState::Disabled,
            epoch: access.snapshot.relation_epoch,
        });
    }
    let start = resolve_seed_in(&access.snapshot, start_seed)?;
    let end = resolve_seed_in(&access.snapshot, end_seed)?;
    let options = PathQueryOptions::new().with_max_depth(max_depth.min(config.max_call_depth));
    let path = access
        .searcher
        .find_path(start, end, &options)
        .map_err(|error| RelationServiceError::Execution(error.to_string()))?
        .map(|nodes| {
            nodes
                .into_iter()
                .map(|node| ChainNode {
                    entity: entity_ref(&access.snapshot, node.function_id),
                    depth: node.depth,
                    relation_type: node.relation_type.to_string(),
                    call_line: node.call_line,
                })
                .collect()
        });
    Ok(CallPath {
        path,
        stamp: access.stamp().await,
    })
}

/// Transitive inheritance closure of a class.
pub async fn class_hierarchy(
    state: &SharedContext,
    project_id: i64,
    seed: &str,
) -> Result<ClassHierarchy, RelationServiceError> {
    let access = relation_access(state, project_id).await?;
    let entity_id = resolve_seed_in(&access.snapshot, seed)?;
    let tree = access
        .searcher
        .get_inheritance_tree(entity_id, RelationSearcher::INHERITANCE_MAX_DEPTH);
    let describe = |entries: Vec<(EntityId, usize)>| {
        entries
            .into_iter()
            .map(|(id, depth)| (entity_ref(&access.snapshot, id), depth))
            .collect::<Vec<_>>()
    };
    Ok(ClassHierarchy {
        class: entity_ref(&access.snapshot, entity_id),
        ancestors: describe(tree.ancestors),
        descendants: describe(tree.descendants),
        stamp: access.stamp().await,
    })
}

/// Interface implementation relations of a class.
pub async fn class_implementations(
    state: &SharedContext,
    project_id: i64,
    seed: &str,
) -> Result<ClassImplementations, RelationServiceError> {
    let access = relation_access(state, project_id).await?;
    let entity_id = resolve_seed_in(&access.snapshot, seed)?;
    let interfaces = access
        .searcher
        .get_implemented_interfaces(entity_id)
        .into_iter()
        .map(|id| entity_ref(&access.snapshot, id))
        .collect();
    let implementors = access
        .searcher
        .get_implementing_classes(entity_id)
        .into_iter()
        .map(|id| entity_ref(&access.snapshot, id))
        .collect();
    Ok(ClassImplementations {
        class: entity_ref(&access.snapshot, entity_id),
        interfaces,
        implementors,
        stamp: access.stamp().await,
    })
}

/// Detail view of one entity.
pub async fn function_detail(
    state: &SharedContext,
    project_id: i64,
    seed: &str,
) -> Result<FunctionDetail, RelationServiceError> {
    let access = relation_access(state, project_id).await?;
    let entity_id = resolve_seed_in(&access.snapshot, seed)?;
    let entity = access
        .snapshot
        .index
        .get_function_by_entity_id(entity_id)
        .ok_or_else(|| RelationServiceError::UnknownSeed(seed.to_string()))?;
    let detail = FunctionDetail {
        entity: entity_ref(&access.snapshot, entity_id),
        signature: entity.signature.clone(),
        start_line: entity.span.start_position.row as u32,
        end_line: entity.span.end_position.row as u32,
        doc_comment: entity.doc_comment.clone(),
        stamp: access.stamp().await,
    };
    Ok(detail)
}

/// Resolved relations whose external classification matches `category`.
pub async fn relations_by_classification(
    state: &SharedContext,
    project_id: i64,
    category: ClassificationCategory,
    limit: usize,
) -> Result<Vec<ResolvedRelation>, RelationServiceError> {
    let access = relation_access(state, project_id).await?;
    let probe = category.into_probe();
    let relations = access
        .snapshot
        .index
        .get_relations_by_classification(&probe);
    let matches = |external: Option<&ExternalCallType>| {
        external
            .map(|value| std::mem::discriminant(value) == std::mem::discriminant(&probe))
            .unwrap_or(false)
    };
    Ok(relations
        .into_iter()
        .filter(|relation| matches(relation.external_type.as_ref()))
        .take(limit)
        .collect())
}

/// Counts of resolved relations grouped by external classification.
pub async fn classification_stats(
    state: &SharedContext,
    project_id: i64,
) -> Result<ClassificationStats, RelationServiceError> {
    let access = relation_access(state, project_id).await?;
    let stats_map = access.snapshot.index.get_classification_stats();
    let total: usize = stats_map.values().sum();
    let stats = stats_map
        .into_iter()
        .map(|(category, count)| {
            let label = match category {
                ExternalCallType::StandardLibrary { .. } => ClassificationCategory::Stdlib,
                ExternalCallType::ExternalLibrary { .. } => ClassificationCategory::External,
                ExternalCallType::DevDependency { .. } => ClassificationCategory::Dev,
                ExternalCallType::LocalDependency { .. } => ClassificationCategory::Local,
                ExternalCallType::Unknown { .. } => ClassificationCategory::Unknown,
            };
            (label.label().to_string(), count)
        })
        .collect();
    Ok(ClassificationStats { stats, total })
}

/// Entity and relation counts of the project's most recent relation snapshot.
///
/// The orchestration snapshot is preferred because it is the source the index
/// pipeline last published; the process-local runtime is the fallback used
/// before that snapshot is wired.
pub async fn published_relation_counts(
    state: &SharedContext,
    project_id: i64,
) -> Option<(usize, usize)> {
    if let Ok(orchestrator) = state.engine.get_orchestrator(project_id).await {
        let orchestrator = orchestrator.lock().await;
        if let Some(snapshot) = orchestrator.get_published_relation_snapshot() {
            return Some((
                snapshot.function_count(),
                snapshot.resolved_relation_count(),
            ));
        }
    }
    let runtime = state.engine.get_relation_runtime(project_id).await.ok()?;
    let snapshot = runtime.get_snapshot().await?;
    Some((snapshot.entity_count(), snapshot.relation_count()))
}

/// Project path of an entity, read from the served relation snapshot.
pub async fn entity_file_path(
    state: &SharedContext,
    project_id: i64,
    entity_id: EntityId,
) -> Option<String> {
    if let Ok(orchestrator) = state.engine.get_orchestrator(project_id).await {
        let orchestrator = orchestrator.lock().await;
        if let Some(snapshot) = orchestrator.get_published_relation_snapshot()
            && let Some(path) = snapshot.get_file_path_by_entity(entity_id)
        {
            return Some(path);
        }
    }
    let runtime = state.engine.get_relation_runtime(project_id).await.ok()?;
    let snapshot = runtime.get_snapshot().await?;
    snapshot.index.get_file_path_by_entity(entity_id)
}

/// Number of resolved relations originating in one file.
pub async fn file_relation_count(
    state: &SharedContext,
    project_id: i64,
    file_path: &str,
) -> Option<usize> {
    if let Ok(orchestrator) = state.engine.get_orchestrator(project_id).await {
        let orchestrator = orchestrator.lock().await;
        if let Some(snapshot) = orchestrator.get_published_relation_snapshot() {
            return Some(
                snapshot
                    .get_resolved_relations_by_file(file_path)
                    .iter()
                    .map(|(_, edges)| edges.len())
                    .sum(),
            );
        }
    }
    let runtime = state.engine.get_relation_runtime(project_id).await.ok()?;
    let snapshot = runtime.get_snapshot().await?;
    Some(
        snapshot
            .index
            .get_resolved_relations_by_file(file_path)
            .iter()
            .map(|(_, edges)| edges.len())
            .sum(),
    )
}

/// Describe one resolved edge with endpoint identities.
fn describe_edge(snapshot: &PublishedSnapshot, relation: ResolvedRelation) -> CallEdge {
    CallEdge {
        caller: entity_ref(snapshot, relation.caller),
        callee: relation.callee_id.map(|id| entity_ref(snapshot, id)),
        target_name: relation.callee_name,
        relation_type: relation.relation_type.to_string(),
        is_external: relation.is_external,
        start_line: relation.span.start_position.row as u32,
    }
}

/// Which form of seed the caller supplied.
enum SeedRef<'a> {
    /// A stable symbol id (`sym_…`).
    StableId,
    /// A name optionally scoped to one file (`file#name`, `#name`).
    Named {
        file: Option<&'a str>,
        name: &'a str,
    },
}

/// Parse a raw seed string into its form.
fn parse_seed(raw: &str) -> SeedRef<'_> {
    if let Some((file, name)) = raw.split_once('#') {
        let file = file.trim();
        return SeedRef::Named {
            file: (!file.is_empty()).then_some(file),
            name: name.trim(),
        };
    }
    // Stable ids always start with `sym_`; anything else is a bare symbol name.
    if raw.starts_with("sym_") {
        return SeedRef::StableId;
    }
    SeedRef::Named {
        file: None,
        name: raw.trim(),
    }
}

/// Candidate entity ids for a seed, plus their wire descriptions.
///
/// The name index covers every registered entity regardless of kind, so both
/// the bare-name and the file-scoped form cost a single O(1) index lookup plus
/// O(matches of the name) filtering.
fn collect_candidates<I>(index: &I, seed: &str) -> (Vec<EntityId>, Vec<SymbolCandidate>)
where
    I: SnapshotEntityQueryOps + SnapshotSymbolQueryOps,
{
    let SeedRef::Named { file, name } = parse_seed(seed) else {
        return match index.get_entity_id_by_stable_symbol_id(seed) {
            Some(id) => (vec![id], Vec::new()),
            None => (Vec::new(), Vec::new()),
        };
    };
    if name.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut ids = index.get_function_ids_by_name(name);
    if let Some(file) = file {
        let normalized = cce_types::normalize_project_path(file);
        ids.retain(|id| {
            index
                .get_file_path_by_entity(*id)
                .is_some_and(|path| cce_types::normalize_project_path(&path) == normalized)
        });
    }
    ids.sort();
    ids.dedup();
    let candidates = ids
        .iter()
        .take(MAX_SEED_CANDIDATES)
        .filter_map(|id| candidate_for(index, *id))
        .collect();
    (ids, candidates)
}

/// Wire description of one candidate entity.
fn candidate_for<I>(index: &I, id: EntityId) -> Option<SymbolCandidate>
where
    I: SnapshotEntityQueryOps + SnapshotSymbolQueryOps,
{
    let key = index.get_symbol_key_by_entity_id(id)?;
    Some(SymbolCandidate {
        stable_id: key.stable_id().0,
        file_path: key.file_path.clone(),
        scoped_name: key.scoped_name.clone(),
        kind: key.kind.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_every_label_aliases() {
        for (raw, expected) in [
            ("stdlib", ClassificationCategory::Stdlib),
            ("STD", ClassificationCategory::Stdlib),
            ("ext", ClassificationCategory::External),
            ("third_party", ClassificationCategory::External),
            ("development", ClassificationCategory::Dev),
            ("path", ClassificationCategory::Local),
            ("other", ClassificationCategory::Unknown),
        ] {
            assert_eq!(
                ClassificationCategory::parse(raw),
                Some(expected),
                "label {raw}"
            );
        }
        assert_eq!(ClassificationCategory::parse("nope"), None);
    }

    #[test]
    fn canonicalizes_every_direction_spelling() {
        for raw in ["down", "forward", "callees", "FORWARD"] {
            assert_eq!(CallDirection::parse(raw), Some(CallDirection::Forward));
        }
        for raw in ["up", "backward", "callers", "Up"] {
            assert_eq!(CallDirection::parse(raw), Some(CallDirection::Backward));
        }
        assert_eq!(CallDirection::parse("sideways"), None);
    }

    #[test]
    fn call_domain_options_are_call_only_and_internal() {
        let options = call_domain_options();
        assert_eq!(options.relation_domains, vec!["call".to_string()]);
        assert!(!options.include_external);
    }

    #[test]
    fn comma_separated_filters_reach_query_options() {
        let options = RelationFilterSpec {
            exclude_tests: true,
            directory_prefix: Some("src/flask".to_string()),
            excluded_files: Some("src/flask/app.py, src/flask/cli.py".to_string()),
            domains: Some("Call, Dependency".to_string()),
            include_external: false,
        }
        .apply(RelationQueryOptions::new());
        assert!(
            options
                .exclude_content_types
                .contains(&cce_orchestrator::query::ExcludableContentType::Test)
        );
        assert_eq!(options.directory_prefix.as_deref(), Some("src/flask"));
        assert_eq!(
            options.excluded_files,
            vec![
                "src/flask/app.py".to_string(),
                "src/flask/cli.py".to_string()
            ]
        );
        assert_eq!(
            options.relation_domains,
            vec!["call".to_string(), "dependency".to_string()]
        );
        assert!(!options.include_external);
    }

    #[test]
    fn parses_every_seed_form() {
        assert!(matches!(parse_seed("sym_abc"), SeedRef::StableId));
        assert!(matches!(
            parse_seed("src/a.rs#foo"),
            SeedRef::Named {
                file: Some("src/a.rs"),
                name: "foo"
            }
        ));
        assert!(matches!(
            parse_seed("#foo"),
            SeedRef::Named {
                file: None,
                name: "foo"
            }
        ));
        assert!(matches!(
            parse_seed("src/a.rs#"),
            SeedRef::Named {
                file: Some("src/a.rs"),
                name: ""
            }
        ));
    }
}
