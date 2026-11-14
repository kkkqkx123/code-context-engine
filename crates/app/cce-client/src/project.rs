//! Project lifecycle, configuration and LLM settings.

use cce_api::models::{
    CreateProjectRequest, LlmTestRequest, LlmTestResponse, ProjectConfigGetResponse,
    ProjectConfigReloadResponse, ProjectConfigUpdateRequest, ProjectConfigUpdateResponse,
    ProjectDeleteResponse, ProjectDetailResponse, ProjectIndexResponse, ProjectListResponse,
    ProjectLlmOverviewResponse, ProjectLlmUpdateRequest, ProjectLlmUpdateResponse,
    UpdateProjectRequest,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// List every project.
    pub async fn list_projects(&self) -> Result<ProjectListResponse, ClientError> {
        self.get_json(paths::PROJECT.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Create a project and return its details.
    pub async fn create_project(
        &self,
        request: &CreateProjectRequest,
    ) -> Result<ProjectDetailResponse, ClientError> {
        self.post_json(paths::PROJECT.to_string(), request, INTERACTIVE_BUDGET)
            .await
    }

    /// Fetch one project's details.
    pub async fn get_project(&self, project_id: i64) -> Result<ProjectDetailResponse, ClientError> {
        self.get_json(
            paths::project_scoped(paths::PROJECT_DETAIL_TEMPLATE, project_id),
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Rename or reconfigure a project.
    pub async fn update_project(
        &self,
        project_id: i64,
        request: &UpdateProjectRequest,
    ) -> Result<ProjectDetailResponse, ClientError> {
        self.put_json(
            paths::project_scoped(paths::PROJECT_DETAIL_TEMPLATE, project_id),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Delete a project and everything indexed for it.
    pub async fn delete_project(
        &self,
        project_id: i64,
    ) -> Result<ProjectDeleteResponse, ClientError> {
        self.delete_json(
            paths::project_scoped(paths::PROJECT_DETAIL_TEMPLATE, project_id),
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Run a full index pass over the project's configured root path.
    pub async fn index_project(
        &self,
        project_id: i64,
    ) -> Result<ProjectIndexResponse, ClientError> {
        self.post_json_empty(
            paths::project_scoped(paths::PROJECT_INDEX_TEMPLATE, project_id),
            self.long_task_budget(),
        )
        .await
    }

    /// Read the stored per-project configuration document.
    pub async fn project_config(
        &self,
        project_id: i64,
    ) -> Result<ProjectConfigGetResponse, ClientError> {
        self.get_json(
            paths::project_scoped(paths::PROJECT_CONFIG_TEMPLATE, project_id),
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Replace the per-project configuration document.
    pub async fn update_project_config(
        &self,
        project_id: i64,
        request: &ProjectConfigUpdateRequest,
    ) -> Result<ProjectConfigUpdateResponse, ClientError> {
        self.put_json(
            paths::project_scoped(paths::PROJECT_CONFIG_TEMPLATE, project_id),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Reload the project configuration from disk.
    pub async fn reload_project(
        &self,
        project_id: i64,
    ) -> Result<ProjectConfigReloadResponse, ClientError> {
        self.post_json_empty(
            paths::project_scoped(paths::PROJECT_RELOAD_TEMPLATE, project_id),
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Read the effective LLM settings of a project.
    pub async fn project_llm_overview(
        &self,
        project_id: i64,
    ) -> Result<ProjectLlmOverviewResponse, ClientError> {
        self.get_json(
            paths::project_scoped(paths::PROJECT_LLM_OVERVIEW_TEMPLATE, project_id),
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Patch the LLM settings of a project.
    pub async fn update_project_llm_config(
        &self,
        project_id: i64,
        request: &ProjectLlmUpdateRequest,
    ) -> Result<ProjectLlmUpdateResponse, ClientError> {
        self.put_json(
            paths::project_scoped(paths::PROJECT_LLM_CONFIG_TEMPLATE, project_id),
            request,
            INTERACTIVE_BUDGET,
        )
        .await
    }

    /// Probe one LLM provider with a test prompt.
    pub async fn test_project_llm(
        &self,
        project_id: i64,
        request: &LlmTestRequest,
    ) -> Result<LlmTestResponse, ClientError> {
        self.post_json(
            paths::project_scoped(paths::PROJECT_LLM_TEST_TEMPLATE, project_id),
            request,
            self.long_task_budget(),
        )
        .await
    }
}
