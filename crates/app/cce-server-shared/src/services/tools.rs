//! Stateless tool application service shared by the HTTP and MCP surfaces.
//!
//! Fold, keyword search, compression, symbol lookup, AST diagnosis and
//! summary generation all live here exactly once. Transports decode their wire
//! payloads into the neutral specs below and map the domain results back onto
//! their response shapes.

use cce_orchestrator::{
    AstDiagnosis, BatchCompressionRequest, CompressionRequest, CompressionResponse,
    CompressionRetrieval, DiagnosisRequest, DiagnosisResponse, FileFoldBatchItem,
    FileFoldBatchRequest, FileFoldBatchResponse, FileFoldMode, FileFoldRequest, FileFoldResponse,
    FileFoldTool, FindReferencesRequest, FindReferencesResponse, FindReferencesTool,
    GetSymbolsRequest, GetSymbolsResponse, GetSymbolsTool, GotoDefinitionRequest,
    GotoDefinitionResponse, GotoDefinitionTool, KeywordSearchRequest, KeywordSearchResponse,
    KeywordSearchTool,
};
use cce_parser::grouper::PreprocessingPipeline;
use cce_parser::parser::ParseCoordinator;
use cce_parser::summary::RuleBasedGenerator;
use cce_scanner::{FSScanner, ScanOptions};
use cce_storage_bm25::TermOperator;
use cce_types::language::Language;
use cce_utils::file::read_file_to_utf8_async;
use std::path::PathBuf;
use tracing::{debug, info};

use crate::context::SharedContext;
use crate::runtime::PublishedSnapshot;

/// Failure of the shared tool service.
#[derive(Debug, thiserror::Error)]
pub enum ToolsServiceError {
    /// A wire value or request shape is not acceptable.
    #[error("invalid request: {0}")]
    InvalidInput(String),
    /// The underlying tool failed.
    #[error("{0}")]
    Execution(String),
}

/// A relation-tool answer: the tool result plus the capability map that
/// describes the served snapshot. Availability failures carry the capability
/// map as well so transports can still render the runtime state.
#[derive(Debug, Clone)]
pub struct ToolAnswer<T> {
    pub result: Result<T, String>,
    pub relation_info: Option<serde_json::Value>,
}

/// Resolve the served snapshot and capability map for a relation tool.
async fn relation_tool_context(
    state: &SharedContext,
    project_id: i64,
) -> Result<
    (std::sync::Arc<PublishedSnapshot>, serde_json::Value),
    (Option<serde_json::Value>, String),
> {
    if project_id <= 0 {
        return Err((None, "Invalid project_id".to_string()));
    }
    let runtime = state
        .engine
        .get_relation_runtime(project_id)
        .await
        .map_err(|error| (None, format!("Failed to get relation runtime: {error}")))?;
    if !runtime.can_serve_queries().await {
        let info = runtime.get_capability_info().await;
        return Err((
            Some(info.to_json_map()),
            format!("Relation index not available: {:?}", info.state),
        ));
    }
    let Some(snapshot) = runtime.get_snapshot().await else {
        return Err((None, "No relation snapshot available".to_string()));
    };
    let capability = runtime.get_capability_info().await.to_json_map();
    Ok((snapshot, capability))
}

/// Stateless fold request.
#[derive(Debug, Clone)]
pub struct FoldSpec {
    pub text: String,
    pub language: Option<String>,
    pub file_name: Option<String>,
    pub max_tokens: Option<usize>,
    pub mode: Option<String>,
}

/// One batch fold entry.
#[derive(Debug, Clone)]
pub struct FoldBatchItemSpec {
    pub id: String,
    pub text: String,
    pub language: Option<String>,
    pub file_name: Option<String>,
    pub max_tokens: Option<usize>,
    pub mode: Option<String>,
}

/// Batch fold request with global defaults.
#[derive(Debug, Clone)]
pub struct FoldBatchSpec {
    pub items: Vec<FoldBatchItemSpec>,
    pub language: Option<String>,
    pub max_tokens: Option<usize>,
    pub mode: Option<String>,
    pub max_concurrency: Option<usize>,
}

fn resolve_language(name: Option<&str>) -> Option<Language> {
    name.and_then(Language::from_name)
}

fn fold_request(spec: FoldSpec) -> FileFoldRequest {
    let mut request = FileFoldRequest::new(spec.text);
    if let Some(language) = resolve_language(spec.language.as_deref()) {
        request = request.with_language(language);
    }
    if let Some(file_name) = spec.file_name {
        request = request.with_file_name(file_name);
    }
    if let Some(max_tokens) = spec.max_tokens {
        request = request.with_max_tokens(max_tokens);
    }
    if let Some(mode) = spec.mode.as_deref() {
        request = request.with_mode(FileFoldMode::parse(Some(mode)));
    }
    request
}

/// Fold one text fragment. Degenerate inputs degrade instead of erroring.
pub fn fold(spec: FoldSpec) -> FileFoldResponse {
    let mut coordinator = ParseCoordinator::new();
    FileFoldTool::fold(&mut coordinator, fold_request(spec))
}

/// Fold several text fragments with shared defaults.
pub fn fold_batch(spec: FoldBatchSpec) -> Result<FileFoldBatchResponse, ToolsServiceError> {
    let items = spec
        .items
        .into_iter()
        .map(|item| {
            let mut entry = FileFoldBatchItem::new(item.id, item.text);
            let language = resolve_language(item.language.as_deref())
                .or_else(|| resolve_language(spec.language.as_deref()));
            if let Some(language) = language {
                entry = entry.with_language(language);
            }
            if let Some(file_name) = item.file_name {
                entry = entry.with_file_name(file_name);
            }
            if let Some(max_tokens) = item.max_tokens.or(spec.max_tokens) {
                entry = entry.with_max_tokens(max_tokens);
            }
            if let Some(mode) = item.mode.as_deref().or(spec.mode.as_deref()) {
                entry = entry.with_mode(FileFoldMode::parse(Some(mode)));
            }
            entry
        })
        .collect();
    let mut request = FileFoldBatchRequest::new(items);
    if let Some(max_concurrency) = spec.max_concurrency {
        request = request.with_max_concurrency(max_concurrency);
    }
    FileFoldTool::fold_batch(request)
        .map_err(|error| ToolsServiceError::InvalidInput(error.to_string()))
}

/// Keyword term combination accepted on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordOperator {
    Or,
    And,
}

impl KeywordOperator {
    /// Parse a term operator label, case-insensitively; `Or` is the default.
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::to_ascii_lowercase).as_deref() {
            Some("and") => Self::And,
            _ => Self::Or,
        }
    }

    fn into_term_operator(self) -> TermOperator {
        match self {
            Self::Or => TermOperator::Or,
            Self::And => TermOperator::And,
        }
    }
}

/// Keyword (BM25) search request.
#[derive(Debug, Clone)]
pub struct KeywordSpec {
    pub query: String,
    pub top_n: usize,
    pub project_id: i64,
    pub epoch: Option<i64>,
    pub offset: usize,
    pub operator: KeywordOperator,
}

/// Run keyword search against the project's full-text index.
pub async fn keyword_search(
    state: &SharedContext,
    spec: KeywordSpec,
) -> Result<KeywordSearchResponse, ToolsServiceError> {
    let mut tool = KeywordSearchTool::new(state.engine.fulltext_clone());
    if let Some(sqlite) = state.engine.metadata_store_clone() {
        tool = tool.with_sqlite(sqlite);
    }
    tool.search(KeywordSearchRequest {
        query: spec.query,
        top_n: spec.top_n,
        project_id: spec.project_id,
        epoch: spec.epoch,
        offset: spec.offset,
        term_operator: spec.operator.into_term_operator(),
    })
    .await
    .map_err(|error| ToolsServiceError::Execution(error.to_string()))
}

/// Compression request for one file.
#[derive(Debug, Clone)]
pub struct CompressSpec {
    pub file_path: String,
    pub include_entities: bool,
    pub include_groups: bool,
}

/// Compress one file into its semantic representation.
pub async fn compress(spec: CompressSpec) -> Result<CompressionResponse, ToolsServiceError> {
    CompressionRetrieval::new()
        .compress(CompressionRequest {
            file_path: spec.file_path,
            include_entities: spec.include_entities,
            include_groups: spec.include_groups,
        })
        .await
        .map_err(|error| ToolsServiceError::Execution(error.to_string()))
}

/// Batch compression request.
#[derive(Debug, Clone)]
pub struct BatchCompressSpec {
    pub file_paths: Vec<String>,
    pub include_entities: bool,
    pub include_groups: bool,
    pub max_concurrency: usize,
}

/// Compress several files, reporting per-file successes and failures.
pub async fn compress_batch(spec: BatchCompressSpec) -> cce_orchestrator::BatchCompressionResponse {
    CompressionRetrieval::new()
        .compress_batch(BatchCompressionRequest {
            file_paths: spec.file_paths,
            include_entities: spec.include_entities,
            include_groups: spec.include_groups,
            max_concurrency: spec.max_concurrency,
        })
        .await
}

/// AST diagnosis request.
#[derive(Debug, Clone)]
pub struct DiagnosisSpec {
    pub code: String,
    pub language: Option<String>,
    pub file_name: Option<String>,
    pub include_ast: bool,
}

/// A byte offset or line/column pair, described without parser types.
#[derive(Debug, Clone, Copy)]
pub struct PositionView {
    pub row: usize,
    pub column: usize,
}

/// A source span described without parser types.
#[derive(Debug, Clone, Copy)]
pub struct SpanView {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start: PositionView,
    pub end: PositionView,
}

/// One AST node of a diagnosis result.
#[derive(Debug, Clone)]
pub struct AstNodeView {
    pub kind: String,
    pub text: String,
    pub span: SpanView,
    pub children: Vec<AstNodeView>,
}

/// One diagnostic reported for a source fragment.
#[derive(Debug, Clone)]
pub struct DiagnosticView {
    pub kind: String,
    pub position: PositionView,
    pub span: Option<SpanView>,
    pub message: String,
    pub precision: String,
}

/// Diagnosis result described without parser types.
#[derive(Debug, Clone)]
pub struct DiagnosisView {
    pub language: String,
    pub is_valid: bool,
    pub ast: Option<AstNodeView>,
    pub diagnostics: Vec<DiagnosticView>,
}

fn position_view(position: cce_types::position::Position) -> PositionView {
    PositionView {
        row: position.row,
        column: position.column,
    }
}

fn span_view(span: cce_types::position::Span) -> SpanView {
    SpanView {
        start_byte: span.start_byte,
        end_byte: span.end_byte,
        start: position_view(span.start_position),
        end: position_view(span.end_position),
    }
}

fn ast_node_view(node: cce_parser::parser::ast_parser::AstNode) -> AstNodeView {
    AstNodeView {
        kind: node.kind,
        text: node.text,
        span: span_view(node.span),
        children: node.children.into_iter().map(ast_node_view).collect(),
    }
}

fn diagnosis_view(response: DiagnosisResponse) -> DiagnosisView {
    let DiagnosisResponse {
        language,
        is_valid,
        ast,
        diagnostics,
    } = response;
    DiagnosisView {
        language,
        is_valid,
        ast: ast.map(ast_node_view),
        diagnostics: diagnostics
            .into_iter()
            .map(|entry| DiagnosticView {
                kind: format!("{:?}", entry.kind),
                position: position_view(entry.position),
                span: entry.span.map(span_view),
                message: entry.message,
                precision: format!("{:?}", entry.precision),
            })
            .collect(),
    }
}

/// Diagnose one source fragment for syntax errors.
pub fn diagnose(spec: DiagnosisSpec) -> Result<DiagnosisView, ToolsServiceError> {
    let mut request = DiagnosisRequest::new(&spec.code);
    if let Some(language) = resolve_language(spec.language.as_deref()) {
        request = request.with_language(language);
    }
    if let Some(file_name) = spec.file_name.as_deref() {
        request = request.with_file_name(file_name);
    }
    request = request.with_ast(spec.include_ast);
    AstDiagnosis::new()
        .diagnose(request)
        .map(diagnosis_view)
        .map_err(|error| ToolsServiceError::Execution(error.to_string()))
}

/// Find all references to a symbol.
pub async fn find_references(
    state: &SharedContext,
    project_id: i64,
    request: FindReferencesRequest,
) -> ToolAnswer<FindReferencesResponse> {
    let (snapshot, capability) = match relation_tool_context(state, project_id).await {
        Ok(context) => context,
        Err((relation_info, message)) => {
            return ToolAnswer {
                result: Err(message),
                relation_info,
            };
        }
    };
    let mut tool = FindReferencesTool::new(snapshot.index.clone(), project_id);
    if let Some(sqlite) = state.engine.metadata_store()
        && let Ok(project) = sqlite.for_project(project_id)
    {
        tool = tool.with_sqlite(project);
    }
    ToolAnswer {
        result: tool
            .find_references(request)
            .map_err(|error| error.to_string()),
        relation_info: Some(capability),
    }
}

/// List symbols of one or more files.
pub async fn get_symbols(
    state: &SharedContext,
    project_id: i64,
    request: GetSymbolsRequest,
) -> ToolAnswer<GetSymbolsResponse> {
    let (snapshot, capability) = match relation_tool_context(state, project_id).await {
        Ok(context) => context,
        Err((relation_info, message)) => {
            return ToolAnswer {
                result: Err(message),
                relation_info,
            };
        }
    };
    let tool = GetSymbolsTool::new(snapshot.index.clone());
    ToolAnswer {
        result: tool.get_symbols(request).map_err(|error| error.to_string()),
        relation_info: Some(capability),
    }
}

/// Jump to the definition of a symbol.
pub async fn goto_definition(
    state: &SharedContext,
    project_id: i64,
    request: GotoDefinitionRequest,
) -> ToolAnswer<GotoDefinitionResponse> {
    let (snapshot, capability) = match relation_tool_context(state, project_id).await {
        Ok(context) => context,
        Err((relation_info, message)) => {
            return ToolAnswer {
                result: Err(message),
                relation_info,
            };
        }
    };
    let mut tool = GotoDefinitionTool::new(snapshot.index.clone(), project_id);
    if let Some(sqlite) = state.engine.metadata_store()
        && let Ok(project) = sqlite.for_project(project_id)
    {
        tool = tool.with_sqlite(project);
    }
    ToolAnswer {
        result: tool
            .goto_definition(request)
            .map_err(|error| error.to_string()),
        relation_info: Some(capability),
    }
}

/// Summary request over explicit files and scanned directories.
#[derive(Debug, Clone)]
pub struct SummarySpec {
    pub file_paths: Vec<String>,
    pub directory_paths: Vec<String>,
    pub extensions: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub respect_gitignore: bool,
    pub ignore_patterns: Vec<String>,
    pub max_files: usize,
}

/// One generated file summary.
#[derive(Debug, Clone)]
pub struct SummaryItem {
    pub file_path: String,
    pub language: String,
    pub summary: String,
    pub main_entities: Vec<String>,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
    pub entity_count: u32,
    pub line_count: u32,
    pub tags: Vec<String>,
    pub importance_level: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Result of a summary generation request.
#[derive(Debug, Clone)]
pub struct SummaryOutcome {
    pub items: Vec<SummaryItem>,
    pub warnings: Vec<String>,
    pub success_count: usize,
    pub failed_count: usize,
    pub elapsed_ms: u64,
}

impl SummaryOutcome {
    /// Whether the request produced any usable summary.
    pub fn succeeded(&self) -> bool {
        self.failed_count == 0 || self.success_count > 0
    }
}

fn summary_failure(file_path: String, message: String) -> SummaryItem {
    SummaryItem {
        file_path,
        language: String::new(),
        summary: String::new(),
        main_entities: Vec::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        entity_count: 0,
        line_count: 0,
        tags: Vec::new(),
        importance_level: "unknown".to_string(),
        success: false,
        error: Some(message),
    }
}

/// Collect the files a summary request addresses, honouring scan options.
fn collect_files(spec: &SummarySpec) -> (Vec<PathBuf>, Vec<String>) {
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    for path in &spec.file_paths {
        let path_buf = PathBuf::from(path);
        if path_buf.is_file() {
            files.push(path_buf);
        } else {
            warnings.push(format!("File does not exist or is not a file: {path}"));
        }
    }
    let include_patterns: Vec<String> = spec
        .extensions
        .iter()
        .map(|extension| format!("**/*.{extension}"))
        .collect();
    for dir_path in &spec.directory_paths {
        let dir_path_buf = PathBuf::from(dir_path);
        if !dir_path_buf.exists() {
            warnings.push(format!("Directory does not exist: {dir_path}"));
            continue;
        }
        if !dir_path_buf.is_dir() {
            warnings.push(format!("Path is not a directory: {dir_path}"));
            continue;
        }
        let options = ScanOptions {
            root_path: dir_path.clone(),
            include_patterns: include_patterns.clone(),
            exclude_patterns: spec.exclude_dirs.clone(),
            respect_gitignore: spec.respect_gitignore,
            gitignore_patterns: spec.ignore_patterns.clone(),
            follow_symlinks: false,
            gitignore_path: None,
            max_content_size: None,
            max_file_size: None,
            parallelism: None,
        };
        let mut scanner = FSScanner::new();
        match scanner.scan(&options) {
            Ok(entries) => {
                debug!(count = entries.len(), dir = %dir_path, "Scanned directory");
                files.extend(entries.into_iter().map(|entry| entry.path));
            }
            Err(error) => warnings.push(format!("Scanning directory failed {dir_path}: {error}")),
        }
    }
    if files.len() > spec.max_files {
        warnings.push(format!(
            "The number of files exceeds the limit {}, only the first {} files will be processed",
            spec.max_files,
            files.len()
        ));
        files.truncate(spec.max_files);
    }
    let mut seen = std::collections::HashSet::new();
    files.retain(|path| seen.insert(path.to_string_lossy().to_string()));
    (files, warnings)
}

/// Generate temporary summaries without storing anything.
pub async fn summarize(spec: SummarySpec) -> Result<SummaryOutcome, ToolsServiceError> {
    if spec.file_paths.is_empty() && spec.directory_paths.is_empty() {
        return Err(ToolsServiceError::InvalidInput(
            "Must provide file_paths or directory_paths".to_string(),
        ));
    }
    let start = std::time::Instant::now();
    let (files, warnings) = collect_files(&spec);
    info!(count = files.len(), "Processing files for summary");

    let generator = RuleBasedGenerator::new();
    let pipeline = PreprocessingPipeline::new();
    let mut parser = ParseCoordinator::new();
    let mut items = Vec::new();
    let mut success_count = 0;
    let mut failed_count = 0;
    for file_path in files {
        let path = file_path.to_string_lossy().to_string();
        debug!(path = %path, "Processing file");
        let content = match read_file_to_utf8_async(&file_path).await {
            Ok(content) => content,
            Err(error) => {
                items.push(summary_failure(path, format!("fail to read file: {error}")));
                failed_count += 1;
                continue;
            }
        };
        let parsed = match parser.parse(&path, &content) {
            Ok(parsed) => parsed,
            Err(error) => {
                items.push(summary_failure(
                    path,
                    format!("fail to parse file: {error}"),
                ));
                failed_count += 1;
                continue;
            }
        };
        // Grouping enriches the summary with structure the parser alone does
        // not expose (group headers, ordering, cross-entity context).
        let groups = pipeline.process(&parsed);
        let summary = generator.generate_with_groups(&parsed, &groups).await;
        items.push(SummaryItem {
            file_path: path,
            language: summary.language,
            summary: summary.summary_text,
            main_entities: summary.main_entities,
            imports: summary.imports,
            exports: summary.exports,
            entity_count: summary.entity_count,
            line_count: summary.line_count,
            tags: summary.tags,
            importance_level: format!("{:?}", summary.importance_level).to_lowercase(),
            success: true,
            error: None,
        });
        success_count += 1;
    }
    let elapsed_ms = start.elapsed().as_millis() as u64;
    info!(
        total = items.len(),
        success = success_count,
        failed = failed_count,
        elapsed_ms,
        "Summary generation completed"
    );
    Ok(SummaryOutcome {
        items,
        warnings,
        success_count,
        failed_count,
        elapsed_ms,
    })
}

/// One parsed entity described without parser types.
#[derive(Debug, Clone)]
pub struct ParsedEntityView {
    pub id: u64,
    pub kind: String,
    pub name: String,
    pub signature: String,
    pub start_line: u32,
    pub end_line: u32,
    pub doc_comment: Option<String>,
}

/// One raw relation of a parsed file, with intra-file endpoints resolved.
#[derive(Debug, Clone)]
pub struct ParsedRelationView {
    pub caller_id: u64,
    pub callee_id: u64,
    pub relation_type: String,
    pub line: u32,
}

/// Result of parsing one file on demand.
#[derive(Debug, Clone)]
pub struct ParsedFileView {
    pub file_path: String,
    pub language: String,
    pub encoding: String,
    pub entities: Vec<ParsedEntityView>,
    pub relations: Vec<ParsedRelationView>,
    pub elapsed_ms: u64,
}

/// Parse one file from disk into entities and intra-file relations.
pub async fn parse_file(file_path: &str) -> Result<ParsedFileView, ToolsServiceError> {
    let start = std::time::Instant::now();
    if file_path.trim().is_empty() {
        return Err(ToolsServiceError::InvalidInput(
            "File path cannot be empty".to_string(),
        ));
    }
    let (content, encoding) =
        cce_utils::file::read_file_to_utf8_with_encoding_async(std::path::Path::new(file_path))
            .await
            .map_err(|error| {
                ToolsServiceError::InvalidInput(format!("Unable to read file: {error}"))
            })?;
    let mut parser = ParseCoordinator::new();
    let parsed = parser
        .parse(file_path, &content)
        .map_err(|error| ToolsServiceError::Execution(error.to_string()))?;
    let entities = parsed
        .entities
        .iter()
        .map(|entity| ParsedEntityView {
            id: entity.id.0,
            kind: format!("{:?}", entity.kind),
            name: entity.name.clone(),
            signature: entity.signature.clone(),
            start_line: entity.span.start_position.row as u32 + 1,
            end_line: entity.span.end_position.row as u32 + 1,
            doc_comment: entity.doc_comment.clone(),
        })
        .collect();
    let entity_by_name: std::collections::HashMap<&str, u64> = parsed
        .entities
        .iter()
        .map(|entity| (entity.name.as_str(), entity.id.0))
        .collect();
    let relations = parsed
        .raw_relations
        .iter()
        .map(|relation| ParsedRelationView {
            caller_id: relation.src.0,
            callee_id: entity_by_name
                .get(relation.dst_name.as_str())
                .copied()
                .unwrap_or(0),
            relation_type: relation.relation_type.to_string(),
            line: relation.span.start_position.row as u32 + 1,
        })
        .collect();
    Ok(ParsedFileView {
        file_path: file_path.to_string(),
        language: parsed.language.to_string(),
        encoding,
        entities,
        relations,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}
