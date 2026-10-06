//! Admission counters for rejection auditing.
//!
//! The counters are lock-free so the middleware never blocks on observability.
//! A snapshot renders them for the admission statistics endpoint.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

/// Lock-free admission counters.
#[derive(Debug, Default)]
pub struct AdmissionMetrics {
    admitted: AtomicU64,
    auth_rejections: AtomicU64,
    scope_rejections: AtomicU64,
    rate_rejections: AtomicU64,
    body_rejections: AtomicU64,
}

impl AdmissionMetrics {
    /// Record one admitted request.
    pub fn record_admitted(&self) {
        self.admitted.fetch_add(1, Ordering::Relaxed);
    }

    /// Record one authentication rejection.
    pub fn record_auth_rejection(&self) {
        self.auth_rejections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record one token scope rejection.
    pub fn record_scope_rejection(&self) {
        self.scope_rejections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record one rate limit rejection.
    pub fn record_rate_rejection(&self) {
        self.rate_rejections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record one oversized body rejection.
    pub fn record_body_rejection(&self) {
        self.body_rejections.fetch_add(1, Ordering::Relaxed);
    }

    /// Render the current counters.
    pub fn snapshot(&self) -> AdmissionStats {
        AdmissionStats {
            admitted: self.admitted.load(Ordering::Relaxed),
            auth_rejections: self.auth_rejections.load(Ordering::Relaxed),
            scope_rejections: self.scope_rejections.load(Ordering::Relaxed),
            rate_rejections: self.rate_rejections.load(Ordering::Relaxed),
            body_rejections: self.body_rejections.load(Ordering::Relaxed),
        }
    }
}

/// Point-in-time admission counters.
#[derive(Debug, Clone, Serialize)]
pub struct AdmissionStats {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_accumulate_and_snapshot() {
        let metrics = AdmissionMetrics::default();
        metrics.record_admitted();
        metrics.record_auth_rejection();
        metrics.record_scope_rejection();
        let stats = metrics.snapshot();
        assert_eq!(stats.admitted, 1);
        assert_eq!(stats.auth_rejections, 1);
        assert_eq!(stats.scope_rejections, 1);
        assert_eq!(stats.rate_rejections, 0);
    }
}
