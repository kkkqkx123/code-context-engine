//! Stable, storage-independent relationship snapshot model.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::types::{
    Entity, EntityId, EntityKind, ExternalCallType, FileInfo, RelationType, ResolvedRelation, Span,
    StandardizedImport, StdlibCategory,
};

pub const RELATION_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const RELATION_PARSER_VERSION: u32 = 1;
pub const RELATION_RESOLVER_VERSION: u32 = 1;
pub const RELATION_PATH_NORMALIZATION_VERSION: u32 = crate::path::PATH_NORMALIZATION_VERSION;

/// Upper bound on the number of symbol key conflict samples retained in a
/// snapshot's build metadata (first-wins registration collisions).
pub const SYMBOL_KEY_CONFLICT_SAMPLE_CAP: usize = 64;

/// A single first-wins symbol key registration collision: `kept_entity` won
/// the mapping, `rejected_entity` was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolKeyConflictRecord {
    pub file_path: String,
    pub scoped_name: String,
    pub kind: EntityKind,
    pub kept_entity: u64,
    pub rejected_entity: u64,
}

/// Build-time diagnostics carried by a snapshot but excluded from its
/// fingerprint and normalization.
///
/// Diagnostic data must never influence integrity verification, so the field
/// is `#[serde(skip)]` on the snapshot and the fingerprint hashes only the
/// explicit component payload.
#[derive(Debug, Clone, Default)]
pub struct SnapshotBuildMetadata {
    pub symbol_key_conflict_count: u64,
    pub symbol_key_conflict_samples: Vec<SymbolKeyConflictRecord>,
    /// Number of entities exported with a derived symbol key instead of a
    /// registered one (snapshot degradation; the entity stays addressable
    /// through the derived `file + name + kind + signature` identity).
    pub entity_derived_key_count: u64,
    /// Number of relation callers/targets exported with a derived symbol key
    /// instead of a registered one (snapshot degradation).
    pub relation_derived_key_count: u64,
}

/// Why a relation target could not be resolved.
///
/// Enumerated so reason-bucketed observability metrics (see the relation
/// metrics module) get a stable label source instead of free-form strings.
/// The current variant persists as the legacy `symbol_not_resolved` value so
/// stored reason strings round-trip unchanged; future variants (e.g. an
/// `Ambiguous` bucket for multiple same-name candidates) extend this enum
/// without touching the persistence format.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedReason {
    /// The target was not found in the project or any known external source.
    #[serde(rename = "symbol_not_resolved")]
    SymbolNotFound,
}

impl UnresolvedReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SymbolNotFound => "symbol_not_resolved",
        }
    }
}

impl std::str::FromStr for UnresolvedReason {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "symbol_not_resolved" => Ok(Self::SymbolNotFound),
            other => Err(format!("invalid unresolved reason: {other}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Delta types for incremental snapshot updates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportDiff {
    pub file_path: String,
    pub removed_imports: Vec<StandardizedImport>,
    pub added_imports: Vec<StandardizedImport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportDiff {
    pub file_path: String,
    pub removed_exports: Vec<CanonicalExport>,
    pub added_exports: Vec<CanonicalExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyDiff {
    pub source_file: String,
    pub removed_dependencies: Vec<String>,
    pub added_dependencies: Vec<String>,
}

/// File-scoped relation edge changes carried by a delta.
///
/// File-level relations (imports, uses, module-level calls) are stored per
/// file path in `file_relation_index` instead of being attributed to a
/// placeholder entity, so their diff is carried separately from
/// `removed_relations` / `added_relations` (which are keyed by entity).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRelationDiff {
    pub file_path: String,
    pub removed_relations: Vec<ResolvedRelation>,
    pub added_relations: Vec<ResolvedRelation>,
}

/// Entity added by a delta, carrying its stable symbol key and owning file.
///
/// `apply_delta` uses this to register the symbol key mapping and the
/// `entity_file_index` membership so delta replay is equivalent to a full
/// build: added entities are addressable by stable ID and by file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddedEntity {
    pub entity: Entity,
    pub symbol_key: StableSymbolKey,
    pub file_path: String,
}

/// Records only what changed between two snapshots at the file level.
///
/// Serialised as JSON + zstd for storage. `EntityId` references are
/// valid across sessions because base-snapshot entities preserve their IDs
/// through the `entity_id` SQLite column.
///
/// Relation edges are treated as full edges (caller + callee + relation
/// type); `callee_id = None` edges (external/unresolved) participate in the
/// diff as well, distinguished by classification / raw target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDelta {
    pub epoch: i64,
    pub base_epoch: i64,
    pub config_fingerprint: String,

    // File-level changes
    pub removed_files: Vec<String>,
    pub added_files: Vec<FileInfo>,

    // Entity-level changes
    pub removed_entities: Vec<EntityId>,
    pub added_entities: Vec<AddedEntity>,

    // Relation-level changes: complete edges (caller + callee + relation type)
    pub removed_relations: Vec<ResolvedRelation>,
    pub added_relations: Vec<ResolvedRelation>,

    // File-scoped relation changes (imports, uses, module-level calls)
    pub file_relation_diffs: Vec<FileRelationDiff>,

    // Import / export / dependency diffs
    pub import_diffs: Vec<ImportDiff>,
    pub export_diffs: Vec<ExportDiff>,
    pub dependency_diffs: Vec<DependencyDiff>,

    /// Number of relation edges whose caller file is outside the affected
    /// scope and whose callee was removed by this delta: those edges are
    /// dropped from the graph and never restored by a later update (the
    /// caller file is not re-parsed). Non-zero values indicate a hot update
    /// whose change was not fully propagated; operators should surface
    /// the count and schedule a full rebuild.
    #[serde(default)]
    pub relation_edges_dropped_unbounded: u64,

    /// Detected rename pairs: (old_id, new_id, old_name, new_name).
    /// Used by `apply_delta` to migrate caller edges instead of drop+add.
    #[serde(default)]
    pub renamed_entities: Vec<(EntityId, EntityId, String, String)>,
}

// ---------------------------------------------------------------------------

/// Opaque, deterministic identifier exposed across API and process boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StableSymbolId(pub String);

impl std::fmt::Display for StableSymbolId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Normalize a project-relative path for every relationship identity boundary.
///
/// Implemented in `cce_core::utils::path` as the single canonical path
/// normalizer; re-exported here so existing `relation`-scoped imports keep
/// working.
pub use crate::path::normalize_project_path;

/// Stable symbol identity. It intentionally contains no database or runtime ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StableSymbolKey {
    pub file_path: String,
    pub scoped_name: String,
    pub kind: EntityKind,
    /// Full signature is used as the language-independent overload discriminator.
    pub overload_discriminator: String,
}

impl StableSymbolKey {
    /// The single discriminator rule shared by every key constructor.
    ///
    /// Hash inputs (in order): the normalized signature — or a `__span__`
    /// marker when the signature is empty — the kind, the `cfg` predicate,
    /// the duplicate-sibling ordinal, and the byte span when `fold_span` is
    /// set. Empty inputs add no bytes, so plain signature-addressed keys are
    /// unaffected by the extra slots.
    fn discriminator(
        normalized_signature: &str,
        kind: EntityKind,
        cfg_predicate: &str,
        sibling_ordinal: &str,
        span_start: usize,
        span_end: usize,
        fold_span: bool,
    ) -> String {
        let mut hasher = Sha256::new();
        if normalized_signature.is_empty() {
            hasher.update(b"__span__");
        } else {
            hasher.update(normalized_signature.as_bytes());
        }
        hasher.update(kind.to_string().as_bytes());
        hasher.update(cfg_predicate.as_bytes());
        hasher.update(sibling_ordinal.as_bytes());
        if fold_span {
            hasher.update(span_start.to_le_bytes().as_ref());
            hasher.update(span_end.to_le_bytes().as_ref());
        }
        format!("{:x}", hasher.finalize())
    }

    /// Build a key for a signature-addressed symbol that has no live entity:
    /// placeholders (`<file>`, unknown targets) and synthetic export names.
    ///
    /// Entities must go through [`Self::for_entity`] instead: position-scoped
    /// kinds, empty signatures and duplicate siblings take their discriminator
    /// inputs from the entity, which this constructor does not have.
    pub fn new(file_path: &str, scoped_name: &str, kind: EntityKind, signature: &str) -> Self {
        debug_assert!(
            !kind.is_position_scoped() && !signature.trim().is_empty(),
            "entity-backed keys must use for_entity: position-scoped kinds and empty signatures fold the entity span into the discriminator"
        );
        let normalized_signature = signature.split_whitespace().collect::<Vec<_>>().join(" ");
        Self {
            file_path: normalize_project_path(file_path),
            scoped_name: scoped_name.to_string(),
            kind,
            overload_discriminator: Self::discriminator(
                &normalized_signature,
                kind,
                "",
                "",
                0,
                0,
                false,
            ),
        }
    }

    /// Build the key from a live entity, folding the inputs that define a
    /// stable symbol identity into the discriminator.
    ///
    /// Discriminator inputs (in order): normalized signature, kind, the
    /// entity's own `cfg` predicate, the duplicate-sibling ordinal, and the
    /// byte span when the signature is empty or the kind is position-scoped
    /// (`is_position_scoped`: variable-like kinds and annotations).
    ///
    /// The `cfg` fold lets mutually-exclusive conditional-compilation variants
    /// of a same-named symbol each keep their own key. An empty predicate adds
    /// no bytes to the hash, so entities without `cfg` produce the same
    /// discriminator as the plain-signature path. The span fold for
    /// position-scoped kinds separates same-named siblings that share a
    /// signature — local bindings, enum-variant fields, and repeated
    /// decorators/annotations: those kinds never participate in overload
    /// resolution, so position is their identity. The sibling-ordinal fold
    /// separates signature-addressed duplicates (a name rebound twice in one
    /// scope, e.g. two `class Module` mocks in a test function) that the
    /// extractor tagged with `DUPLICATE_SIBLING_ORDINAL`; entities without the
    /// tag keep the plain hash.
    ///
    /// Every input is persisted on the entity (signature, span, metadata), so a
    /// later reload reconstructs an identical key and stays conflict-free.
    pub fn for_entity(file_path: &str, scoped_name: &str, entity: &Entity) -> Self {
        let normalized_signature = entity
            .signature
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let cfg_predicate = entity
            .get_metadata(crate::types::entity::meta_keys::CFG_PREDICATE)
            .map(String::as_str)
            .unwrap_or("");
        let sibling_ordinal = entity
            .get_metadata(crate::types::entity::meta_keys::DUPLICATE_SIBLING_ORDINAL)
            .map(String::as_str)
            .unwrap_or("");
        let fold_span = normalized_signature.is_empty() || entity.kind.is_position_scoped();

        Self {
            file_path: normalize_project_path(file_path),
            scoped_name: scoped_name.to_string(),
            kind: entity.kind,
            overload_discriminator: Self::discriminator(
                &normalized_signature,
                entity.kind,
                cfg_predicate,
                sibling_ordinal,
                entity.span.start_byte,
                entity.span.end_byte,
                fold_span,
            ),
        }
    }

    pub fn sort_key(&self) -> String {
        format!(
            "{}\u{0}{}\u{0}{}\u{0}{}",
            self.file_path, self.scoped_name, self.kind, self.overload_discriminator
        )
    }

    pub fn stable_id(&self) -> StableSymbolId {
        let mut hasher = Sha256::new();
        hasher.update(RELATION_SNAPSHOT_SCHEMA_VERSION.to_le_bytes());
        let kind = self.kind.to_string();
        for component in [
            self.file_path.as_str(),
            self.scoped_name.as_str(),
            kind.as_str(),
            self.overload_discriminator.as_str(),
        ] {
            hasher.update(component.len().to_le_bytes());
            hasher.update(component.as_bytes());
        }
        StableSymbolId(format!("sym_{:x}", hasher.finalize()))
    }

    pub fn is_file_placeholder(&self) -> bool {
        self.scoped_name == "<file>"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalEntity {
    pub key: StableSymbolKey,
    /// Persistent entity ID for cross-session stability.
    /// Populated during SQLite round-trips; excluded from fingerprint/serialization.
    #[serde(skip)]
    pub entity_id: Option<u64>,
    pub name: String,
    pub signature: String,
    pub parameters: Vec<(String, Option<String>)>,
    pub return_type: Option<String>,
    pub span: Span,
    pub depth: usize,
    pub parent: Option<StableSymbolKey>,
    pub doc_comment: Option<String>,
    pub modifiers: Vec<String>,
    pub attributes: BTreeMap<String, String>,
    pub metadata: BTreeMap<String, String>,
    pub is_stdlib: bool,
    pub stdlib_category: Option<StdlibCategory>,
    pub subtype: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CanonicalRelationTarget {
    Internal {
        key: StableSymbolKey,
    },
    External {
        classification: Option<ExternalCallType>,
    },
    Unresolved {
        reason: UnresolvedReason,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalRelation {
    pub caller: StableSymbolKey,
    pub target: CanonicalRelationTarget,
    pub raw_target: String,
    pub relation_type: RelationType,
    pub span: Span,
    pub stdlib_category: Option<StdlibCategory>,
    /// Dispatched overload signature carried through snapshot round-trips.
    ///
    /// Optional so snapshots written before overload annotation keep
    /// loading; absent signatures simply mean single-candidate edges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overload_signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalExport {
    pub symbol: StableSymbolKey,
    pub export_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalFile {
    pub path: String,
    pub language: String,
    pub input_hash: String,
    pub file_size: u64,
    /// Only imports that influence resolution are retained; derived statistics are omitted.
    pub imports: Vec<StandardizedImport>,
    pub exports: Vec<CanonicalExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalDependency {
    pub source_file: String,
    pub target_file: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalRelationSnapshot {
    pub schema_version: u32,
    pub parser_version: u32,
    pub resolver_version: u32,
    pub path_normalization_version: u32,
    pub config_fingerprint: String,
    /// The relation epoch this snapshot was built from.
    /// Used for compare-and-swap during publication: if the active epoch has
    /// advanced past this value the publish is rejected to prevent lost updates.
    pub base_relation_epoch: Option<i64>,
    pub files: Vec<CanonicalFile>,
    pub entities: Vec<CanonicalEntity>,
    pub relations: Vec<CanonicalRelation>,
    pub dependencies: Vec<CanonicalDependency>,
    /// Build-time diagnostics (symbol key conflicts). Excluded from
    /// serialization, normalization, and fingerprinting.
    #[serde(skip)]
    pub build_metadata: SnapshotBuildMetadata,
}

impl CanonicalRelationSnapshot {
    pub fn new(config_fingerprint: String) -> Self {
        Self {
            schema_version: RELATION_SNAPSHOT_SCHEMA_VERSION,
            parser_version: RELATION_PARSER_VERSION,
            resolver_version: RELATION_RESOLVER_VERSION,
            path_normalization_version: RELATION_PATH_NORMALIZATION_VERSION,
            config_fingerprint,
            base_relation_epoch: None,
            files: Vec::new(),
            entities: Vec::new(),
            relations: Vec::new(),
            dependencies: Vec::new(),
            build_metadata: SnapshotBuildMetadata::default(),
        }
    }

    pub fn normalize(&mut self) {
        sort_components(
            &mut self.files,
            &mut self.entities,
            &mut self.relations,
            &mut self.dependencies,
        );
    }

    pub fn validate_versions(&self) -> Result<(), String> {
        if self.schema_version != RELATION_SNAPSHOT_SCHEMA_VERSION
            || self.parser_version != RELATION_PARSER_VERSION
            || self.resolver_version != RELATION_RESOLVER_VERSION
            || self.path_normalization_version != RELATION_PATH_NORMALIZATION_VERSION
        {
            return Err("relationship snapshot version is not supported".to_string());
        }
        Ok(())
    }

    pub fn input_fingerprint(&self) -> String {
        let mut files: Vec<(&str, &str)> = self
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.input_hash.as_str()))
            .collect();
        files.sort_unstable();
        cce_utils::hash::hash_serializable(&files)
    }

    /// Deterministic fingerprint of this snapshot.
    ///
    /// The snapshot is normalized (sorted) and hashed through the fixed-order
    /// component payload, so the output is byte-identical to
    /// [`fingerprint_from_components`] hashing the same components.
    pub fn fingerprint(&self) -> String {
        let mut normalized = self.clone();
        normalized.normalize();
        fingerprint_from_components(&FingerprintComponents {
            schema_version: normalized.schema_version,
            parser_version: normalized.parser_version,
            resolver_version: normalized.resolver_version,
            path_normalization_version: normalized.path_normalization_version,
            config_fingerprint: &normalized.config_fingerprint,
            base_relation_epoch: normalized.base_relation_epoch,
            files: &normalized.files,
            entities: &normalized.entities,
            relations: &normalized.relations,
            dependencies: &normalized.dependencies,
        })
    }
}

/// Sort the four canonical component lists with the same rules as
/// [`CanonicalRelationSnapshot::normalize`]. Shared with
/// [`fingerprint_from_components`] so byte streams are identical regardless
/// of which entry point produced them.
fn sort_components(
    files: &mut [CanonicalFile],
    entities: &mut [CanonicalEntity],
    relations: &mut [CanonicalRelation],
    dependencies: &mut [CanonicalDependency],
) {
    for file in files.iter_mut() {
        file.path = normalize_project_path(&file.path);
        file.imports.sort_by_key(stable_json);
        file.exports.sort_by_key(|export| {
            format!("{}\u{0}{}", export.symbol.sort_key(), export.export_type)
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    entities.sort_by_key(|entity| entity.key.sort_key());
    relations.sort_by_key(relation_sort_key);
    dependencies.sort_by_key(|dependency| {
        format!(
            "{}\u{0}{}\u{0}{}",
            dependency.source_file, dependency.target_file, dependency.source
        )
    });
}

/// Fingerprint payload serialized with a fixed field order, identical to the
/// legacy full-`CanonicalRelationSnapshot` serialization minus the diagnostic
/// `build_metadata` (which never participated in hashing).
#[derive(Serialize)]
struct FingerprintPayload<'a> {
    schema_version: u32,
    parser_version: u32,
    resolver_version: u32,
    path_normalization_version: u32,
    config_fingerprint: &'a str,
    base_relation_epoch: Option<i64>,
    files: Vec<CanonicalFile>,
    entities: Vec<CanonicalEntity>,
    relations: Vec<CanonicalRelation>,
    dependencies: Vec<CanonicalDependency>,
}

/// Version metadata and component lists hashed into a relation fingerprint.
///
/// Field order mirrors the fixed-order [`FingerprintPayload`] so the byte
/// stream is identical to [`CanonicalRelationSnapshot::fingerprint`] of the
/// equivalent snapshot.
pub struct FingerprintComponents<'a> {
    pub schema_version: u32,
    pub parser_version: u32,
    pub resolver_version: u32,
    pub path_normalization_version: u32,
    pub config_fingerprint: &'a str,
    pub base_relation_epoch: Option<i64>,
    pub files: &'a [CanonicalFile],
    pub entities: &'a [CanonicalEntity],
    pub relations: &'a [CanonicalRelation],
    pub dependencies: &'a [CanonicalDependency],
}

/// Hash canonical components into the snapshot fingerprint.
///
/// The four component lists are sorted defensively (identical rules to
/// [`CanonicalRelationSnapshot::normalize`]) before hashing, so callers may
/// hand in components in any order without diverging byte streams. Use for
/// index-side fingerprints that must be byte-identical to
/// [`CanonicalRelationSnapshot::fingerprint`] of the equivalent snapshot.
pub fn fingerprint_from_components(components: &FingerprintComponents) -> String {
    let mut files = components.files.to_vec();
    let mut entities = components.entities.to_vec();
    let mut relations = components.relations.to_vec();
    let mut dependencies = components.dependencies.to_vec();
    sort_components(&mut files, &mut entities, &mut relations, &mut dependencies);
    let payload = FingerprintPayload {
        schema_version: components.schema_version,
        parser_version: components.parser_version,
        resolver_version: components.resolver_version,
        path_normalization_version: components.path_normalization_version,
        config_fingerprint: components.config_fingerprint,
        base_relation_epoch: components.base_relation_epoch,
        files,
        entities,
        relations,
        dependencies,
    };
    cce_utils::hash::hash_serializable(&payload)
}

fn relation_sort_key(relation: &CanonicalRelation) -> String {
    let target = match &relation.target {
        CanonicalRelationTarget::Internal { key } => key.sort_key(),
        CanonicalRelationTarget::External { classification } => stable_json(classification),
        CanonicalRelationTarget::Unresolved { reason } => reason.as_str().to_string(),
    };
    format!(
        "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
        relation.caller.sort_key(),
        relation.relation_type,
        target,
        relation.span.start_byte,
        relation.span.end_byte
    )
}

fn stable_json<T: Serialize>(value: &T) -> String {
    match serde_json::to_string(value) {
        Ok(json) => json,
        Err(error) => format!("serialization_error:{error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_normalization_is_platform_independent() {
        assert_eq!(
            normalize_project_path(r".\src\module\..\lib.rs"),
            "src/lib.rs"
        );
        assert_eq!(
            normalize_project_path("/workspace/src/lib.rs"),
            "/workspace/src/lib.rs"
        );
    }

    fn entity_with(
        kind: EntityKind,
        signature: &str,
        span: Span,
        cfg: Option<&str>,
    ) -> crate::types::Entity {
        let mut entity = Entity::new(EntityId(1), kind, "sym".to_string(), span)
            .with_signature(signature.to_string());
        if let Some(cfg) = cfg {
            entity.set_metadata(
                crate::types::entity::meta_keys::CFG_PREDICATE.to_string(),
                cfg.to_string(),
            );
        }
        entity
    }

    /// An entity with a non-empty signature, no `cfg`, and a kind that
    /// participates in overloading must hash identically to the plain
    /// `new` path, so folding the new inputs is a zero-regression change.
    #[test]
    fn for_entity_matches_plain_key_without_cfg_or_span_fold() {
        let entity = entity_with(EntityKind::Function, "run()", Span::default(), None);
        assert_eq!(
            StableSymbolKey::for_entity("src/lib.rs", "run", &entity),
            StableSymbolKey::new("src/lib.rs", "run", EntityKind::Function, "run()"),
        );
    }

    /// Variable-like kinds are position-scoped and never overloaded, so their
    /// span is folded to keep same-named, same-signature bindings apart.
    #[test]
    fn for_entity_folds_span_for_variable_like_kind() {
        let first = entity_with(
            EntityKind::Variable,
            "err",
            Span::new(10, 20, 0, 0, 0, 0),
            None,
        );
        let second = entity_with(
            EntityKind::Variable,
            "err",
            Span::new(80, 90, 0, 0, 0, 0),
            None,
        );
        let first_key = StableSymbolKey::for_entity("src/lib.rs", "scope::err", &first);
        let second_key = StableSymbolKey::for_entity("src/lib.rs", "scope::err", &second);
        assert_ne!(
            first_key.overload_discriminator, second_key.overload_discriminator,
            "distinct spans must separate same-named locals"
        );

        // Reconstructing from the persisted fields reproduces the key exactly,
        // which is what keeps reload and delta replay conflict-free.
        let reload = entity_with(
            EntityKind::Variable,
            first.signature.as_str(),
            first.span,
            None,
        );
        assert_eq!(
            first_key.overload_discriminator,
            StableSymbolKey::for_entity("src/lib.rs", "scope::err", &reload).overload_discriminator
        );
    }

    /// Repeated decorators/annotations share a name and a signature within one
    /// file; as a position-scoped kind their span must keep them addressable.
    #[test]
    fn for_entity_folds_span_for_annotation_kind() {
        let first = entity_with(
            EntityKind::Annotation,
            "@setupmethod",
            Span::new(100, 112, 0, 0, 0, 0),
            None,
        );
        let second = entity_with(
            EntityKind::Annotation,
            "@setupmethod",
            Span::new(400, 412, 0, 0, 0, 0),
            None,
        );
        let first_key = StableSymbolKey::for_entity("src/app.py", "setupmethod", &first);
        let second_key = StableSymbolKey::for_entity("src/app.py", "setupmethod", &second);
        assert_ne!(
            first_key.overload_discriminator, second_key.overload_discriminator,
            "distinct decorator occurrences must not collide on one stable key"
        );

        let reload = entity_with(EntityKind::Annotation, "@setupmethod", first.span, None);
        assert_eq!(
            first_key.overload_discriminator,
            StableSymbolKey::for_entity("src/app.py", "setupmethod", &reload)
                .overload_discriminator
        );
    }

    /// The entity's own `cfg` predicate is folded so mutually-exclusive
    /// conditional-compilation variants of a same-named symbol each keep their
    /// own identity; an absent and an empty predicate stay equivalent.
    #[test]
    fn for_entity_folds_cfg_predicate() {
        let span = Span::new(0, 5, 0, 0, 0, 0);
        let std_variant = entity_with(
            EntityKind::Module,
            "mod imp",
            span,
            Some(r#"cfg(feature = "std")"#),
        );
        let no_std_variant = entity_with(
            EntityKind::Module,
            "mod imp",
            span,
            Some(r#"cfg(not(feature = "std"))"#),
        );
        assert_ne!(
            StableSymbolKey::for_entity("src/lib.rs", "imp", &std_variant).overload_discriminator,
            StableSymbolKey::for_entity("src/lib.rs", "imp", &no_std_variant)
                .overload_discriminator,
        );

        let empty_cfg = entity_with(EntityKind::Module, "mod imp", span, Some(""));
        let no_cfg = entity_with(EntityKind::Module, "mod imp", span, None);
        assert_eq!(
            StableSymbolKey::for_entity("src/lib.rs", "imp", &empty_cfg).overload_discriminator,
            StableSymbolKey::for_entity("src/lib.rs", "imp", &no_cfg).overload_discriminator,
            "empty cfg must not perturb the discriminator"
        );
    }

    /// Duplicate siblings tagged with distinct ordinals keep distinct keys,
    /// while an untagged entity hashes exactly like the plain path.
    #[test]
    fn for_entity_folds_duplicate_sibling_ordinal() {
        let span = Span::new(0, 12, 0, 0, 0, 0);
        let mut first = entity_with(EntityKind::Class, "class Module", span, None);
        let mut second = entity_with(EntityKind::Class, "class Module", span, None);
        first.set_metadata(
            crate::types::entity::meta_keys::DUPLICATE_SIBLING_ORDINAL,
            "0".to_string(),
        );
        second.set_metadata(
            crate::types::entity::meta_keys::DUPLICATE_SIBLING_ORDINAL,
            "1".to_string(),
        );
        assert_ne!(
            StableSymbolKey::for_entity("tests/t.py", "fn::Module", &first).overload_discriminator,
            StableSymbolKey::for_entity("tests/t.py", "fn::Module", &second).overload_discriminator,
            "source-order ordinals must separate rebound same-signature siblings"
        );

        let untagged = entity_with(EntityKind::Class, "class Module", span, None);
        assert_ne!(
            StableSymbolKey::for_entity("tests/t.py", "fn::Module", &first).overload_discriminator,
            StableSymbolKey::for_entity("tests/t.py", "fn::Module", &untagged)
                .overload_discriminator,
            "an ordinal-tagged key must differ from its untagged form"
        );
        assert_eq!(
            StableSymbolKey::for_entity("tests/t.py", "fn::Module", &untagged),
            StableSymbolKey::new(
                "tests/t.py",
                "fn::Module",
                EntityKind::Class,
                "class Module"
            ),
            "untagged entities keep the plain new-path key"
        );
    }

    #[test]
    fn fingerprint_ignores_top_level_insertion_order() {
        let mut first = CanonicalRelationSnapshot::new("config".to_string());
        first.dependencies = vec![
            CanonicalDependency {
                source_file: "b".to_string(),
                target_file: "c".to_string(),
                source: "relation".to_string(),
            },
            CanonicalDependency {
                source_file: "a".to_string(),
                target_file: "b".to_string(),
                source: "import".to_string(),
            },
        ];
        let mut second = first.clone();
        second.dependencies.reverse();
        assert_eq!(first.fingerprint(), second.fingerprint());
    }

    /// A resolver semantic change (e.g. re-export resolution) must invalidate
    /// previously stored snapshots: the fingerprint must differ when the
    /// resolver version differs, everything else held constant.
    #[test]
    fn fingerprint_distinguishes_resolver_versions() {
        let versioned = |resolver_version| {
            fingerprint_from_components(&FingerprintComponents {
                schema_version: 2,
                parser_version: 1,
                resolver_version,
                path_normalization_version: 1,
                config_fingerprint: "",
                base_relation_epoch: None,
                files: &[],
                entities: &[],
                relations: &[],
                dependencies: &[],
            })
        };
        assert_ne!(versioned(1), versioned(2));
    }

    /// UnresolvedReason persists as the legacy snake_case string so stored
    /// reason values round-trip unchanged and old rows stay readable.
    #[test]
    fn unresolved_reason_round_trips_legacy_string() {
        let reason = UnresolvedReason::SymbolNotFound;
        let json = serde_json::to_string(&reason).expect("serialize");
        assert_eq!(json, "\"symbol_not_resolved\"");

        let parsed: UnresolvedReason = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, UnresolvedReason::SymbolNotFound);

        assert_eq!(
            "symbol_not_resolved"
                .parse::<UnresolvedReason>()
                .expect("parse legacy value"),
            UnresolvedReason::SymbolNotFound
        );
        assert!("symbol_not_found".parse::<UnresolvedReason>().is_err());
        assert_eq!(reason.as_str(), "symbol_not_resolved");
    }

    /// The tagged enum serialization must keep the legacy reason field so a
    /// persisted snapshot body is byte-compatible with the previous format.
    #[test]
    fn unresolved_target_serializes_with_legacy_reason_field() {
        let target = CanonicalRelationTarget::Unresolved {
            reason: UnresolvedReason::SymbolNotFound,
        };
        let json = serde_json::to_string(&target).expect("serialize");
        assert_eq!(
            json,
            "{\"state\":\"unresolved\",\"reason\":\"symbol_not_resolved\"}"
        );
        let parsed: CanonicalRelationTarget = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(
            parsed,
            CanonicalRelationTarget::Unresolved {
                reason: UnresolvedReason::SymbolNotFound
            }
        ));
    }

    #[test]
    fn overload_signature_survives_canonical_round_trip() {
        let relation = CanonicalRelation {
            caller: StableSymbolKey::new("./src/lib.rs", "run", EntityKind::Function, "run()"),
            target: CanonicalRelationTarget::Unresolved {
                reason: UnresolvedReason::SymbolNotFound,
            },
            raw_target: "parse".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            stdlib_category: None,
            overload_signature: Some("parse(String) -> Integer".to_string()),
        };
        let json = serde_json::to_string(&relation).expect("serialize");
        assert!(json.contains("parse(String) -> Integer"));
        let parsed: CanonicalRelation = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            parsed.overload_signature,
            Some("parse(String) -> Integer".to_string())
        );
    }

    #[test]
    fn canonical_relation_without_signature_loads_as_none() {
        let relation = CanonicalRelation {
            caller: StableSymbolKey::new("./src/lib.rs", "run", EntityKind::Function, "run()"),
            target: CanonicalRelationTarget::Unresolved {
                reason: UnresolvedReason::SymbolNotFound,
            },
            raw_target: "other".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            stdlib_category: None,
            overload_signature: None,
        };
        let json = serde_json::to_string(&relation).expect("serialize");
        assert!(!json.contains("overload_signature"));
        let parsed: CanonicalRelation = serde_json::from_str(&json).expect("legacy shape loads");
        assert_eq!(parsed.overload_signature, None);
    }

    /// The component-level fingerprint must be byte-identical to the
    /// snapshot-level fingerprint of the same content.
    #[test]
    fn fingerprint_from_components_matches_snapshot_fingerprint() {
        let mut snapshot = CanonicalRelationSnapshot::new("config".to_string());
        snapshot.base_relation_epoch = Some(7);
        snapshot.files.push(CanonicalFile {
            path: "./src/lib.rs".to_string(),
            language: "rust".to_string(),
            input_hash: "h1".to_string(),
            file_size: 42,
            imports: Vec::new(),
            exports: Vec::new(),
        });
        snapshot.entities.push(CanonicalEntity {
            key: StableSymbolKey::new("./src/lib.rs", "run", EntityKind::Function, "run()"),
            entity_id: Some(1),
            name: "run".to_string(),
            signature: "run()".to_string(),
            parameters: Vec::new(),
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: BTreeMap::new(),
            metadata: BTreeMap::new(),
            is_stdlib: false,
            stdlib_category: None,
            subtype: None,
        });
        snapshot.relations.push(CanonicalRelation {
            caller: StableSymbolKey::new("./src/lib.rs", "run", EntityKind::Function, "run()"),
            target: CanonicalRelationTarget::Unresolved {
                reason: UnresolvedReason::SymbolNotFound,
            },
            raw_target: "other".to_string(),
            relation_type: RelationType::DirectCall,
            span: Span::default(),
            stdlib_category: None,
            overload_signature: None,
        });
        snapshot.dependencies.push(CanonicalDependency {
            source_file: "./src/lib.rs".to_string(),
            target_file: "./src/other.rs".to_string(),
            source: "import".to_string(),
        });

        let mut normalized = snapshot.clone();
        normalized.normalize();
        let components = fingerprint_from_components(&FingerprintComponents {
            schema_version: normalized.schema_version,
            parser_version: normalized.parser_version,
            resolver_version: normalized.resolver_version,
            path_normalization_version: normalized.path_normalization_version,
            config_fingerprint: &normalized.config_fingerprint,
            base_relation_epoch: normalized.base_relation_epoch,
            files: &normalized.files,
            entities: &normalized.entities,
            relations: &normalized.relations,
            dependencies: &normalized.dependencies,
        });
        assert_eq!(components, snapshot.fingerprint());
    }

    /// Component order must not affect the fingerprint (defensive sorting).
    #[test]
    fn fingerprint_from_components_ignores_component_order() {
        let mut snapshot = CanonicalRelationSnapshot::new("config".to_string());
        snapshot.files.push(CanonicalFile {
            path: "src/a.rs".to_string(),
            language: "rust".to_string(),
            input_hash: "h1".to_string(),
            file_size: 1,
            imports: Vec::new(),
            exports: Vec::new(),
        });
        snapshot.files.push(CanonicalFile {
            path: "src/b.rs".to_string(),
            language: "rust".to_string(),
            input_hash: "h2".to_string(),
            file_size: 2,
            imports: Vec::new(),
            exports: Vec::new(),
        });
        snapshot.entities.push(CanonicalEntity {
            key: StableSymbolKey::new("src/a.rs", "a", EntityKind::Function, "a()"),
            entity_id: None,
            name: "a".to_string(),
            signature: "a()".to_string(),
            parameters: Vec::new(),
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: BTreeMap::new(),
            metadata: BTreeMap::new(),
            is_stdlib: false,
            stdlib_category: None,
            subtype: None,
        });
        snapshot.entities.push(CanonicalEntity {
            key: StableSymbolKey::new("src/b.rs", "b", EntityKind::Function, "b()"),
            entity_id: None,
            name: "b".to_string(),
            signature: "b()".to_string(),
            parameters: Vec::new(),
            return_type: None,
            span: Span::default(),
            depth: 0,
            parent: None,
            doc_comment: None,
            modifiers: Vec::new(),
            attributes: BTreeMap::new(),
            metadata: BTreeMap::new(),
            is_stdlib: false,
            stdlib_category: None,
            subtype: None,
        });
        snapshot.dependencies.push(CanonicalDependency {
            source_file: "src/b.rs".to_string(),
            target_file: "src/a.rs".to_string(),
            source: "import".to_string(),
        });

        let mut reversed = snapshot.clone();
        reversed.files.reverse();
        reversed.entities.reverse();
        reversed.dependencies.reverse();
        assert_eq!(
            fingerprint_from_components(&FingerprintComponents {
                schema_version: snapshot.schema_version,
                parser_version: snapshot.parser_version,
                resolver_version: snapshot.resolver_version,
                path_normalization_version: snapshot.path_normalization_version,
                config_fingerprint: &snapshot.config_fingerprint,
                base_relation_epoch: snapshot.base_relation_epoch,
                files: &snapshot.files,
                entities: &snapshot.entities,
                relations: &snapshot.relations,
                dependencies: &snapshot.dependencies,
            }),
            fingerprint_from_components(&FingerprintComponents {
                schema_version: reversed.schema_version,
                parser_version: reversed.parser_version,
                resolver_version: reversed.resolver_version,
                path_normalization_version: reversed.path_normalization_version,
                config_fingerprint: &reversed.config_fingerprint,
                base_relation_epoch: reversed.base_relation_epoch,
                files: &reversed.files,
                entities: &reversed.entities,
                relations: &reversed.relations,
                dependencies: &reversed.dependencies,
            }),
        );
    }

    /// Build diagnostics must never influence the fingerprint, and the
    /// metadata round-trips through the serde path.
    #[test]
    fn build_metadata_is_excluded_from_fingerprint_and_round_trips() {
        let mut snapshot = CanonicalRelationSnapshot::new("config".to_string());
        snapshot.files.push(CanonicalFile {
            path: "src/a.rs".to_string(),
            language: "rust".to_string(),
            input_hash: "h1".to_string(),
            file_size: 1,
            imports: Vec::new(),
            exports: Vec::new(),
        });
        let mut with_metadata = snapshot.clone();
        with_metadata.build_metadata = SnapshotBuildMetadata {
            symbol_key_conflict_count: 2,
            entity_derived_key_count: 3,
            relation_derived_key_count: 4,
            symbol_key_conflict_samples: vec![SymbolKeyConflictRecord {
                file_path: "src/a.rs".to_string(),
                scoped_name: "dup".to_string(),
                kind: EntityKind::Function,
                kept_entity: 1,
                rejected_entity: 2,
            }],
        };

        assert_eq!(snapshot.fingerprint(), with_metadata.fingerprint());

        // The serde-skipped field must not leak into serialized bytes and must
        // default back to empty on deserialization.
        let json = serde_json::to_string(&with_metadata).expect("serialize");
        let parsed: CanonicalRelationSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.build_metadata.symbol_key_conflict_count, 0);
        assert!(parsed.build_metadata.symbol_key_conflict_samples.is_empty());
    }
}
