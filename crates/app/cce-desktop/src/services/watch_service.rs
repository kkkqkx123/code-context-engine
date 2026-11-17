//! Directory watch commands.
//!
//! Watching is server-side: the panel triggers start and stop and reads the
//! status snapshot, but never observes the filesystem itself.

use cce_api::models::{
    StartWatchRequest, StartWatchResponse, StopWatchResponse, WatchStatusResponse,
};
use cce_client::{Client, ClientError};
use gpui_kit::{App, Task};

use crate::runtime;

/// Debounce the panel offers, in milliseconds.
pub const DEFAULT_DEBOUNCE_MS: u64 = 500;

pub fn status(
    cx: &App,
    client: Client,
    project_id: i64,
) -> Task<Result<WatchStatusResponse, ClientError>> {
    runtime::spawn(cx, async move { client.watch_status(project_id).await })
}

pub fn start(
    cx: &App,
    client: Client,
    project_id: i64,
    path: String,
    extensions: Vec<String>,
) -> Task<Result<StartWatchResponse, ClientError>> {
    runtime::spawn(cx, async move {
        let request = StartWatchRequest {
            path,
            extensions,
            debounce_ms: DEFAULT_DEBOUNCE_MS,
        };
        client.watch_start(project_id, &request).await
    })
}

pub fn stop(
    cx: &App,
    client: Client,
    project_id: i64,
) -> Task<Result<StopWatchResponse, ClientError>> {
    runtime::spawn(cx, async move { client.watch_stop(project_id).await })
}
