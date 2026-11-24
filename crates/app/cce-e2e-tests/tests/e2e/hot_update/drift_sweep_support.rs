//! Shared scaffolding for the chunking/embedding drift-sweep e2e scenarios.
//!
//! Builds a project over durable state (file-backed SQLite, real BM25 client,
//! capturing mock Qdrant) and drives embedding/BM25 processors through their
//! real prepare → process → commit lifecycle, exactly like a hot-update
//! operation would. No server, no network, no real LLM: vectors come from
//! [`HashEmbedder`], points land in the [`CapturingMockQdrant`].

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context;
use tempfile::TempDir;

use cce_config::modules::{DistanceMetric, QdrantConfig};
use cce_config::{AstToNlConfig, NestProcessorConfig};
use cce_e2e_tests::mock_qdrant::CapturingMockQdrant;
use cce_llm_client::OpenAICompatibleProvider;
use cce_orchestrator::hot_update::processors::ProcessorContext;
use cce_orchestrator::hot_update::{BatchChangeResult, FileChangeType, ParseResultWithChanges};
use cce_orchestrator::index::MetadataStore;
use cce_orchestrator::index::StorageCoordinator;
use cce_orchestrator::operation::{OperationContext, OperationProcessResult, OperationType};
use cce_orchestrator::{
    Bm25UpdateProcessor, CheckpointManager, EmbeddingUpdateProcessor, UpdateProcessor,
};
use cce_storage_bm25::{Bm25Client, Bm25Config};
use cce_storage_common::MetadataStorage;
use cce_storage_common::metadb::MetadataBackendStore;
use cce_storage_vector_qdrant::{QdrantClient, types as qdrant_types};

/// Qdrant payload isolation group used for every scenario.
pub const GROUP_ID: &str = "drift-project-root";
const PROJECT_ID: i64 = 1;
const EMBEDDER_DIMENSION: usize = 8;

/// Durable state shared by all operations of one drift-sweep test.
pub struct DriftScaffold {
    pub fixture: crate::helper::EmptyFixture,
    pub store: MetadataStore,
    pub checkpoint_manager: Arc<CheckpointManager>,
    pub bm25: Arc<Bm25Client>,
    pub qdrant: CapturingMockQdrant,
    entity_id_seed: Arc<AtomicU64>,
    _db_dir: TempDir,
    _bm25_dir: TempDir,
}

impl DriftScaffold {
    pub const GROUP_ID: &str = GROUP_ID;

    pub async fn new() -> anyhow::Result<Self> {
        let fixture = crate::helper::EmptyFixture::new().context("failed to create fixture")?;
        let db_dir = TempDir::new().context("failed to create db temp dir")?;
        let scoped = Arc::new(
            cce_storage_metadb_sqlite::SqliteClient::with_path(
                db_dir.path().join("cce.db").to_string_lossy(),
            )
            .context("failed to create file-backed SQLite")?,
        )
        .for_project(PROJECT_ID)
        .context("failed to scope store to project")?;

        // Production semantics: root_path is the real project directory;
        // chunking-drift sweeps resolve swept files against it.
        scoped
            .ensure_project(PROJECT_ID, &fixture.root().to_string_lossy())
            .await
            .context("failed to point the project row at the fixture root")?;

        let bm25_dir = TempDir::new().context("failed to create bm25 temp dir")?;
        let bm25_config = Bm25Config::default()
            .enabled()
            .with_index_name("default")
            .with_index_path(bm25_dir.path().join("index").to_string_lossy());
        let mut bm25 = Bm25Client::new(bm25_config);
        bm25.connect().await.context("failed to connect BM25")?;

        let store = MetadataBackendStore::Local(scoped);
        Ok(Self {
            fixture,
            store: store.clone(),
            checkpoint_manager: Arc::new(CheckpointManager::new_for_project(PROJECT_ID, store)),
            bm25: Arc::new(bm25),
            qdrant: CapturingMockQdrant::spawn().await,
            entity_id_seed: Arc::new(AtomicU64::new(0)),
            _db_dir: db_dir,
            _bm25_dir: bm25_dir,
        })
    }

    pub fn root(&self) -> &Path {
        self.fixture.root()
    }

    pub fn add_file(&self, relative: &str, content: &str) -> anyhow::Result<PathBuf> {
        self.fixture
            .add_file(relative, content)
            .context("failed to add fixture file")
    }

    /// Storage coordinator pointed at the shared durable state. The epoch is
    /// only a fallback: `prepare_operation` advances it to the candidate
    /// epoch before any write happens.
    pub async fn storage_with_embedder(
        &self,
        embedder: Arc<OpenAICompatibleProvider>,
    ) -> Arc<StorageCoordinator> {
        self.storage_inner(Some(embedder)).await
    }

    /// Storage coordinator without an embedder (BM25-only scenarios).
    pub async fn storage_without_embedder(&self) -> Arc<StorageCoordinator> {
        self.storage_inner(None).await
    }

    async fn storage_inner(
        &self,
        embedder: Option<Arc<OpenAICompatibleProvider>>,
    ) -> Arc<StorageCoordinator> {
        let active_epoch = self.active_epoch().await;
        let qdrant_config = QdrantConfig {
            url: self.qdrant.url().to_string(),
            vector_size: EMBEDDER_DIMENSION,
            distance_metric: DistanceMetric::Cosine,
            timeout_ms: 5000,
            max_retries: 0,
            retry_delay_ms: 10,
            enabled: true,
            ..Default::default()
        };
        let qdrant =
            Arc::new(QdrantClient::new(qdrant_config, ".").expect("qdrant client must build"));
        let mut storage = StorageCoordinator::new(PROJECT_ID)
            .expect("valid project id")
            .with_metadata_store(self.store.clone())
            .with_bm25(self.bm25.clone())
            .with_qdrant(qdrant)
            .with_project_group_id(GROUP_ID)
            .with_epoch(active_epoch);
        if let Some(embedder) = embedder {
            storage = storage.with_embedder(embedder);
        }
        Arc::new(storage)
    }

    async fn active_epoch(&self) -> i64 {
        self.store
            .manifest_active(PROJECT_ID)
            .await
            .ok()
            .flatten()
            .map(|manifest| manifest.data_epoch)
            .unwrap_or(0)
    }

    /// The currently published (active) data epoch, or 0 when never published.
    pub async fn published_epoch(&self) -> i64 {
        self.active_epoch().await
    }

    /// Processor context for the shared SQLite database. `ast_to_nl`
    /// overrides the chunking configuration (used to construct a drifted
    /// pipeline fingerprint).
    ///
    /// Both chunk paths are enabled so the embedding processor has vectors to
    /// write; the default config only emits BM25-path chunks.
    pub fn context(
        &self,
        storage: Arc<StorageCoordinator>,
        ast_to_nl: Option<&AstToNlConfig>,
    ) -> Arc<ProcessorContext> {
        let config = match ast_to_nl {
            Some(config) => config.clone(),
            None => Self::base_ast_to_nl_config(),
        };
        let index_processor = cce_orchestrator::index::FileProcessor::with_config(&config)
            .with_project_id(PROJECT_ID);
        let context = ProcessorContext::new(storage, NestProcessorConfig::default());
        let mut context = context.with_checkpoint_manager(self.checkpoint_manager.clone());
        context.file_processor = Arc::new(tokio::sync::Mutex::new(index_processor));
        Arc::new(context)
    }

    /// Base AST-to-NL config with both chunk output paths enabled.
    pub fn base_ast_to_nl_config() -> AstToNlConfig {
        AstToNlConfig {
            default_mode: cce_types::OutputMode::Both,
            ..AstToNlConfig::default()
        }
    }

    /// Embedding + BM25 processor pair sharing `context`.
    pub fn processor_pair(context: Arc<ProcessorContext>) -> Vec<Arc<dyn UpdateProcessor>> {
        vec![
            Arc::new(EmbeddingUpdateProcessor::new(Arc::clone(&context))),
            Arc::new(Bm25UpdateProcessor::new(context)),
        ]
    }

    /// Parse a fixture file with the hot-update pipeline, mirroring
    /// production's monotonic entity-ID seeding across files.
    pub async fn parse_file(&self, relative: &str) -> anyhow::Result<ParseResultWithChanges> {
        use cce_orchestrator::hot_update::FileProcessor;
        let absolute = self.root().join(relative);
        let seed = self.entity_id_seed.load(Ordering::Relaxed);
        let mut processor = FileProcessor::with_entity_id_seed(seed);
        let result = processor
            .process_file_change_at(
                &absolute,
                relative,
                FileChangeType::Modified,
                &Some(self.store.clone()),
                PROJECT_ID,
            )
            .await
            .context("failed to parse fixture file")?;
        if let Some(max) = result.parsed_file.entities.iter().map(|e| e.id.0).max() {
            self.entity_id_seed
                .store(max.saturating_add(1), Ordering::Relaxed);
        }
        Ok(result)
    }

    /// Run one full hot-update operation (prepare → process → commit) over
    /// `batch` with the given processors. Mirrors production wiring by
    /// creating the operation checkpoint first — module progress markers
    /// carry a foreign key to it.
    pub async fn run_operation(
        &self,
        processors: &[Arc<dyn UpdateProcessor>],
        batch: BatchChangeResult,
        operation_id: &str,
    ) -> anyhow::Result<Vec<OperationProcessResult>> {
        use cce_orchestrator::operation::checkpoint::CreateCheckpointParams;
        use cce_types::OperationKind;

        self.checkpoint_manager
            .create_checkpoint(CreateCheckpointParams {
                operation_id,
                operation_type: OperationKind::HotUpdate,
                root_dir: &self.root().to_string_lossy(),
                total_files: batch.parse_results.len() as u32,
                batch_size: 1,
                file_list_hash: "",
            })
            .await
            .context("failed to create operation checkpoint")?;
        let path_of =
            |result: &ParseResultWithChanges| result.file_path.to_string_lossy().into_owned();
        let first_file = batch.parse_results.first().map(path_of).unwrap_or_default();
        let last_file = batch.parse_results.last().map(path_of).unwrap_or_default();
        self.checkpoint_manager
            .create_batch_checkpoint(
                operation_id,
                0,
                &first_file,
                &last_file,
                batch.parse_results.len() as u32,
            )
            .await
            .context("failed to create batch checkpoint")?;

        let ctx = OperationContext::new(
            PROJECT_ID,
            operation_id.to_string(),
            OperationType::HotUpdate,
            batch.parse_results.len(),
        );
        for processor in processors {
            processor
                .prepare_operation(&ctx)
                .await
                .context("prepare_operation failed")?;
        }
        let mut batch = batch;
        let mut results = Vec::new();
        for processor in processors {
            results.push(
                processor
                    .process_operation(&ctx, &mut batch)
                    .await
                    .context("process_operation failed")?,
            );
        }
        for processor in processors {
            processor
                .commit_operation(&ctx)
                .await
                .context("commit_operation failed")?;
        }

        // Mirror the change detector's post-publication hash backfill:
        // candidate file rows are written without content hashes and only
        // get them once the manifest is active.
        let published = self.published_epoch().await;
        let backfill: Vec<(String, String)> = batch
            .parse_results
            .iter()
            .filter_map(|result| {
                result
                    .parsed_file
                    .file_hash
                    .clone()
                    .map(|hash| (result.file_path.to_string_lossy().to_string(), hash))
            })
            .collect();
        self.store
            .files_record_hashes(PROJECT_ID, published, &backfill)
            .await
            .expect("backfill file content hash");

        Ok(results)
    }

    pub async fn meta_string(&self, key: &str) -> Option<String> {
        self.store
            .project_meta_get_string_optional(PROJECT_ID, key)
            .await
            .expect("read project meta")
    }

    /// Chunk rows of one file at one epoch: `(chunk_id, entity_ids_json)`.
    pub async fn chunks_at_epoch(&self, epoch: i64, path: &str) -> Vec<(String, String)> {
        self.store
            .chunks_by_file_epoch(PROJECT_ID, path, epoch)
            .await
            .expect("query chunks")
            .into_iter()
            .map(|chunk| (chunk.chunk_id, chunk.entity_ids))
            .collect()
    }

    /// Entity detail mappings of one file at one epoch:
    /// `(source_entity_db_id, [qdrant point ids])`.
    pub async fn mappings_at_epoch(&self, epoch: i64, path: &str) -> Vec<(i64, Vec<String>)> {
        self.store
            .mappings_point_ids_by_file_epoch(PROJECT_ID, epoch, path)
            .await
            .expect("query mappings")
            .into_iter()
            .map(|(entity_id, ids_json)| {
                let ids: Vec<String> =
                    serde_json::from_str(&ids_json).unwrap_or_else(|_| Vec::new());
                (entity_id, ids)
            })
            .collect()
    }

    /// Number of file rows for `path` at `epoch`.
    pub async fn file_rows_at_epoch(&self, epoch: i64, path: &str) -> i64 {
        if self
            .store
            .files_exists_at_epoch(PROJECT_ID, epoch, path)
            .await
            .expect("count file rows")
        {
            1
        } else {
            0
        }
    }

    /// Chunk row contents of one file at one epoch (embedding path).
    pub async fn chunk_contents_at_epoch(&self, epoch: i64, path: &str) -> Vec<String> {
        self.store
            .chunks_by_file_epoch(PROJECT_ID, path, epoch)
            .await
            .expect("query chunk contents")
            .into_iter()
            .filter(|chunk| chunk.path == "emb")
            .map(|chunk| chunk.content)
            .collect()
    }

    /// Total physical chunk rows of one file across all epochs.
    pub async fn total_chunks_for_path(&self, path: &str) -> i64 {
        self.store
            .chunks_count_by_path(PROJECT_ID, path)
            .await
            .expect("count total chunks")
    }

    /// BM25 documents visible at one epoch.
    pub async fn bm25_docs_at_epoch(&self, epoch: i64) -> usize {
        self.bm25
            .snapshot_documents(PROJECT_ID, epoch)
            .await
            .expect("snapshot bm25 documents")
            .len()
    }

    /// The UUID the client derives from a logical point id.
    pub fn uuid_of(logical_point_id: &str) -> String {
        qdrant_types::to_qdrant_point_id(logical_point_id).to_string()
    }

    /// Deterministic embedder bound to `model_name`, backed by a
    /// model-seeded mock server.
    ///
    /// Vectors are pure functions of `(model_name, text)`, so switching the
    /// model name changes every vector byte-exactly while repeated calls
    /// under one model are stable. The server guard must stay alive for the
    /// whole test; dropping it shuts the endpoint down.
    pub async fn embedder(model_name: &str) -> (Arc<OpenAICompatibleProvider>, MockModelServer) {
        let server = MockModelServer::start().await;
        let mut config = cce_config::AppConfig::default();
        let mut providers = HashMap::new();
        providers.insert(
            "mock".to_string(),
            cce_config::modules::ProviderConfig {
                id: "mock".to_string(),
                name: "Mock".to_string(),
                base_url: server.base_url.clone(),
                api_keys: vec!["sk-mock".to_string()],
                max_retries: 0,
                retry_delay_ms: 0,
                rate_limit: 0,
                ..Default::default()
            },
        );
        config.llm.providers = providers;
        let mut models = HashMap::new();
        models.insert(
            model_name.to_string(),
            cce_config::modules::EmbeddingModelConfig {
                provider_id: "mock".to_string(),
                model: model_name.to_string(),
                vector_dimension: EMBEDDER_DIMENSION,
                ..Default::default()
            },
        );
        config.llm.embedding_models = models;
        config.embedder.default_model = model_name.to_string();
        let provider = Arc::new(
            OpenAICompatibleProvider::from_model(&config, model_name)
                .expect("mock embedder must build"),
        );
        (provider, server)
    }
}

/// Mock embeddings endpoint whose vectors are seeded by the request's model
/// name, so an embedder-model change is observable as byte-exact vector
/// drift. Serves the OpenAI-compatible `/embeddings` shape over plain TCP
/// for the real `OpenAICompatibleProvider`.
pub struct MockModelServer {
    /// Base URL to configure the embedder with.
    pub base_url: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl MockModelServer {
    async fn start() -> Self {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock embedding server");
        let port = listener.local_addr().expect("local addr").port();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            loop {
                let conn = tokio::select! {
                    conn = listener.accept() => conn,
                    _ = &mut shutdown_rx => break,
                };
                let (mut stream, _) = match conn {
                    Ok(conn) => conn,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let mut buf = Vec::with_capacity(4096);
                    let mut tmp = [0u8; 4096];
                    let body = loop {
                        match stream.read(&mut tmp).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => {
                                buf.extend_from_slice(&tmp[..n]);
                                if let Some((header_end, content_length)) = parse_http_headers(&buf)
                                {
                                    let body_start = header_end + 4;
                                    if buf.len() >= body_start + content_length {
                                        break buf[body_start..body_start + content_length]
                                            .to_vec();
                                    }
                                }
                            }
                        }
                    };
                    let reply = embedding_reply(&body);
                    let _ = stream.write_all(&reply).await;
                });
            }
        });
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            shutdown: Some(shutdown_tx),
            task: Some(task),
        }
    }

    /// Shut the server down explicitly (also happens on drop).
    #[allow(dead_code)]
    pub async fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for MockModelServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

fn parse_http_headers(buf: &[u8]) -> Option<(usize, usize)> {
    let haystack = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let header = String::from_utf8_lossy(&buf[..haystack]);
    let content_length = header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    Some((haystack, content_length))
}

fn embedding_reply(body: &[u8]) -> Vec<u8> {
    let json_body = |status: &str, payload: String| {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len(),
        )
        .into_bytes()
    };
    let request: serde_json::Value = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => {
            return b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec();
        }
    };
    let model = request
        .get("model")
        .and_then(|model| model.as_str())
        .unwrap_or("mock");
    let inputs: Vec<String> = match request.get("input") {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        Some(serde_json::Value::String(text)) => vec![text.clone()],
        _ => {
            return b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec();
        }
    };
    let data: Vec<serde_json::Value> = inputs
        .iter()
        .enumerate()
        .map(|(index, text)| {
            serde_json::json!({
                "index": index,
                "embedding": model_seeded_vector(model, text, EMBEDDER_DIMENSION),
            })
        })
        .collect();
    let payload = serde_json::json!({
        "model": model,
        "data": data,
        "usage": {"prompt_tokens": 0, "total_tokens": 0},
    })
    .to_string();
    json_body("200 OK", payload)
}

fn model_seeded_vector(model: &str, text: &str, dimension: usize) -> Vec<f32> {
    let mut vector = Vec::with_capacity(dimension);
    for index in 0..dimension {
        let hash = fnv1a(format!("{model}:{text}:{index}").as_bytes());
        vector.push((hash % 20_001) as f32 / 10_000.0 - 1.0);
    }
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        vector.iter().map(|value| value / norm).collect()
    } else {
        vector
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Convenience: paths changed by an operation (for skip-set assertions).
pub fn changed_paths(batch: &BatchChangeResult) -> HashSet<String> {
    batch
        .parse_results
        .iter()
        .map(|result| result.file_path.to_string_lossy().to_string())
        .collect()
}
