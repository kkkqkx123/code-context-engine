use std::time::Duration;

use cce_config::Validate;
use cce_config::modules::RelationRemoteConfig;
use cce_types::StorageError;
use tokio_postgres::Config as PgConfig;

#[derive(Debug, Clone)]
pub struct PostgresConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub pool_size: u32,
    pub connect_timeout: Duration,
    pub acquire_timeout: Duration,
    pub statement_timeout: Duration,
}

impl PostgresConfig {
    pub fn from_remote(remote: &RelationRemoteConfig) -> Result<Self, StorageError> {
        remote.validate_structured().map_err(|error| {
            StorageError::validation(format!("invalid relation remote config: {error}"))
        })?;
        let url = remote
            .url
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                StorageError::validation(
                    "database.relation_remote.url must be set for the remote branch",
                )
            })?;
        Ok(Self {
            url: url.to_string(),
            username: remote.effective_username().map(str::to_string),
            password: remote.password.clone(),
            pool_size: remote.pool_size,
            connect_timeout: Duration::from_millis(remote.connect_timeout_ms),
            acquire_timeout: Duration::from_millis(remote.acquire_timeout_ms),
            statement_timeout: Duration::from_millis(remote.statement_timeout_ms),
        })
    }

    pub fn pg_config(&self) -> Result<PgConfig, StorageError> {
        let mut config: PgConfig = self.url.parse().map_err(|error| {
            StorageError::validation(format!("invalid relation remote url: {error}"))
        })?;
        if let Some(ref username) = self.username {
            config.user(username);
        }
        if let Some(ref password) = self.password {
            config.password(password);
        }
        config.connect_timeout(self.connect_timeout);
        if !self.statement_timeout.is_zero() {
            config.options(format!(
                "-c statement_timeout={}",
                self.statement_timeout.as_millis()
            ));
        }
        Ok(config)
    }
}
