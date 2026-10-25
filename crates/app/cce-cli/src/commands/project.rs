//! Project command handlers

use clap::Subcommand;

use anyhow::Result;

use crate::client::ApiClient;
use crate::output::{print_error, print_success, print_table, truncate};
use cce_api::models::{
    CreateProjectRequest, ProjectConfig, ProjectConfigReloadResponse, ProjectConfigUpdateRequest,
    ProjectConfigUpdateResponse, ProjectDeleteResponse, ProjectDetailResponse,
    ProjectIndexResponse, ProjectListResponse, UpdateProjectRequest,
};

fn project_path(id: &str) -> String {
    cce_api::paths::PROJECT_DETAIL_TEMPLATE.replace(cce_api::paths::PROJECT_ID_PLACEHOLDER, id)
}

pub async fn execute(cmd: &ProjectCommands, server: &str, verbose: bool) -> Result<()> {
    let client = ApiClient::new(server)?;

    match cmd {
        ProjectCommands::Create {
            path,
            name,
            extensions,
            exclude,
        } => create_project(&client, path, name, extensions, exclude, verbose).await,
        ProjectCommands::List => list_projects(&client, verbose).await,
        ProjectCommands::Get { id } => get_project(&client, id, verbose).await,
        ProjectCommands::Update { id, name } => update_project(&client, id, name, verbose).await,
        ProjectCommands::Delete { id } => delete_project(&client, id, verbose).await,
        ProjectCommands::Index { id } => index_project(&client, id, verbose).await,
        ProjectCommands::Reload { id } => reload_project_config(&client, id, verbose).await,
        ProjectCommands::Config { id } => update_project_config(&client, id, verbose).await,
    }
}

/// Execute project commands through the domain service abstraction.
///
/// Local and remote share parameter semantics; only the transport differs.
pub async fn execute_with_service(
    cmd: &ProjectCommands,
    service: &crate::service::CliService,
    server: &str,
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
            match service.create_project(request).await {
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
        ProjectCommands::List => match service.list_projects().await {
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
            let project_id: i64 = id.parse().unwrap_or(-1);
            match service.get_project(project_id).await {
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
            let project_id: i64 = id.parse().unwrap_or(-1);
            match service.delete_project(project_id).await {
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
        ProjectCommands::Index { id } => {
            let client = ApiClient::new(server)?;
            index_project(&client, id, verbose).await
        }
        ProjectCommands::Reload { id } => {
            let project_id: i64 = id.parse().unwrap_or(-1);
            match service.reload_project(project_id).await {
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
        ProjectCommands::Config { id } => {
            let client = ApiClient::new(server)?;
            update_project_config(&client, id, verbose).await
        }
        ProjectCommands::Update { id, name } => {
            let client = ApiClient::new(server)?;
            update_project(&client, id, name, verbose).await
        }
    }
}

async fn create_project(
    client: &ApiClient,
    path: &str,
    name: &Option<String>,
    extensions: &str,
    exclude: &str,
    verbose: bool,
) -> Result<()> {
    let ext_list: Vec<String> = extensions
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let exclude_list: Vec<String> = exclude.split(',').map(|s| s.trim().to_string()).collect();

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

    let response: ProjectDetailResponse = client.post(cce_api::paths::PROJECT, &request).await?;

    if response.success {
        print_success(&format!("Project created: {}", response.project.name));
        println!();
        print_project_details(&response.project);
    } else {
        print_error("Failed to create project");
    }

    Ok(())
}

async fn list_projects(client: &ApiClient, verbose: bool) -> Result<()> {
    if verbose {
        println!("Fetching project list...");
    }

    let response: ProjectListResponse = client.get(cce_api::paths::PROJECT).await?;

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

async fn get_project(client: &ApiClient, id: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Fetching project: {}", id);
    }

    let path = project_path(id);
    let response: ProjectDetailResponse = client.get(&path).await?;

    if response.success {
        print_success(&format!("Project: {}", response.project.name));
        println!();
        print_project_details(&response.project);
    } else {
        print_error("Failed to get project");
    }

    Ok(())
}

async fn update_project(
    client: &ApiClient,
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

    let path = project_path(id);
    let response: ProjectDetailResponse = client.put(&path, &request).await?;

    if response.success {
        print_success(&format!("Project updated: {}", response.project.name));
        println!();
        print_project_details(&response.project);
    } else {
        print_error("Failed to update project");
    }

    Ok(())
}

async fn delete_project(client: &ApiClient, id: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Deleting project: {}", id);
    }

    let path = project_path(id);
    let response: ProjectDeleteResponse = client.delete(&path).await?;

    if response.success {
        print_success(&format!("Project {} deleted", response.project_id));
    } else {
        print_error("Failed to delete project");
    }

    Ok(())
}

async fn index_project(client: &ApiClient, id: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Indexing project: {}", id);
    }

    let path =
        cce_api::paths::PROJECT_INDEX_TEMPLATE.replace(cce_api::paths::PROJECT_ID_PLACEHOLDER, id);
    let response: ProjectIndexResponse = client.post(&path, &serde_json::json!({})).await?;

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

async fn reload_project_config(client: &ApiClient, id: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Reloading project config: {}", id);
    }

    let path =
        cce_api::paths::PROJECT_RELOAD_TEMPLATE.replace(cce_api::paths::PROJECT_ID_PLACEHOLDER, id);
    let response: ProjectConfigReloadResponse = client.post(&path, &serde_json::json!({})).await?;

    if response.success {
        print_success(&format!(
            "Project {} configuration reloaded",
            response.project_id
        ));
        println!("  {}", response.message);
    } else {
        print_error("Failed to reload project configuration");
    }

    Ok(())
}

async fn update_project_config(client: &ApiClient, id: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Updating project config: {}", id);
    }

    let path =
        cce_api::paths::PROJECT_CONFIG_TEMPLATE.replace(cce_api::paths::PROJECT_ID_PLACEHOLDER, id);
    let request = ProjectConfigUpdateRequest {
        config: serde_json::json!({}),
    };
    let response: ProjectConfigUpdateResponse = client.put(&path, &request).await?;

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
