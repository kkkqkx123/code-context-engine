use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::FixtureSpec;
use crate::baselines::{entity_based, full_pipeline, full_pipeline_raw};
use crate::bench_data::{
    BenchmarkData, Bm25DocRecord, CallEdgeData, ChunkData, QueryData, RetrieverDataset,
};
use crate::embedding::{EmbeddingConfig, EmbeddingProviderType};
use cce_config::{
    AppConfig,
    modules::{EmbeddingModelConfig, ProviderConfig},
};
use cce_llm::{EmbeddingResult, LlmError};
use cce_llm_client::OpenAICompatibleProvider;
use cce_orchestrator::index::FileProcessor;
use cce_parser::ast_to_nl::chunker::{ChunkPath, ChunkedResult};
use cce_plugin::PluginRegistry;
use cce_plugin_runtime::FilePluginSource;
use cce_scanner::{FSScanner, ScanOptions};
use cce_types::{OutputMode, ParsedFile};

pub fn data_dir(baseline: &str, project: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("benchmark")
        .join(baseline)
        .join(project)
        .join("bge-m3")
}

pub struct GeneratedChunks {
    pub embedding_chunks: Vec<ChunkData>,
    pub embedding_texts: Vec<String>,
    pub bm25_chunks: Vec<ChunkData>,
    pub bm25_texts: Vec<String>,
    /// BM25 document fields (title/content/keywords), index-aligned with
    /// `bm25_chunks` and `bm25_texts`.
    pub bm25_documents: Vec<Bm25DocRecord>,
    /// In-project direct call edges (full_pipeline only). Empty for baselines
    /// that do not build a relation index.
    pub call_edges: Vec<CallEdgeData>,
}

/// Build the BM25 document records for a chunk list.
///
/// Mirrors the production field rules in
/// `cce_orchestrator::index::storage_coordinator::mapping::build_bm25_documents`
/// (title = `bm25_title`, keywords = `bm25_keywords` joined, content = text)
/// so the offline scorer consumes exactly the fields production indexes.
pub fn bm25_doc_records(
    chunks: &[cce_parser::ast_to_nl::chunker::ChunkedResult],
) -> Vec<Bm25DocRecord> {
    chunks
        .iter()
        .map(|chunk| Bm25DocRecord {
            title: chunk.bm25_title.as_deref().unwrap_or("").to_string(),
            keywords: chunk.bm25_keywords.join(" "),
            content: chunk.text.clone(),
        })
        .collect()
}

pub fn fixture_source_path(spec: FixtureSpec) -> PathBuf {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");

    let mut path = base.join(spec.category.dir_name());
    if !spec.subdirectory.is_empty() {
        path = path.join(&spec.subdirectory);
    }
    path
}

/// A file processor that mirrors production for a given project root.
///
/// The production index loads the project's own plugin registry, so a project
/// that ships plugins — a domain vocabulary, for instance — indexes the same
/// way the real project does. A project without a registry keeps the plain
/// processor.
pub fn file_processor_for_root(root: &Path) -> FileProcessor {
    // Benchmark baselines must reflect the true retrieval level without any
    // domain vocabulary injection. Plugin auto-loading is therefore disabled
    // by default; set CCE_BENCH_ENABLE_PLUGINS=1 to opt into the production
    // plugin registry for a separate vocabulary baseline.
    if std::env::var("CCE_BENCH_ENABLE_PLUGINS").as_deref() != Ok("1") {
        return FileProcessor::new();
    }
    let mut processor = FileProcessor::new();
    let source = FilePluginSource::from_project(root, None);
    if !source.registry_path().exists() {
        return processor;
    }
    let mut registry = PluginRegistry::new();
    match registry.load_source(&source) {
        Ok(count) if count > 0 => {
            processor = processor.with_plugin_registry(std::sync::Arc::new(registry));
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("plugin registry of {} not loaded: {e}", root.display());
        }
    }
    processor
}

/// A file processor that mirrors production for a fixture.
///
/// The production index loads the project's own plugin registry, so a fixture
/// that ships plugins — a domain vocabulary, for instance — indexes the same
/// way the real project does. A fixture without a registry keeps the plain
/// processor.
pub fn file_processor_for(spec: &FixtureSpec) -> FileProcessor {
    file_processor_for_root(&fixture_source_path(spec.clone()))
}

pub fn collect_files_by_ext(
    root_path: &Path,
    dir: &Path,
    files: &mut Vec<(String, String)>,
    extension: &str,
) -> io::Result<()> {
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                collect_files_by_ext(root_path, &path, files, extension)?;
            } else if path.extension().is_some_and(|e| e == extension) {
                // Production decoding handles non-UTF8 sources (for example
                // GBK punctuation in media-server C files). Files that even
                // production decoding rejects (low-confidence detection) are
                // skipped like the full-pipeline baseline skips failed
                // `process_file_complete` results, instead of aborting the
                // whole collection on one file.
                let content = match cce_utils::file::read_file_to_utf8(&path) {
                    Ok(content) => content,
                    Err(e) => {
                        eprintln!("  Skipping undecodable file {}: {e}", path.display());
                        continue;
                    }
                };
                let rel = path
                    .strip_prefix(root_path)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                files.push((rel, content));
            }
        }
    }
    Ok(())
}

pub fn load_fixture_files(spec: FixtureSpec) -> io::Result<Vec<(String, String)>> {
    // C/C++ fixtures mix implementation and header files (e.g. the
    // media-server tree under `cpp/review` is mostly `.c`/`.h`); all native
    // extensions must be collected so range judgments over `.c` sources can
    // match indexed chunks.
    let extensions: &[&str] = match spec.category {
        crate::FixtureCategory::Rust => &["rs"],
        crate::FixtureCategory::Python => &["py"],
        crate::FixtureCategory::Java => &["java"],
        crate::FixtureCategory::TypeScript => &["ts"],
        crate::FixtureCategory::CSharp => &["cs"],
        crate::FixtureCategory::C => &["c", "h"],
        crate::FixtureCategory::Cpp => &["cpp", "c", "h", "hpp"],
        crate::FixtureCategory::Go => &["go"],
        crate::FixtureCategory::Kotlin => &["kt"],
        crate::FixtureCategory::Scala => &["scala"],
        crate::FixtureCategory::Php => &["php"],
        crate::FixtureCategory::Ruby => &["rb"],
        crate::FixtureCategory::Dart => &["dart"],
        crate::FixtureCategory::JavaScript => &["js"],
        crate::FixtureCategory::Bash => &["sh"],
        crate::FixtureCategory::Lua => &["lua"],
        crate::FixtureCategory::MultiLanguage => &["*"],
        crate::FixtureCategory::Documents => &["*"],
        crate::FixtureCategory::Tools => &["*"],
    };
    let source_path = fixture_source_path(spec);
    let mut files = Vec::new();
    for extension in extensions {
        collect_files_by_ext(&source_path, &source_path, &mut files, extension)?;
    }
    Ok(files)
}

pub fn scan_fixture(spec: FixtureSpec) -> anyhow::Result<Vec<cce_scanner::FileEntry>> {
    let include_patterns: Vec<String> = match spec.category {
        crate::FixtureCategory::Rust => vec!["*.rs".to_string()],
        crate::FixtureCategory::Python => vec!["*.py".to_string()],
        crate::FixtureCategory::Java => vec!["*.java".to_string()],
        crate::FixtureCategory::TypeScript => vec!["*.ts".to_string()],
        crate::FixtureCategory::CSharp => vec!["*.cs".to_string()],
        crate::FixtureCategory::C => vec!["*.c".to_string(), "*.h".to_string()],
        crate::FixtureCategory::Cpp => vec![
            "*.cpp".to_string(),
            "*.c".to_string(),
            "*.h".to_string(),
            "*.hpp".to_string(),
        ],
        crate::FixtureCategory::Go => vec!["*.go".to_string()],
        crate::FixtureCategory::Kotlin => vec!["*.kt".to_string()],
        crate::FixtureCategory::Scala => vec!["*.scala".to_string()],
        crate::FixtureCategory::Php => vec!["*.php".to_string()],
        crate::FixtureCategory::Ruby => vec!["*.rb".to_string()],
        crate::FixtureCategory::Dart => vec!["*.dart".to_string()],
        crate::FixtureCategory::JavaScript => vec!["*.js".to_string()],
        crate::FixtureCategory::Bash => vec!["*.sh".to_string()],
        crate::FixtureCategory::Lua => vec!["*.lua".to_string()],
        crate::FixtureCategory::MultiLanguage => vec!["*".to_string()],
        crate::FixtureCategory::Documents => vec!["*".to_string()],
        crate::FixtureCategory::Tools => vec!["*".to_string()],
    };
    let root = fixture_source_path(spec);
    let mut scanner = FSScanner::new();
    let opts = ScanOptions {
        root_path: root.to_string_lossy().to_string(),
        include_patterns,
        ..Default::default()
    };
    Ok(scanner.scan(&opts)?)
}

/// Retry budget for one outer embedding batch in benchmark data generation.
///
/// Data generation is an offline throughput workload: waiting out a rate
/// window is far cheaper than discarding the whole run. Six retries with a
/// capped exponential schedule sleep about two minutes in total, enough to
/// ride out per-minute token and request windows.
const EMBED_BATCH_MAX_RETRIES: u32 = 6;
const EMBED_RETRY_BASE_DELAY_MS: u64 = 2000;
const EMBED_RETRY_MAX_DELAY_MS: u64 = 60_000;
/// Pacing between diagnostic single-text probes so fault isolation never
/// bursts at full concurrency into a recovering rate window.
const EMBED_PROBE_PACING_MS: u64 = 500;
/// Outer batch size for benchmark embedding: bounds one provider request so
/// a persistent batch-level rejection converges through halving.
const MAX_BATCH_CHUNKS: usize = 16;

/// Whether one more attempt may succeed for a data-generation batch error.
///
/// Timeouts, transport failures, rate limits, gateway failures and server
/// errors are transient. HTTP 400 is included because the provider surfaces
/// throttling as a bare 400 parameter error; a genuinely invalid payload
/// converges through the batch-split fallback instead of burning retries.
/// Billing, auth, model, configuration and token-budget rejections are
/// permanent and fail fast.
fn is_transient_batch_error(err: &LlmError) -> bool {
    match err {
        LlmError::RateLimitExceeded(_)
        | LlmError::Timeout(_)
        | LlmError::Http(_)
        | LlmError::Api(_)
        | LlmError::InvalidResponse(_)
        | LlmError::CircuitBreakerOpen(_) => true,
        LlmError::HttpStatus { status, .. } => *status == 400 || (500..=599).contains(status),
        _ => false,
    }
}

/// Wait before a batch retry: capped exponential backoff honoring the
/// provider-reported retry window as a lower bound.
fn embed_retry_delay_ms(attempt: u32, err: &LlmError) -> u64 {
    let backoff = EMBED_RETRY_BASE_DELAY_MS
        .saturating_mul(1u64 << attempt.saturating_sub(1).min(6))
        .min(EMBED_RETRY_MAX_DELAY_MS);
    match err {
        LlmError::RateLimitExceeded(retry_after_ms) => backoff.max(*retry_after_ms),
        _ => backoff,
    }
}

/// Embed one outer batch with retries, halving the batch when a persistent
/// HTTP 400 indicates a payload-size rejection rather than throttling.
async fn embed_outer_batch(
    embedder: &OpenAICompatibleProvider,
    refs: &[&str],
    batch_idx: usize,
) -> anyhow::Result<EmbeddingResult> {
    let mut attempt = 0u32;
    loop {
        match embedder.embed(refs).await {
            Ok(result) => return Ok(result),
            Err(err) => {
                if !is_transient_batch_error(&err) || attempt >= EMBED_BATCH_MAX_RETRIES {
                    return handle_exhausted_batch(embedder, refs, batch_idx, err).await;
                }
                attempt += 1;
                let delay_ms = embed_retry_delay_ms(attempt, &err);
                eprintln!(
                    "  Embedding batch {batch_idx} attempt {attempt} failed ({err}); retrying in {delay_ms}ms"
                );
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }
}

/// Diagnose an outer batch whose retries are exhausted.
///
/// A persistent HTTP 400 over several texts halves the batch so a genuinely
/// oversized payload converges to passable halves; rate-limit errors skip
/// single-text probing because the probes would only amplify the throttle.
/// Remaining failures fall back to paced single-text probes that report the
/// offending global index.
async fn handle_exhausted_batch(
    embedder: &OpenAICompatibleProvider,
    refs: &[&str],
    batch_idx: usize,
    err: LlmError,
) -> anyhow::Result<EmbeddingResult> {
    if matches!(err, LlmError::HttpStatus { status: 400, .. }) && refs.len() > 1 {
        let mid = refs.len() / 2;
        eprintln!(
            "  Embedding batch {batch_idx} persistently rejected; splitting {} texts into {mid} + {}",
            refs.len(),
            refs.len() - mid
        );
        let mut left = Box::pin(embed_outer_batch(embedder, &refs[..mid], batch_idx)).await?;
        let right = Box::pin(embed_outer_batch(embedder, &refs[mid..], batch_idx)).await?;
        left.embeddings.extend(right.embeddings);
        left.prompt_tokens += right.prompt_tokens;
        left.total_tokens += right.total_tokens;
        return Ok(left);
    }
    if matches!(err, LlmError::RateLimitExceeded(_)) {
        return Err(err.into());
    }
    for (offset, text) in refs.iter().enumerate() {
        tokio::time::sleep(std::time::Duration::from_millis(EMBED_PROBE_PACING_MS)).await;
        if let Err(single_err) = embedder.embed(std::slice::from_ref(text)).await {
            anyhow::bail!(
                "Embedding failed for batch {batch_idx}, text index {} \
                 (len {}): {single_err}\nBatch error: {err}\n\
                 Text preview: {:?}",
                batch_idx * MAX_BATCH_CHUNKS + offset,
                text.len(),
                &text[..text.len().min(200)]
            );
        }
    }
    Err(err.into())
}

pub async fn embed_texts(
    embedder: &OpenAICompatibleProvider,
    texts: &[String],
) -> anyhow::Result<(Vec<f32>, usize)> {
    if texts.is_empty() {
        anyhow::bail!("embed_texts called with empty input");
    }

    // Providers reject empty/whitespace-only inputs with an opaque 400
    // ("parameter is invalid"); reject them here with the offending index.
    for (i, text) in texts.iter().enumerate() {
        if text.trim().is_empty() {
            anyhow::bail!("embed_texts: empty/whitespace-only text at index {i}");
        }
    }

    let mut all_vectors = Vec::new();
    let mut dim = 0;

    for (batch_idx, chunk_batch) in texts.chunks(MAX_BATCH_CHUNKS).enumerate() {
        let refs: Vec<&str> = chunk_batch.iter().map(|s| s.as_str()).collect();
        eprintln!(
            "  Embedding batch {}: {} texts, max_len={}",
            batch_idx,
            refs.len(),
            refs.iter().map(|s| s.len()).max().unwrap_or(0)
        );
        let result = embed_outer_batch(embedder, &refs, batch_idx).await?;
        if result.embeddings.len() != refs.len() {
            anyhow::bail!(
                "Embedding response count mismatch in batch {batch_idx}: expected {}, received {}",
                refs.len(),
                result.embeddings.len()
            );
        }
        if dim == 0 {
            dim = result.embeddings.first().map(|v| v.len()).ok_or_else(|| {
                anyhow::anyhow!("Embedding batch {batch_idx} returned no vectors")
            })?;
            if dim == 0 {
                anyhow::bail!("Embedding batch {batch_idx} returned zero-dimension vectors");
            }
        }
        for emb in &result.embeddings {
            if emb.len() != dim {
                anyhow::bail!(
                    "Embedding dimension mismatch in batch {batch_idx}: expected {dim}, received {}",
                    emb.len()
                );
            }
            all_vectors.extend_from_slice(emb);
        }
    }

    if all_vectors.len() != texts.len() * dim {
        anyhow::bail!(
            "Embedding vector length mismatch: {} texts, dim {dim}, flattened {}",
            texts.len(),
            all_vectors.len()
        );
    }

    Ok((all_vectors, dim))
}

pub fn build_embedder() -> (EmbeddingConfig, AppConfig) {
    let api_key = std::env::var("CCE_EMB_API_KEY_SILICONFLOW").unwrap_or_default();
    if api_key.is_empty() {
        tracing::warn!("CCE_EMB_API_KEY_SILICONFLOW not set; embedding will fail");
    }

    let emb_config = EmbeddingConfig {
        provider_type: EmbeddingProviderType::LlamaCppServer,
        endpoint: "https://api.siliconflow.cn/v1".to_string(),
        model: "BAAI/bge-m3".to_string(),
        dimension: 1024,
        api_key,
    };

    let app_config = {
        let mut providers = HashMap::new();
        providers.insert(
            "test-provider".into(),
            ProviderConfig {
                id: "test-provider".into(),
                name: "Test Provider".into(),
                base_url: emb_config.endpoint.clone(),
                api_keys: vec![emb_config.api_key.clone()],
                ..Default::default()
            },
        );
        let mut models = HashMap::new();
        models.insert(
            emb_config.model.clone(),
            EmbeddingModelConfig {
                provider_id: "test-provider".into(),
                model: emb_config.model.clone(),
                vector_dimension: emb_config.dimension,
                max_item_tokens: 8192,
                max_batch_tokens: 16384,
                ..Default::default()
            },
        );
        let mut ac = AppConfig::default();
        ac.llm.providers = providers;
        ac.llm.embedding_models = models;
        ac.embedder.default_model = emb_config.model.clone();
        ac
    };
    (emb_config, app_config)
}

/// Assert every chunk has a unique (file_path, chunk_id) pair.
///
/// A violation indicates a bug upstream in the chunking pipeline.
/// Panics immediately — do not silently hide defects.
pub fn validate_unique_chunks(chunks: &[ChunkData], label: &str) {
    let mut seen = std::collections::HashSet::new();
    for chunk in chunks {
        let key = (chunk.file_path.as_str(), chunk.chunk_id.as_str());
        if !seen.insert(key) {
            panic!(
                "DUPLICATE CHUNK in {label}: file={} chunk_id={} entity={}",
                chunk.file_path, chunk.chunk_id, chunk.entity_name,
            );
        }
    }
}

pub fn persist(dir: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(dir).unwrap_or_else(|_| panic!("Failed to create dir: {:?}", dir));
    let path = dir.join("bench_data.rkyv");
    std::fs::write(&path, bytes).unwrap_or_else(|_| panic!("Failed to write {}", path.display()));
    tracing::info!("Saved {} bytes to {}", bytes.len(), path.display());
}

pub fn normalize_file_path(abs_path: &str) -> String {
    crate::bench_data::normalize_path(abs_path)
}

/// Extract in-project direct call edges from a relation index.
///
/// Keeps only resolved, non-external call relations (`is_call()`,
/// `!is_external`, `callee_id` present), skips self-loops and dedups by
/// (caller, callee). Entity IDs come from the same `ParsedFile` snapshots
/// that produced the chunk data, so they align with
/// `ChunkData::entity_ids` without a cross-process id mapping.
pub fn collect_call_edges(index: &cce_codegraph::RelationIndex) -> Vec<CallEdgeData> {
    use cce_codegraph::index::{EntityIndexOps, RelationQueryOps};

    let mut edges = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in index.resolved_relation_index().iter() {
        let caller = *entry.key();
        for rel in entry.value().iter() {
            if !rel.relation_type.is_call() || rel.is_external {
                continue;
            }
            let Some(callee) = rel.callee_id else {
                continue;
            };
            if callee == caller || !seen.insert((caller, callee)) {
                continue;
            }
            let Some(entity) = index.function_index().get(&callee) else {
                continue;
            };
            let (start, end) = entity.span.line_range_opt().unwrap_or((0, 0));
            let file = index
                .get_file_path_by_entity(callee)
                .map(|p| normalize_file_path(&p))
                .unwrap_or_default();
            edges.push(CallEdgeData {
                caller_entity_id: caller.0 as i64,
                callee_entity_id: callee.0 as i64,
                callee_name: entity.name.clone(),
                callee_file: file,
                callee_start_line: start as u32,
                callee_end_line: end as u32,
                relation_type: rel.relation_type.to_string(),
            });
        }
    }
    edges
}

pub fn chunk_data_from_result(chunk: &cce_parser::ast_to_nl::chunker::ChunkedResult) -> ChunkData {
    let (start_line, end_line) = chunk
        .metadata
        .source_span
        .line_range_opt()
        .unwrap_or((0, 0));
    let source_ranges = chunk
        .metadata
        .source_ranges()
        .iter()
        .map(|span| {
            let (start_line, end_line) = span.line_range_opt().unwrap_or((0, 0));
            crate::bench_data::ChunkSourceRange {
                start_line,
                end_line,
            }
        })
        .collect();
    ChunkData {
        chunk_id: chunk.chunk_id.clone(),
        entity_name: get_chunk_entity_name(chunk).to_string(),
        file_path: normalize_file_path(&chunk.metadata.file_path),
        start_line,
        end_line,
        source_ranges,
        source_span_kind: chunk.metadata.source_span_kind.to_string(),
        test_info: chunk.metadata.test_info,
        language: chunk.metadata.content_type.language(),
        entity_ids: chunk
            .metadata
            .content_entity_ids()
            .iter()
            .map(|id| id.0 as i64)
            .collect(),
        segment_id: chunk.metadata.segment_id.clone(),
    }
}

/// Single-parse products shared by the full-pipeline baselines.
///
/// The production index parses each file once and derives both retrieval
/// paths from the same parse. Both `full_pipeline` (NL text) and
/// `full_pipeline_raw_source` (sliced raw source) share these products so a
/// large fixture is parsed only once per `run_bench_gen` run; derivation is
/// a pure clone + text replacement without any reparsing.
pub struct FullPipelineParse {
    /// NL embedding-path chunks (`OutputMode::Both` split, unfiltered).
    pub embedding_nl: Vec<ChunkedResult>,
    /// NL BM25-path chunks (`OutputMode::Both` split, unfiltered).
    pub bm25_nl: Vec<ChunkedResult>,
    /// Target-fixture parses feeding the relation-edge sidecar.
    pub target_parsed: Vec<ParsedFile>,
    /// Decoded source by normalized path, backing raw-source slicing.
    pub source_map: HashMap<String, String>,
}

pub(crate) fn split_full_pipeline_chunks(
    results: Vec<ChunkedResult>,
) -> (Vec<ChunkedResult>, Vec<ChunkedResult>) {
    let mut emb = Vec::new();
    let mut bm25 = Vec::new();
    for chunk in results {
        match chunk.path {
            ChunkPath::Bm25 => bm25.push(chunk),
            ChunkPath::Embedding => emb.push(chunk),
        }
    }
    (emb, bm25)
}

/// Parse target + distractor fixtures once for the full-pipeline baselines.
///
/// Runs one `process_file_complete(entry, OutputMode::Both)` pass with the
/// unified `file_processor_for(&target_spec)` processor, splits each result
/// into both retrieval paths, and records the decoded source map plus the
/// target parses. Callers derive `full_pipeline` and
/// `full_pipeline_raw_source` chunks from this without reparsing.
pub async fn collect_full_pipeline_parse(
    target_spec: FixtureSpec,
    distractor_spec: FixtureSpec,
) -> anyhow::Result<FullPipelineParse> {
    let target_entries = scan_fixture(target_spec.clone())?;
    let dist_entries = scan_fixture(distractor_spec)?;
    let mut processor = file_processor_for(&target_spec);
    let mut embedding_nl = Vec::new();
    let mut bm25_nl = Vec::new();
    let mut target_parsed = Vec::new();
    let mut source_map: HashMap<String, String> = HashMap::new();
    for entry in &target_entries {
        if let Ok(result) = processor
            .process_file_complete(entry, OutputMode::Both)
            .await
        {
            record_source(&mut source_map, entry, &result.parsed_file);
            let (emb, bm25) = split_full_pipeline_chunks(result.chunks);
            embedding_nl.extend(emb);
            bm25_nl.extend(bm25);
            target_parsed.push(result.parsed_file);
        }
    }
    for entry in &dist_entries {
        if let Ok(result) = processor
            .process_file_complete(entry, OutputMode::Both)
            .await
        {
            record_source(&mut source_map, entry, &result.parsed_file);
            let (emb, bm25) = split_full_pipeline_chunks(result.chunks);
            embedding_nl.extend(emb);
            bm25_nl.extend(bm25);
        }
    }
    eprintln!(
        "=== FULL PIPELINE PARSE: {} embedding chunks, {} bm25 chunks over {} target files ===",
        embedding_nl.len(),
        bm25_nl.len(),
        target_parsed.len(),
    );
    Ok(FullPipelineParse {
        embedding_nl,
        bm25_nl,
        target_parsed,
        source_map,
    })
}

fn record_source(
    source_map: &mut HashMap<String, String>,
    entry: &cce_scanner::FileEntry,
    parsed_file: &ParsedFile,
) {
    let decoded = parsed_file.source.to_string();
    let entry_key = normalize_file_path(&entry.relative_path.to_string_lossy());
    let parsed_key = normalize_file_path(&parsed_file.path);
    source_map
        .entry(entry_key)
        .or_insert_with(|| decoded.clone());
    source_map.entry(parsed_key).or_insert(decoded);
}

/// Project-level configuration for benchmark data generation.
pub struct ProjectConfig {
    pub name: &'static str,
    pub target_spec: FixtureSpec,
    pub distractor_spec: FixtureSpec,
    pub queries: Vec<QueryData>,
}

/// Run the full 3-baseline benchmark data generation for a project.
///
/// Parses target + distractor fixtures once and derives both `full_pipeline`
/// (NL text) and `full_pipeline_raw_source` (sliced raw source) from that
/// single parse; `direct_chunking` keeps its separate lightweight pipeline.
/// Persists to `data/benchmark/{baseline}/{name}/bge-m3/`.
pub async fn run_bench_gen(config: ProjectConfig) -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    crate::init_minimal_logging();

    let (emb_config, app_config) = build_embedder();
    let embedder =
        cce_llm_client::OpenAICompatibleProvider::from_model(&app_config, &emb_config.model)?;

    let queries = &config.queries;
    eprintln!("=== QUERIES: {} ===", queries.len());

    let target_spec = config.target_spec;
    let distractor_spec = config.distractor_spec;

    eprintln!("=== Phase 1: parsing once for full-pipeline variants ===");
    let shared = collect_full_pipeline_parse(target_spec.clone(), distractor_spec.clone()).await?;
    let full_chunks = full_pipeline::derive_full_pipeline_chunks(&shared, target_spec.clone())?;
    let raw_chunks = full_pipeline_raw::derive_full_pipeline_raw_source_chunks(&shared)?;
    eprintln!("=== Phase 1b: generating direct_chunking chunks ===");
    let direct_chunks =
        entity_based::gen_entity_based_chunks(target_spec.clone(), distractor_spec.clone()).await?;
    let baselines: Vec<(&str, GeneratedChunks)> = vec![
        ("full_pipeline", full_chunks),
        ("full_pipeline_raw_source", raw_chunks),
        ("direct_chunking", direct_chunks),
    ];
    for (baseline, chunks) in &baselines {
        eprintln!("\n{}", "=".repeat(60));
        eprintln!("=== Baseline: {baseline} ===");
        eprintln!("{}\n", "=".repeat(60));

        eprintln!("=== Phase 1: chunks ready (no reparsing) ===");

        if chunks.embedding_chunks.len() != chunks.embedding_texts.len() {
            anyhow::bail!(
                "{baseline}: embedding chunk/text misalignment: {} chunks vs {} texts",
                chunks.embedding_chunks.len(),
                chunks.embedding_texts.len()
            );
        }
        if chunks.embedding_texts.is_empty() {
            anyhow::bail!("{baseline}: no embedding texts generated");
        }

        eprintln!("\n=== Phase 2: embedding chunks ===");
        eprintln!(
            "  Embedding {} code chunks...",
            chunks.embedding_texts.len()
        );
        let (chunk_vectors, dimension) = embed_texts(&embedder, &chunks.embedding_texts).await?;

        eprintln!("\n=== Phase 3: assembling and persisting ===");
        for (variant_queries, suffix) in [(queries, "")] {
            eprintln!("\n--- {baseline}{suffix} ---");
            let query_texts: Vec<String> = variant_queries.iter().map(|q| q.text.clone()).collect();
            if query_texts.is_empty() {
                anyhow::bail!("{baseline}: no query texts to embed");
            }

            let (query_vectors, query_dim) = embed_texts(&embedder, &query_texts).await?;
            if query_dim != dimension {
                anyhow::bail!(
                    "{baseline}: query embedding dimension {query_dim} != chunk dimension {dimension}"
                );
            }

            if chunks.bm25_documents.len() != chunks.bm25_chunks.len() {
                anyhow::bail!(
                    "BM25 document/chunk misalignment: {} documents vs {} chunks",
                    chunks.bm25_documents.len(),
                    chunks.bm25_chunks.len()
                );
            }

            let code_bench = BenchmarkData {
                queries: variant_queries.to_vec(),
                query_texts,
                embedding: RetrieverDataset {
                    chunks: chunks.embedding_chunks.clone(),
                    texts: chunks.embedding_texts.clone(),
                    vectors: chunk_vectors.clone(),
                    query_vectors,
                    dimension: dimension as u32,
                },
                bm25: RetrieverDataset {
                    chunks: chunks.bm25_chunks.clone(),
                    texts: chunks.bm25_texts.clone(),
                    vectors: vec![],
                    query_vectors: vec![],
                    dimension: 0,
                },
                bm25_documents: chunks.bm25_documents.clone(),
                call_edges: chunks.call_edges.clone(),
            };
            let code_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&code_bench)?.to_vec();

            let baseline_name = format!("{baseline}{suffix}");
            persist(&data_dir(&baseline_name, config.name), &code_bytes);
        }
    }

    eprintln!("\n{}", "=".repeat(60));
    eprintln!("=== All baselines generated ===");
    eprintln!("{}", "=".repeat(60));
    Ok(())
}

fn get_chunk_entity_name(chunk: &cce_parser::ast_to_nl::chunker::ChunkedResult) -> &str {
    chunk.bm25_title.as_deref().unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_llm::LlmError;
    use cce_types::error::common::TimeoutError;

    #[test]
    fn embed_retry_schedule_spans_about_two_minutes() {
        let total: u64 = (1..=EMBED_BATCH_MAX_RETRIES)
            .map(|attempt| {
                embed_retry_delay_ms(attempt, &LlmError::Timeout(TimeoutError::new("t")))
            })
            .sum();
        assert!(
            (110_000..=140_000).contains(&total),
            "retry sleeps must total about two minutes, got {total}ms"
        );
    }

    #[test]
    fn embed_retry_honors_retry_after_floor() {
        let err = LlmError::RateLimitExceeded(45_000);
        assert_eq!(embed_retry_delay_ms(1, &err), 45_000);
        assert_eq!(embed_retry_delay_ms(6, &err), 60_000);
    }

    #[test]
    fn transient_batch_errors_retry_and_permanent_errors_fail_fast() {
        for err in [
            LlmError::Timeout(TimeoutError::new("t")),
            LlmError::RateLimitExceeded(1000),
            LlmError::HttpStatus {
                status: 400,
                message: "throttle".into(),
            },
            LlmError::HttpStatus {
                status: 503,
                message: "busy".into(),
            },
        ] {
            assert!(is_transient_batch_error(&err), "must retry: {err}");
        }
        for err in [
            LlmError::Auth("bad key".into()),
            LlmError::QuotaExhausted("empty".into()),
            LlmError::ModelNotFound("gone".into()),
            LlmError::TokenLimitExceeded(9000, 8192),
            LlmError::ContextLengthExceeded("long".into()),
            LlmError::InvalidInput("bad".into()),
        ] {
            assert!(!is_transient_batch_error(&err), "must fail fast: {err}");
        }
    }

    #[test]
    fn collect_files_by_ext_decodes_non_utf8_c_source() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let mut raw = Vec::new();
        raw.extend_from_slice(b"int add(int a, int b) {\n    return a + b;\n}\n");
        raw.extend_from_slice(b"// a \xa1\xae1\xa1\xaf bit\n");
        std::fs::write(dir.path().join("mov-chnl.c"), &raw).expect("write gbk file");
        std::fs::write(dir.path().join("clean.h"), b"int add(int a, int b);\n")
            .expect("write header");

        assert!(
            std::fs::read_to_string(dir.path().join("mov-chnl.c")).is_err(),
            "fixture must be non-UTF8 so plain read_to_string fails"
        );

        let mut files = Vec::new();
        collect_files_by_ext(dir.path(), dir.path(), &mut files, "c").expect("gbk c file");
        assert_eq!(files.len(), 1, "only the .c file matches extension c");
        assert_eq!(files[0].0, "mov-chnl.c");
        assert!(files[0].1.contains("int add"));
    }

    #[test]
    fn collect_files_by_ext_skips_undecodable_file() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let mut raw = vec![b'a'; 9000];
        raw.extend_from_slice(b"\n// \xa1\xaa undecodable tail\n");
        std::fs::write(dir.path().join("late.c"), &raw).expect("write file");
        std::fs::write(dir.path().join("ok.c"), b"int ok(void) { return 0; }\n")
            .expect("write file");

        let mut files = Vec::new();
        collect_files_by_ext(dir.path(), dir.path(), &mut files, "c")
            .expect("undecodable files must be skipped, not abort collection");
        assert!(
            files.iter().any(|(name, _)| name == "ok.c"),
            "decodable file must be collected, got {files:?}"
        );
        assert!(
            files.iter().all(|(name, _)| name != "late.c") || files.len() == 2,
            "undecodable file must either be skipped or decoded, never abort, got {files:?}"
        );
    }
}
