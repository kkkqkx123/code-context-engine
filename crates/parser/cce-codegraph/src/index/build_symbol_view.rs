//! Bounded disk-backed view of the symbol tables used by a full build.
//!
//! Module records and name candidates live in the operation-local SQLite
//! database. Only small working sets are materialized; module records keep
//! their file-local scopes on the existing disk symbol store.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cce_config::SymbolResolutionConfig;
use lru::LruCache;
use parking_lot::Mutex;

use super::build_store::BuildRelationStore;
use crate::symbol_table::ModuleSymbolTable;
use crate::symbol_table::candidate::SymbolCandidate;

impl std::fmt::Debug for BuildSymbolView {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BuildSymbolView")
            .field("metrics", &self.metrics())
            .finish()
    }
}

fn max_atomic(counter: &AtomicU64, value: u64) {
    counter.fetch_max(value, Ordering::Relaxed);
}

/// Observable working-set and truncation counters for one build view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SymbolViewMetrics {
    pub module_queries: u64,
    pub module_hits: u64,
    pub candidate_queries: u64,
    pub candidate_hits: u64,
    pub module_peak_entries: u64,
    pub module_peak_bytes: u64,
    pub candidate_peak_entries: u64,
    pub candidate_peak_bytes: u64,
    pub truncated_candidates: u64,
}

/// Full-build symbol view backed by the relation build store.
///
/// The in-memory incremental path uses `ProjectSymbolTable` directly; this
/// view supplies the same module and candidate data through bounded caches.
pub struct BuildSymbolView {
    store: Arc<BuildRelationStore>,
    modules: Mutex<LruCache<String, Arc<ModuleSymbolTable>>>,
    candidates: Mutex<LruCache<String, Vec<SymbolCandidate>>>,
    max_candidates_per_bucket: usize,
    module_queries: AtomicU64,
    module_hits: AtomicU64,
    candidate_queries: AtomicU64,
    candidate_hits: AtomicU64,
    module_peak_entries: AtomicU64,
    module_peak_bytes: AtomicU64,
    candidate_peak_entries: AtomicU64,
    candidate_peak_bytes: AtomicU64,
    truncated_candidates: AtomicU64,
}

impl BuildSymbolView {
    pub fn new(store: Arc<BuildRelationStore>, config: &SymbolResolutionConfig) -> Self {
        let module_capacity =
            NonZeroUsize::new(config.module_workset_entries).unwrap_or(NonZeroUsize::MIN);
        let candidate_capacity =
            NonZeroUsize::new(config.name_bucket_workset_entries).unwrap_or(NonZeroUsize::MIN);
        Self {
            store,
            modules: Mutex::new(LruCache::new(module_capacity)),
            candidates: Mutex::new(LruCache::new(candidate_capacity)),
            max_candidates_per_bucket: config.max_name_candidates_per_bucket,
            module_queries: AtomicU64::new(0),
            module_hits: AtomicU64::new(0),
            candidate_queries: AtomicU64::new(0),
            candidate_hits: AtomicU64::new(0),
            module_peak_entries: AtomicU64::new(0),
            module_peak_bytes: AtomicU64::new(0),
            candidate_peak_entries: AtomicU64::new(0),
            candidate_peak_bytes: AtomicU64::new(0),
            truncated_candidates: AtomicU64::new(0),
        }
    }

    pub fn store(&self) -> &Arc<BuildRelationStore> {
        &self.store
    }

    /// Persist one module record. Repeated registration of the same path is
    /// idempotent and refreshes the stored record.
    pub fn install_module(
        &self,
        package_id: &str,
        module: &ModuleSymbolTable,
    ) -> Result<(), String> {
        self.store
            .write_module_record(package_id, &module.to_record())
    }

    pub fn contains_module(&self, path: &str) -> bool {
        self.modules.lock().contains(path) || self.store.module_record(path).is_some()
    }

    pub fn module_paths(&self) -> Vec<(String, String)> {
        self.store.module_paths()
    }

    /// Read a module's type-member index without materializing its full table.
    pub fn module_type_index(&self, path: &str) -> Option<crate::symbol_table::TypeMemberIndex> {
        self.store
            .module_record(path)
            .map(|record| record.type_index)
    }

    /// Materialize one module through the bounded working set.
    pub fn module(&self, path: &str) -> Option<Arc<ModuleSymbolTable>> {
        self.module_queries.fetch_add(1, Ordering::Relaxed);
        if let Some(module) = self.modules.lock().get(path).cloned() {
            self.module_hits.fetch_add(1, Ordering::Relaxed);
            return Some(module);
        }
        let record = self.store.module_record(path)?;
        let bytes = serde_json::to_string(&record)
            .map(|json| json.len() as u64)
            .unwrap_or(0);
        let module = Arc::new(ModuleSymbolTable::from_disk_record(
            record,
            path,
            self.store.local_symbol_store(),
        ));
        {
            let mut cache = self.modules.lock();
            cache.put(path.to_string(), Arc::clone(&module));
            max_atomic(&self.module_peak_entries, cache.len() as u64);
            max_atomic(
                &self.module_peak_bytes,
                bytes.saturating_mul(cache.len() as u64),
            );
        }
        Some(module)
    }

    /// Persist one bounded name candidate. Truncation is counted and surfaced
    /// to the operation manifest rather than silently dropping candidates.
    pub fn install_candidate(&self, candidate: &SymbolCandidate) -> Result<(), String> {
        if !self
            .store
            .write_name_candidate(candidate, self.max_candidates_per_bucket)?
        {
            self.truncated_candidates.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Read candidates for a simple name, restricted to arities accepting
    /// `arg_count` when the call site knows it.
    pub fn candidates(&self, name: &str, arg_count: Option<usize>) -> Vec<SymbolCandidate> {
        self.candidate_queries.fetch_add(1, Ordering::Relaxed);
        let key = format!(
            "{name}\0{}",
            arg_count.map(|count| count.to_string()).unwrap_or_default()
        );
        if let Some(cached) = self.candidates.lock().get(&key).cloned() {
            self.candidate_hits.fetch_add(1, Ordering::Relaxed);
            return cached;
        }
        let candidates: Vec<SymbolCandidate> = self
            .store
            .name_candidates(name, None)
            .into_iter()
            .filter(|candidate| arg_count.is_none_or(|count| candidate.accepts(count)))
            .collect();
        let bytes: u64 = candidates
            .iter()
            .map(|candidate| candidate.sort_key.len() as u64 + 128)
            .sum();
        {
            let mut cache = self.candidates.lock();
            cache.put(key, candidates.clone());
            max_atomic(&self.candidate_peak_entries, cache.len() as u64);
            max_atomic(
                &self.candidate_peak_bytes,
                bytes.saturating_mul(cache.len() as u64),
            );
        }
        candidates
    }

    pub fn metrics(&self) -> SymbolViewMetrics {
        SymbolViewMetrics {
            module_queries: self.module_queries.load(Ordering::Relaxed),
            module_hits: self.module_hits.load(Ordering::Relaxed),
            candidate_queries: self.candidate_queries.load(Ordering::Relaxed),
            candidate_hits: self.candidate_hits.load(Ordering::Relaxed),
            module_peak_entries: self.module_peak_entries.load(Ordering::Relaxed),
            module_peak_bytes: self.module_peak_bytes.load(Ordering::Relaxed),
            candidate_peak_entries: self.candidate_peak_entries.load(Ordering::Relaxed),
            candidate_peak_bytes: self.candidate_peak_bytes.load(Ordering::Relaxed),
            truncated_candidates: self.truncated_candidates.load(Ordering::Relaxed),
        }
    }

    pub fn truncated_candidates(&self) -> u64 {
        self.truncated_candidates.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{SymbolLocation, SymbolMetadata, Visibility};
    use cce_types::{EntityId, EntityKind, Language, Span};

    fn candidate(sort_key: &str, id: u64) -> SymbolCandidate {
        SymbolCandidate {
            name: "duplicate".to_string(),
            arity: crate::type_inference::overload::ParamArity::exact(1),
            sort_key: sort_key.to_string(),
            kind: crate::symbol_table::candidate::SymbolCandidateKind::FileSymbol,
            entity_id: EntityId(id),
            file_path: Some(format!("src/{id}.rs")),
            module_path: "src".to_string(),
            package_id: None,
        }
    }

    #[test]
    fn module_records_round_trip_through_bounded_view() {
        let root = tempfile::tempdir().expect("temp dir");
        let store = Arc::new(
            BuildRelationStore::new(root.path().join("view.sqlite3")).expect("store must open"),
        );
        let view = BuildSymbolView::new(Arc::clone(&store), &SymbolResolutionConfig::default());
        let mut module = ModuleSymbolTable::new(
            "src/main",
            "src/main.rs".to_string(),
            Language::Rust,
            "pkg".to_string(),
        );
        let metadata = SymbolMetadata::new(
            "public_symbol".to_string(),
            EntityKind::Function,
            SymbolLocation::new("src/main.rs".to_string(), Span::default(), Language::Rust),
        );
        module.add_export("public_symbol".to_string(), metadata, Visibility::Public);
        view.install_module("pkg", &module).expect("module write");

        let loaded = view.module("src/main.rs").expect("module must load");
        assert!(loaded.has_export("public_symbol"));
        assert_eq!(loaded.file_path, "src/main.rs");
        assert_eq!(view.metrics().module_queries, 1);
        assert_eq!(
            view.module("src/main.rs")
                .map(|module| module.file_path.clone()),
            Some("src/main.rs".to_string())
        );
        assert_eq!(view.metrics().module_hits, 1);
    }

    #[test]
    fn candidate_bucket_is_bounded_and_truncation_is_reported() {
        let root = tempfile::tempdir().expect("temp dir");
        let store = Arc::new(
            BuildRelationStore::new(root.path().join("view.sqlite3")).expect("store must open"),
        );
        let config = SymbolResolutionConfig {
            max_name_candidates_per_bucket: 2,
            ..SymbolResolutionConfig::default()
        };
        let view = BuildSymbolView::new(Arc::clone(&store), &config);
        for (key, id) in [("000::a", 1), ("001::b", 2), ("002::c", 3)] {
            view.install_candidate(&candidate(key, id))
                .expect("candidate write");
        }
        let candidates = view.candidates("duplicate", Some(1));
        assert_eq!(candidates.len(), 2);
        assert_eq!(view.truncated_candidates(), 1);
        assert!(candidates.iter().all(|item| item.arity.accepts(1)));
    }
}
