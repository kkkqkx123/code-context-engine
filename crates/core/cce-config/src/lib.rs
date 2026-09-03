//! Configuration system for the code context engine

pub mod env_loader;
pub mod global;
pub mod loader;
pub mod merge;
pub mod modules;
pub mod project;
pub mod project_registry;
pub mod serde_helpers;
pub mod settings;
pub mod validation;

pub use global::{
    AppConfig, DatabaseConfig, LogFormat, LogLevel, LogOutput, LoggingConfig, ServerConfig,
    SqliteConfig, SqliteSyncMode,
};
pub use loader::ConfigLoader;
pub use project::{
    ProjectAppConfig, ProjectConfigPaths, ProjectEmbedderConfig, ProjectLlmConfig,
    ProjectOrchestratorConfig,
};
pub use settings::Settings;
pub use validation::{
    ConfigWarning, DependencyParams, Validate, WarningSeverity, validate_all_dependencies,
};

pub use modules::{
    AstToNlConfig, BatchConfig, Bm25Config, Bm25GeneratorConfig, CacheConfig, ChunkingConfig,
    DebounceConfig, EmbedderConfig, EmbeddingGeneratorConfig, FileWatchConfig, HotUpdateConfig,
    IndexConfig, IndexerConfig, NestProcessorConfig, OrchestratorConfig, PreprocessorConfig,
    QdrantConfig, RelationBuilderParams, RelationConfig, RerankConfig, ScannerConfig,
    SummaryConfig, SummaryGenerationStrategy, SymbolResolutionConfig,
};

pub use project_registry::{ProjectEntry, ProjectMetadata, ProjectScope, RegistryError};
