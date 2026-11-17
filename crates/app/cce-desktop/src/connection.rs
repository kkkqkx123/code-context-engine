//! Process-wide holder of the typed API client.
//!
//! The [`Client`] is cheap to clone (it shares one connection pool), so the
//! store keeps a single instance built from the current base URL and token.
//! UI state mirrors only snapshots in [`crate::state::AppState`]; the token
//! itself never leaves this module and is never persisted.

use cce_client::{Client, ClientError};
use gpui_kit::{App, Global, Task};

/// Server base URL used when no previous connection was persisted.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:9000";

pub struct ClientStore {
    client: Option<Client>,
    /// Keeps the background health-check loop alive for the whole app run.
    _heartbeat: Option<Task<()>>,
}

impl Global for ClientStore {}

impl ClientStore {
    pub fn init(cx: &mut App) {
        cx.set_global(Self {
            client: None,
            _heartbeat: None,
        });
    }

    /// Build a client for `base_url` without touching the stored instance.
    /// An empty token falls back to the `CCE_API_TOKEN` environment variable
    /// inside the client constructor.
    pub fn build(base_url: &str, token: &str) -> Result<Client, ClientError> {
        let token = token.trim();
        let token = if token.is_empty() {
            None
        } else {
            Some(token.to_string())
        };
        Client::with_token(base_url, token)
    }

    /// Replace the stored client after a successful probe.
    pub fn replace(client: Client, cx: &mut App) {
        cx.global_mut::<Self>().client = Some(client);
    }

    pub fn client(cx: &App) -> Option<Client> {
        cx.global::<Self>().client.clone()
    }

    /// Forget the stored client. The token it carried is dropped with it and
    /// was never persisted, so nothing needs scrubbing from disk.
    pub fn clear(cx: &mut App) {
        cx.global_mut::<Self>().client = None;
    }

    pub fn set_heartbeat(&mut self, task: Task<()>) {
        self._heartbeat = Some(task);
    }
}
