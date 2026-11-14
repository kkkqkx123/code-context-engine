//! Project command handlers

use clap::Subcommand;

use anyhow::Result;

use crate::output::{print_error, print_success, print_table, truncate};
use cce_api::models::{
    CreateProjectRequest, ProjectConfig, ProjectConfigUpdateRequest, UpdateProjectRequest,
};
use cce_client::Client;

/// Parse a project id given on the command line.
fn project_id(raw: &str) -> Result<i64> {
    raw.parse()
        .map_err(|_| anyhow::anyhow!("project id must be a number: {raw}"))
}

/// Execute project commands through the client abstraction.
pub async fn execute_with_client(
    cmd: &ProjectCommands,
    client: &Client,
    verbose: bool,
) -> Result<()> {
    match cmd {
        ProjectCommands::Create {
            path,
            name,
            extensions,
            exclude,
        } => {
            let ext_list: Vec<String> = extensions
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            let exclude_list: Vec<String> =
                exclude.split(',').map(|s| s.trim().to_string()).collect();
            let request = CreateProjectRequest {
                name: name.clone(),
                root_path: path.to_string(),
                extensions: ext_list,
                exclude_dirs: exclude_list,
                respect_gitignore: true,
                ignore_patterns: vec![],
                supply_mode: None,
            };
            if verbose {
                println!("Creating project at: {}", path);
            }
            match client.create_project(&request).await {
                Ok(response) => {
                    if response.success {
                        print_success(&format!("Project created: {}", response.project.name));
                        println!();
                        print_project_details(&response.project);
                    } else {
                        print_error("Failed to create project");
                    }
                }
                Err(e) => print_error(&format!("Failed to create project: {e}")),
            }
            Ok(())
        }
        ProjectCommands::List => match client.list_projects().await {
            Ok(response) => {
                if response.success {
                    println!("Projects ({} total):", response.total);
                    println!();
                    if response.projects.is_empty() {
                        println!("No projects found");
                    } else {
                        let headers = vec!["#", "ID", "Name", "Path"];
                        let rows: Vec<Vec<String>> = response
                            .projects
                            .iter()
                            .enumerate()
                            .map(|(i, p)| {
                                vec![
                                    (i + 1).to_string(),
                                    p.id.clone(),
                                    p.name.clone(),
                                    truncate(&p.root_path, 40),
                                ]
                            })
                            .collect();
                        print_table(&headers, &rows);
                    }
                } else {
                    print_error("Failed to list projects");
                }
                Ok(())
            }
            Err(e) => {
                print_error(&format!("Failed to list projects: {e}"));
                Ok(())
            }
        },
        ProjectCommands::Get { id } => {
            match client.get_project(project_id(id)?).await {
                Ok(response) => {
                    if response.success {
                        print_success(&format!("Project: {}", response.project.name));
                        println!();
                        print_project_details(&response.project);
                    } else {
                        print_error("Failed to get project");
                    }
                }
                Err(e) => print_error(&format!("Failed to get project: {e}")),
            }
            Ok(())
        }
        ProjectCommands::Delete { id } => {
            match client.delete_project(project_id(id)?).await {
                Ok(response) => {
                    if response.success {
                        print_success(&format!("Project {} deleted", response.project_id));
                    } else {
                        print_error("Failed to delete project");
                    }
                }
                Err(e) => print_error(&format!("Failed to delete project: {e}")),
            }
            Ok(())
        }
        ProjectCommands::Index { id } => index_project(client, id, verbose).await,
        ProjectCommands::Reload { id } => {
            match client.reload_project(project_id(id)?).await {
                Ok(response) => {
                    if response.success {
                        print_success(&format!(
                            "Project {} configuration reloaded",
                            response.project_id
                        ));
                        println!("  {}", response.message);
                    } else {
                        print_error("Failed to reload project configuration");
                    }
                }
                Err(e) => print_error(&format!("Failed to reload project configuration: {e}")),
            }
            Ok(())
        }
        ProjectCommands::Config { id } => update_project_config(client, id, verbose).await,
        ProjectCommands::Update { id, name } => update_project(client, id, name, verbose).await,
    }
}

async fn update_project(
    client: &Client,
    id: &str,
    name: &Option<String>,
    verbose: bool,
) -> Result<()> {
    let request = UpdateProjectRequest {
        name: name.clone(),
        ..Default::default()
    };

    if verbose {
        println!("Updating project: {}", id);
    }

    let response = client.update_project(project_id(id)?, &request).await?;

    if response.success {
        print_success(&format!("Project updated: {}", response.project.name));
        println!();
        print_project_details(&response.project);
    } else {
        print_error("Failed to update project");
    }

    Ok(())
}

async fn index_project(client: &Client, id: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Indexing project: {}", id);
    }

    let response = client.index_project(project_id(id)?).await?;

    if response.success {
        print_success(&format!(
            "Project indexed: {} files, {} entities",
            response.indexed_files, response.total_entities
        ));
    } else {
        print_error("Failed to index project");
    }

    Ok(())
}

fn print_project_details(project: &ProjectConfig) {
    println!("  ID:            {}", project.id);
    println!("  Name:          {}", project.name);
    println!("  Root path:     {}", project.root_path);
    println!("  Extensions:    {}", project.extensions.join(", "));
    println!("  Exclude dirs:  {}", project.exclude_dirs.join(", "));
    println!("  Respect gitignore: {}", project.respect_gitignore);
    println!("  Created at:    {}", project.created_at);

    if let Some(ref last_indexed) = project.last_indexed {
        println!("  Last indexed:  {}", last_indexed);
    }
}

async fn update_project_config(client: &Client, id: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Updating project config: {}", id);
    }

    let request = ProjectConfigUpdateRequest {
        config: serde_json::json!({}),
    };
    let response = client
        .update_project_config(project_id(id)?, &request)
        .await?;

    if response.success {
        print_success(&format!("Project {} configuration updated", id));
        println!("  {}", response.message);
    } else {
        print_error("Failed to update project configuration");
    }

    Ok(())
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
