//! Diagnostics grouping for relation index.
//!
//! Extracts the 5 diagnostic counters + metrics sink that were previously
//! scattered as top-level fields in `RelationIndex` / `RelationSnapshotIndex`.
//! Grouping them makes the core index layout explicit and prepares the
//! remaining stores (entity / relation / symbol / file) to be extracted in the
//! same fashion.

use std::collections::VecDeque;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, RwLock};

use cce_metrics::domain::pipeline::RelationMetrics;
use cce_types::SymbolKeyConflictRecord;

/// Diagnostic state shared between `RelationIndex` and its snapshots.
///
/// All counters are `Arc`-shared so `RelationIndex::clone()` (shallow) and
/// `RelationSnapshotIndex::from_index_shared()` share the same counters without
/// deep copies. Snapshots are read-only; only the mutable index increments.
#[derive(Debug, Default)]
pub struct RelationDiagnostics {
    /// First-wins symbol key registration collisions.
    pub symbol_key_conflict_count: AtomicU64,
    /// Bounded sample buffer of the most recent conflicts.
    pub symbol_key_conflict_samples: Mutex<VecDeque<SymbolKeyConflictRecord>>,
    /// Entities exported with a derived symbol key.
    pub entity_derived_key_count: AtomicU64,
    /// Relations exported with a derived symbol key.
    pub relation_derived_key_count: AtomicU64,
    /// Exports skipped during `apply_delta` because stable key unresolvable.
    pub delta_export_unresolved_count: AtomicU64,
    /// Optional metrics sink for observability.
    pub metrics_sink: RwLock<Option<Arc<RelationMetrics>>>,
}

/// Diagnostic summary snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticSummary {
    pub symbol_key_conflicts: u64,
    pub module_path_conflicts: u64,
    pub unresolved_deltas: u64,
    pub truncated_relations: u64,
    pub import_fallbacks: u64,
}

impl RelationDiagnostics {
    pub fn new() -> Self {
        Self {
            symbol_key_conflict_count: AtomicU64::new(0),
            symbol_key_conflict_samples: Mutex::new(VecDeque::new()),
            entity_derived_key_count: AtomicU64::new(0),
            relation_derived_key_count: AtomicU64::new(0),
            delta_export_unresolved_count: AtomicU64::new(0),
            metrics_sink: RwLock::new(None),
        }
    }

    /// Get diagnostic summary snapshot.
    pub fn summary(&self) -> DiagnosticSummary {
        let symbol_key_conflicts = self
            .symbol_key_conflict_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let unresolved_deltas = self
            .delta_export_unresolved_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let (module_path_conflicts, truncated_relations, import_fallbacks) =
            if let Ok(guard) = self.metrics_sink.read() {
                if let Some(metrics) = guard.as_ref() {
                    (
                        metrics.module_path_conflicts.get(),
                        metrics.truncated_relations.get(),
                        metrics.import_fallback_total.get(),
                    )
                } else {
                    (0, 0, 0)
                }
            } else {
                (0, 0, 0)
            };
        DiagnosticSummary {
            symbol_key_conflicts,
            module_path_conflicts,
            unresolved_deltas,
            truncated_relations,
            import_fallbacks,
        }
    }

    /// Get quality score (0-100) based on conflict and unresolved rates.
    pub fn quality_score(&self, total_entities: usize) -> f64 {
        if total_entities == 0 {
            return 100.0;
        }
        let total = total_entities as f64;
        let summary = self.summary();
        let conflicts = summary.symbol_key_conflicts as f64;
        let unresolved = summary.unresolved_deltas as f64;
        let error_rate = (conflicts + unresolved) / total;
        (1.0 - error_rate).max(0.0) * 100.0
    }
}
