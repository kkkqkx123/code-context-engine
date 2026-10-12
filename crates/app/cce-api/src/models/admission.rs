//! Admission observability wire models.
//!
//! The server renders these shapes from its admission middleware counters and
//! the durable audit repository. The gateway never produces them; clients use
//! them to inspect rejection causes and per-token quotas.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Global admission counters for the current process.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct AdmissionGlobalStats {
    /// Requests admitted past the admission layer.
    pub admitted: u64,
    /// Requests rejected for missing or unknown tokens.
    pub auth_rejections: u64,
    /// Requests rejected for project scope mismatch.
    pub scope_rejections: u64,
    /// Requests rejected by per-token rate limiting.
    pub rate_rejections: u64,
    /// Requests rejected for oversized bodies.
    pub body_rejections: u64,
    /// Requests rejected for exceeding stored byte quotas.
    #[serde(default)]
    pub quota_rejections: u64,
}

/// Per-token audit entry for persistent admission observability.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenAuditEntry {
    /// Log-safe token fingerprint.
    pub fingerprint: String,
    /// Bound projects as comma separated ids.
    pub projects: String,
    /// Configured byte quota, if any.
    pub quota_bytes: Option<i64>,
    /// Stored bytes accepted.
    pub bytes_used: i64,
    /// Admitted batches.
    pub admitted: i64,
    /// Rejections by cause.
    pub auth_rejections: i64,
    /// Rejections by cause.
    pub scope_rejections: i64,
    /// Rejections by cause.
    pub rate_rejections: i64,
    /// Rejections by cause.
    pub body_rejections: i64,
    /// Rejections for exceeding the byte quota.
    pub quota_rejections: i64,
    /// Last use as seconds since the unix epoch, if any.
    pub last_used: Option<i64>,
    /// Last rejection reason, if any.
    pub last_reject_reason: Option<String>,
}

/// Admission counters with per-token durable audit.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct AdmissionStatsResponse {
    /// Global in-memory counters.
    #[serde(flatten)]
    pub global: AdmissionGlobalStats,
    /// Per-token durable audit rows.
    #[serde(default)]
    pub tokens: Vec<TokenAuditEntry>,
}
