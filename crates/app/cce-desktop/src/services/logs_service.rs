//! Log commands: filtered buffer pages, files and logger settings.
//!
//! The console log endpoints are part of the shared contract, so the panel
//! only wraps client methods here. No polling happens in this layer: the
//! refresh schedule decides when a panel replays its last input, keeping
//! timing in one place.

use cce_api::models::{LogConfig, LogFileInfo, LogsQuery, LogsResponse, UpdateLogConfigRequest};
use cce_client::{Client, ClientError};
use gpui_kit::{App, Task};

use crate::runtime;

/// Inputs the user can change without reconnecting.
///
/// Time bounds stay out of the panel: the ring buffer is recent history, and
/// a text range that excludes everything is indistinguishable from an empty
/// buffer. Level, module and message are the filters that actually select.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogsInput {
    pub level: Option<String>,
    pub module: Option<String>,
    pub query: Option<String>,
    pub page: usize,
    pub page_size: usize,
}

impl LogsInput {
    pub fn to_query(&self) -> LogsQuery {
        LogsQuery {
            level: self.level.clone(),
            module: self.module.clone(),
            query: self.query.clone(),
            start_time: None,
            end_time: None,
            page: Some(self.page.max(1)),
            page_size: Some(self.page_size.clamp(1, 500)),
        }
    }
}

pub fn entries(
    cx: &App,
    client: Client,
    input: LogsInput,
) -> Task<Result<LogsResponse, ClientError>> {
    let query = input.to_query();
    runtime::spawn(cx, async move { client.logs(&query).await })
}

pub fn files(cx: &App, client: Client) -> Task<Result<Vec<LogFileInfo>, ClientError>> {
    runtime::spawn(cx, async move { client.log_files().await })
}

pub fn config(cx: &App, client: Client) -> Task<Result<LogConfig, ClientError>> {
    runtime::spawn(cx, async move { client.log_config().await })
}

pub fn update_config(
    cx: &App,
    client: Client,
    request: UpdateLogConfigRequest,
) -> Task<Result<(), ClientError>> {
    runtime::spawn(cx, async move { client.update_log_config(&request).await })
}

pub fn rotate(cx: &App, client: Client) -> Task<Result<(), ClientError>> {
    runtime::spawn(cx, async move { client.rotate_logs().await })
}

pub fn delete_file(cx: &App, client: Client, name: String) -> Task<Result<(), ClientError>> {
    runtime::spawn(cx, async move { client.delete_log_file(&name).await })
}

/// Page number of the last page, or one when the server reports no rows.
pub fn last_page(response: &LogsResponse) -> usize {
    response.total.div_ceil(response.page_size.max(1)).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(total: usize, page_size: usize) -> LogsResponse {
        LogsResponse {
            entries: Vec::new(),
            total,
            page: 1,
            page_size,
        }
    }

    #[test]
    fn last_page_uses_total_not_the_current_page() {
        assert_eq!(last_page(&response(0, 50)), 1);
        assert_eq!(last_page(&response(50, 50)), 1);
        assert_eq!(last_page(&response(51, 50)), 2);
    }

    #[test]
    fn input_normalises_page_and_size() {
        let input = LogsInput {
            page: 0,
            page_size: 0,
            ..LogsInput::default()
        };
        let query = input.to_query();
        assert_eq!(query.page, Some(1));
        assert_eq!(query.page_size, Some(1));
    }
}
