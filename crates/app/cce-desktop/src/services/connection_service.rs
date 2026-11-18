//! Connection probing against the CCE server.
//!
//! The desktop client performs one probe per call; retries, backoff and
//! scheduling stay with the caller. The heartbeat loop only flips the
//! connection snapshot between connected and unhealthy.

use std::time::Duration;

use cce_api::models::HealthStatus;
use cce_client::{Client, ClientError};
use gpui_kit::{App, AsyncApp, SharedString, Task};

use crate::connection::ClientStore;
use crate::runtime;
use crate::state::{AppState, ConnectionStatus, HealthSummary};

/// How often the background loop probes the server while connected.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Build a client for `base_url` and prove it with one health probe. The
/// token never leaves this call.
pub fn probe(
    cx: &App,
    base_url: &str,
    token: &str,
) -> Task<Result<(Client, HealthStatus), ClientError>> {
    let base_url = base_url.to_string();
    let token = token.to_string();
    runtime::spawn(cx, async move {
        let client = ClientStore::build(&base_url, &token)?;
        let health = client.health().await?;
        Ok((client, health))
    })
}

/// Render a probe result as the status bar and health view snapshot.
pub fn summarize(health: &HealthStatus) -> HealthSummary {
    let mut down = Vec::new();
    if !health.qdrant.reachable {
        down.push("qdrant");
    }
    if !health.bm25.reachable {
        down.push("bm25");
    }
    if !health.embedding.reachable {
        down.push("embedding");
    }
    let detail = if down.is_empty() {
        SharedString::from("all services reachable")
    } else {
        SharedString::from(format!("unreachable: {}", down.join(", ")))
    };
    HealthSummary {
        healthy: health.healthy && down.is_empty(),
        detail,
    }
}

/// Drop the stored client and return to the connection form. The next
/// connection rebuilds the client from fresh input.
pub fn disconnect(cx: &mut App) {
    ClientStore::clear(cx);
    crate::components::toast::clear(cx);
    AppState::global(cx).update(cx, |state, cx| state.set_disconnected(cx));
}

/// Spawn the one-per-app heartbeat loop: while the UI believes a connection
/// is live, probe the server and flip the status snapshot on failure.
pub fn start_heartbeat(cx: &mut App) -> Task<()> {
    cx.spawn(async move |cx: &mut AsyncApp| {
        loop {
            cx.background_executor().timer(HEARTBEAT_INTERVAL).await;

            let task = cx.update(|cx| {
                let connected = AppState::global(cx).read(cx).status == ConnectionStatus::Connected;
                if !connected {
                    return None;
                }
                ClientStore::client(cx).map(|client| {
                    runtime::spawn(cx, async move {
                        client.health().await.map(|health| health.healthy)
                    })
                })
            });
            let Some(task) = task else {
                continue;
            };
            let healthy = task.await.unwrap_or(false);
            cx.update(|cx| {
                AppState::global(cx).update(cx, |state, cx| state.set_unhealthy(!healthy, cx));
            });
        }
    })
}
