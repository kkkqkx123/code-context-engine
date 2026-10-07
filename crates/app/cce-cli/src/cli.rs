//! CLI argument definitions using clap

use clap::{Parser, Subcommand};

use crate::commands;

/// Code Context Engine CLI Client
#[derive(Parser)]
#[command(name = "cce-cli")]
#[command(about = "CLI client for Code Context Engine", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Server URL (e.g., http://127.0.0.1:9000)
    #[arg(
        short,
        long,
        env = "CCE_SERVER_URL",
        default_value = "http://127.0.0.1:9000"
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
#[derive(clap::ValueEnum, Clone, Copy, Default)]
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

    /// Graph traversal over the relation snapshot
    #[command(subcommand)]
    Graph(GraphCommands),

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

    /// Local file gateway for remote hosting (push only)
    #[command(subcommand)]
    Gateway(GatewayCommands),

    /// Run the MCP (Model Context Protocol) server locally
    #[command(subcommand)]
    Mcp(McpCommands),
}

pub use crate::commands::mcp::McpCommands;

pub use crate::commands::index::IndexCommands;

pub use crate::commands::search::SearchCommands;

pub use crate::commands::project::ProjectCommands;

pub use crate::commands::entity::EntityCommands;

pub use crate::commands::graph::GraphCommands;

pub use crate::commands::watch::WatchCommands;

pub use crate::commands::storage::StorageCommands;

pub use crate::commands::tools::ToolCommands;

pub use crate::commands::summary::SummaryCommands;

pub use crate::commands::config::ConfigCommands;

pub use crate::commands::qdrant::QdrantCommands;

pub use crate::commands::metrics::MetricsCommands;

pub use crate::commands::gateway::GatewayCommands;

pub use crate::commands::health::HealthCommands;

impl Cli {
    /// Execute the CLI command
    pub async fn execute(&self) -> anyhow::Result<()> {
        match &self.command {
            Commands::Index(cmd) => commands::index::execute(cmd, &self.server, self.verbose).await,
            Commands::Search(cmd) => {
                commands::search::execute(cmd, &self.server, self.verbose, self.format).await
            }
            Commands::AggSearch(cmd) => {
                let client = crate::client::ApiClient::new(&self.server)?;
                cmd.execute(&client, self.verbose).await
            }
            Commands::Project(cmd) => {
                commands::project::execute(cmd, &self.server, self.verbose).await
            }
            Commands::Entity(cmd) => {
                commands::entity::execute(cmd, &self.server, self.verbose, self.format).await
            }
            Commands::Graph(cmd) => {
                commands::graph::execute(cmd, &self.server, self.verbose, self.format).await
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
                commands::status::execute(&self.server, self.verbose, self.format).await
            }
            Commands::Gateway(cmd) => {
                commands::gateway::execute(cmd, &self.server, self.verbose, self.format).await
            }
            Commands::Mcp(cmd) => commands::mcp::execute(cmd).await,
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
        commands::health::execute(&health_cmd, &self.server, self.verbose, self.format).await
    }
}
