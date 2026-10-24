//! Configuration merge trait and implementations.
//!
//! Provides the `Mergeable` trait for declarative config merging.
//! Project profile sections are patches: every field is optional, absent
//! fields leave the global value untouched, and only explicitly given
//! fields override it.

/// Trait for types that can merge an override into themselves.
///
/// Implementors define how an `Override` value is applied to `self`.
/// The merge mutates `self` in place.
pub trait Mergeable {
    /// The override type that provides partial values.
    type Override;

    /// Apply `override_config` onto `self`.
    ///
    /// Fields present in `override_config` replace the corresponding
    /// fields in `self`. Fields absent (None) leave `self` unchanged.
    fn merge(&mut self, override_config: &Self::Override);
}

/// Merge for Option<T>: if override is Some, replace self.
impl<T: Clone> Mergeable for Option<T> {
    type Override = Option<T>;

    fn merge(&mut self, override_config: &Option<T>) {
        if let Some(value) = override_config {
            *self = Some(value.clone());
        }
    }
}

use crate::modules::pattern_detection::GetterSetterDetectionConfig;
use crate::modules::{
    AstToNlConfig, BatchConfig, Bm25GeneratorConfig, CacheConfig, ChunkingConfig, DebounceConfig,
    EmbeddingGeneratorConfig, FileWatchConfig, HotUpdateConfig, IndexConfig, IndexerConfig,
    LicenseHeaderConfig, NestProcessorConfig, OrchestratorConfig, RelationConfig, ScannerConfig,
    SummaryConfig,
};
use crate::project::{
    ProjectAstToNlPatch, ProjectBatchPatch, ProjectBm25GeneratorPatch, ProjectCachePatch,
    ProjectChunkingPatch, ProjectDebouncePatch, ProjectEmbeddingGeneratorPatch,
    ProjectFileWatchPatch, ProjectGetterSetterPatch, ProjectGrouperPatch, ProjectHotUpdatePatch,
    ProjectIndexerPatch, ProjectLicenseHeaderPatch, ProjectOrchestratorConfig,
    ProjectRelationIndexPatch, ProjectRelationPatch, ProjectScannerPatch, ProjectSummaryPatch,
    ProjectTextCleanerPatch,
};
use cce_text::Bm25TextCleanerConfig;

impl Mergeable for ScannerConfig {
    type Override = ProjectScannerPatch;

    fn merge(&mut self, patch: &ProjectScannerPatch) {
        if let Some(value) = patch.follow_symlinks {
            self.follow_symlinks = value;
        }
        if let Some(value) = patch.respect_gitignore {
            self.respect_gitignore = value;
        }
        if let Some(ref value) = patch.exclude_patterns {
            self.exclude_patterns = value.clone();
        }
        if let Some(ref value) = patch.include_patterns {
            self.include_patterns = value.clone();
        }
        if let Some(ref value) = patch.gitignore_patterns {
            self.gitignore_patterns = value.clone();
        }
        if let Some(value) = patch.binary_check_size {
            self.binary_check_size = value;
        }
        if let Some(value) = patch.max_hash_file_size {
            self.max_hash_file_size = value;
        }
        if let Some(value) = patch.default_max_content_size {
            self.default_max_content_size = value;
        }
        if let Some(value) = patch.max_file_size {
            self.max_file_size = value;
        }
        if let Some(value) = patch.plugin_filter_enabled {
            self.plugin_filter_enabled = value;
        }
        if let Some(value) = patch.parallelism {
            self.parallelism = value;
        }
    }
}

impl Mergeable for IndexerConfig {
    type Override = ProjectIndexerPatch;

    fn merge(&mut self, patch: &ProjectIndexerPatch) {
        if let Some(ref value) = patch.extensions {
            self.extensions = value.clone();
        }
        if let Some(ref value) = patch.exclude_dirs {
            self.exclude_dirs = value.clone();
        }
        if let Some(value) = patch.store_vectors {
            self.store_vectors = value;
        }
        if let Some(value) = patch.store_bm25 {
            self.store_bm25 = value;
        }
        if let Some(value) = patch.store_summaries {
            self.store_summaries = value;
        }
        if let Some(value) = patch.embed_summaries {
            self.embed_summaries = value;
        }
        if let Some(value) = patch.build_relations {
            self.build_relations = value;
        }
        if let Some(value) = patch.dead_letter_truncate_retry {
            self.dead_letter_truncate_retry = value;
        }
        if let Some(value) = patch.embed_input_token_limit {
            self.embed_input_token_limit = value;
        }
    }
}

impl Mergeable for BatchConfig {
    type Override = ProjectBatchPatch;

    fn merge(&mut self, patch: &ProjectBatchPatch) {
        if let Some(value) = patch.scan_batch_size {
            self.scan_batch_size = value;
        }
        if let Some(value) = patch.parse_concurrency {
            self.parse_concurrency = value;
        }
        if let Some(value) = patch.process_concurrency {
            self.process_concurrency = value;
        }
        if let Some(value) = patch.store_batch_size {
            self.store_batch_size = value;
        }
        if let Some(value) = patch.embedding_batch_size {
            self.embedding_batch_size = value;
        }
        if let Some(value) = patch.embedding_batch_delay_ms {
            self.embedding_batch_delay_ms = value;
        }
        if let Some(value) = patch.embedding_stage_timeout_secs {
            self.embedding_stage_timeout_secs = value;
        }
    }
}

impl Mergeable for DebounceConfig {
    type Override = ProjectDebouncePatch;

    fn merge(&mut self, patch: &ProjectDebouncePatch) {
        if let Some(value) = patch.pending_interval_secs {
            self.pending_interval_secs = value;
        }
        if let Some(value) = patch.max_wait_time_secs {
            self.max_wait_time_secs = value;
        }
    }
}

impl Mergeable for FileWatchConfig {
    type Override = ProjectFileWatchPatch;

    fn merge(&mut self, patch: &ProjectFileWatchPatch) {
        if let Some(value) = patch.enabled {
            self.enabled = value;
        }
        if let Some(value) = patch.event_threshold {
            self.event_threshold = value;
        }
        if let Some(value) = patch.fallback_interval_secs {
            self.fallback_interval_secs = value;
        }
        if let Some(value) = patch.verification_interval_secs {
            self.verification_interval_secs = value;
        }
        if let Some(value) = patch.watch_config_files {
            self.watch_config_files = value;
        }
        if let Some(value) = patch.storm_duration_secs {
            self.storm_duration_secs = value;
        }
        if let Some(value) = patch.recovery_threshold {
            self.recovery_threshold = value;
        }
        if let Some(value) = patch.recovery_duration_secs {
            self.recovery_duration_secs = value;
        }
    }
}

impl Mergeable for HotUpdateConfig {
    type Override = ProjectHotUpdatePatch;

    fn merge(&mut self, patch: &ProjectHotUpdatePatch) {
        if let Some(value) = patch.enabled {
            self.enabled = value;
        }
        if let Some(ref value) = patch.debounce {
            self.debounce.merge(value);
        }
        if let Some(ref value) = patch.scanner {
            match value {
                None => self.scanner = None,
                Some(scanner_patch) => {
                    let mut base = self.scanner.clone().unwrap_or_default();
                    base.merge(scanner_patch);
                    self.scanner = Some(base);
                }
            }
        }
        if let Some(value) = patch.build_relations {
            self.build_relations = value;
        }
        if let Some(value) = patch.store_summaries {
            self.store_summaries = value;
        }
        if let Some(value) = patch.store_vectors {
            self.store_vectors = value;
        }
        if let Some(value) = patch.store_bm25 {
            self.store_bm25 = value;
        }
        if let Some(value) = patch.batch_size {
            self.batch_size = value;
        }
        if let Some(value) = patch.embedding_batch_size {
            self.embedding_batch_size = value;
        }
        if let Some(value) = patch.storage_batch_size {
            self.storage_batch_size = value;
        }
        if let Some(ref value) = patch.file_watch {
            self.file_watch.merge(value);
        }
    }
}

impl Mergeable for CacheConfig {
    type Override = ProjectCachePatch;

    fn merge(&mut self, patch: &ProjectCachePatch) {
        if let Some(value) = patch.chunk_cache_size {
            self.chunk_cache_size = value;
        }
        if let Some(value) = patch.enabled {
            self.enabled = value;
        }
    }
}

impl Mergeable for OrchestratorConfig {
    type Override = ProjectOrchestratorConfig;

    fn merge(&mut self, override_config: &ProjectOrchestratorConfig) {
        if let Some(ref batch) = override_config.batch {
            self.batch.merge(batch);
        }
        if let Some(ref hot_update) = override_config.hot_update {
            self.hot_update.merge(hot_update);
        }
        if let Some(ref indexer) = override_config.indexer {
            self.indexer.merge(indexer);
        }
        if let Some(ref cache) = override_config.cache {
            self.cache.merge(cache);
        }
        if let Some(ttl) = override_config.checkpoint_ttl_seconds {
            self.checkpoint_ttl_seconds = ttl;
        }
    }
}

impl Mergeable for GetterSetterDetectionConfig {
    type Override = ProjectGetterSetterPatch;

    fn merge(&mut self, patch: &ProjectGetterSetterPatch) {
        if let Some(value) = patch.strict_mode {
            self.strict_mode = value;
        }
        if let Some(value) = patch.max_simple_lines {
            self.max_simple_lines = value;
        }
        if let Some(value) = patch.kotlin_accessor_support {
            self.kotlin_accessor_support = value;
        }
    }
}

impl Mergeable for NestProcessorConfig {
    type Override = ProjectGrouperPatch;

    fn merge(&mut self, patch: &ProjectGrouperPatch) {
        if let Some(value) = patch.small_class_threshold {
            self.small_class_threshold = value;
        }
        if let Some(value) = patch.simple_call_merge_threshold {
            self.simple_call_merge_threshold = value;
        }
        if let Some(value) = patch.enable_class_method_association {
            self.enable_class_method_association = value;
        }
        if let Some(value) = patch.enable_call_merging {
            self.enable_call_merging = value;
        }
        if let Some(value) = patch.enable_getter_setter_merging {
            self.enable_getter_setter_merging = value;
        }
        if let Some(value) = patch.getter_setter_complexity_threshold {
            self.getter_setter_complexity_threshold = value;
        }
        if let Some(value) = patch.enable_test_entity_grouping {
            self.enable_test_entity_grouping = value;
        }
        if let Some(value) = patch.include_assertions_in_groups {
            self.include_assertions_in_groups = value;
        }
        if let Some(value) = patch.max_test_suite_nesting {
            self.max_test_suite_nesting = value;
        }
        if let Some(value) = patch.enable_nested_entity_grouping {
            self.enable_nested_entity_grouping = value;
        }
        if let Some(value) = patch.max_nesting_depth {
            self.max_nesting_depth = value;
        }
        if let Some(value) = patch.min_nested_size {
            self.min_nested_size = value;
        }
        if let Some(value) = patch.enable_impl_association {
            self.enable_impl_association = value;
        }
        if let Some(value) = patch.enable_plugin_pattern_detection {
            self.enable_plugin_pattern_detection = value;
        }
        if let Some(value) = patch.enable_function_member_grouping {
            self.enable_function_member_grouping = value;
        }
        if let Some(value) = patch.enable_small_fragment_merging {
            self.enable_small_fragment_merging = value;
        }
        if let Some(value) = patch.small_fragment_max_span_lines {
            self.small_fragment_max_span_lines = value;
        }
        if let Some(value) = patch.enable_group_hierarchy {
            self.enable_group_hierarchy = value;
        }
        if let Some(value) = patch.near_merge_proximity_bytes {
            self.near_merge_proximity_bytes = value;
        }
        if let Some(ref value) = patch.getter_setter {
            self.getter_setter.merge(value);
        }
    }
}

impl Mergeable for IndexConfig {
    type Override = ProjectRelationIndexPatch;

    fn merge(&mut self, patch: &ProjectRelationIndexPatch) {
        if let Some(value) = patch.enabled {
            self.enabled = value;
        }
        if let Some(value) = patch.max_relations_per_file {
            self.max_relations_per_file = value;
        }
        if let Some(value) = patch.resolve_call_chains {
            self.resolve_call_chains = value;
        }
        if let Some(value) = patch.filter_stdlib_calls {
            self.filter_stdlib_calls = value;
        }
    }
}

impl Mergeable for RelationConfig {
    type Override = ProjectRelationPatch;

    fn merge(&mut self, patch: &ProjectRelationPatch) {
        if let Some(value) = patch.max_call_depth {
            self.max_call_depth = value;
        }
        if let Some(value) = patch.analyze_imports {
            self.analyze_imports = value;
        }
        if let Some(value) = patch.track_cross_file_deps {
            self.track_cross_file_deps = value;
        }
        if let Some(value) = patch.max_propagation_depth {
            self.max_propagation_depth = value;
        }
        if let Some(value) = patch.max_fingerprint_scope_ratio {
            self.max_fingerprint_scope_ratio = value;
        }
        if let Some(value) = patch.plugin_symbols_enabled {
            self.plugin_symbols_enabled = value;
        }
        if let Some(value) = patch.plugin_symbol_extract_enabled {
            self.plugin_symbol_extract_enabled = value;
        }
        if let Some(value) = patch.manifest_scan_depth {
            self.manifest_scan_depth = value;
        }
        if let Some(ref value) = patch.index {
            self.index.merge(value);
        }
    }
}

impl Mergeable for Bm25GeneratorConfig {
    type Override = ProjectBm25GeneratorPatch;

    fn merge(&mut self, patch: &ProjectBm25GeneratorPatch) {
        if let Some(value) = patch.max_keywords {
            self.max_keywords = value;
        }
    }
}

impl Mergeable for EmbeddingGeneratorConfig {
    type Override = ProjectEmbeddingGeneratorPatch;

    fn merge(&mut self, patch: &ProjectEmbeddingGeneratorPatch) {
        if let Some(value) = patch.max_summary_words {
            self.max_summary_words = value;
        }
        if let Some(value) = patch.include_docstring {
            self.include_docstring = value;
        }
    }
}

impl Mergeable for ChunkingConfig {
    type Override = ProjectChunkingPatch;

    fn merge(&mut self, patch: &ProjectChunkingPatch) {
        if let Some(value) = patch.max_tokens {
            self.max_tokens = value;
        }
        if let Some(value) = patch.overlap_tokens {
            self.overlap_tokens = value;
        }
        if let Some(value) = patch.max_bm25_terms {
            self.max_bm25_terms = value;
        }
        if let Some(value) = patch.overlap_bm25_terms {
            self.overlap_bm25_terms = value;
        }
        if let Some(value) = patch.max_overlap_ratio {
            self.max_overlap_ratio = value;
        }
        if let Some(value) = patch.min_chunk_tokens {
            self.min_chunk_tokens = value;
        }
        if let Some(value) = patch.min_chunk_bm25_terms {
            self.min_chunk_bm25_terms = value;
        }
        if let Some(value) = patch.respect_boundaries {
            self.respect_boundaries = value;
        }
        if let Some(value) = patch.cross_group_merge_threshold {
            self.cross_group_merge_threshold = value;
        }
        if let Some(value) = patch.glossary_reserve_ratio {
            self.glossary_reserve_ratio = value;
        }
        if let Some(value) = patch.glossary_max_entries {
            self.glossary_max_entries = value;
        }
        if let Some(value) = patch.glossary_scope {
            self.glossary_scope = value;
        }
    }
}

impl Mergeable for Bm25TextCleanerConfig {
    type Override = ProjectTextCleanerPatch;

    fn merge(&mut self, patch: &ProjectTextCleanerPatch) {
        if let Some(ref value) = patch.redundant_patterns {
            self.redundant_patterns = value.clone();
        }
    }
}

impl Mergeable for AstToNlConfig {
    type Override = ProjectAstToNlPatch;

    fn merge(&mut self, patch: &ProjectAstToNlPatch) {
        if let Some(value) = patch.default_mode {
            self.default_mode = value;
        }
        if let Some(ref value) = patch.bm25 {
            self.bm25.merge(value);
        }
        if let Some(ref value) = patch.embedding {
            self.embedding.merge(value);
        }
        if let Some(ref value) = patch.chunking {
            self.chunking.merge(value);
        }
        if let Some(ref value) = patch.document_chunking {
            match value {
                None => self.document_chunking = None,
                Some(chunking_patch) => {
                    let mut base = self.document_chunking.clone().unwrap_or_default();
                    base.merge(chunking_patch);
                    self.document_chunking = Some(base);
                }
            }
        }
        if let Some(ref value) = patch.text_cleaner {
            self.text_cleaner.merge(value);
        }
    }
}

impl Mergeable for LicenseHeaderConfig {
    type Override = ProjectLicenseHeaderPatch;

    fn merge(&mut self, patch: &ProjectLicenseHeaderPatch) {
        if let Some(value) = patch.enabled {
            self.enabled = value;
        }
        if let Some(value) = patch.max_comments {
            self.max_comments = value;
        }
        if let Some(ref value) = patch.rules {
            self.rules = value.clone();
        }
    }
}

impl Mergeable for SummaryConfig {
    type Override = ProjectSummaryPatch;

    fn merge(&mut self, patch: &ProjectSummaryPatch) {
        if let Some(ref value) = patch.strategy {
            self.strategy = value.clone();
        }
        if let Some(value) = patch.max_summary_length {
            self.max_summary_length = value;
        }
        if let Some(value) = patch.max_entities {
            self.max_entities = value;
        }
        if let Some(value) = patch.max_imports {
            self.max_imports = value;
        }
        if let Some(value) = patch.max_concurrent {
            self.max_concurrent = value;
        }
        if let Some(value) = patch.max_retries {
            self.max_retries = value;
        }
        if let Some(value) = patch.request_timeout_secs {
            self.request_timeout_secs = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_option_merge_some_replaces() {
        let mut base: Option<String> = Some("old".into());
        base.merge(&Some("new".into()));
        assert_eq!(base.as_deref(), Some("new"));
    }

    #[test]
    fn test_option_merge_none_preserves() {
        let mut base: Option<String> = Some("old".into());
        base.merge(&None);
        assert_eq!(base.as_deref(), Some("old"));
    }

    #[test]
    fn test_option_merge_none_to_some() {
        let mut base: Option<String> = None;
        base.merge(&Some("new".into()));
        assert_eq!(base.as_deref(), Some("new"));
    }

    #[test]
    fn test_option_merge_none_to_none() {
        let mut base: Option<String> = None;
        base.merge(&None);
        assert_eq!(base, None);
    }

    #[test]
    fn test_orchestrator_merge_partial() {
        use crate::modules::OrchestratorConfig;
        use crate::project::ProjectOrchestratorConfig;

        let mut base = OrchestratorConfig {
            checkpoint_ttl_seconds: 100,
            ..Default::default()
        };

        let override_config = ProjectOrchestratorConfig {
            checkpoint_ttl_seconds: Some(200),
            ..Default::default()
        };
        base.merge(&override_config);
        assert_eq!(base.checkpoint_ttl_seconds, 200);
    }

    #[test]
    fn test_orchestrator_merge_preserves_unoverridden() {
        use crate::modules::OrchestratorConfig;
        use crate::project::ProjectOrchestratorConfig;

        let mut base = OrchestratorConfig::default();
        let original_batch = base.batch.clone();
        let override_config = ProjectOrchestratorConfig {
            checkpoint_ttl_seconds: Some(999),
            ..Default::default()
        };
        base.merge(&override_config);
        assert_eq!(base.batch.scan_batch_size, original_batch.scan_batch_size);
        assert_eq!(base.checkpoint_ttl_seconds, 999);
    }

    #[test]
    fn test_merge_with_project_uses_trait() {
        use crate::global::AppConfig;
        use crate::project::{ProjectAppConfig, ProjectScannerPatch};

        let global = AppConfig::default();
        let project = ProjectAppConfig {
            scanner: Some(ProjectScannerPatch {
                follow_symlinks: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = global.merge_with_project(&project);
        assert!(merged.scanner.follow_symlinks);
    }

    #[test]
    fn test_partial_patch_preserves_sibling_fields() {
        use crate::global::AppConfig;
        use crate::project::{ProjectAppConfig, ProjectScannerPatch};

        let mut global = AppConfig::default();
        global.scanner.follow_symlinks = true;
        global.scanner.exclude_patterns = vec!["custom".to_string()];

        let project = ProjectAppConfig {
            scanner: Some(ProjectScannerPatch {
                respect_gitignore: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = global.merge_with_project(&project);
        assert!(!merged.scanner.respect_gitignore);
        assert!(merged.scanner.follow_symlinks);
        assert_eq!(merged.scanner.exclude_patterns, vec!["custom".to_string()]);
    }

    #[test]
    fn test_partial_indexer_patch_preserves_sibling_fields() {
        use crate::global::AppConfig;
        use crate::project::{ProjectAppConfig, ProjectIndexerPatch, ProjectOrchestratorConfig};

        let mut global = AppConfig::default();
        global.orchestrator.indexer.exclude_dirs = vec!["custom".to_string()];

        let project = ProjectAppConfig {
            orchestrator: Some(ProjectOrchestratorConfig {
                indexer: Some(ProjectIndexerPatch {
                    extensions: Some(vec!["rs".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = global.merge_with_project(&project);
        assert_eq!(
            merged.orchestrator.indexer.extensions,
            vec!["rs".to_string()]
        );
        assert_eq!(
            merged.orchestrator.indexer.exclude_dirs,
            vec!["custom".to_string()]
        );
    }
}
