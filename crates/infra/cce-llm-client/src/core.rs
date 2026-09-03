//! LLM Core Module
//!
//! Provides the foundational HTTP client infrastructure for LLM APIs.

pub mod client;
pub mod config;
pub mod error;

pub(crate) mod http_service;
pub(crate) mod rate_limiter;
pub(crate) mod retry;

pub use cce_circuit_breaker::{CircuitBreaker, CircuitBreakerRejected};
pub use client::{HttpLlmClient, HttpLlmClientBuilder};
pub use config::{ChatConfig, EmbeddingConfig, LlmConfig, ProviderType, ResponseFormat};
pub use error::{LlmConfigError, LlmError};
pub use http_service::{HttpRequestConfig, HttpRequestService};
pub use rate_limiter::{ConfigurableRateLimiter, RateLimiter, TokenBucket};
pub use retry::{FixedIntervalPolicy, NoRetry, RetryPolicy};
