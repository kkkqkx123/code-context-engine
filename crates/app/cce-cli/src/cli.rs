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

    /// Server lifecycle management (local process only; business commands stay pure HTTP clients)
    #[command(subcommand)]
    Server(ServerCommands),

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

pub use crate::commands::server::ServerCommands;

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
    /// Execute the CLI command.
    ///
    /// One shared client is built at the entry for every HTTP business
    /// command; handlers receive the client instead of constructing their
    /// own. Local lifecycle commands (`server`, `mcp`) stay client-free.
    pub async fn execute(&self) -> anyhow::Result<()> {
        match &self.command {
            Commands::Index(cmd) => self.execute_index(cmd).await,
            Commands::Search(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                commands::search::execute_with_client(cmd, &client, self.verbose, self.format).await
            }
            Commands::AggSearch(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                cmd.execute_with_client(&client, self.verbose).await
            }
            Commands::Project(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                commands::project::execute_with_client(cmd, &client, self.verbose).await
            }
            Commands::Entity(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                commands::entity::execute_with_client(cmd, &client, self.verbose, self.format).await
            }
            Commands::Graph(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                commands::graph::execute_with_client(cmd, &client, self.verbose, self.format).await
            }
            Commands::Watch(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                commands::watch::execute_with_client(cmd, &client, self.verbose).await
            }
            Commands::Storage(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                commands::storage::execute_with_client(cmd, &client, self.verbose).await
            }
            Commands::Tools(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                commands::tools::execute_with_client(cmd, &client, self.verbose).await
            }
            Commands::Summary(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                self.execute_summary_with_client(cmd, &client).await
            }
            Commands::Config(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                self.execute_config_with_client(cmd, &client).await
            }
            Commands::Qdrant(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                commands::qdrant::execute_with_client(cmd, &client, self.verbose).await
            }
            Commands::Metrics(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                self.execute_metrics_with_client(cmd, &client).await
            }
            Commands::Server(cmd) => {
                commands::server::execute(cmd, &self.server, self.verbose, self.format).await
            }
            Commands::Health(cmd) => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                self.execute_health_with_client(cmd, &client).await
            }
            Commands::Status => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                commands::status::execute_with_client(&client, self.verbose, self.format).await
            }
            Commands::Gateway(cmd) => {
                let gateway_client = cce_gateway::GatewayClient::new_with_token(
                    &self.server,
                    commands::gateway::explicit_token(cmd),
                )?;
                commands::gateway::execute_with_client(
                    cmd,
                    &gateway_client,
                    self.verbose,
                    self.format,
                )
                .await
            }
            Commands::Mcp(cmd) => commands::mcp::execute(cmd).await,
        }
    }

    /// Execute index commands, routing push mode to ingest and honoring
    /// per-command timeout overrides for the single-request path.
    async fn execute_index(&self, cmd: &IndexCommands) -> anyhow::Result<()> {
        match cmd {
            IndexCommands::Run {
                push,
                token,
                timeout_secs,
                ..
            } if *push => {
                let gateway_client = cce_gateway::GatewayClient::new_with_token(
                    &self.server,
                    token.clone(),
                )?;
                commands::index::execute_push_with_gateway(cmd, &gateway_client, self.verbose).await
            }
            IndexCommands::Run { timeout_secs, .. } if timeout_secs.is_some() => {
                let secs = timeout_secs.unwrap_or(300).max(1);
                let client =
                    cce_transport::SharedHttpClient::new_with_token_and_timeout(
                        &self.server,
                        None,
                        std::time::Duration::from_secs(secs),
                    )?;
                commands::index::execute_with_client(cmd, &client, self.verbose).await
            }
            _ => {
                let client = cce_transport::SharedHttpClient::new(&self.server)?;
                commands::index::execute_with_client(cmd, &client, self.verbose).await
            }
        }
    }

    /// Execute summary command
    async fn execute_summary_with_client(
        &self,
        cmd: &SummaryCommands,
        client: &cce_transport::SharedHttpClient,
    ) -> anyhow::Result<()> {
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

                commands::summary::execute_with_client(options, client).await
            }
        }
    }

    /// Execute config command through the client abstraction.
    async fn execute_config_with_client(
        &self,
        cmd: &ConfigCommands,
        client: &cce_transport::SharedHttpClient,
    ) -> anyhow::Result<()> {
        match cmd {
            ConfigCommands::Reload { project_id } => {
                commands::config::execute_reload_with_client(client, *project_id, self.verbose)
                    .await
            }
            ConfigCommands::Info => {
                commands::config::execute_info_with_client(client, self.verbose).await
            }
            ConfigCommands::Validate => {
                commands::config::execute_validate_with_client(client, self.verbose).await
            }
        }
    }

    /// Execute metrics command
    async fn execute_metrics_with_client(
        &self,
        cmd: &MetricsCommands,
        client: &cce_transport::SharedHttpClient,
    ) -> anyhow::Result<()> {
        match cmd {
            MetricsCommands::Prometheus => {
                commands::metrics::execute_with_client(
                    commands::metrics::MetricsFormat::Prometheus,
                    client,
                    self.verbose,
                )
                .await
            }
            MetricsCommands::Json => {
                commands::metrics::execute_with_client(
                    commands::metrics::MetricsFormat::Json,
                    client,
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
                commands::metrics::execute_history_with_client(
                    from.as_deref(),
                    to.as_deref(),
                    metric.as_deref(),
                    *project_id,
                    operation_type.as_deref(),
                    client,
                    self.verbose,
                )
                .await
            }
            MetricsCommands::Cleanup {
                all,
                before,
                keep_days,
            } => {
                commands::metrics::execute_cleanup_with_client(
                    *all,
                    before.as_deref(),
                    *keep_days,
                    client,
                    self.verbose,
                )
                .await
            }
        }
    }

    /// Execute health command
    async fn execute_health_with_client(
        &self,
        cmd: &HealthCommands,
        client: &cce_transport::SharedHttpClient,
    ) -> anyhow::Result<()> {
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
        commands::health::execute_with_client(&health_cmd, client, self.verbose, self.format).await
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
        assert!(matches!(
            parse(&["server", "run"]).command,
            Commands::Server(_)
        ));
        assert!(matches!(
            parse(&["server", "status"]).command,
            Commands::Server(_)
        ));
        assert!(matches!(
            parse(&["server", "logs"]).command,
            Commands::Server(_)
        ));
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
