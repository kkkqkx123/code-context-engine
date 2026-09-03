//! Shared default-value functions for serde deserialization across config modules.

/// Default serde default: `true`.
pub(crate) fn default_true() -> bool {
    true
}

/// Default timeout in seconds for network requests.
pub(crate) fn default_timeout() -> u64 {
    30
}

/// Default maximum retry count for embedder requests.
pub(crate) fn default_embedder_max_retries() -> u32 {
    3
}

/// Default maximum retry count for LLM model requests.
pub(crate) fn default_llm_max_retries() -> u32 {
    5
}

/// Default retry delay in milliseconds.
pub(crate) fn default_retry_delay() -> u64 {
    1000
}
