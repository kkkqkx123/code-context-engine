//! Shared API models for CCE CLI and Server

pub mod models;

/// Read the admission token from the environment.
///
/// Single source of truth for every client that talks to an
/// admission-enabled remote. The token travels as a bearer credential only
/// when configured, so local use sends exactly the same requests as before.
pub fn gateway_token_from_env() -> Option<String> {
    std::env::var("CCE_API_TOKEN")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}
