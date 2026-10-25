//! Remote transmission for gateway pushes and CLI remote mode.
//!
//! This crate owns all network transport behavior: base URL handling, token
//! resolution, timeout setup, and the shared request client. The wire
//! contract stays in `cce-api` (models, paths, validation); this crate
//! encodes those models onto HTTP. Local execution never depends on this
//! crate.

pub mod client;
pub mod policy;
pub mod remote;

pub use client::{SharedHttpClient, build_client, normalize_base_url};
pub use policy::{
    GATEWAY_BATCH_WIRE_BYTES, should_flush_batch,
    total_chunks_for_size as total_chunks_for_size_with_bytes,
};
pub use remote::RemoteService;

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

/// Resolve the admission token with explicit arguments outranking the environment.
///
/// An explicit non-empty value wins; otherwise the environment is consulted.
/// Empty or whitespace-only explicit values fall back to the environment.
pub fn resolve_gateway_token(explicit: Option<String>) -> Option<String> {
    match explicit {
        Some(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                gateway_token_from_env()
            } else {
                Some(trimmed)
            }
        }
        None => gateway_token_from_env(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_token_is_trimmed_and_outranks_environment() {
        assert_eq!(
            resolve_gateway_token(Some("  explicit-token  ".to_string())),
            Some("explicit-token".to_string())
        );
    }
}
