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

    /// Execution mode: local calls the engine directly, remote uses HTTP.
    #[arg(long, value_enum, env = "CCE_SERVICE_MODE")]
    pub mode: Option<crate::service::CliServiceMode>,

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
        let mode = self.resolved_mode();
        match &self.command {
            Commands::Index(cmd) => {
                let service =
                    crate::service::CliService::build(&self.server, self.resolved_mode()).await?;
                commands::index::execute_with_service(
                    cmd,
                    service.as_trait(),
                    &self.server,
                    self.verbose,
                )
                .await
            }
            Commands::Search(cmd) => {
                let service =
                    crate::service::CliService::build(&self.server, self.resolved_mode()).await?;
                commands::search::execute_with_service(
                    cmd,
                    service.as_trait(),
                    self.verbose,
                    self.format,
                )
                .await
            }
            Commands::AggSearch(cmd) => {
                let service =
                    crate::service::CliService::build(&self.server, self.resolved_mode()).await?;
                cmd.execute_with_service(service.as_trait(), self.verbose)
                    .await
            }
            Commands::Project(cmd) => {
                let service =
                    crate::service::CliService::build(&self.server, self.resolved_mode()).await?;
                commands::project::execute_with_service(
                    cmd,
                    service.as_trait(),
                    &self.server,
                    self.verbose,
                )
                .await
            }
            Commands::Entity(cmd) => {
                if mode == crate::service::CliServiceMode::Local {
                    eprintln!(
                        "Entity commands require a running server; local embedded mode does not expose them. Use --mode remote or start a server."
                    );
                    return Ok(());
                }
                commands::entity::execute(cmd, &self.server, self.verbose, self.format).await
            }
            Commands::Graph(cmd) => {
                let service =
                    crate::service::CliService::build(&self.server, self.resolved_mode()).await?;
                commands::graph::execute_with_service(
                    cmd,
                    service.as_trait(),
                    &self.server,
                    self.verbose,
                    self.format,
                )
                .await
            }
            Commands::Watch(cmd) => {
                let service =
                    crate::service::CliService::build(&self.server, self.resolved_mode()).await?;
                commands::watch::execute_with_service(cmd, service.as_trait(), self.verbose).await
            }
            Commands::Storage(cmd) => {
                if mode == crate::service::CliServiceMode::Local {
                    eprintln!(
                        "Storage commands require a running server; local embedded mode does not expose them. Use --mode remote or start a server."
                    );
                    return Ok(());
                }
                commands::storage::execute(cmd, &self.server, self.verbose).await
            }
            Commands::Tools(cmd) => {
                if mode == crate::service::CliServiceMode::Local {
                    eprintln!(
                        "Tool commands require a running server; local embedded mode does not expose them. Use --mode remote or start a server."
                    );
                    return Ok(());
                }
                commands::tools::execute(cmd, &self.server, self.verbose).await
            }
            Commands::Summary(cmd) => {
                if mode == crate::service::CliServiceMode::Local {
                    eprintln!(
                        "Summary commands require a running server; local embedded mode does not expose them. Use --mode remote or start a server."
                    );
                    return Ok(());
                }
                self.execute_summary(cmd).await
            }
            Commands::Config(cmd) => {
                let service =
                    crate::service::CliService::build(&self.server, self.resolved_mode()).await?;
                self.execute_config_with_service(cmd, service.as_trait())
                    .await
            }
            Commands::Qdrant(cmd) => {
                if mode == crate::service::CliServiceMode::Local {
                    eprintln!(
                        "Qdrant commands require a running server; local embedded mode does not expose them. Use --mode remote or start a server."
                    );
                    return Ok(());
                }
                commands::qdrant::execute(cmd, &self.server, self.verbose).await
            }
            Commands::Metrics(cmd) => {
                if mode == crate::service::CliServiceMode::Local {
                    eprintln!(
                        "Metrics commands require a running server; local embedded mode does not expose them. Use --mode remote or start a server."
                    );
                    return Ok(());
                }
                self.execute_metrics(cmd).await
            }
            Commands::Health(cmd) => {
                if mode == crate::service::CliServiceMode::Local {
                    eprintln!(
                        "Health commands require a running server; local embedded mode does not expose them. Use --mode remote or start a server."
                    );
                    return Ok(());
                }
                self.execute_health(cmd).await
            }
            Commands::Status => {
                if mode == crate::service::CliServiceMode::Local {
                    eprintln!(
                        "Status command requires a running server; local embedded mode does not expose it. Use --mode remote or start a server."
                    );
                    return Ok(());
                }
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

    /// Execute config command through the domain service abstraction.
    async fn execute_config_with_service(
        &self,
        cmd: &ConfigCommands,
        service: &crate::service::CliService,
    ) -> anyhow::Result<()> {
        match cmd {
            ConfigCommands::Reload { project_id } => {
                commands::config::execute_reload_with_service(service, *project_id, self.verbose)
                    .await
            }
            ConfigCommands::Info => {
                commands::config::execute_info(&self.server, self.verbose).await
            }
            ConfigCommands::Validate => {
                commands::config::execute_validate(&self.server, self.verbose).await
            }
        }
    }

    /// Resolve the effective service mode for this invocation.
    fn resolved_mode(&self) -> crate::service::CliServiceMode {
        crate::service::CliServiceMode::resolve(self.mode)
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
            HealthCommands::DeadList => commands::health::HealthCommand::DeadList,
            HealthCommands::DeadClear => commands::health::HealthCommand::DeadClear,
            HealthCommands::AdmissionStats => commands::health::HealthCommand::AdmissionStats,
        };
        commands::health::execute(&health_cmd, &self.server, self.verbose, self.format).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn global_project_id_reaches_status_command() {
        let cli = Cli::try_parse_from(["cce-cli", "-P", "7", "status"])
            .expect("status with global project id parses");
        assert_eq!(cli.project_id, Some(7));
        assert!(matches!(cli.command, Commands::Status));
    }

    #[test]
    fn output_format_flag_selects_json() {
        let cli = Cli::try_parse_from(["cce-cli", "--format", "json", "status"])
            .expect("json format parses");
        assert!(matches!(cli.format, OutputFormat::Json));
    }

    #[test]
    fn explicit_server_overrides_default() {
        let cli = Cli::try_parse_from(["cce-cli", "--server", "http://10.0.0.2:9000", "status"])
            .expect("server override parses");
        assert_eq!(cli.server, "http://10.0.0.2:9000");
    }

    fn parse(args: &[&str]) -> Cli {
        let mut full = vec!["cce-cli"];
        full.extend(args);
        Cli::try_parse_from(full).expect("command parses")
    }

    #[test]
    fn every_command_group_parses_minimal_args() {
        assert!(matches!(
            parse(&["index", "run", "-P", "1", "--path", "/tmp"]).command,
            Commands::Index(_)
        ));
        assert!(matches!(
            parse(&["index", "parse", "--file", "/tmp/a.rs"]).command,
            Commands::Index(_)
        ));
        assert!(matches!(
            parse(&["search", "query", "-P", "1", "--query", "fold"]).command,
            Commands::Search(_)
        ));
        assert!(matches!(
            parse(&["agg-search", "--bm25", "fold"]).command,
            Commands::AggSearch(_)
        ));
        assert!(matches!(
            parse(&["project", "list"]).command,
            Commands::Project(_)
        ));
        assert!(matches!(
            parse(&["project", "create", "--path", "/tmp"]).command,
            Commands::Project(_)
        ));
        assert!(matches!(
            parse(&["entity", "function", "sym_x", "--project-id", "1"]).command,
            Commands::Entity(_)
        ));
        assert!(matches!(
            parse(&["graph", "ego", "sym_x", "--project-id", "1"]).command,
            Commands::Graph(_)
        ));
        assert!(matches!(
            parse(&["watch", "status", "-P", "1"]).command,
            Commands::Watch(_)
        ));
        assert!(matches!(
            parse(&["storage", "status"]).command,
            Commands::Storage(_)
        ));
        assert!(matches!(
            parse(&["storage", "stats", "-P", "1"]).command,
            Commands::Storage(_)
        ));
        assert!(matches!(
            parse(&["tools", "compress", "--file-path", "/tmp/a.rs"]).command,
            Commands::Tools(_)
        ));
        assert!(matches!(
            parse(&["summary", "generate", "--file-paths", "/tmp/a.rs"]).command,
            Commands::Summary(_)
        ));
        assert!(matches!(
            parse(&["config", "info"]).command,
            Commands::Config(_)
        ));
        assert!(matches!(
            parse(&["qdrant", "process", "status"]).command,
            Commands::Qdrant(_)
        ));
        assert!(matches!(
            parse(&["metrics", "json"]).command,
            Commands::Metrics(_)
        ));
        assert!(matches!(
            parse(&["health", "check"]).command,
            Commands::Health(_)
        ));
        assert!(matches!(
            parse(&["gateway", "sync", "-P", "1", "--path", "/tmp"]).command,
            Commands::Gateway(_)
        ));
        assert!(matches!(
            parse(&["gateway", "watch", "-P", "1", "--path", "/tmp"]).command,
            Commands::Gateway(_)
        ));
        assert!(matches!(parse(&["mcp", "stdio"]).command, Commands::Mcp(_)));
        assert!(matches!(parse(&["mcp", "http"]).command, Commands::Mcp(_)));
        assert!(matches!(parse(&["status"]).command, Commands::Status));
    }

    #[test]
    fn gateway_sync_no_commit_maps_to_staging_only() {
        let cli = parse(&[
            "gateway",
            "sync",
            "-P",
            "1",
            "--path",
            "/tmp",
            "--no-commit",
        ]);
        match cli.command {
            Commands::Gateway(commands::gateway::GatewayCommands::Sync { no_commit, .. }) => {
                assert!(no_commit)
            }
            _ => panic!("gateway sync must parse"),
        }
    }

    #[test]
    fn search_query_type_defaults_to_hybrid() {
        let cli = parse(&["search", "query", "-P", "1", "--query", "fold"]);
        match cli.command {
            Commands::Search(commands::search::SearchCommands::Query { query_type, .. }) => {
                assert_eq!(query_type, "hybrid")
            }
            _ => panic!("search query must parse"),
        }
    }
}
