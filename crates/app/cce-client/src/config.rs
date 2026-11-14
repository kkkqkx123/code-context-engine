//! Server-wide configuration inspection and reload.

use cce_api::models::{
    ConfigInfoResponse, ConfigReloadQuery, ConfigReloadResponse, ConfigValidateResponse,
};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// Reload server configuration for one project or for every project.
    pub async fn config_reload(
        &self,
        query: &ConfigReloadQuery,
    ) -> Result<ConfigReloadResponse, ClientError> {
        self.post_json_with_query(
            paths::CONFIG_RELOAD.to_string(),
            query,
            self.long_task_budget(),
        )
        .await
    }

    /// Effective server configuration.
    pub async fn config_info(&self) -> Result<ConfigInfoResponse, ClientError> {
        self.get_json(paths::CONFIG_INFO.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Validate the on-disk configuration without applying it.
    pub async fn config_validate(&self) -> Result<ConfigValidateResponse, ClientError> {
        self.get_json(paths::CONFIG_VALIDATE.to_string(), INTERACTIVE_BUDGET)
            .await
    }
}
