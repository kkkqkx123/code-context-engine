//! Project command handlers

use anyhow::Result;

use crate::cli::ProjectCommands;
use crate::client::ApiClient;
use crate::output::{print_error, print_success, print_table, truncate};
use cce_api::models::{
    CreateProjectRequest, ProjectConfig, ProjectDetailResponse, ProjectListResponse,
    UpdateProjectRequest,
};

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
    };

    if verbose {
        println!("Creating project at: {}", path);
    }

    let response: ProjectDetailResponse = client.post("/api/project", &request).await?;

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

    let response: ProjectListResponse = client.get("/api/project").await?;

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

    let path = format!("/api/project/{}", id);
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

    let path = format!("/api/project/{}", id);
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

    let path = format!("/api/project/{}", id);
    let response: serde_json::Value = client.delete(&path).await?;

    if response["success"].as_bool().unwrap_or(false) {
        print_success(&format!("Project {} deleted", id));
    } else {
        print_error("Failed to delete project");
    }

    Ok(())
}

async fn index_project(client: &ApiClient, id: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Indexing project: {}", id);
    }

    let path = format!("/api/project/{}/index", id);
    let response: serde_json::Value = client.post(&path, &serde_json::json!({})).await?;

    if response["success"].as_bool().unwrap_or(false) {
        print_success(&format!(
            "Project indexed: {} files, {} entities",
            response["indexed_files"].as_u64().unwrap_or(0),
            response["total_entities"].as_u64().unwrap_or(0)
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

    let path = format!("/api/project/{}/reload", id);
    let response: serde_json::Value = client.post(&path, &serde_json::json!({})).await?;

    if response["success"].as_bool().unwrap_or(false) {
        print_success(&format!("Project {} configuration reloaded", id));
        if let Some(message) = response["message"].as_str() {
            println!("  {}", message);
        }
    } else {
        print_error("Failed to reload project configuration");
    }

    Ok(())
}

async fn update_project_config(client: &ApiClient, id: &str, verbose: bool) -> Result<()> {
    if verbose {
        println!("Updating project config: {}", id);
    }

    let path = format!("/api/project/{}/config", id);
    let response: serde_json::Value = client.put(&path, &serde_json::json!({})).await?;

    if response["success"].as_bool().unwrap_or(false) {
        print_success(&format!("Project {} configuration updated", id));
        if let Some(message) = response["message"].as_str() {
            println!("  {}", message);
        }
    } else {
        print_error("Failed to update project configuration");
    }

    Ok(())
}
