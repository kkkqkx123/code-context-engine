//! CLI argument definitions using clap

use clap::{Parser, Subcommand};

use crate::commands;

/// Code Context Engine CLI Client
#[derive(Parser)]
#[command(name = "cce-cli")]
#[command(about = "CLI client for Code Context Engine", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Server URL (e.g., http://localhost:3000)
    #[arg(
        short,
        long,
        env = "CCE_SERVER_URL",
        default_value = "http://localhost:3000"
    )]
    pub server: String,

    /// Output format
    #[arg(short, long, value_enum, default_value = "table")]
    pub format: OutputFormat,

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Global project ID (can be used instead of per-command --project-id)
    #[arg(short = 'P', long, global = true)]
    pub project_id: Option<i64>,

    #[command(subcommand)]
    pub command: Commands,
}

/// Output format options
#[derive(clap::ValueEnum, Clone, Default)]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    Plain,
}

/// Available commands
#[derive(Subcommand)]
pub enum Commands {
    /// Index operations
    #[command(subcommand)]
    Index(IndexCommands),

    /// Search operations
    #[command(subcommand)]
    Search(SearchCommands),

    /// Aggregated search (advanced multi-query search)
    AggSearch(commands::agg_search::AggSearchCommand),

    /// Project management
    #[command(subcommand)]
    Project(ProjectCommands),

    /// Entity queries
    #[command(subcommand)]
    Entity(EntityCommands),

    /// Watch operations
    #[command(subcommand)]
    Watch(WatchCommands),

    /// Storage management
    #[command(subcommand)]
    Storage(StorageCommands),

    /// Tools
    #[command(subcommand)]
    Tools(ToolCommands),

    /// Generate file summaries
    #[command(subcommand)]
    Summary(SummaryCommands),

    /// Configuration management
    #[command(subcommand)]
    Config(ConfigCommands),

    /// Qdrant process management
    #[command(subcommand)]
    Qdrant(QdrantCommands),

    /// Metrics export
    #[command(subcommand)]
    Metrics(MetricsCommands),

    /// Health monitoring and retry queue management
    #[command(subcommand)]
    Health(HealthCommands),

    /// Server status and health check
    Status,
}

/// Index commands
#[derive(Subcommand)]
pub enum IndexCommands {
    /// Execute full index on a directory
    Run {
        /// Project ID (required)
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Root directory to index
        #[arg(short, long)]
        path: String,

        /// File extensions to include (comma-separated)
        #[arg(short, long, default_value = "rs,py,js,ts,c,cpp,java")]
        extensions: String,

        /// Directories to exclude (comma-separated)
        #[arg(short, long, default_value = "node_modules,target,.git,vendor")]
        exclude: String,

        /// Respect .gitignore
        #[arg(long, default_value = "true")]
        gitignore: bool,

        /// Custom gitignore file path
        #[arg(long)]
        custom_gitignore: Option<String>,
    },

    /// Execute incremental index
    Incremental {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Files to index (comma-separated paths)
        #[arg(short, long)]
        add: Option<String>,

        /// Files to remove (comma-separated paths)
        #[arg(short, long)]
        remove: Option<String>,

        /// Force re-index
        #[arg(short, long)]
        force: bool,
    },

    /// Parse a single file
    Parse {
        /// File path to parse
        #[arg(short, long)]
        file: String,

        /// Language hint (optional)
        #[arg(short, long)]
        language: Option<String>,
    },
}

/// Search commands
#[derive(Subcommand)]
pub enum SearchCommands {
    /// Search code
    Query {
        /// Project ID (required)
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Project root path (optional if --project-id is provided)
        #[arg(long)]
        project_path: Option<String>,

        /// Search query
        #[arg(short, long)]
        query: String,

        /// Query type: vector, bm25, hybrid, hierarchical
        #[arg(short = 't', long, default_value = "hybrid")]
        query_type: String,

        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,

        /// Minimum score threshold
        #[arg(long)]
        min_score: Option<f32>,

        /// Filter by file extensions (comma-separated)
        #[arg(long)]
        extensions: Option<String>,

        /// Filter by directory prefix
        #[arg(long)]
        directory: Option<String>,

        /// Filter by entity types (comma-separated)
        #[arg(long)]
        entities: Option<String>,

        /// Filter by languages (comma-separated)
        #[arg(long)]
        languages: Option<String>,

        /// Content types to exclude (comma-separated): test, generated, vendor
        #[arg(long)]
        exclude_content_types: Option<String>,

        /// Exclude patterns (comma-separated)
        #[arg(long)]
        exclude: Option<String>,

        /// Include patterns (comma-separated)
        #[arg(long)]
        include: Option<String>,

        /// Call chain depth (optional, defaults to 3)
        #[arg(long)]
        call_chain_depth: Option<usize>,

        /// Include call chain in results
        #[arg(long)]
        include_call_chain: bool,

        /// Force reranking on/off for this query (defaults to config)
        #[arg(long)]
        enable_rerank: Option<bool>,

        /// Override the maximum number of rerank candidates
        #[arg(long)]
        rerank_max_candidates: Option<usize>,
    },
}

/// Project commands
#[derive(Subcommand)]
pub enum ProjectCommands {
    /// Create a new project
    Create {
        /// Root directory path
        #[arg(short, long)]
        path: String,

        /// Project name (optional, auto-generated if not provided)
        #[arg(short, long)]
        name: Option<String>,

        /// File extensions to include
        #[arg(long, default_value = "rs,py,js,ts")]
        extensions: String,

        /// Directories to exclude
        #[arg(long, default_value = "node_modules,target,.git")]
        exclude: String,
    },

    /// List all projects
    List,

    /// Get project details
    Get {
        /// Project ID
        id: String,
    },

    /// Update project
    Update {
        /// Project ID
        id: String,

        /// New project name
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Delete project
    Delete {
        /// Project ID
        id: String,
    },

    /// Index a project
    Index {
        /// Project ID
        id: String,
    },

    /// Reload project configuration (hot reload)
    Reload {
        /// Project ID
        id: String,
    },

    /// Update project configuration
    Config {
        /// Project ID
        id: String,
    },
}

/// Entity commands
#[derive(Subcommand)]
pub enum EntityCommands {
    /// Get function details
    Function {
        /// Function ID
        id: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Get function calls (callees)
    Calls {
        /// Function ID
        id: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Get function callers
    Callers {
        /// Function ID
        id: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Get call chain
    CallChain {
        /// Function ID
        id: String,

        /// Direction: up (callers) or down (callees)
        #[arg(short, long, default_value = "down")]
        direction: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Find call path between two functions
    CallPath {
        /// Start function ID
        #[arg(long)]
        from: String,

        /// End function ID
        #[arg(long)]
        to: String,

        /// Maximum search depth
        #[arg(long, default_value = "10")]
        depth: usize,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Get class inheritance
    Inheritance {
        /// Class ID
        id: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Get class implementations
    Implementations {
        /// Class ID
        id: String,

        /// Project ID
        #[arg(long)]
        project_id: i64,
    },

    /// Search entities (FTS5 full-text search)
    Search {
        /// Search query
        query: String,

        /// Project ID (optional)
        #[arg(long)]
        project_id: Option<i64>,

        /// Maximum number of results
        #[arg(short, long, default_value = "20")]
        limit: i64,

        /// Filter by entity kind (optional)
        #[arg(long)]
        kind: Option<String>,
    },
}

/// Watch commands
#[derive(Subcommand)]
pub enum WatchCommands {
    /// Start watching a directory
    Start {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Directory to watch
        #[arg(short, long)]
        path: String,

        /// File extensions to watch
        #[arg(long)]
        extensions: Option<String>,

        /// Debounce interval in milliseconds
        #[arg(long, default_value = "500")]
        debounce: u64,
    },

    /// Stop watching
    Stop {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Get watch status
    Status {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },
}

/// Storage commands
#[derive(Subcommand)]
pub enum StorageCommands {
    /// Get storage status
    Status,

    /// Get index statistics
    Stats {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Clear index
    Clear {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Clear vectors
        #[arg(long, default_value = "true")]
        vectors: bool,

        /// Clear BM25 index
        #[arg(long, default_value = "true")]
        bm25: bool,

        /// Clear relations
        #[arg(long, default_value = "true")]
        relations: bool,

        /// Clear cache
        #[arg(long, default_value = "true")]
        cache: bool,
    },

    /// Delete a file from index
    DeleteFile {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// File path
        path: String,
    },

    /// Delete an entity from index
    DeleteEntity {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Entity ID
        id: String,
    },

    /// Batch delete
    BatchDelete {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// File paths to delete (comma-separated)
        #[arg(long)]
        files: Option<String>,

        /// Entity IDs to delete (comma-separated)
        #[arg(long)]
        entities: Option<String>,
    },
}

/// Tool commands
#[derive(Subcommand)]
pub enum ToolCommands {
    /// Compress code from file
    Compress {
        /// File path to compress
        #[arg(short, long)]
        file_path: String,

        /// Include entities in compression
        #[arg(long, default_value_t = false)]
        include_entities: bool,

        /// Include groups in compression
        #[arg(long, default_value_t = false)]
        include_groups: bool,

        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Batch compress multiple files
    BatchCompress {
        /// File paths to compress (multiple --file-path flags)
        #[arg(short, long = "file-path")]
        file_paths: Vec<String>,

        /// Include entities in compression
        #[arg(long, default_value_t = false)]
        include_entities: bool,

        /// Include groups in compression
        #[arg(long, default_value_t = false)]
        include_groups: bool,

        /// Maximum concurrency
        #[arg(long)]
        max_concurrency: Option<usize>,
    },

    /// Diagnose code
    Diagnose {
        /// Code to diagnose
        #[arg(short, long)]
        code: String,

        /// Language
        #[arg(short, long)]
        language: Option<String>,

        /// File name hint
        #[arg(long)]
        file_name: Option<String>,

        /// Include AST in output
        #[arg(long, default_value_t = false)]
        include_ast: bool,
    },

    /// Get symbols from files
    Symbols {
        /// File paths
        #[arg(short, long)]
        paths: Vec<String>,

        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Find references at a location
    References {
        /// File path
        #[arg(short, long)]
        path: String,

        /// Line number (0-based)
        #[arg(short, long)]
        line: usize,

        /// Column number (optional)
        #[arg(long)]
        column: Option<usize>,

        /// Symbol name (optional, used if file/line/column not provided)
        #[arg(long)]
        symbol: Option<String>,

        /// Number of context lines to show
        #[arg(long)]
        context_lines: Option<usize>,

        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Go to definition
    Definition {
        /// File path
        #[arg(short, long)]
        path: String,

        /// Line number (0-based)
        #[arg(short, long)]
        line: usize,

        /// Column number (optional)
        #[arg(long)]
        column: Option<usize>,

        /// Symbol name (optional, used if file/line/column not provided)
        #[arg(long)]
        symbol: Option<String>,

        /// Include function body in output
        #[arg(long, default_value_t = false)]
        include_body: bool,

        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Keyword search (BM25-based with highlighted snippets)
    KeyWordSearch {
        /// Search query
        #[arg(short, long)]
        query: String,

        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,

        /// Maximum number of results
        #[arg(short, long, default_value = "10")]
        top_n: usize,
    },
}

/// Summary commands
#[derive(Subcommand)]
pub enum SummaryCommands {
    /// Generate summaries for files or directories
    Generate {
        /// File paths to summarize
        #[arg(long)]
        file_paths: Vec<String>,

        /// Directory paths to scan and summarize
        #[arg(long)]
        directory_paths: Vec<String>,

        /// File extensions to include
        #[arg(long)]
        extensions: Vec<String>,

        /// Directories to exclude
        #[arg(long)]
        exclude_dirs: Vec<String>,

        /// Respect .gitignore
        #[arg(long, default_value_t = false)]
        respect_gitignore: bool,
    },
}

/// Config commands
#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Reload configuration
    Reload {
        /// Project ID
        #[arg(short = 'P', long)]
        project_id: i64,
    },

    /// Show current configuration info
    Info,

    /// Validate configuration
    Validate,
}

/// Qdrant process management
#[derive(Debug, Subcommand)]
pub enum QdrantCommands {
    /// Process management actions
    Process {
        #[command(subcommand)]
        action: QdrantProcessAction,
    },
}

/// Qdrant process actions
#[derive(Debug, Subcommand)]
pub enum QdrantProcessAction {
    /// Check Qdrant process status
    Status,
    /// Start Qdrant process
    Start,
    /// Stop Qdrant process
    Stop,
    /// Restart Qdrant process
    Restart,
}

/// Metrics commands
#[derive(Subcommand)]
pub enum MetricsCommands {
    /// Export metrics in Prometheus format
    Prometheus,

    /// Export metrics in JSON format
    Json,

    /// Get metrics history
    History {
        /// Start time in RFC3339 format
        #[arg(long)]
        from: Option<String>,

        /// End time in RFC3339 format
        #[arg(long)]
        to: Option<String>,

        /// Metric name filter
        #[arg(long)]
        metric: Option<String>,

        /// Project ID filter
        #[arg(long)]
        project_id: Option<i64>,

        /// Operation type filter
        #[arg(long)]
        operation_type: Option<String>,
    },

    /// Clean up old metrics data
    Cleanup {
        /// Delete all historical metrics
        #[arg(long)]
        all: bool,

        /// Delete records before this RFC3339 timestamp
        #[arg(long)]
        before: Option<String>,

        /// Keep data for the last N days
        #[arg(short, long, default_value = "30")]
        keep_days: u64,
    },
}

/// Health monitoring and retry queue commands
#[derive(Subcommand)]
pub enum HealthCommands {
    /// Unified health check for all external services
    Check,

    /// Qdrant detailed diagnostics (circuit breaker, collection info)
    Qdrant,

    /// Embedding service health (per-provider status)
    Embedding,

    /// BM25 index health
    Bm25,

    /// Get retry queue status (pending query count)
    QueueStatus,

    /// Manually trigger retry queue processing
    QueueProcess,

    /// Clear retry queue (discard all pending queries)
    QueueClear,
}

impl Cli {
    /// Execute the CLI command
    pub async fn execute(&self) -> anyhow::Result<()> {
        match &self.command {
            Commands::Index(cmd) => commands::index::execute(cmd, &self.server, self.verbose).await,
            Commands::Search(cmd) => {
                commands::search::execute(cmd, &self.server, self.verbose, self.format.clone())
                    .await
            }
            Commands::AggSearch(cmd) => {
                let client = crate::client::ApiClient::new(&self.server)?;
                cmd.execute(&client, self.verbose).await
            }
            Commands::Project(cmd) => {
                commands::project::execute(cmd, &self.server, self.verbose).await
            }
            Commands::Entity(cmd) => {
                commands::entity::execute(cmd, &self.server, self.verbose, self.format.clone())
                    .await
            }
            Commands::Watch(cmd) => commands::watch::execute(cmd, &self.server, self.verbose).await,
            Commands::Storage(cmd) => {
                commands::storage::execute(cmd, &self.server, self.verbose).await
            }
            Commands::Tools(cmd) => commands::tools::execute(cmd, &self.server, self.verbose).await,
            Commands::Summary(cmd) => self.execute_summary(cmd).await,
            Commands::Config(cmd) => self.execute_config(cmd).await,
            Commands::Qdrant(cmd) => {
                commands::qdrant::execute(cmd, &self.server, self.verbose).await
            }
            Commands::Metrics(cmd) => self.execute_metrics(cmd).await,
            Commands::Health(cmd) => self.execute_health(cmd).await,
            Commands::Status => {
                commands::status::execute(&self.server, self.verbose, self.format.clone()).await
            }
        }
    }

    /// Execute summary command
    async fn execute_summary(&self, cmd: &SummaryCommands) -> anyhow::Result<()> {
        match cmd {
            SummaryCommands::Generate {
                file_paths,
                directory_paths,
                extensions,
                exclude_dirs,
                respect_gitignore,
            } => {
                let options = commands::summary::SummaryOptions::new(
                    commands::summary::InputPaths {
                        files: file_paths.clone(),
                        directories: directory_paths.clone(),
                    },
                    commands::summary::FilterConfig {
                        extensions: extensions.clone(),
                        exclude_dirs: exclude_dirs.clone(),
                        ignore_patterns: Vec::new(),
                        respect_gitignore: *respect_gitignore,
                        max_files: 100,
                    },
                    commands::summary::ExecutionContext {
                        server: self.server.clone(),
                        verbose: self.verbose,
                    },
                );

                commands::summary::execute(options).await
            }
        }
    }

    /// Execute config command
    async fn execute_config(&self, cmd: &ConfigCommands) -> anyhow::Result<()> {
        match cmd {
            ConfigCommands::Reload { project_id } => {
                commands::config::execute_reload(&self.server, *project_id, self.verbose).await
            }
            ConfigCommands::Info => {
                commands::config::execute_info(&self.server, self.verbose).await
            }
            ConfigCommands::Validate => {
                commands::config::execute_validate(&self.server, self.verbose).await
            }
        }
    }

    /// Execute metrics command
    async fn execute_metrics(&self, cmd: &MetricsCommands) -> anyhow::Result<()> {
        match cmd {
            MetricsCommands::Prometheus => {
                commands::metrics::execute(
                    commands::metrics::MetricsFormat::Prometheus,
                    &self.server,
                    self.verbose,
                )
                .await
            }
            MetricsCommands::Json => {
                commands::metrics::execute(
                    commands::metrics::MetricsFormat::Json,
                    &self.server,
                    self.verbose,
                )
                .await
            }
            MetricsCommands::History {
                from,
                to,
                metric,
                project_id,
                operation_type,
            } => {
                commands::metrics::execute_history(
                    from.as_deref(),
                    to.as_deref(),
                    metric.as_deref(),
                    *project_id,
                    operation_type.as_deref(),
                    &self.server,
                    self.verbose,
                )
                .await
            }
            MetricsCommands::Cleanup {
                all,
                before,
                keep_days,
            } => {
                commands::metrics::execute_cleanup(
                    *all,
                    before.as_deref(),
                    *keep_days,
                    &self.server,
                    self.verbose,
                )
                .await
            }
        }
    }

    /// Execute health command
    async fn execute_health(&self, cmd: &HealthCommands) -> anyhow::Result<()> {
        let health_cmd = match cmd {
            HealthCommands::Check => commands::health::HealthCommand::Check,
            HealthCommands::Qdrant => commands::health::HealthCommand::Qdrant,
            HealthCommands::Embedding => commands::health::HealthCommand::Embedding,
            HealthCommands::Bm25 => commands::health::HealthCommand::Bm25,
            HealthCommands::QueueStatus => commands::health::HealthCommand::QueueStatus,
            HealthCommands::QueueProcess => commands::health::HealthCommand::QueueProcess,
            HealthCommands::QueueClear => commands::health::HealthCommand::QueueClear,
        };
        commands::health::execute(&health_cmd, &self.server, self.verbose, self.format.clone())
            .await
    }
}
