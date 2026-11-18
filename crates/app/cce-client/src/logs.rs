//! Console log buffer, log files and logger settings.
//!
//! These endpoints used to be console-only and were reached with hand-built
//! URLs from the web client. They are contract operations now, so the desktop
//! client and the web console read one shape.
//!
//! The WebSocket stream stays out of scope: its frames are not HTTP payloads,
//! and a push transport does not fit a one-request-per-call client.

use cce_api::models::{LogConfig, LogFileInfo, LogsQuery, LogsResponse, UpdateLogConfigRequest};
use cce_api::paths;

use crate::error::ClientError;
use crate::transport::{Client, INTERACTIVE_BUDGET};

impl Client {
    /// One page of the in-memory log buffer.
    ///
    /// Filtering happens server side over the ring buffer, so paging applies
    /// to the filtered set.
    pub async fn logs(&self, query: &LogsQuery) -> Result<LogsResponse, ClientError> {
        self.get_json_query(paths::LOGS.to_string(), query, INTERACTIVE_BUDGET)
            .await
    }

    /// Files present in the configured log directory.
    pub async fn log_files(&self) -> Result<Vec<LogFileInfo>, ClientError> {
        self.get_json(paths::LOGS_FILES.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Absolute URL of one log file download.
    ///
    /// The payload is an opaque attachment, so the path is handed out rather
    /// than fetched: callers stream it with whatever they already use for
    /// files instead of buffering it in memory here.
    pub fn log_file_download_url(&self, name: &str) -> String {
        let path = paths::LOGS_FILE_DOWNLOAD_TEMPLATE.replace("{name}", &urlencoding::encode(name));
        format!("{}{}", self.base_url(), path)
    }

    /// Removes one log file. The active file is refused by the server.
    pub async fn delete_log_file(&self, name: &str) -> Result<(), ClientError> {
        let path = paths::LOGS_FILE_TEMPLATE.replace("{name}", &urlencoding::encode(name));
        self.delete_no_content(path, INTERACTIVE_BUDGET).await
    }

    /// Applies the log retention policy now.
    pub async fn rotate_logs(&self) -> Result<(), ClientError> {
        self.post_no_content(paths::LOGS_ROTATE.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Effective logger configuration of the running process.
    pub async fn log_config(&self) -> Result<LogConfig, ClientError> {
        self.get_json(paths::LOGS_CONFIG.to_string(), INTERACTIVE_BUDGET)
            .await
    }

    /// Changes the level, format or filter of the running logger.
    pub async fn update_log_config(
        &self,
        request: &UpdateLogConfigRequest,
    ) -> Result<(), ClientError> {
        self.put_json_no_content(paths::LOGS_CONFIG.to_string(), request, INTERACTIVE_BUDGET)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_url_encodes_the_file_name() {
        let client = Client::with_config(
            crate::transport::ClientConfig::new("http://127.0.0.1:1").with_token(None),
        )
        .expect("client builds");
        let url = client.log_file_download_url("cce server.log");
        assert_eq!(
            url,
            "http://127.0.0.1:1/api/logs/files/cce%20server.log/download"
        );
    }
}
