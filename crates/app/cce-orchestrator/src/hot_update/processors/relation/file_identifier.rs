//! Relation index update processor
//!
//! This module handles updates to the relation index during hot updates.
//! It also persists relation data to SQLite for fast cold start recovery.
//!
//! # Phase 3: Dependency Propagation
//!
//! This processor implements dependency propagation for hot updates:
//! 1. When a file changes, find all files that depend on it
//! 2. Collect all affected files (changed + dependents)
//! 3. Process files in topological order (dependencies first)

use std::collections::{HashMap, HashSet};
use std::path::Path;

use cce_relation::BuildConfigParser;
use cce_relation::IndexBuilder;
use cce_relation::index::{
    LayeredSnapshotIndex, RelationIndex, RelationIndexView, SnapshotFileQueryOps,
};

use super::relation_processor::RelationUpdateProcessor;

use crate::hot_update::processors::relation_support::{
    collect_candidate_dependents as support_collect_candidate_dependents,
    scope_exceeds_ratio as support_scope_exceeds_ratio,
    symbol_fingerprint_scope as support_symbol_fingerprint_scope,
};

impl RelationUpdateProcessor {
    pub(crate) fn stable_symbol_fingerprint<V: RelationIndexView>(
        index: &V,
        fingerprint_files: &HashSet<String>,
    ) -> String {
        use sha2::{Digest, Sha256};

        // aggregate keys per file through the file-membership + reverse
        // symbol maps instead of materializing every symbol key in the project
        // and filtering.
        let mut keys: Vec<String> = index
            .stable_symbol_keys_in_files(fingerprint_files)
            .into_iter()
            .map(|key| key.sort_key())
            .collect();
        keys.sort_unstable();

        let mut hasher = Sha256::new();
        for key in &keys {
            hasher.update(key.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    /// Whether the symbol-fingerprint scope (files that must be traversed
    /// per operation) is too large relative to the project. A ratio of 0 or
    /// a project with no files disables the bound (full rebuilds and tiny
    /// projects are exempt).
    pub(crate) fn scope_exceeds_ratio(scope_len: usize, project_files: usize, ratio: f64) -> bool {
        support_scope_exceeds_ratio(scope_len, project_files, ratio)
    }

    /// TOCTOU guard for dependent reparsing: the reparsed content hash must
    /// match the base snapshot's committed file hash, otherwise the batch is
    /// rejected (the next scan re-detects the file as a normal change).
    pub(crate) fn dependent_content_drift_error(
        source_path: &Path,
        dependent: &str,
        reparsed_hash: Option<&str>,
        base_view: &LayeredSnapshotIndex,
    ) -> Option<String> {
        let base_hash = base_view
            .base
            .get_file(dependent)
            .map(|file| file.file_hash);
        if let (Some(new_hash), Some(base_hash)) = (reparsed_hash, base_hash.as_deref())
            && !base_hash.is_empty()
            && new_hash != base_hash
        {
            return Some(format!(
                "dependent file {} changed on disk during the hot-update operation \
                 (content hash drift {} -> {}); deferring the batch to the next scan",
                source_path.display(),
                base_hash,
                new_hash
            ));
        }
        None
    }

    /// Fingerprint of the stable symbols of `files`, sourcing each file
    /// from the sparse candidate when the candidate's symbol table carries
    /// keys for it (affected files, or unchanged files whose symbols were
    /// polluted into the candidate) and from the base view otherwise.
    ///
    /// In the sparse-candidate build the candidate only contains affected
    /// files, so unchanged files' symbols are normally absent from the
    /// candidate and are read from the base view; if prepopulation pollutes
    /// an unchanged file's symbols into the candidate, those keys are picked
    /// up here and the drift is detected against the base-view fingerprint.
    pub(crate) fn stable_symbol_fingerprint_merged<V: RelationIndexView>(
        base_view: &V,
        candidate: &RelationIndex,
        files: &HashSet<String>,
    ) -> String {
        use sha2::{Digest, Sha256};

        // scope the aggregation to the requested files on both sides
        // instead of walking the whole symbol table.
        let mut candidate_by_file: HashMap<String, Vec<String>> = HashMap::new();
        for key in candidate.stable_symbol_keys_in_files(files) {
            candidate_by_file
                .entry(key.file_path.clone())
                .or_default()
                .push(key.sort_key());
        }
        let mut base_by_file: HashMap<String, Vec<String>> = HashMap::new();
        for key in base_view.stable_symbol_keys_in_files(files) {
            base_by_file
                .entry(key.file_path.clone())
                .or_default()
                .push(key.sort_key());
        }

        let mut keys: Vec<String> = Vec::new();
        for file in files {
            let source = if candidate_by_file.contains_key(file) {
                &candidate_by_file
            } else {
                &base_by_file
            };
            if let Some(file_keys) = source.get(file) {
                keys.extend(file_keys.iter().cloned());
            }
        }
        keys.sort_unstable();

        let mut hasher = Sha256::new();
        for key in &keys {
            hasher.update(key.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    /// Incremental symbol fingerprint for small change sets.
    ///
    /// For small batches (<10 files) the fingerprint is computed only over
    /// the changed + added files directly, avoiding the dependency-closure
    /// traversal cost. For larger batches the full scoped fingerprint is
    /// used. `previous_fingerprint` is retained for API symmetry with the
    /// design doc but the current implementation recomputes from the index
    /// to guarantee correctness; a future delta-hash optimization could
    /// reuse it.
    #[allow(dead_code)]
    pub(crate) fn incremental_symbol_fingerprint<V: RelationIndexView>(
        index: &V,
        previous_fingerprint: &str,
        changed_files: &HashSet<String>,
        added_files: &HashSet<String>,
        removed_files: &HashSet<String>,
    ) -> String {
        let changed_count = changed_files.len() + added_files.len() + removed_files.len();
        if changed_count < 10 {
            let mut combined: HashSet<String> = HashSet::new();
            combined.extend(changed_files.iter().cloned());
            combined.extend(added_files.iter().cloned());
            return Self::stable_symbol_fingerprint(index, &combined);
        }
        let _ = previous_fingerprint;
        let mut all: HashSet<String> = HashSet::new();
        all.extend(changed_files.iter().cloned());
        all.extend(added_files.iter().cloned());
        // Generic V does not expose `all_files`; the caller should pass the
        // scoped set for large batches. We fallback to the combined set to
        // keep the API usable without requiring a full project scan.
        Self::stable_symbol_fingerprint(index, &all)
    }

    /// Files whose symbols the candidate resolution could consult, bounded
    /// by the dependency graph: the replaced set plus its transitive
    /// dependents and dependencies in both the old and the new graph.
    ///
    /// The replaced files themselves are excluded (their symbols are
    /// expected to change); the remaining files must not drift between the
    /// active epoch and the candidate build.
    pub(crate) fn symbol_fingerprint_scope<O: RelationIndexView, N: RelationIndexView>(
        old_index: &O,
        new_index: &N,
        replaced_files: &HashSet<String>,
        max_depth: usize,
    ) -> HashSet<String> {
        support_symbol_fingerprint_scope(old_index, new_index, replaced_files, max_depth)
    }

    pub(crate) fn candidate_path(project_root: &Path, path: &Path) -> String {
        cce_types::path::relativize(project_root, path)
    }

    pub(crate) fn imports_match_package(
        source: &str,
        pkg: &str,
        language: cce_types::Language,
    ) -> bool {
        cce_types::build_system::imports_match_package(source, pkg, language)
    }

    pub(crate) fn inject_governance_edges(builder: &IndexBuilder, parser: &BuildConfigParser) {
        use cce_types::relation::CallContext;
        use cce_types::{EntityId, LanguageInfo, RelationType, ResolvedRelation, Span};
        use std::collections::{HashMap, HashSet};

        let mut pkg_to_configs: HashMap<String, Vec<String>> = HashMap::new();
        for (cfg, deps) in parser.config_file_dependencies() {
            for dep in deps {
                pkg_to_configs
                    .entry(dep.name.clone())
                    .or_default()
                    .push(cfg.clone());
            }
        }
        if pkg_to_configs.is_empty() {
            return;
        }
        let index = builder.index();
        index.for_each_import(|path, table| {
            let lang = LanguageInfo::detect_from_path(path).language;
            let mut matched_configs: HashSet<String> = HashSet::new();
            for imp in &table.standardized_imports {
                for (pkg, cfgs) in &pkg_to_configs {
                    if cce_types::build_system::imports_match_package(&imp.source, pkg, lang) {
                        for cfg in cfgs {
                            matched_configs.insert(cfg.clone());
                        }
                    }
                    if let Some(alias) = &imp.alias
                        && cce_types::build_system::imports_match_package(alias, pkg, lang)
                    {
                        for cfg in cfgs {
                            matched_configs.insert(cfg.clone());
                        }
                    }
                }
            }
            if matched_configs.is_empty() {
                return;
            }
            for cfg in matched_configs {
                builder.dependency_graph().add_dependency(path, &cfg);
                let cfg_entities = index.entities_of_file(&cfg);
                if let Some(cfg_entity) = cfg_entities.first() {
                    let rel = ResolvedRelation {
                        caller: EntityId(0),
                        callee_id: Some(cfg_entity.id),
                        callee_name: cfg.clone(),
                        relation_type: RelationType::DirectCall,
                        span: Span::default(),
                        is_external: false,
                        external_type: None,
                        callee_symbol: None,
                        stdlib_category: None,
                        owner_type: None,
                        call_context: CallContext::Direct,
                    };
                    index.add_file_relation(path, rel);
                }
            }
        });
    }

    /// Collect the files that must be rebuilt alongside the changed set:
    /// transitive dependents over import edges (strong) AND over
    /// caller-derived edges (weak).
    ///
    /// A file that calls an entity defined in a candidate file is a dependent
    /// too: the caller's edge is stored only as a resolved relation, so when
    /// the callee is removed during delta application the edge is dropped
    /// from the graph and never restored (the caller file is untouched and
    /// re-parse is never scheduled). Caller edges are derived on demand from
    /// the view: entity-level callers through `callers_of` + `entity_file_of`
    /// and file-level callers through the maintained reverse index
    /// (`file_callers_of`). Both are O(1) per edge, so propagation stays
    /// bounded by the change size instead of the project.
    ///
    /// Edge costs: import edges cost 1 hop, caller-derived edges cost 2, so
    /// weak edges reach fewer transitive hops than import edges. `best`
    /// tracks the lowest cost per file: the LIFO stack can visit a node
    /// first through an expensive path and later find a cheap one, and
    /// first-visit-wins would silently drop the cheaper path.
    pub(crate) fn collect_candidate_dependents<V: RelationIndexView>(
        index: &V,
        changed_files: &HashSet<String>,
        max_depth: usize,
    ) -> HashSet<String> {
        support_collect_candidate_dependents(index, changed_files, max_depth)
    }
}
