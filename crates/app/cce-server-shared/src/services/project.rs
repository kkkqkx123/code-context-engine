//! Shared project-creation orchestration for local and HTTP transports.

use cce_api::models::{CreateProjectRequest, SUPPLY_MODE_GATEWAY, SUPPLY_MODE_KEY, SUPPLY_MODE_LOCAL};
use cce_config::ProjectAppConfig;
use cce_storage_metadb_sqlite::{
    NewProjectRecord, ProjectRepository, generate_project_name, SqliteClient,
};
use std::path::PathBuf;
use std::sync::Arc;

/// Result of a successful project creation.
#[derive(Debug, Clone)]
pub struct CreatedProject {
    pub project_id: i64,
    pub name: String,
    pub root_path: String,
    pub extensions: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub respect_gitignore: bool,
    pub ignore_patterns: Vec<String>,
}

/// Errors that can occur during shared project creation.
#[derive(Debug)]
pub enum ProjectCreateError {
    Invalid(String),
    MetadataUnavailable,
    PathExists,
    Conflict(String),
    Storage(String),
}

impl std::fmt::Display for ProjectCreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectCreateError::Invalid(m) => write!(f, "{m}"),
            ProjectCreateError::MetadataUnavailable => write!(f, "metadata store not initialized"),
            ProjectCreateError::PathExists => write!(f, "project already exists at this path"),
            ProjectCreateError::Conflict(m) => write!(f, "{m}"),
            ProjectCreateError::Storage(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ProjectCreateError {}

/// Shared project creation logic.
pub fn create_project(
    store: &Arc<SqliteClient>,
    request: &CreateProjectRequest,
) -> Result<CreatedProject, ProjectCreateError> {
    let supply_is_gateway = request
        .supply_mode
        .as_deref()
        .map(|m| m.trim().to_lowercase() == SUPPLY_MODE_GATEWAY)
        .unwrap_or(false);
    if request.root_path.is_empty() && !supply_is_gateway {
        return Err(ProjectCreateError::Invalid(
            "root path cannot be empty".to_string(),
        ));
    }
    let root_path_str = if supply_is_gateway {
        cce_types::normalize_logical_path(&request.root_path)
    } else {
        let root = PathBuf::from(&request.root_path);
        if !root.exists() {
            return Err(ProjectCreateError::Invalid(
                "root path does not exist".to_string(),
            ));
        }
        if !root.is_dir() {
            return Err(ProjectCreateError::Invalid(
                "root path is not a directory".to_string(),
            ));
        }
        root.canonicalize()
            .map_err(|e| ProjectCreateError::Invalid(format!("failed to resolve path: {e}")))?
            .to_string_lossy()
            .to_string()
    };

    let client = store.as_ref();
    let exists = client
        .with_transaction(|tx| ProjectRepository::path_exists(tx, &root_path_str))
        .map_err(|e| ProjectCreateError::Storage(e.to_string()))?;
    if exists {
        return Err(ProjectCreateError::PathExists);
    }

    let name = match &request.name {
        Some(n) if !n.is_empty() => n.clone(),
        _ => generate_project_name(&root_path_str),
    };
    let mut new_project = NewProjectRecord::new(name.clone(), root_path_str.clone());
    let initial_profile = ProjectAppConfig::from_filter_args(
        request.extensions.clone(),
        request.exclude_dirs.clone(),
        Some(request.respect_gitignore),
        request.ignore_patterns.clone(),
    );
    new_project.profile_json = Some(initial_profile.to_canonical_json());

    let project_id = match client.with_transaction(|tx| ProjectRepository::insert(tx, &new_project))
    {
        Ok(id) => id,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("UNIQUE constraint failed") {
                return Err(ProjectCreateError::Conflict(
                    "Project with this name or root path already exists".to_string(),
                ));
            }
            return Err(ProjectCreateError::Storage(format!(
                "Failed to create project: {msg}"
            )));
        }
    };

    // Initialize project metadata (non-critical).
    if let Err(e) = crate::runtime::recovery::ProjectMeta::init_for_project(client, project_id) {
        tracing::warn!(
            project_id = project_id,
            error = %e,
            "Failed to initialize project metadata (non-critical)"
        );
    }

    if let Some(supply_mode) = request.supply_mode.as_deref() {
        let normalized = supply_mode.trim().to_lowercase();
        if normalized == SUPPLY_MODE_GATEWAY || normalized == SUPPLY_MODE_LOCAL {
            if let Err(e) = client.with_transaction(|tx| {
                ProjectRepository::meta_set_string(tx, project_id, SUPPLY_MODE_KEY, &normalized)
            }) {
                tracing::warn!(
                    project_id = project_id,
                    error = %e,
                    "Failed to record supply mode (non-critical)"
                );
            }
        }
    }

    Ok(CreatedProject {
        project_id,
        name,
        root_path: root_path_str,
        extensions: request.extensions.clone(),
        exclude_dirs: request.exclude_dirs.clone(),
        respect_gitignore: request.respect_gitignore,
        ignore_patterns: request.ignore_patterns.clone(),
    })
}
