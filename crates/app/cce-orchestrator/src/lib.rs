//! cce_orchestrator crate - Index, query, and hot-update orchestration
//!
//! This crate provides high-level workflow coordination across all subsystems:
//! - **index**: Full index orchestration
//! - **query**: Search and retrieval orchestration
//! - **hot_update**: File watching and incremental update
//! - **tools**: AST diagnosis, compression, symbol lookup
//! - **index_state**: Index state tracking
//! - **checkpoint**: Unified progress and checkpoint management for resumable operations

pub mod export;
pub mod export_processor;
pub mod operation;

pub mod hot_update;
pub mod index;
pub mod index_state;
pub mod index_state_tracker;
pub mod query;
pub mod tools;

mod error;

pub use cce_config::{
    BatchConfig, CacheConfig, DebounceConfig, HotUpdateConfig, IndexerConfig, OrchestratorConfig,
};

pub use query::{
    AggregatedQueryOptions, CallInfo, ExecutionStrategy, PathQueryOptions, QueryCoordinator,
    QueryError, QueryOptions, QueryOptions as SearchOptions, QueryResult,
    QueryResult as SearchQueryResult, RelationQueryOptions, RelationSearcher, Relations,
    Result as QueryResultType, SearchConfig, SearchResult, SearchSources, Searcher, SubQuery,
};

pub use index::{
    IndexOptions, IndexOrchestrator, IndexResult, RelationPublication, RelationSnapshotPublisher,
};

pub use hot_update::{
    DebounceConfigBuilder, DebounceInfo, GlobalDebounce, HotUpdateCoordinator, HotUpdateError,
    HotUpdateMode, HotUpdateState, UpdateProcessor,
};

pub use hot_update::processors::{
    Bm25UpdateProcessor, BoxedUpdateProcessor, EmbeddingUpdateProcessor, ProcessorCollection,
    RelationUpdateProcessor, SummaryUpdateProcessor,
};

pub use export::{ExportConfig, NlDocumentExporter};
pub use export_processor::NlDocumentUpdateProcessor;

pub use index_state::{
    ChangeTrigger, Checkpoint, FileUpdateState, FileUpdateStatusSummary, IndexOperationType,
    IndexPhase, IndexStateQuery, IndexStateReport, MAX_RETRY_COUNT, ModuleType, ModuleUpdateRecord,
    ModuleUpdateState, StateTrackerError, calculate_retry_delay,
};

pub use index_state_tracker::UpdateStateTracker;

pub use tools::{
    AstDiagnosis, BatchCompressionRequest, BatchCompressionResponse, CompressionError,
    CompressionRequest, CompressionResponse, CompressionRetrieval, DefinitionCode,
    DefinitionLocation, DiagnosisError, DiagnosisRequest, DiagnosisResponse, Diagnostic,
    DiagnosticKind, DiagnosticPrecision, FileSymbolResult, FindReferencesConfig,
    FindReferencesRequest, FindReferencesResponse, FindReferencesTool, GetSymbolsRequest,
    GetSymbolsResponse, GetSymbolsTool, GotoDefinitionRequest, GotoDefinitionResponse,
    GotoDefinitionTool, GroupedReferences, KeywordSearchError, KeywordSearchItem,
    KeywordSearchRequest, KeywordSearchResponse, KeywordSearchTool, ReferenceLocation, SymbolInfo,
    SymbolKind, SymbolLookupError,
};

pub use operation::{
    ActiveOperation, AggregatedMetrics, CheckpointManager, ModuleFailure, ModuleProcessResult,
    OperationContext, OperationCoordinator, OperationPriority, OperationProcessResult,
    OperationQueue, OperationResult, OperationState, OperationStatus, OperationSummary,
    OperationType, PendingOperation, RecoveryManager,
};

pub use error::OrchestratorError;

pub type Result<T> = std::result::Result<T, OrchestratorError>;
