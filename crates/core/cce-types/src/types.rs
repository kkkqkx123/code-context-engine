//! Type definitions for the code context engine

pub mod ast_to_nl;
pub mod chunk_refs;
pub mod entity;
pub mod error;
pub mod file_meta;
pub mod grouper;
pub mod import;
pub mod language;
pub mod llm;
pub mod module_path;
pub mod operation_kind;
pub mod plugin;
pub mod point_kind;
pub mod position;
pub mod relation;
pub mod stdlib_category;
pub mod test_info;

pub use language::{ContentRoute, FileType, Language, LanguageInfo};

pub const INDEX_FORMAT_VERSION: u32 = 2;

pub use chunk_refs::ChunkEntityRefs;

pub use entity::{
    BehaviorFact, BehaviorFactKind, BehaviorStore, ControlFlowFact, ControlFlowFactKind,
    ControlFlowStore, EmbeddedBlockSnapshot, Entity, EntityBehavior, EntityControlFlow, EntityId,
    EntityKind, EntitySnapshot, FILE_DOC_SENTINEL_ID, GroupedEntity, ParsedFile, RawRelationData,
};

pub use position::{Position, Span};

pub use file_meta::{FileInfo, ImportTable};

pub use ast_to_nl::{
    ChunkContentType, ChunkMetadata, ChunkPath, ChunkedResult, CodeSpecificMetadata,
    ConversionResult, DocumentSpecificMetadata, FileCategory, GroupConversions, GroupRelation,
    GroupRelationType, OutputMode, OverlapRegion, OverlapType, QueryType, RerankCandidate,
    RerankResult, RerankedCandidate, SourceSpanKind, SplitReason,
};

pub use grouper::{EntityGroup, GroupType, ProcessingResult, ProcessingStats};

pub use plugin::{
    FileFilterDecision, FusionWeights, GroupPluginContext, PluginDocument, PluginEntity,
    PluginExport, PluginImport, PluginRelation, PluginSymbol, PluginSymbolLocation,
    QueryRewriteResult, ResultFilterEntry,
};

pub use import::ImportSource;

pub use import::{
    ClassificationMetadata, ExportKind, ExportTarget, ImportClass, ImportClassification,
    ImportKind, ImportTarget, StandardizedExport, StandardizedExportTable, StandardizedImport,
    StandardizedImportTable, TargetKind,
};

pub use relation::{
    AddedEntity, CanonicalDependency, CanonicalEntity, CanonicalExport, CanonicalFile,
    CanonicalRelation, CanonicalRelationSnapshot, CanonicalRelationTarget, DependencyDiff,
    ExportDiff, ExternalCallType, FileRelationDiff, FingerprintComponents, ImportDiff,
    RELATION_PARSER_VERSION, RELATION_PATH_NORMALIZATION_VERSION, RELATION_RESOLVER_VERSION,
    RELATION_SNAPSHOT_SCHEMA_VERSION, Relation, RelationCapture, RelationLevel,
    RelationSnapshotManifest, RelationSnapshotState, RelationSymbolLocation, RelationSymbolRecord,
    RelationTarget, RelationType, RelationVerificationStatus, ResolvedRelation,
    SYMBOL_KEY_CONFLICT_SAMPLE_CAP, SnapshotBuildMetadata, SnapshotDelta, StableSymbolId,
    StableSymbolKey, SymbolKeyConflictRecord, UnresolvedReason, VirtualRelation, VirtualSymbolId,
    fingerprint_from_components, normalize_project_path,
};

pub use error::{ParseError, StorageError};

pub use stdlib_category::StdlibCategory;

pub use test_info::{TestGranularity, TestInfo, TestSource, TestStatus};

pub use point_kind::PointKind;

pub use operation_kind::OperationKind;

pub use module_path::NamespacePath;
