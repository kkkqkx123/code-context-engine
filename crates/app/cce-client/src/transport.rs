//! Request pipeline shared by every operation module.
//!
//! The pipeline owns everything that is identical for all calls: base URL
//! joining, bearer credentials, per-call timeout, response decoding and error
//! classification. Operation modules never build URLs or inspect statuses.

use std::fmt;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{ApiMethod, ClientError};

/// Environment variable holding the admission token.
pub const TOKEN_ENV: &str = "CCE_API_TOKEN";

/// Ceiling for one interactive call: a request/response round trip whose work
/// the caller waits for and that is expected to finish quickly.
pub const INTERACTIVE_BUDGET: Duration = Duration::from_secs(300);

/// Ceiling for one long-running call: full scans, relation rebuilds, LLM
/// batches and process lifecycle actions.
pub const LONG_TASK_BUDGET: Duration = Duration::from_secs(1800);

/// Construction settings of a [`Client`].
///
/// Contains no cloneable diagnostics: the token is resolved once here and is
/// never part of `Debug` output.
pub struct ClientConfig {
    base_url: String,
    token: Option<String>,
    long_task_budget: Duration,
}

impl ClientConfig {
    /// Configure a client for the given server base URL.
    pub fn new(base_url: impl AsRef<str>) -> Self {
        Self {
            base_url: base_url.as_ref().to_string(),
            token: None,
            long_task_budget: LONG_TASK_BUDGET,
        }
    }

    /// Set an explicit token, outranking the environment when non-empty.
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    /// Override the ceiling applied to long-running operations.
    pub fn with_long_task_budget(mut self, budget: Duration) -> Self {
        self.long_task_budget = budget;
        self
    }
}

/// HTTP client for the CCE server.
///
/// Cloning yields another handle onto the same connection pool, so one client
/// can be shared by concurrent callers.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: Option<String>,
    long_task_budget: Duration,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("base_url", &self.base_url)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("long_task_budget", &self.long_task_budget)
            .finish()
    }
}

impl Client {
    /// Configure the client from the environment only.
    pub fn new(base_url: impl AsRef<str>) -> Result<Self, ClientError> {
        Self::with_config(ClientConfig::new(base_url))
    }

    /// Configure the client with an explicit token outranking the environment.
    pub fn with_token(
        base_url: impl AsRef<str>,
        token: Option<String>,
    ) -> Result<Self, ClientError> {
        Self::with_config(ClientConfig::new(base_url).with_token(token))
    }

    /// Build the client from explicit settings.
    pub fn with_config(config: ClientConfig) -> Result<Self, ClientError> {
        let url = reqwest::Url::parse(&config.base_url).map_err(|source| ClientError::Config {
            detail: format!(
                "base URL {:?} is not a valid URL: {source}",
                config.base_url
            ),
        })?;
        let http = reqwest::Client::builder()
            // Fixed identity for server-side diagnostics; never carries semantics.
            .user_agent("cce-client")
            .build()
            .map_err(|source| ClientError::Config {
                detail: format!("HTTP stack could not be initialized: {source}"),
            })?;
        Ok(Self {
            http,
            base_url: url.as_str().trim_end_matches('/').to_string(),
            token: resolve_token(config.token),
            long_task_budget: config.long_task_budget,
        })
    }

    /// Base URL without trailing slashes.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Ceiling applied to long-running operations.
    pub fn long_task_budget(&self) -> Duration {
        self.long_task_budget
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn prepare(
        &self,
        method: reqwest::Method,
        path: &str,
        budget: Duration,
    ) -> reqwest::RequestBuilder {
        let builder = self.http.request(method, self.url(path)).timeout(budget);
        match &self.token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }

    async fn send(
        &self,
        method: ApiMethod,
        path: &str,
        budget: Duration,
        request: reqwest::RequestBuilder,
    ) -> Result<String, ClientError> {
        let response = request
            .send()
            .await
            .map_err(|source| transport_failure(method, path, budget, source))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|source| transport_failure(method, path, budget, source))?;
        if (200..300).contains(&status) {
            Ok(body)
        } else {
            Err(ClientError::from_failure(method, path, status, &body))
        }
    }

    fn get(&self, path: String, budget: Duration) -> reqwest::RequestBuilder {
        self.prepare(reqwest::Method::GET, &path, budget)
    }

    pub(crate) async fn get_json<T: DeserializeOwned>(
        &self,
        path: String,
        budget: Duration,
    ) -> Result<T, ClientError> {
        let body = self
            .send(
                ApiMethod::Get,
                &path,
                budget,
                self.get(path.clone(), budget),
            )
            .await?;
        decode(ApiMethod::Get, &path, &body)
    }

    pub(crate) async fn get_json_query<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: String,
        query: &Q,
        budget: Duration,
    ) -> Result<T, ClientError> {
        let request = self.get(path.clone(), budget).query(query);
        let body = self.send(ApiMethod::Get, &path, budget, request).await?;
        decode(ApiMethod::Get, &path, &body)
    }

    pub(crate) async fn get_text(
        &self,
        path: String,
        budget: Duration,
    ) -> Result<String, ClientError> {
        self.send(
            ApiMethod::Get,
            &path,
            budget,
            self.get(path.clone(), budget),
        )
        .await
    }

    pub(crate) async fn post_json<T: DeserializeOwned, B: Serialize>(
        &self,
        path: String,
        body: &B,
        budget: Duration,
    ) -> Result<T, ClientError> {
        let request = self
            .prepare(reqwest::Method::POST, &path, budget)
            .json(body);
        let body = self.send(ApiMethod::Post, &path, budget, request).await?;
        decode(ApiMethod::Post, &path, &body)
    }

    pub(crate) async fn post_json_empty<T: DeserializeOwned>(
        &self,
        path: String,
        budget: Duration,
    ) -> Result<T, ClientError> {
        let request = self.prepare(reqwest::Method::POST, &path, budget);
        let body = self.send(ApiMethod::Post, &path, budget, request).await?;
        decode(ApiMethod::Post, &path, &body)
    }

    pub(crate) async fn post_json_with_query<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: String,
        query: &Q,
        budget: Duration,
    ) -> Result<T, ClientError> {
        let request = self
            .prepare(reqwest::Method::POST, &path, budget)
            .query(query);
        let body = self.send(ApiMethod::Post, &path, budget, request).await?;
        decode(ApiMethod::Post, &path, &body)
    }

    pub(crate) async fn put_json<T: DeserializeOwned, B: Serialize>(
        &self,
        path: String,
        body: &B,
        budget: Duration,
    ) -> Result<T, ClientError> {
        let request = self.prepare(reqwest::Method::PUT, &path, budget).json(body);
        let body = self.send(ApiMethod::Put, &path, budget, request).await?;
        decode(ApiMethod::Put, &path, &body)
    }

    pub(crate) async fn delete_json<T: DeserializeOwned>(
        &self,
        path: String,
        budget: Duration,
    ) -> Result<T, ClientError> {
        let request = self.prepare(reqwest::Method::DELETE, &path, budget);
        let body = self.send(ApiMethod::Delete, &path, budget, request).await?;
        decode(ApiMethod::Delete, &path, &body)
    }

    pub(crate) async fn delete_json_body<T: DeserializeOwned, B: Serialize>(
        &self,
        path: String,
        body: &B,
        budget: Duration,
    ) -> Result<T, ClientError> {
        let request = self
            .prepare(reqwest::Method::DELETE, &path, budget)
            .json(body);
        let body = self.send(ApiMethod::Delete, &path, budget, request).await?;
        decode(ApiMethod::Delete, &path, &body)
    }

    pub(crate) async fn delete_json_with_query<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: String,
        query: &Q,
        budget: Duration,
    ) -> Result<T, ClientError> {
        let request = self
            .prepare(reqwest::Method::DELETE, &path, budget)
            .query(query);
        let body = self.send(ApiMethod::Delete, &path, budget, request).await?;
        decode(ApiMethod::Delete, &path, &body)
    }

    pub(crate) async fn delete_json_body_with_query<
        T: DeserializeOwned,
        B: Serialize,
        Q: Serialize,
    >(
        &self,
        path: String,
        query: &Q,
        body: &B,
        budget: Duration,
    ) -> Result<T, ClientError> {
        let request = self
            .prepare(reqwest::Method::DELETE, &path, budget)
            .query(query)
            .json(body);
        let body = self.send(ApiMethod::Delete, &path, budget, request).await?;
        decode(ApiMethod::Delete, &path, &body)
    }
}

/// Resolve the explicit token against the environment.
fn resolve_token(explicit: Option<String>) -> Option<String> {
    explicit
        .and_then(|value| non_empty(value.trim()))
        .or_else(|| {
            std::env::var(TOKEN_ENV)
                .ok()
                .and_then(|v| non_empty(v.trim()))
        })
}

fn non_empty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Build a project scoped path whose trailing `{id}` placeholder carries one
/// percent-encoded identifier.
pub(crate) fn id_scoped(template: &str, project_id: i64, id: &str) -> String {
    cce_api::paths::project_scoped(template, project_id)
        .replace(cce_api::paths::ID_PLACEHOLDER, &urlencoding::encode(id))
}

/// Build a path whose `{path}` placeholder carries one percent-encoded file
/// path.
pub(crate) fn file_scoped(template: &str, file_path: &str) -> String {
    template.replace(
        cce_api::paths::FILE_PATH_PLACEHOLDER,
        &urlencoding::encode(file_path),
    )
}

/// Turn a transport failure into the unavailable class.
///
/// No response means nothing was served, so it stays retryable regardless of
/// the underlying cause; timeouts name the budget that expired.
fn transport_failure(
    method: ApiMethod,
    path: &str,
    budget: Duration,
    source: reqwest::Error,
) -> ClientError {
    ClientError::Unavailable {
        method,
        path: path.to_string(),
        status: None,
        detail: if source.is_timeout() {
            format!("no response within {}s", budget.as_secs())
        } else {
            source.to_string()
        },
    }
}

fn decode<T: DeserializeOwned>(
    method: ApiMethod,
    path: &str,
    body: &str,
) -> Result<T, ClientError> {
    serde_json::from_str(body).map_err(|source| ClientError::Protocol {
        method,
        path: path.to_string(),
        detail: source.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_drops_trailing_slashes() {
        let client = Client::new("http://127.0.0.1:9000///").expect("client builds");
        assert_eq!(client.base_url(), "http://127.0.0.1:9000");
        assert_eq!(
            client.url("/api/health"),
            "http://127.0.0.1:9000/api/health"
        );
    }

    #[test]
    fn invalid_base_url_is_config_error() {
        let error = Client::new("not a url").expect_err("invalid url rejected");
        assert!(matches!(error, ClientError::Config { .. }));
    }

    #[test]
    fn explicit_token_outranks_environment() {
        let explicit = Client::with_token("http://localhost:1", Some("  abc  ".to_string()))
            .expect("client builds");
        assert_eq!(explicit.token.as_deref(), Some("abc"));
    }

    #[test]
    fn blank_explicit_token_is_ignored() {
        let client =
            Client::with_token("http://localhost:1", Some("   ".to_string())).expect("client");
        assert_eq!(client.token, resolve_token(None));
    }

    #[test]
    fn debug_never_prints_the_token() {
        let client = Client::with_token("http://localhost:1", Some("secret-value".to_string()))
            .expect("client builds");
        let rendered = format!("{client:?}");
        assert!(
            !rendered.contains("secret-value"),
            "token leaked: {rendered}"
        );
    }

    #[test]
    fn budgets_differ_between_call_classes() {
        assert!(INTERACTIVE_BUDGET < LONG_TASK_BUDGET);
        assert_eq!(
            Client::new("http://localhost:1")
                .expect("client builds")
                .long_task_budget(),
            LONG_TASK_BUDGET
        );
    }

    #[test]
    fn client_shares_across_tasks_and_apps() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Client>();
        assert_send_sync::<ClientConfig>();
    }

    #[test]
    fn protocol_error_names_the_path() {
        let error = decode::<cce_api::models::HealthStatus>(ApiMethod::Get, "/api/health", "{}")
            .expect_err("an empty object is not a health status");
        assert!(matches!(error, ClientError::Protocol { .. }));
        assert_eq!(error.path(), "/api/health");
    }
}
