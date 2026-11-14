//! Shared project use cases for the HTTP and MCP transports.
//!
//! Creation validates the requested root and persists the initial profile;
//! reads project the stored rows onto the wire contract; profile changes
//! invalidate the per-project components and ask the hot-update coordinator
//! to reload so both transports observe the same behavior.

use cce_api::models::{
    CreateProjectRequest, ProjectConfig, SUPPLY_MODE_GATEWAY, SUPPLY_MODE_KEY, SUPPLY_MODE_LOCAL,
};
use cce_config::ProjectAppConfig;
use cce_config::Settings;
use cce_config::project_registry::RegistryError;
use cce_config::validation::Validate;
use cce_llm::LlmClient;
use cce_storage_metadb_sqlite::{
    NewProjectRecord, ProjectRecord, ProjectRepository, SqliteClient, generate_project_name,
};
use std::path::PathBuf;
use std::sync::Arc;

use crate::context::SharedContext;

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

/// Failure of a project read, write or reload request.
#[derive(Debug)]
pub enum ProjectServiceError {
    /// No metadata store is configured for this engine.
    MetadataUnavailable,
    /// The project row does not exist.
    NotFound(i64),
    /// The requested value is not acceptable.
    Invalid(String),
    /// The requested value conflicts with stored state.
    Conflict(String),
    /// Reading or writing the project row failed.
    Storage(String),
}

impl std::fmt::Display for ProjectServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectServiceError::MetadataUnavailable => {
                write!(f, "metadata store not initialized")
            }
            ProjectServiceError::NotFound(id) => write!(f, "project {id} not found"),
            ProjectServiceError::Invalid(message) => write!(f, "{message}"),
            ProjectServiceError::Conflict(message) => write!(f, "{message}"),
            ProjectServiceError::Storage(message) => write!(f, "{message}"),
        }
    }
}

/// Translate one registry failure into the service error contract.
pub fn registry_error(error: RegistryError) -> ProjectServiceError {
    match error {
        RegistryError::ProjectNotFound(id) => ProjectServiceError::NotFound(id),
        RegistryError::PathNotFound(path) => {
            ProjectServiceError::Invalid(format!("Path does not exist: {path}"))
        }
        RegistryError::DuplicatePath(path) => {
            ProjectServiceError::Conflict(format!("Path already registered: {path}"))
        }
        RegistryError::Validation(message) | RegistryError::Configuration(message) => {
            ProjectServiceError::Invalid(message)
        }
        RegistryError::Serialization(message) | RegistryError::Deserialization(message) => {
            ProjectServiceError::Invalid(format!("Invalid configuration format: {message}"))
        }
        RegistryError::Io(error) => ProjectServiceError::Storage(error.to_string()),
        RegistryError::Database(error) => ProjectServiceError::Storage(error),
    }
}

impl std::error::Error for ProjectServiceError {}

/// Project every stored project row onto the wire view.
pub fn list_projects(state: &SharedContext) -> Result<Vec<ProjectConfig>, ProjectServiceError> {
    let store = state
        .engine
        .metadata_store()
        .ok_or(ProjectServiceError::MetadataUnavailable)?;
    let records = store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::get_all(tx))
        .map_err(|e| ProjectServiceError::Storage(format!("Failed to query projects: {e}")))?;
    Ok(records.iter().map(project_config_from_record).collect())
}

/// Project one stored project row onto the wire view.
pub fn get_project(
    state: &SharedContext,
    project_id: i64,
) -> Result<ProjectConfig, ProjectServiceError> {
    let store = state
        .engine
        .metadata_store()
        .ok_or(ProjectServiceError::MetadataUnavailable)?;
    let record = store
        .as_ref()
        .with_transaction(|tx| ProjectRepository::get_by_id(tx, project_id))
        .map_err(|e| ProjectServiceError::Storage(format!("Failed to query project: {e}")))?
        .ok_or(ProjectServiceError::NotFound(project_id))?;
    Ok(project_config_from_record(&record))
}

/// Apply the side effects of a stored profile change.
///
/// Drops the per-project component caches so orchestrators, searchers and
/// coordinators rebuild from the stored profile, then asks the hot-update
/// coordinator to reload. Returns whether the hot reload took effect; the
/// stored profile is authoritative either way.
pub async fn reload_project_after_profile_change(state: &SharedContext, project_id: i64) -> bool {
    state
        .engine
        .clear_project_component_caches(project_id)
        .await;
    match state.engine.get_hot_update_coordinator(project_id).await {
        Ok(coordinator) => {
            let mut coordinator = coordinator.lock().await;
            match coordinator.reload_project_config().await {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!(
                        project_id,
                        error = %e,
                        "Hot-update reload failed after profile change"
                    );
                    false
                }
            }
        }
        Err(e) => {
            tracing::debug!(
                project_id,
                error = %e,
                "Hot-update coordinator unavailable; profile changes apply on next start"
            );
            false
        }
    }
}

/// Convert a stored project row into the wire view.
///
/// This is a display and request-prefill view only. The profile document is
/// the single source of truth for scan lists. A section absent from the
/// profile is reported as empty so callers can tell that no explicit value
/// was registered. Effective indexing values always come from the merged
/// registry configuration, not from this view.
pub fn project_config_from_record(record: &ProjectRecord) -> ProjectConfig {
    let profile = ProjectAppConfig::from_json_str(&record.profile_json).ok();
    let (extensions, exclude_dirs, respect_gitignore, ignore_patterns) = match profile {
        Some(profile) => (
            profile
                .orchestrator
                .as_ref()
                .and_then(|orchestrator| orchestrator.indexer.as_ref())
                .and_then(|indexer| indexer.extensions.clone())
                .unwrap_or_default(),
            profile
                .orchestrator
                .as_ref()
                .and_then(|orchestrator| orchestrator.indexer.as_ref())
                .and_then(|indexer| indexer.exclude_dirs.clone())
                .unwrap_or_default(),
            profile
                .scanner
                .as_ref()
                .and_then(|scanner| scanner.respect_gitignore)
                .unwrap_or(true),
            profile
                .scanner
                .as_ref()
                .and_then(|scanner| scanner.gitignore_patterns.clone())
                .unwrap_or_default(),
        ),
        None => (Vec::new(), Vec::new(), true, Vec::new()),
    };

    ProjectConfig {
        id: record.id.to_string(),
        name: record.name.clone(),
        root_path: record.root_path.clone(),
        extensions,
        exclude_dirs,
        respect_gitignore,
        ignore_patterns,
        created_at: record.created_at.to_string(),
        last_indexed: record.last_indexed.clone(),
    }
}

/// Top-level keys that belong to the global configuration and must never be
/// accepted as part of a project profile update.
const FORBIDDEN_PROFILE_KEYS: &[&str] = &[
    "server",
    "database",
    "logger",
    "metrics",
    "export",
    "mcp",
    "search",
    "cache",
    "symbol_resolution",
];

/// Outcome of a stored profile write.
#[derive(Debug, Clone)]
pub struct ProfileUpdateOutcome {
    pub config_version: u64,
    /// Whether the hot-update coordinator picked up the new profile.
    pub hot_reload_applied: bool,
}

/// Read the stored profile document and its combined version.
pub async fn read_project_profile(
    state: &SharedContext,
    project_id: i64,
) -> Result<(serde_json::Value, u64), ProjectServiceError> {
    let registry = state.engine.project_registry();
    let (profile, version) = registry
        .get_profile(project_id)
        .await
        .map_err(registry_error)?;
    Ok((serde_json::to_value(&profile).unwrap_or_default(), version))
}

/// Validate and persist a project profile document, then apply its side
/// effects.
///
/// Global-only top-level keys are rejected before deserialization so a caller
/// cannot smuggle a global section into the project profile.
pub async fn update_project_profile(
    state: &SharedContext,
    project_id: i64,
    document: serde_json::Value,
) -> Result<ProfileUpdateOutcome, ProjectServiceError> {
    if let Some(forbidden) = document.as_object().and_then(|object| {
        FORBIDDEN_PROFILE_KEYS
            .iter()
            .find(|key| object.contains_key(**key))
    }) {
        return Err(ProjectServiceError::Invalid(format!(
            "Field '{forbidden}' belongs to global configuration and cannot be set per project"
        )));
    }
    let profile = serde_json::from_value::<ProjectAppConfig>(document).map_err(|error| {
        ProjectServiceError::Invalid(format!("Invalid project profile format: {error}"))
    })?;
    profile.validate_structured().map_err(|error| {
        ProjectServiceError::Invalid(format!("Configuration validation failed: {error}"))
    })?;
    // The write bumps `profile_version` atomically and invalidates the cached
    // effective configuration.
    let registry = state.engine.project_registry();
    let version = registry
        .update_profile(project_id, profile)
        .await
        .map_err(registry_error)?;
    let hot_reload_applied = reload_project_after_profile_change(state, project_id).await;
    Ok(ProfileUpdateOutcome {
        config_version: version,
        hot_reload_applied,
    })
}

/// Project LLM selections accepted by the typed update endpoint.
#[derive(Debug, Clone, Default)]
pub struct ProjectLlmUpdateSpec {
    pub default_chat: Option<String>,
    pub default_rerank: Option<String>,
    pub rerank_enabled: Option<bool>,
    pub rerank_max_candidates: Option<usize>,
    /// Whether the caller tried to patch global chat model parameters.
    pub chat_model_params_present: bool,
}

/// Persist project LLM selections into the project profile.
///
/// Only project-effective chat and rerank selections are accepted; provider
/// connection details and per-model tuning stay global.
pub async fn update_project_llm(
    state: &SharedContext,
    project_id: i64,
    spec: ProjectLlmUpdateSpec,
) -> Result<ProfileUpdateOutcome, ProjectServiceError> {
    if spec.chat_model_params_present {
        return Err(ProjectServiceError::Invalid(
            "chat_model_params belongs to the global model registry; change it in the server configuration file"
                .to_string(),
        ));
    }
    if let Some(max_candidates) = spec.rerank_max_candidates
        && !(1..=1000).contains(&max_candidates)
    {
        return Err(ProjectServiceError::Invalid(
            "rerank_max_candidates must be within 1..=1000".to_string(),
        ));
    }
    let global = Settings::global().map_err(|error| {
        ProjectServiceError::Storage(format!("Failed to load global config: {error}"))
    })?;
    if let Some(ref default_chat) = spec.default_chat
        && !global.llm.chat_models.contains_key(default_chat)
    {
        return Err(ProjectServiceError::Invalid(format!(
            "Unknown chat model '{default_chat}'"
        )));
    }
    if let Some(ref default_rerank) = spec.default_rerank
        && !global.llm.rerank_models.contains_key(default_rerank)
    {
        return Err(ProjectServiceError::Invalid(format!(
            "Unknown rerank model '{default_rerank}'"
        )));
    }

    let registry = state.engine.project_registry();
    let (mut profile, _) = registry
        .get_profile(project_id)
        .await
        .map_err(registry_error)?;
    let mut llm = profile.llm.take().unwrap_or_default();
    if spec.default_chat.is_some() {
        llm.chat_model = spec.default_chat;
    }
    if spec.default_rerank.is_some() {
        llm.rerank_model = spec.default_rerank;
    }
    if let Some(enabled) = spec.rerank_enabled {
        llm.enable_rerank = Some(enabled);
    }
    if let Some(max_candidates) = spec.rerank_max_candidates {
        llm.rerank_max_candidates = Some(max_candidates);
    }
    profile.llm = Some(llm);
    let version = registry
        .update_profile(project_id, profile)
        .await
        .map_err(registry_error)?;
    let hot_reload_applied = reload_project_after_profile_change(state, project_id).await;
    Ok(ProfileUpdateOutcome {
        config_version: version,
        hot_reload_applied,
    })
}

/// Service probed for LLM connectivity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProbeService {
    Embedding,
    Chat,
}

impl LlmProbeService {
    /// Parse a probe service label, case-insensitively.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "embedding" => Some(Self::Embedding),
            "chat" => Some(Self::Chat),
            _ => None,
        }
    }

    /// Canonical label of the service.
    pub fn label(self) -> &'static str {
        match self {
            Self::Embedding => "embedding",
            Self::Chat => "chat",
        }
    }
}

/// Result of an LLM connectivity probe.
#[derive(Debug, Clone)]
pub struct LlmProbeOutcome {
    pub success: bool,
    pub service: LlmProbeService,
    pub model_key: String,
    pub latency_ms: u64,
    pub dimension: Option<usize>,
    pub message: String,
}

/// Probe a configured model using the stored project configuration and discard
/// all transient client state afterwards.
pub async fn probe_project_llm(
    state: &SharedContext,
    project_id: i64,
    service: LlmProbeService,
    model_key: &str,
) -> Result<LlmProbeOutcome, ProjectServiceError> {
    let model_key = model_key.trim().to_string();
    if model_key.is_empty() {
        return Err(ProjectServiceError::Invalid(
            "model_key must not be empty".to_string(),
        ));
    }
    let project_config = state
        .engine
        .project_registry()
        .get_or_load(project_id)
        .await
        .map_err(registry_error)?
        .config
        .clone();
    match service {
        LlmProbeService::Embedding => {
            if !project_config.llm.embedding_models.contains_key(&model_key) {
                return Err(ProjectServiceError::Invalid(format!(
                    "Unknown embedding model '{model_key}'"
                )));
            }
            let probe =
                cce_llm_client::OpenAICompatibleProvider::from_model(&project_config, &model_key)
                    .map_err(|error| {
                    ProjectServiceError::Invalid(format!(
                        "Failed to resolve embedding model '{model_key}': {error}"
                    ))
                })?;
            let started = std::time::Instant::now();
            let (success, dimension, message) =
                match probe.embed_one("cce connectivity probe").await {
                    Ok(vector) => {
                        let dimension = vector.len();
                        (
                            true,
                            Some(dimension),
                            format!("Embedding probe succeeded with {dimension} dimensions"),
                        )
                    }
                    Err(error) => (false, None, format!("Embedding probe failed: {error}")),
                };
            Ok(LlmProbeOutcome {
                success,
                service,
                model_key,
                latency_ms: started.elapsed().as_millis() as u64,
                dimension,
                message,
            })
        }
        LlmProbeService::Chat => {
            if !project_config.llm.chat_models.contains_key(&model_key) {
                return Err(ProjectServiceError::Invalid(format!(
                    "Unknown chat model '{model_key}'"
                )));
            }
            let handle = cce_llm_client::factory::build_chat_client(&project_config, &model_key)
                .map_err(|error| {
                    ProjectServiceError::Invalid(format!(
                        "Failed to resolve chat model '{model_key}': {error}"
                    ))
                })?;
            let started = std::time::Instant::now();
            let messages = vec![cce_llm::Message::user("Reply with ok.")];
            let (success, message) = match handle.client.chat(&messages, &handle.config).await {
                Ok(result) => (
                    true,
                    format!("Chat probe succeeded ({} chars)", result.content.len()),
                ),
                Err(error) => (false, format!("Chat probe failed: {error}")),
            };
            Ok(LlmProbeOutcome {
                success,
                service,
                model_key,
                latency_ms: started.elapsed().as_millis() as u64,
                dimension: None,
                message,
            })
        }
    }
}
