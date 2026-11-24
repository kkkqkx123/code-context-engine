//! Shared harness for hot-update resume/recovery e2e tests.
//!
//! Provides a "crash simulator": the durable state (SQLite, BM25 index,
//! checkpoint manager) is owned by the harness and survives across coordinator
//! instances. A test builds a coordinator, runs it to a specific crash point,
//! drops it (simulating a crash), then rebuilds a fresh coordinator over the
//! same durable state and resumes the interrupted operation.
//!
//! Counters (parse / summary / bm25-index) are shared across instances so a
//! resumed run's probes can prove that work was NOT duplicated.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::Context;
use tempfile::TempDir;
use tokio::sync::Mutex;

use cce_config::{AstToNlConfig, NestProcessorConfig, ScannerConfig};
use cce_orchestrator::hot_update::processors::ProcessorContext;
use cce_orchestrator::hot_update::progress::read_module_progress;
use cce_orchestrator::hot_update::{
    BatchChangeResult, FileChangeType, HotUpdateError, ParseResultWithChanges,
};
use cce_orchestrator::index::MetadataStore;
use cce_orchestrator::index::StorageCoordinator;
use cce_orchestrator::{
    Bm25UpdateProcessor, CheckpointManager, EmbeddingUpdateProcessor, ExportConfig,
    HotUpdateCoordinator, IndexOptions, IndexOrchestrator, NlDocumentExporter,
    NlDocumentUpdateProcessor, OperationContext, OperationProcessResult, OperationResult,
    OperationType, SummaryUpdateProcessor, UpdateProcessor,
};
use cce_parser::summary::{FileSummary, RuleBasedGenerator, SummaryGenerator};
use cce_storage_bm25::{Bm25Client, Bm25Config};
use cce_storage_common::MetadataStorage;
use cce_storage_common::metadb::MetadataBackendStore;
use cce_utils::hash::hash_serializable;

use crate::helper::EmptyFixture;

/// Summary generator wrapper that counts every generated summary.
pub struct CountingSummaryGenerator {
    inner: RuleBasedGenerator,
    count: Arc<AtomicUsize>,
}

impl CountingSummaryGenerator {
    pub fn new(count: Arc<AtomicUsize>) -> Self {
        Self {
            inner: RuleBasedGenerator::default(),
            count,
        }
    }
}

#[async_trait::async_trait]
impl SummaryGenerator for CountingSummaryGenerator {
    async fn generate(&self, parsed_file: &cce_types::ParsedFile) -> FileSummary {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.inner.generate(parsed_file).await
    }
}

/// Which processors a coordinator instance runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessorSelection {
    pub embedding: bool,
    pub bm25: bool,
    pub summary: bool,
    pub export: bool,
}

impl ProcessorSelection {
    pub fn bm25_only() -> Self {
        Self {
            bm25: true,
            ..Self::default()
        }
    }

    pub fn embedding_bm25() -> Self {
        Self {
            embedding: true,
            bm25: true,
            ..Self::default()
        }
    }

    pub fn full() -> Self {
        Self {
            embedding: true,
            bm25: true,
            summary: true,
            export: true,
        }
    }

    pub fn is_empty(self) -> bool {
        !self.embedding && !self.bm25 && !self.summary && !self.export
    }
}

/// A coordinator instance: processors + storage coordinator sharing the
/// harness's durable state.
/// Callback used to wrap an [`UpdateProcessor`] in a test (e.g. failure injection).
type ProcessorTransform<'a> =
    Option<&'a dyn Fn(Arc<dyn UpdateProcessor>) -> Arc<dyn UpdateProcessor>>;

pub struct CoordinatorInstance {
    pub coordinator: HotUpdateCoordinator,
    pub processors: Vec<Arc<dyn UpdateProcessor>>,
    pub storage: Arc<StorageCoordinator>,
}

/// A wrapper that makes the wrapped processor fail on the Nth
/// `process_operation` call, simulating a real mid-batch module failure.
///
/// All lifecycle hooks (prepare/commit/abort) delegate to the inner processor
/// so the abort path executes with the real candidate-retirement semantics
/// instead of a hand-crafted crash state.
pub struct FailingProcessor {
    inner: Arc<dyn UpdateProcessor>,
    fail_on_call: usize,
    call_count: std::sync::atomic::AtomicUsize,
}

impl FailingProcessor {
    /// Create a wrapper that fails the `fail_on_call`-th `process_operation`
    /// call (1-based), returning the given error.
    pub fn new(inner: Arc<dyn UpdateProcessor>, fail_on_call: usize) -> Self {
        Self {
            inner,
            fail_on_call,
            call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// The number of `process_operation` calls so far.
    pub fn call_count(&self) -> usize {
        self.call_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl UpdateProcessor for FailingProcessor {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn is_enabled(&self) -> bool {
        self.inner.is_enabled()
    }

    fn supports_config_reload(&self) -> bool {
        self.inner.supports_config_reload()
    }

    async fn prepare_operation(
        &self,
        ctx: &OperationContext,
    ) -> cce_orchestrator::hot_update::Result<()> {
        self.inner.prepare_operation(ctx).await
    }

    async fn process_operation(
        &self,
        ctx: &OperationContext,
        batch_result: &mut BatchChangeResult,
    ) -> cce_orchestrator::hot_update::Result<OperationProcessResult> {
        let call = self
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        if call == self.fail_on_call {
            return Err(HotUpdateError::bm25(format!(
                "simulated module failure on call {call}"
            )));
        }
        self.inner.process_operation(ctx, batch_result).await
    }

    async fn commit_operation(
        &self,
        ctx: &OperationContext,
    ) -> cce_orchestrator::hot_update::Result<()> {
        self.inner.commit_operation(ctx).await
    }

    async fn abort_operation(
        &self,
        ctx: &OperationContext,
        reason: &str,
    ) -> cce_orchestrator::hot_update::Result<()> {
        self.inner.abort_operation(ctx, reason).await
    }
}

impl CoordinatorInstance {
    pub async fn run_hot_update(&mut self) -> anyhow::Result<OperationResult> {
        let refs: Vec<&dyn UpdateProcessor> = self.processors.iter().map(|p| p.as_ref()).collect();
        let mut ctx = self
            .coordinator
            .begin_operation(OperationType::HotUpdate)
            .await?;
        self.coordinator
            .run_operation(&mut ctx, &refs)
            .await
            .map_err(|e| anyhow::anyhow!("run_operation failed: {e}"))
    }
}

/// Durable harness state shared by all coordinator instances of one test.
pub struct HotUpdateHarness {
    pub fixture: EmptyFixture,
    pub project_id: i64,
    /// Keeps the relation database file alive for the test duration.
    _db_dir: TempDir,
    pub store: MetadataStore,
    /// Keeps the BM25 index directory alive for the test duration.
    _bm25_dir: TempDir,
    pub bm25: Arc<Bm25Client>,
    pub checkpoint_manager: Arc<CheckpointManager>,
    pub parse_counter: Arc<AtomicUsize>,
    pub summary_counter: Arc<AtomicUsize>,
    pub bm25_index_counter: Arc<AtomicUsize>,
    pub embedding_counter: Arc<AtomicUsize>,
    pub grouper_config: NestProcessorConfig,
    /// Next raw entity-ID seed handed to a `parse_file` call.
    ///
    /// A real run parses every changed file with a single coordinator whose
    /// entity-ID counter is monotonic across the batch; this mirror ensures
    /// crash-state envelopes carry the same globally-unique entity IDs a real
    /// interrupted run would have left behind. Without it every envelope
    /// re-seeds at 0 and the resulting `group_0` chunk IDs collide across
    /// files in the BM25 index.
    entity_id_seed: Arc<AtomicU64>,
    /// Cached parses keyed by project-relative path.
    ///
    /// `envelope_for` and `seed_candidate_epoch_data` describe the same
    /// interrupted run, so both must reuse the identical `ParsedFile` — a real
    /// run parses each file once and feeds the same result to the checkpoint
    /// envelope and the storage writes. Re-parsing in the seed would hand the
    /// candidate a different (later) entity-ID set than the envelope, breaking
    /// the idempotent doc-ID alignment the resume relies on.
    parsed_cache: std::sync::Mutex<HashMap<String, cce_types::entity::ParsedFile>>,
}

impl HotUpdateHarness {
    pub async fn new() -> anyhow::Result<Self> {
        Self::with_project_id(1).await
    }

    pub async fn with_project_id(project_id: i64) -> anyhow::Result<Self> {
        let fixture = EmptyFixture::new().context("failed to create fixture")?;
        let db_dir = TempDir::new().context("failed to create db temp dir")?;
        // Scope the store to the project so reads and writes land in the
        // same project database the orchestrator persists to (mirrors
        // production, where the server hands project-scoped stores to the
        // orchestrator).
        let store = Arc::new(
            cce_storage_metadb_sqlite::SqliteClient::with_path(
                db_dir.path().join("cce.db").to_string_lossy(),
            )
            .context("failed to create file-backed SQLite")?,
        )
        .for_project(project_id)
        .context("scope store")?;
        let store = MetadataBackendStore::Local(store);
        let checkpoint_manager = Arc::new(CheckpointManager::new_for_project(
            project_id,
            store.clone(),
        ));

        // Ensure the project row exists so full-index and hot-update writes
        // satisfy the projects FK before the first scan.
        store
            .ensure_project(project_id, "/")
            .await
            .context("failed to ensure project row")?;

        let bm25_dir = TempDir::new().context("failed to create bm25 temp dir")?;
        let bm25_config = Bm25Config::default()
            .enabled()
            .with_index_name("default")
            .with_index_path(bm25_dir.path().join("index").to_string_lossy());
        let mut bm25 = Bm25Client::new(bm25_config);
        bm25.connect().await.context("failed to connect BM25")?;
        let bm25 = Arc::new(bm25);

        Ok(Self {
            fixture,
            project_id,
            _db_dir: db_dir,
            store,
            _bm25_dir: bm25_dir,
            bm25,
            checkpoint_manager,
            parse_counter: Arc::new(AtomicUsize::new(0)),
            summary_counter: Arc::new(AtomicUsize::new(0)),
            bm25_index_counter: Arc::new(AtomicUsize::new(0)),
            embedding_counter: Arc::new(AtomicUsize::new(0)),
            grouper_config: NestProcessorConfig::default(),
            entity_id_seed: Arc::new(AtomicU64::new(0)),
            parsed_cache: std::sync::Mutex::new(HashMap::new()),
        })
    }

    pub fn root(&self) -> &std::path::Path {
        self.fixture.root()
    }

    /// Add a source file to the fixture.
    pub fn add_file(&self, relative: &str, content: &str) -> anyhow::Result<()> {
        self.fixture
            .add_file(relative, content)
            .map(|_| ())
            .context("failed to add fixture file")
    }

    pub fn file(&self, relative: &str) -> PathBuf {
        self.fixture.root().join(relative)
    }

    /// Scanner configuration that excludes the `.cce` export directory so
    /// exported documents never re-enter change detection.
    fn scanner_config(&self) -> ScannerConfig {
        let mut scanner = ScannerConfig::default();
        scanner.exclude_patterns.push(".cce".to_string());
        scanner
    }

    /// Build a fresh coordinator instance over the durable state.
    ///
    /// `watch` enables file-watch mode (sets `watch_root`), which the export
    /// skip logic needs to validate the persisted `export_path` marker.
    ///
    /// The change-detection root path is configured automatically so the
    /// resumed run can scan when publishing; the baseline hashes stored by
    /// `initialize_cache` are idempotent upserts that do not disturb resume.
    pub async fn build_instance(
        &self,
        selection: ProcessorSelection,
        chunking_config: Option<&AstToNlConfig>,
        watch: bool,
    ) -> anyhow::Result<CoordinatorInstance> {
        self.build_instance_with(selection, chunking_config, watch, false)
            .await
    }

    /// Build a coordinator instance over the durable state.
    ///
    /// `prime_cache` stores baseline hashes for the current files so the first
    /// scan reports no changes (used only by tests that resume from a crash
    /// state without an initial run). The default keeps change detection fresh
    /// so a first run indexes the fixture.
    pub async fn build_instance_with(
        &self,
        selection: ProcessorSelection,
        chunking_config: Option<&AstToNlConfig>,
        watch: bool,
        prime_cache: bool,
    ) -> anyhow::Result<CoordinatorInstance> {
        self.build_instance_with_config(
            selection,
            chunking_config,
            watch,
            prime_cache,
            cce_config::HotUpdateConfig::default(),
        )
        .await
    }

    /// Build a coordinator instance over the durable state with a custom hot
    /// update configuration (e.g. small debounce intervals or low storm
    /// thresholds for watch/mode-switch tests).
    pub async fn build_instance_with_config(
        &self,
        selection: ProcessorSelection,
        chunking_config: Option<&AstToNlConfig>,
        watch: bool,
        prime_cache: bool,
        config: cce_config::HotUpdateConfig,
    ) -> anyhow::Result<CoordinatorInstance> {
        self.build_instance_with_summary_fingerprint(
            selection,
            chunking_config,
            watch,
            prime_cache,
            config,
            String::new(),
        )
        .await
    }

    /// Build a coordinator instance with an explicit summary module
    /// fingerprint (used by config-change tests to invalidate summaries).
    pub async fn build_instance_with_summary_fingerprint(
        &self,
        selection: ProcessorSelection,
        chunking_config: Option<&AstToNlConfig>,
        watch: bool,
        prime_cache: bool,
        config: cce_config::HotUpdateConfig,
        summary_fingerprint: String,
    ) -> anyhow::Result<CoordinatorInstance> {
        self.build_instance_inner(
            selection,
            chunking_config,
            watch,
            prime_cache,
            config,
            summary_fingerprint,
            None,
        )
        .await
    }

    /// Build a coordinator instance, wrapping every built processor with
    /// `transform` (used by failure-injection tests to make a processor fail).
    pub async fn build_instance_with_transform<F>(
        &self,
        selection: ProcessorSelection,
        chunking_config: Option<&AstToNlConfig>,
        watch: bool,
        prime_cache: bool,
        config: cce_config::HotUpdateConfig,
        transform: F,
    ) -> anyhow::Result<CoordinatorInstance>
    where
        F: Fn(Arc<dyn UpdateProcessor>) -> Arc<dyn UpdateProcessor>,
    {
        self.build_instance_inner(
            selection,
            chunking_config,
            watch,
            prime_cache,
            config,
            String::new(),
            Some(&transform),
        )
        .await
    }

    /// Shared constructor: the summary fingerprint is used verbatim by the
    /// summary processor, and an optional transform wraps every processor.
    #[allow(clippy::too_many_arguments)]
    async fn build_instance_inner(
        &self,
        selection: ProcessorSelection,
        chunking_config: Option<&AstToNlConfig>,
        watch: bool,
        prime_cache: bool,
        mut config: cce_config::HotUpdateConfig,
        summary_fingerprint: String,
        transform: ProcessorTransform<'_>,
    ) -> anyhow::Result<CoordinatorInstance> {
        let storage = self.build_storage().await;
        let processors = self.build_processors(
            selection,
            &storage,
            chunking_config,
            summary_fingerprint.as_str(),
        );
        let processors: Vec<Arc<dyn UpdateProcessor>> = processors
            .into_iter()
            .map(|processor| match transform {
                Some(transform) => transform(processor),
                None => processor,
            })
            .collect();

        if config.scanner.is_none() {
            config.scanner = Some(self.scanner_config());
        }
        let mut coordinator = if watch {
            HotUpdateCoordinator::with_file_watch(config, self.project_id)
                .context("failed to create file-watch coordinator")?
        } else {
            HotUpdateCoordinator::new(config, self.project_id)
                .context("failed to create coordinator")?
        };
        coordinator = coordinator
            .with_metadata_store(self.store.clone())
            .with_checkpoint_manager(self.checkpoint_manager.clone())
            .with_storage_coordinator(storage.clone())
            .with_parse_probe(self.parse_counter.clone())
            .with_processors(processors.clone());
        if prime_cache {
            coordinator
                .initialize_cache(self.root())
                .await
                .context("failed to configure change-detection root")?;
        } else {
            coordinator.set_scan_root_path(self.root()).await;
        }

        Ok(CoordinatorInstance {
            coordinator,
            processors,
            storage,
        })
    }

    /// Storage coordinator seeded with the currently active data epoch.
    async fn build_storage(&self) -> Arc<StorageCoordinator> {
        let active_epoch = self
            .active_manifest()
            .await
            .ok()
            .flatten()
            .map(|m| m.data_epoch)
            .unwrap_or(0);
        Arc::new(
            StorageCoordinator::new(self.project_id)
                .expect("valid project id")
                .with_metadata_store(self.store.clone())
                .with_bm25(self.bm25.clone())
                .with_epoch(active_epoch)
                .with_batch_id(0),
        )
    }

    fn build_processors(
        &self,
        selection: ProcessorSelection,
        storage: &Arc<StorageCoordinator>,
        chunking_config: Option<&AstToNlConfig>,
        summary_fingerprint: &str,
    ) -> Vec<Arc<dyn UpdateProcessor>> {
        let mut context = ProcessorContext::new(storage.clone(), NestProcessorConfig::default());
        context = context.with_checkpoint_manager(self.checkpoint_manager.clone());
        if let Some(config) = chunking_config {
            context.file_processor = Arc::new(Mutex::new(
                cce_orchestrator::index::FileProcessor::with_config(config),
            ));
        }
        let context = Arc::new(context);

        let mut processors: Vec<Arc<dyn UpdateProcessor>> = Vec::new();

        if selection.embedding {
            let processor = EmbeddingUpdateProcessor::new(context.clone())
                .with_embedding_counter(self.embedding_counter.clone());
            processors.push(Arc::new(processor));
        }

        if selection.bm25 {
            let processor = Bm25UpdateProcessor::new(context.clone())
                .with_index_counter(self.bm25_index_counter.clone());
            processors.push(Arc::new(processor));
        }

        if selection.summary {
            let generator = Arc::new(CountingSummaryGenerator::new(self.summary_counter.clone()));
            let processor = SummaryUpdateProcessor::new(storage.clone(), generator)
                .with_summary_fingerprint(summary_fingerprint.to_string())
                .with_checkpoint_manager(self.checkpoint_manager.clone());
            processors.push(Arc::new(processor));
        }

        if selection.export {
            let export_config = ExportConfig::new(self.root().to_path_buf(), self.project_id);
            let exporter = Arc::new(NlDocumentExporter::new(export_config));
            let processor = NlDocumentUpdateProcessor::new(exporter)
                .with_checkpoint_manager(self.checkpoint_manager.clone())
                .with_metadata_context(Some(self.store.clone()), self.project_id)
                .with_render_inputs(hash_serializable(&self.grouper_config), String::new());
            processors.push(Arc::new(processor));
        }

        processors
    }

    /// Run a full index over the fixture through the shared storage.
    pub async fn full_index(&self) -> anyhow::Result<cce_orchestrator::IndexResult> {
        let mut orchestrator = IndexOrchestrator::new(self.project_id)
            .context("failed to create index orchestrator")?;
        orchestrator = orchestrator
            .with_metadata_store(self.store.clone())
            .with_bm25_client(self.bm25.clone())
            .with_checkpoint_manager(self.checkpoint_manager.clone());
        let options = IndexOptions {
            root_dir: self.root().to_path_buf(),
            extensions: vec!["rs".to_string()],
            store_vectors: false,
            store_bm25: true,
            store_summaries: true,
            build_relations: false,
            ..Default::default()
        };
        orchestrator
            .execute(options)
            .await
            .context("full index failed")
    }

    // ===== Durable state assertions (DB-backed, never in-memory) =====

    pub async fn active_manifest(
        &self,
    ) -> anyhow::Result<Option<cce_storage_common::metadb::ProjectIndexManifest>> {
        self.store
            .manifest_active(self.project_id)
            .await
            .context("failed to read active manifest")
    }

    pub async fn building_manifest(
        &self,
        operation_id: &str,
    ) -> anyhow::Result<Option<cce_storage_common::metadb::ProjectIndexManifest>> {
        self.store
            .manifest_by_operation(self.project_id, operation_id)
            .await
            .context("failed to read building manifest")
    }

    pub async fn module_progress_of(
        &self,
        operation_id: &str,
        file_path: &str,
    ) -> HashMap<String, String> {
        let record = self
            .checkpoint_manager
            .get_file_checkpoint(operation_id, file_path)
            .await
            .expect("read file checkpoint");
        record
            .and_then(|r| r.module_progress)
            .map(|json| read_module_progress(Some(&json)))
            .unwrap_or_default()
    }

    pub async fn checkpoint_of(
        &self,
        operation_id: &str,
    ) -> Option<cce_storage_common::metadb::CheckpointRecord> {
        self.checkpoint_manager
            .get_checkpoint(operation_id)
            .await
            .expect("read checkpoint")
    }

    /// Resolve the epoch view of a generation: `(parent, excluded files)`.
    ///
    /// Under the zero-copy inheritance model the visible data of a published
    /// generation is `own rows ∪ parent rows − overridden files`, so every
    /// durable-state assertion must resolve this view instead of counting
    /// physical rows of a single epoch.
    async fn generation_view(&self, epoch: i64) -> (Option<i64>, Vec<String>) {
        let parent = self
            .store
            .manifest_parent_epoch(self.project_id, epoch)
            .await
            .expect("read manifest parent")
            .filter(|value| *value > 0);
        let excluded = self
            .store
            .overrides_for_generation(self.project_id, epoch)
            .await
            .expect("read overrides")
            .into_iter()
            .map(|entry| entry.file_path)
            .collect();
        (parent, excluded)
    }

    /// Resolve the epoch that physically holds the visible rows of `path`
    /// for the view rooted at `start_epoch` ("own first, miss → parent").
    async fn visible_epoch_for_path(&self, start_epoch: i64, path: &str) -> Option<i64> {
        let mut current = start_epoch;
        for _ in 0..4 {
            // Rows written inside this generation win.
            if self
                .store
                .files_exists_at_epoch(self.project_id, current, path)
                .await
                .unwrap_or(false)
            {
                return Some(current);
            }
            // Overridden without own rows (deleted): nothing below is visible.
            if self
                .store
                .overrides_for_generation(self.project_id, current)
                .await
                .unwrap_or_default()
                .iter()
                .any(|entry| entry.file_path == path)
            {
                return None;
            }
            let parent: Option<i64> = self
                .store
                .manifest_parent_epoch(self.project_id, current)
                .await
                .ok()
                .flatten()
                .filter(|value| *value > 0);
            current = parent?;
        }
        None
    }

    /// Count chunks visible from one generation's epoch view.
    pub async fn chunks_for_epoch(&self, epoch: i64) -> i64 {
        let (parent, excluded) = self.generation_view(epoch).await;
        let mut total: i64 = self
            .store
            .chunks_count(self.project_id, epoch)
            .await
            .expect("count chunks");
        if let Some(parent) = parent {
            total += self
                .store
                .chunks_count(self.project_id, parent)
                .await
                .expect("count inherited chunks");
            for path in &excluded {
                total -= self
                    .store
                    .chunks_by_file(self.project_id, path, parent)
                    .await
                    .expect("count overridden chunks")
                    .len() as i64;
            }
        }
        total
    }

    /// Count entities visible from one generation's epoch view.
    pub async fn entities_for_epoch(&self, epoch: i64) -> i64 {
        let (parent, excluded) = self.generation_view(epoch).await;
        let mut total: i64 = self
            .store
            .entities_count(self.project_id, epoch)
            .await
            .expect("count entities");
        if let Some(parent) = parent {
            total += self
                .store
                .entities_count(self.project_id, parent)
                .await
                .expect("count inherited entities");
            for path in &excluded {
                total -= self
                    .store
                    .entity_ids_by_file_epoch(self.project_id, parent, path)
                    .await
                    .expect("count overridden entities")
                    .len() as i64;
            }
        }
        total
    }

    /// Count files visible from one generation's epoch view.
    pub async fn files_for_epoch(&self, epoch: i64) -> i64 {
        let (parent, excluded) = self.generation_view(epoch).await;
        let mut total: i64 = self
            .store
            .files_count(self.project_id, epoch)
            .await
            .expect("count files");
        if let Some(parent) = parent {
            total += self
                .store
                .files_count(self.project_id, parent)
                .await
                .expect("count inherited files");
            for path in &excluded {
                if self
                    .store
                    .files_exists_at_epoch(self.project_id, parent, path)
                    .await
                    .expect("count overridden files")
                {
                    total -= 1;
                }
            }
        }
        total
    }

    pub async fn bm25_document_count(&self) -> usize {
        self.bm25
            .document_count()
            .await
            .expect("count bm25 documents")
    }

    pub async fn bm25_documents_for_project(&self) -> usize {
        self.bm25
            .document_count_by_project(self.project_id)
            .await
            .expect("count bm25 documents by project")
    }

    /// Count the BM25 documents visible from one data epoch's generation view.
    ///
    /// Unlike [`Self::bm25_documents_for_project`], this excludes the retained
    /// older generations that a deletion leaves in the index until GC retires
    /// them; it reflects what an epoch-scoped production query actually sees:
    /// own-generation documents plus inherited parent documents, minus the
    /// overridden files.
    pub async fn bm25_documents_for_epoch(&self, epoch: i64) -> usize {
        // Resolve the view before taking the BM25 lock.
        let (parent, excluded) = self.generation_view(epoch).await;

        let client = &self.bm25;
        let mut documents = client
            .snapshot_documents(self.project_id, epoch)
            .await
            .expect("snapshot bm25 documents by epoch");
        if let Some(parent) = parent {
            let own_paths: std::collections::HashSet<String> = documents
                .iter()
                .filter_map(|doc| doc.fields.get("file_path").cloned())
                .collect();
            let inherited = client
                .snapshot_documents(self.project_id, parent)
                .await
                .expect("snapshot inherited bm25 documents");
            for doc in inherited {
                match doc.fields.get("file_path") {
                    // Overridden files' parent rows are hidden; replaced files'
                    // own rows already counted above.
                    Some(path) => {
                        if !excluded.contains(path) && !own_paths.contains(path) {
                            documents.push(doc);
                        }
                    }
                    None => documents.push(doc),
                }
            }
        }
        documents.len()
    }

    /// Run a BM25 query over the shared index and return (title, file_path)
    /// pairs of the top hits.
    pub async fn query_bm25(&self, query_text: &str) -> Vec<(String, String)> {
        use cce_storage_bm25::Bm25Retrieval;
        use cce_storage_common::FulltextSearchOptions;
        let client = &self.bm25;
        let manager = client.index_manager().expect("bm25 index manager").clone();
        let manager = manager.read().await;
        let schema = manager.schema().clone();
        let options = FulltextSearchOptions {
            limit: 10,
            offset: 0,
            field_weights: Default::default(),
            project_id: self.project_id,
            epochs: Vec::new(),
            excluded_files: None,
            exclude_test: false,
            exclude_generated: false,
            exclude_dependency: false,
            include_categories: Vec::new(),
            exclude_categories: Vec::new(),
            term_operator: Default::default(),
        };
        let results = Bm25Retrieval::new()
            .search(&manager, &schema, query_text, &options)
            .expect("bm25 search");
        results
            .into_iter()
            .map(|result| {
                let title = result.fields.get("title").cloned().unwrap_or_default();
                let file_path = result.fields.get("file_path").cloned().unwrap_or_default();
                (title, file_path)
            })
            .collect()
    }

    pub fn parse_count(&self) -> usize {
        self.parse_counter.load(Ordering::Relaxed)
    }

    pub fn summary_count(&self) -> usize {
        self.summary_counter.load(Ordering::Relaxed)
    }

    pub fn bm25_index_count(&self) -> usize {
        self.bm25_index_counter.load(Ordering::Relaxed)
    }

    pub fn embedding_count(&self) -> usize {
        self.embedding_counter.load(Ordering::Relaxed)
    }

    /// Entity IDs of one file in one published epoch, ascending.
    ///
    /// The cross-epoch stability invariant: for a file whose content did not
    /// change, this set must be identical across adjacent epochs; for a
    /// changed file the sets must be disjoint (hot-update parses are seeded
    /// above the previous maximum).
    pub async fn entity_ids_for_epoch(&self, epoch: i64, path: &str) -> Vec<i64> {
        // Resolve through the inheritance chain: an unchanged file's rows
        // stay in the generation that last wrote them.
        let resolved = self
            .visible_epoch_for_path(epoch, path)
            .await
            .unwrap_or(epoch);
        self.store
            .entity_ids_by_file_epoch(self.project_id, resolved, path)
            .await
            .expect("query entity ids")
    }

    /// (chunk_id, entity_ids) pairs of one file in one epoch, ordered by
    /// chunk_id.
    ///
    /// Chunk rows carry the *source* entity IDs of the parse that produced
    /// them. Unlike the entities table (whose AUTOINCREMENT row ids are
    /// renumbered by every candidate clone), the source IDs are the stable
    /// cross-epoch identity the system guarantees: the candidate clone copies
    /// chunk rows verbatim, and fresh parses are seeded above the previous
    /// maximum so their source IDs never collide with preserved ones.
    pub async fn chunks_for_file_epoch(&self, epoch: i64, path: &str) -> Vec<(String, Vec<i64>)> {
        // Resolve through the inheritance chain: an unchanged file's rows
        // stay in the generation that last wrote them.
        let resolved = self
            .visible_epoch_for_path(epoch, path)
            .await
            .unwrap_or(epoch);
        self.store
            .chunks_by_file_epoch(self.project_id, path, resolved)
            .await
            .expect("query chunks")
            .into_iter()
            .map(|chunk| {
                let ids: Vec<i64> =
                    serde_json::from_str(&chunk.entity_ids).unwrap_or_else(|_| Vec::new());
                (chunk.chunk_id, ids)
            })
            .collect()
    }

    /// Count the file-table rows visibly belonging to `path` under the
    /// generation view rooted at `epoch` ("own first, miss → parent").
    pub async fn visible_file_rows(&self, epoch: i64, path: &str) -> i64 {
        let Some(resolved) = self.visible_epoch_for_path(epoch, path).await else {
            return 0;
        };
        if self
            .store
            .files_exists_at_epoch(self.project_id, resolved, path)
            .await
            .expect("count visible file rows")
        {
            1
        } else {
            0
        }
    }

    /// Flip an operation checkpoint back to in_progress (simulating a crash
    /// between publication and checkpoint completion).
    pub async fn reopen_checkpoint(&self, operation_id: &str) -> anyhow::Result<()> {
        self.store
            .checkpoint_set_status(
                self.project_id,
                operation_id,
                cce_storage_common::metadb::CheckpointStatus::InProgress,
            )
            .await
            .context("failed to reopen checkpoint")?;
        Ok(())
    }

    /// Set the module_progress of a file checkpoint to NULL directly.
    pub async fn clear_module_progress_direct(&self, operation_id: &str) -> anyhow::Result<()> {
        self.store
            .file_checkpoints_clear_module_progress(self.project_id, operation_id)
            .await
            .context("failed to clear module progress")?;
        Ok(())
    }
}

/// Helper to establish the change-detection baseline on the shared store.
pub async fn initialize_cache(instance: &mut CoordinatorInstance, root: &std::path::Path) {
    instance
        .coordinator
        .initialize_cache(root)
        .await
        .expect("cache initialization failed");
}

/// Poll `f` until it returns `true` or the timeout elapses.
pub async fn wait_until<F, Fut>(mut f: F, timeout: Duration) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if f().await {
            return true;
        }
        if std::time::Instant::now() > deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// A hot-update config with fast debounce and high storm thresholds so the
/// mode never degrades mid-assertion (shared by watch/debounce tests).
pub fn fast_watch_config() -> cce_config::HotUpdateConfig {
    let mut config = cce_config::HotUpdateConfig::default();
    config.debounce.pending_interval_secs = 1;
    config.debounce.max_wait_time_secs = 5;
    config.file_watch.event_threshold = 1000;
    config.file_watch.storm_duration_secs = 60;
    config.file_watch.recovery_threshold = 500;
    config.file_watch.recovery_duration_secs = 60;
    config.file_watch.fallback_interval_secs = 2;
    config
}

/// Start the real watch chain: `start_watch` + `start_event_loop` + the
/// background processor behind an `Arc<Mutex>` (mirrors production wiring).
///
/// The coordinator is moved out of `instance`; callers must not use
/// `instance.coordinator` afterwards.
pub async fn start_watch_chain(
    instance: &mut CoordinatorInstance,
    root: &std::path::Path,
    project_id: i64,
) -> Arc<Mutex<HotUpdateCoordinator>> {
    instance
        .coordinator
        .start_watch(root)
        .await
        .expect("start watch");
    instance
        .coordinator
        .start_event_loop()
        .await
        .expect("start event loop");
    let placeholder = HotUpdateCoordinator::new(cce_config::HotUpdateConfig::default(), project_id)
        .expect("placeholder coordinator");
    let coordinator = Arc::new(Mutex::new(std::mem::replace(
        &mut instance.coordinator,
        placeholder,
    )));
    HotUpdateCoordinator::start_background_processor_from_arc(coordinator.clone()).await;

    // Let the OS-level watcher fully register its watches before tests write
    // files; writes racing the registration are silently lost by the kernel.
    tokio::time::sleep(Duration::from_millis(500)).await;
    coordinator
}

/// Stop the real watcher so no notify task outlives the test.
pub async fn stop_watch_chain(coordinator: &Arc<Mutex<HotUpdateCoordinator>>) {
    let mut coord = coordinator.lock().await;
    let _ = coord.stop_watch().await;
}

/// The manifest state of a simulated crash point.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CrashManifest {
    /// No building manifest (crash before candidate preparation).
    #[default]
    None,
    /// Building manifest that was never marked ready (crash during clone).
    BuildingUnready,
    /// Fully-cloned building manifest, adoptable on resume.
    BuildingReady,
}

/// One file of a simulated crash checkpoint.
#[derive(Debug, Clone)]
pub struct CrashFile {
    pub path: String,
    /// Compressed parsed envelope to persist. `None` forces a fresh parse on
    /// resume.
    pub envelope: Option<Vec<u8>>,
    /// Content hash recorded on the checkpoint.
    pub content_hash: Option<String>,
    /// Module progress markers recorded on the checkpoint.
    pub module_progress: Option<String>,
}

impl CrashFile {
    pub fn reparsing(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            envelope: None,
            content_hash: None,
            module_progress: None,
        }
    }
}

/// Simulated crash point: an in_progress operation checkpoint whose file
/// checkpoints and manifest reflect exactly the durable state a crash would
/// leave behind at a given pipeline stage.
pub struct CrashState {
    pub operation_id: String,
    pub root_dir: String,
    pub files: Vec<CrashFile>,
    pub manifest: CrashManifest,
}

impl HotUpdateHarness {
    /// Parse a fixture file with the hot-update pipeline (no counters — this
    /// is setup, not the operation under test).
    ///
    /// The parse result records the project-relative path (matching a real
    /// `update()` run) so crash-state envelopes, checkpoint keys and storage
    /// rows stay consistent with the relative-path scheme used by the `files`
    /// table and change detection.
    pub async fn parse_file(
        &self,
        path: &std::path::Path,
    ) -> anyhow::Result<ParseResultWithChanges> {
        use cce_orchestrator::hot_update::FileProcessor;
        let parse_path = self.relativize(path);

        // Reuse an earlier parse of the same file (see `parsed_cache`).
        if let Some(cached) = self
            .parsed_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&parse_path)
        {
            let mut result = ParseResultWithChanges::new(
                parse_path.clone().into(),
                cached.clone(),
                FileChangeType::Modified,
                false,
            );
            result.file_summary = None;
            return Ok(result);
        }

        let seed = self.entity_id_seed.load(Ordering::Relaxed);
        let mut processor = FileProcessor::with_entity_id_seed(seed);
        let result = processor
            .process_file_change_at(
                path,
                &parse_path,
                FileChangeType::Modified,
                &Some(self.store.clone()),
                self.project_id,
            )
            .await
            .context("failed to parse fixture file")?;
        // Mirror production's `entity_id_seed()`: the next file starts one above
        // the maximum entity ID this parse produced.
        if let Some(max) = result.parsed_file.entities.iter().map(|e| e.id.0).max() {
            self.entity_id_seed
                .store(max.saturating_add(1), Ordering::Relaxed);
        }
        self.parsed_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(parse_path, result.parsed_file.clone());
        Ok(result)
    }

    /// Convert an on-disk fixture path into its project-relative form.
    pub fn relativize(&self, path: &std::path::Path) -> String {
        path.strip_prefix(self.fixture.root())
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    /// Persist a simulated crash state on the durable store.
    pub async fn create_crash_state(&self, state: &CrashState) -> anyhow::Result<()> {
        use cce_orchestrator::operation::checkpoint::CreateCheckpointParams;
        use cce_types::OperationKind;

        let first = state
            .files
            .first()
            .map(|f| f.path.clone())
            .unwrap_or_default();
        let last = state
            .files
            .last()
            .map(|f| f.path.clone())
            .unwrap_or_default();

        self.checkpoint_manager
            .create_checkpoint(CreateCheckpointParams {
                operation_id: &state.operation_id,
                operation_type: OperationKind::HotUpdate,
                root_dir: &state.root_dir,
                total_files: state.files.len() as u32,
                batch_size: 1,
                file_list_hash: "",
            })
            .await
            .context("failed to create crash-state operation checkpoint")?;
        self.checkpoint_manager
            .create_batch_checkpoint(
                &state.operation_id,
                0,
                &first,
                &last,
                state.files.len() as u32,
            )
            .await
            .context("failed to create crash-state batch checkpoint")?;

        for file in &state.files {
            let mut record = self
                .checkpoint_manager
                .create_file_checkpoint(&state.operation_id, 0, &file.path)
                .await
                .context("failed to create crash-state file checkpoint")?;
            record.parsed_data = file.envelope.clone();
            record.content_hash = file.content_hash.clone();
            record.module_progress = file.module_progress.clone();
            self.checkpoint_manager
                .save_file_checkpoint(&record)
                .await
                .context("failed to save crash-state file checkpoint")?;
        }

        if state.manifest != CrashManifest::None {
            let active_epoch = self
                .active_manifest()
                .await?
                .map(|m| m.data_epoch)
                .unwrap_or(0);
            let candidate_epoch = active_epoch.saturating_add(1);
            self.store
                .manifest_begin_building(
                    self.project_id,
                    candidate_epoch,
                    &state.operation_id,
                    None,
                )
                .await
                .context("failed to create crash-state manifest")?;
            if state.manifest == CrashManifest::BuildingReady {
                self.store
                    .manifest_mark_candidate_ready(self.project_id, &state.operation_id)
                    .await
                    .context("failed to mark crash-state manifest ready")?;
            }
        }

        Ok(())
    }

    /// Build a real parsed envelope for a fixture file, including the
    /// content hash and an optional module progress marker map.
    pub async fn envelope_for(
        &self,
        path: &std::path::Path,
        module_markers: &[(&str, String)],
    ) -> anyhow::Result<CrashFile> {
        use cce_orchestrator::hot_update::progress::write_module_progress;
        use cce_orchestrator::operation::checkpoint::{
            ParsedCheckpointEnvelope, ParsedCheckpointPayload, encode_parsed_checkpoint,
        };

        let parse_result = self.parse_file(path).await?;
        let content = std::fs::read(path).context("failed to read fixture file")?;
        let content_hash = cce_utils::hash::calculate_hash(&content);
        let payload = ParsedCheckpointPayload::Parsed(Box::new(ParsedCheckpointEnvelope::new(
            cce_orchestrator::hot_update::FileChangeType::Modified,
            parse_result.parsed_file.clone(),
        )));
        let markers: std::collections::HashMap<String, String> = module_markers
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();

        Ok(CrashFile {
            path: self.relativize(path),
            envelope: Some(
                encode_parsed_checkpoint(&payload).context("failed to encode envelope")?,
            ),
            content_hash: Some(content_hash),
            module_progress: if markers.is_empty() {
                None
            } else {
                Some(write_module_progress(&markers))
            },
        })
    }

    /// The BM25 module fingerprint the harness's processors compute for a file
    /// content (must match the `chunking_fingerprint` of `ProcessorContext`'s
    /// default-configured file processor).
    pub fn bm25_marker_for_content(&self, content: &[u8]) -> String {
        use cce_orchestrator::hot_update::progress::module_input_fingerprint;
        let chunking_config = AstToNlConfig::default().chunking.clone();
        let chunking_fp = hash_serializable(&chunking_config);
        module_input_fingerprint(&chunking_fp, &cce_utils::hash::calculate_hash(content))
    }

    /// The embedding module fingerprint for a file content (same inputs as the
    /// BM25 marker: chunking configuration + content hash).
    pub fn embedding_marker_for_content(&self, content: &[u8]) -> String {
        use cce_orchestrator::hot_update::progress::module_input_fingerprint;
        let chunking_config = AstToNlConfig::default().chunking.clone();
        let chunking_fp = hash_serializable(&chunking_config);
        module_input_fingerprint(&chunking_fp, &cce_utils::hash::calculate_hash(content))
    }

    /// Seed the candidate epoch's file/entity rows exactly like a crashed
    /// run's bm25 module would have (the module skipped on resume because its
    /// progress marker matched, so nobody re-writes these rows).
    pub async fn seed_candidate_epoch_data(
        &self,
        epoch: i64,
        paths: &[String],
    ) -> anyhow::Result<()> {
        use cce_orchestrator::index::FileProcessor as IndexFileProcessor;

        let storage = Arc::new(
            StorageCoordinator::new(self.project_id)
                .expect("valid project id")
                .with_metadata_store(self.store.clone())
                .with_bm25(self.bm25.clone())
                .with_epoch(epoch)
                .with_batch_id(0),
        );
        let mut parsed_files = Vec::new();
        let mut bm25_writes = Vec::new();
        let mut processor =
            IndexFileProcessor::with_pre_processor_config(self.grouper_config.clone());
        for path in paths {
            let parse_result = self.parse_file(std::path::Path::new(path)).await?;
            let parsed = parse_result.parsed_file;
            // A real interrupted run writes the per-function BM25 documents
            // before it crashes, so the seed reproduces the bm25 module's
            // storage writes (files/entities rows + BM25 documents/chunk
            // records). Skipping the docs would leave the resumed generation
            // missing every unchanged file's searchable document.
            let chunks = processor
                .process_parsed_file(&parsed)
                .await
                .map(|(chunks, _)| chunks)
                .context("failed to chunk seeded candidate file")?;
            parsed_files.push(parsed.clone());
            bm25_writes.push((parsed.path, chunks));
        }
        storage
            .store_parsed_files(&parsed_files)
            .await
            .context("failed to seed candidate epoch data")?;
        for (path, chunks) in bm25_writes {
            storage
                .hot_update_bm25_file(std::path::Path::new(&path), &chunks)
                .await
                .context("failed to seed candidate bm25 documents")?;
        }
        Ok(())
    }

    /// Rewrite every file checkpoint envelope of an operation with an
    /// incompatible schema version (simulating a parser/schema upgrade).
    pub async fn tamper_envelope_schema_version(&self, operation_id: &str) -> anyhow::Result<()> {
        use cce_orchestrator::operation::checkpoint::{
            ParsedCheckpointPayload, decode_parsed_checkpoint, encode_parsed_checkpoint,
        };
        let records = self
            .checkpoint_manager
            .get_batch_files(operation_id, 0)
            .await
            .context("failed to query envelopes")?;
        for record in records {
            let Some(bytes) = record.parsed_data else {
                continue;
            };
            let mut payload: ParsedCheckpointPayload =
                decode_parsed_checkpoint(&bytes).context("failed to decode envelope")?;
            if let ParsedCheckpointPayload::Parsed(ref mut envelope) = payload {
                envelope.schema_version = 999;
            }
            let encoded =
                encode_parsed_checkpoint(&payload).context("failed to encode envelope")?;
            self.store
                .checkpoint_file_set_parsed_data(
                    self.project_id,
                    operation_id,
                    &record.file_path,
                    &encoded,
                )
                .await
                .context("failed to tamper envelope")?;
        }
        Ok(())
    }

    /// Absolute output document path for a fixture file (matches the export
    /// processor's `.cce/nl_docs` layout).
    pub fn exported_doc_path(&self, relative_source: &str) -> PathBuf {
        self.root()
            .join(".cce")
            .join("nl_docs")
            .join(format!("{relative_source}.md"))
    }
}
