//! Server-enforced, run-scoped call budget for the read-only MCP tool surface.
//!
//! This module is deliberately transport-agnostic: it has no `rmcp` dependency
//! so the same accounting core can be reused by the HTTP surface and tested
//! without any protocol machinery. The MCP layer only parses `_meta` into a
//! [`WikiRunContext`], calls into this core before executing a tool, and
//! writes the returned [`BudgetState`] back into the tool payload.
//!
//! Three limits are enforced, in order of increasing scope:
//! - per-run total billable calls,
//! - per-page billable calls within a run,
//! - per-call item/line caps (applied by the tool itself; the budget core owns
//!   the numeric limits so they stay in one table).
//!
//! Repeated deterministic reads within one run and one snapshot epoch reuse a
//! cached payload and do not consume a step. Failed executions do not consume
//! a billable step either, but count towards a separate attempt budget so a
//! client cannot spin on failing calls.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use moka::future::Cache;
use serde::{Deserialize, Serialize};

/// Maximum length of a `run_id` or `page_id` accepted from `_meta`.
const MAX_ID_LEN: usize = 128;

/// Identifies one downstream generation run and, optionally, one page of it.
///
/// Parsed from `tools/call` `_meta.wiki_run` by the MCP layer; the budget core
/// never sees the wire representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WikiRunContext {
    /// Stable, client-generated identifier for the whole generation run.
    pub run_id: String,
    /// Optional page within the run; budgets are tracked per page as well.
    pub page_id: Option<String>,
    /// Optional relation-snapshot epoch pinned by the downstream generator.
    pub snapshot_epoch: Option<u64>,
}

impl WikiRunContext {
    /// Validate identifier shape so a malicious client cannot grow memory with
    /// oversized ids. Returns a human-readable reason on rejection.
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_id(&self.run_id)?;
        if let Some(page) = &self.page_id
            && let Err(reason) = validate_id(page)
        {
            return Err(reason);
        }
        Ok(())
    }
}

/// Atomically decrement by one without wrapping below zero.
fn saturating_sub(counter: &AtomicU64) {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        if current == 0 {
            return;
        }
        match counter.compare_exchange_weak(
            current,
            current - 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

/// Ids are short, non-empty, and limited to a conservative ASCII whitelist.
fn validate_id(value: &str) -> Result<(), &'static str> {
    if value.is_empty() || value.len() > MAX_ID_LEN {
        return Err("id must be between 1 and 128 characters");
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.' || b == b':')
    {
        return Err("id contains characters outside [A-Za-z0-9_.:-]");
    }
    Ok(())
}

/// Runtime limits copied out of configuration at server start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetLimits {
    pub max_calls_per_run: usize,
    pub max_calls_per_page: usize,
    pub max_active_runs: usize,
}

/// Reason a call was refused before reaching the tool implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetExhaustion {
    /// The run consumed all of its billable calls.
    Run,
    /// The current page consumed all of its billable calls.
    Page,
    /// Too many failed calls on this page (attempt budget).
    Attempts,
    /// The active-run ceiling was reached; reject instead of growing memory.
    ActiveRuns,
}

/// Error payload returned when a call is refused by the budget core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BudgetExhausted {
    pub scope: BudgetExhaustion,
    pub remaining: usize,
    pub fallback: &'static str,
}

impl BudgetExhausted {
    fn new(scope: BudgetExhaustion) -> Self {
        Self {
            scope,
            remaining: 0,
            fallback: "wcp_only",
        }
    }
}

/// Budget state handed back to the MCP layer for `_budget` injection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BudgetState {
    pub run_id: String,
    pub calls_used: usize,
    pub calls_remaining: usize,
    pub page_calls_used: usize,
    pub page_calls_remaining: usize,
    pub snapshot_epoch: Option<u64>,
    /// True when the result was served from the run-local cache.
    pub cached: bool,
}

/// Result of the pre-execution budget check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetDecision {
    /// The call may proceed; one step is already reserved.
    Allowed { state: BudgetState },
    /// The call is refused; no tool work should be performed.
    Exhausted(BudgetExhausted),
}

/// Per-run counters. `last_active` drives lazy TTL reclamation.
#[derive(Debug)]
struct RunState {
    calls_used: AtomicU64,
    page_calls_used: DashMap<String, AtomicU64>,
    attempts: DashMap<String, AtomicU64>,
    last_active: parking_lot::Mutex<Instant>,
}

impl RunState {
    fn new() -> Self {
        Self {
            calls_used: AtomicU64::new(0),
            page_calls_used: DashMap::new(),
            attempts: DashMap::new(),
            last_active: parking_lot::Mutex::new(Instant::now()),
        }
    }

    fn touch(&self) {
        *self.last_active.lock() = Instant::now();
    }

    fn page_key(ctx: &WikiRunContext) -> &str {
        ctx.page_id.as_deref().unwrap_or("")
    }
}

/// Key for the run-local response cache. Includes the tool name, canonicalized
/// arguments and snapshot epoch so a stale index generation never hits.
type CacheKey = (String, String, String, Option<u64>);
type ResponseCache = Cache<CacheKey, serde_json::Value>;

/// Process-in-memory budget tracker. Cloneable so both MCP transports share
/// the same run accounting.
#[derive(Clone)]
pub struct BudgetTracker {
    runs: Arc<DashMap<String, Arc<RunState>>>,
    cache: ResponseCache,
    limits: BudgetLimits,
    ttl: Duration,
    cache_enabled: bool,
}

impl BudgetTracker {
    /// Build a tracker from resolved limits and cache policy.
    pub fn new(limits: BudgetLimits, ttl_secs: u64, cache_enabled: bool) -> Self {
        let cache = Cache::builder()
            .max_capacity((limits.max_active_runs * 32) as u64)
            .time_to_live(Duration::from_secs(ttl_secs.max(1)))
            .build();
        Self {
            runs: Arc::new(DashMap::new()),
            cache,
            limits,
            ttl: Duration::from_secs(ttl_secs.max(1)),
            cache_enabled,
        }
    }

    /// Whether run-local caching is active.
    pub fn cache_enabled(&self) -> bool {
        self.cache_enabled
    }

    /// Reclaim runs untouched for longer than the configured TTL.
    fn evict_stale(&self) {
        let ttl = self.ttl;
        self.runs
            .retain(|_, state| state.last_active.lock().elapsed() <= ttl);
    }

    /// Look up a cached payload for an identical deterministic call.
    pub async fn cache_get(
        &self,
        ctx: &WikiRunContext,
        tool: &str,
        canonical_args: &str,
    ) -> Option<serde_json::Value> {
        if !self.cache_enabled {
            return None;
        }
        let key = (
            ctx.run_id.clone(),
            tool.to_string(),
            canonical_args.to_string(),
            ctx.snapshot_epoch,
        );
        self.cache.get(&key).await
    }

    /// Store a successful deterministic response for later reuse in the run.
    pub async fn cache_put(
        &self,
        ctx: &WikiRunContext,
        tool: &str,
        canonical_args: &str,
        payload: serde_json::Value,
    ) {
        if !self.cache_enabled {
            return;
        }
        let key = (
            ctx.run_id.clone(),
            tool.to_string(),
            canonical_args.to_string(),
            ctx.snapshot_epoch,
        );
        self.cache.insert(key, payload).await;
    }

    /// Check the run/page/attempt limits without reserving a step.
    ///
    /// Used by the cache-hit path: a cached result is still refused once the
    /// run is exhausted, but a hit never consumes a step.
    pub fn check_exhausted(&self, ctx: &WikiRunContext) -> Option<BudgetExhausted> {
        self.evict_stale();
        let page = RunState::page_key(ctx);
        let state = self.runs.get(&ctx.run_id).map(|entry| entry.clone());
        let Some(state) = state else {
            // No counters yet: an empty run is within every limit.
            return None;
        };
        let calls_used = state.calls_used.load(Ordering::Relaxed) as usize;
        if calls_used >= self.limits.max_calls_per_run {
            return Some(BudgetExhausted::new(BudgetExhaustion::Run));
        }
        let page_used = state
            .page_calls_used
            .get(page)
            .map(|v| v.load(Ordering::Relaxed) as usize)
            .unwrap_or(0);
        if page_used >= self.limits.max_calls_per_page {
            return Some(BudgetExhausted::new(BudgetExhaustion::Page));
        }
        let attempts = state
            .attempts
            .get(page)
            .map(|v| v.load(Ordering::Relaxed) as usize)
            .unwrap_or(0);
        if attempts >= self.limits.max_calls_per_page {
            return Some(BudgetExhausted::new(BudgetExhaustion::Attempts));
        }
        None
    }

    /// Check the run/page/attempt limits and reserve one billable step.
    ///
    /// Called before the tool executes, so an exhausted call never incurs any
    /// query cost. Returns the state to inject as `_budget` on success.
    pub fn try_consume(&self, ctx: &WikiRunContext) -> BudgetDecision {
        self.evict_stale();
        let page = RunState::page_key(ctx);

        if let Some(existing) = self.runs.get(&ctx.run_id) {
            let state = existing.clone();
            drop(existing);
            return self.consume_state(ctx, &state, page);
        }

        if self.runs.len() >= self.limits.max_active_runs {
            return BudgetDecision::Exhausted(BudgetExhausted::new(BudgetExhaustion::ActiveRuns));
        }
        let state = Arc::new(RunState::new());
        self.runs.insert(ctx.run_id.clone(), state.clone());
        self.consume_state(ctx, &state, page)
    }

    fn consume_state(&self, ctx: &WikiRunContext, state: &RunState, page: &str) -> BudgetDecision {
        let calls_used = state.calls_used.load(Ordering::Relaxed) as usize;
        if calls_used >= self.limits.max_calls_per_run {
            return BudgetDecision::Exhausted(BudgetExhausted::new(BudgetExhaustion::Run));
        }
        let page_used = state
            .page_calls_used
            .get(page)
            .map(|v| v.load(Ordering::Relaxed) as usize)
            .unwrap_or(0);
        if page_used >= self.limits.max_calls_per_page {
            return BudgetDecision::Exhausted(BudgetExhausted::new(BudgetExhaustion::Page));
        }
        let attempts = state
            .attempts
            .get(page)
            .map(|v| v.load(Ordering::Relaxed) as usize)
            .unwrap_or(0);
        if attempts >= self.limits.max_calls_per_page {
            return BudgetDecision::Exhausted(BudgetExhausted::new(BudgetExhaustion::Attempts));
        }

        state
            .page_calls_used
            .entry(page.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
        let used = state.calls_used.fetch_add(1, Ordering::Relaxed) as usize + 1;
        state.touch();

        BudgetDecision::Allowed {
            state: BudgetState {
                run_id: ctx.run_id.clone(),
                calls_used: used,
                calls_remaining: self.limits.max_calls_per_run.saturating_sub(used),
                page_calls_used: page_used + 1,
                page_calls_remaining: self.limits.max_calls_per_page.saturating_sub(page_used + 1),
                snapshot_epoch: ctx.snapshot_epoch,
                cached: false,
            },
        }
    }

    /// A tool execution failed: refund the reserved step and count an attempt.
    pub fn record_failure(&self, ctx: &WikiRunContext) {
        if let Some(state) = self.runs.get(&ctx.run_id) {
            let page = RunState::page_key(ctx);
            saturating_sub(&state.calls_used);
            if let Some(counter) = state.page_calls_used.get(page) {
                saturating_sub(counter.value());
            }
            state
                .attempts
                .entry(page.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
            state.touch();
        }
    }

    /// Read the current counters without reserving a step.
    ///
    /// Used to build `_budget` metadata when a call is served from cache.
    pub fn peek(&self, ctx: &WikiRunContext, cached: bool) -> BudgetState {
        let empty = BudgetState {
            run_id: ctx.run_id.clone(),
            calls_used: 0,
            calls_remaining: self.limits.max_calls_per_run,
            page_calls_used: 0,
            page_calls_remaining: self.limits.max_calls_per_page,
            snapshot_epoch: ctx.snapshot_epoch,
            cached,
        };
        match self.runs.get(&ctx.run_id) {
            Some(state) => {
                let page = RunState::page_key(ctx);
                let calls_used = state.calls_used.load(Ordering::Relaxed) as usize;
                let page_used = state
                    .page_calls_used
                    .get(page)
                    .map(|v| v.load(Ordering::Relaxed) as usize)
                    .unwrap_or(0);
                BudgetState {
                    run_id: ctx.run_id.clone(),
                    calls_used,
                    calls_remaining: self.limits.max_calls_per_run.saturating_sub(calls_used),
                    page_calls_used: page_used,
                    page_calls_remaining: self.limits.max_calls_per_page.saturating_sub(page_used),
                    snapshot_epoch: ctx.snapshot_epoch,
                    cached,
                }
            }
            None => empty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(run: &str, page: Option<&str>) -> WikiRunContext {
        WikiRunContext {
            run_id: run.to_string(),
            page_id: page.map(str::to_string),
            snapshot_epoch: None,
        }
    }

    fn limits(run: usize, page: usize) -> BudgetLimits {
        BudgetLimits {
            max_calls_per_run: run,
            max_calls_per_page: page,
            max_active_runs: 8,
        }
    }

    #[test]
    fn run_id_validation_accepts_safe_and_rejects_bad() {
        assert!(ctx("run-1:page.2_a", None).validate().is_ok());
        assert!(ctx("", None).validate().is_err());
        assert!(ctx("has space", None).validate().is_err());
        assert!(ctx("has/slash", None).validate().is_err());
        assert!(ctx("run", Some("../escape")).validate().is_err());
        assert!(ctx(&"x".repeat(129), None).validate().is_err());
    }

    #[test]
    fn budget_counts_per_run_and_per_page() {
        let tracker = BudgetTracker::new(limits(3, 2), 900, false);
        let page_a = ctx("r", Some("page-a"));
        assert_eq!(
            tracker.try_consume(&page_a),
            BudgetDecision::Allowed {
                state: BudgetState {
                    run_id: "r".into(),
                    calls_used: 1,
                    calls_remaining: 2,
                    page_calls_used: 1,
                    page_calls_remaining: 1,
                    snapshot_epoch: None,
                    cached: false,
                }
            }
        );
        let _ = tracker.try_consume(&page_a); // page-a now full (2 calls)
        match tracker.try_consume(&page_a) {
            BudgetDecision::Exhausted(e) => assert_eq!(e.scope, BudgetExhaustion::Page),
            other => panic!("expected page exhaustion, got {other:?}"),
        }
        // A different page still has budget.
        assert!(matches!(
            tracker.try_consume(&ctx("r", Some("page-b"))),
            BudgetDecision::Allowed { .. }
        ));
    }

    #[test]
    fn run_budget_exhaustion_reports_remaining_zero_and_fallback() {
        let tracker = BudgetTracker::new(limits(2, 10), 900, false);
        let c = ctx("r", None);
        let _ = tracker.try_consume(&c);
        let _ = tracker.try_consume(&c);
        match tracker.try_consume(&c) {
            BudgetDecision::Exhausted(e) => {
                assert_eq!(e.scope, BudgetExhaustion::Run);
                assert_eq!(e.remaining, 0);
                assert_eq!(e.fallback, "wcp_only");
            }
            other => panic!("expected run exhaustion, got {other:?}"),
        }
    }

    #[test]
    fn failure_refunds_step_and_counts_attempt() {
        let tracker = BudgetTracker::new(limits(10, 2), 900, false);
        let c = ctx("r", None);
        let _ = tracker.try_consume(&c);
        tracker.record_failure(&c);
        let state = tracker.peek(&c, false);
        assert_eq!(state.calls_used, 0);

        let _ = tracker.try_consume(&c);
        tracker.record_failure(&c);
        // Two failed attempts used; the next call is rejected by the attempt
        // budget (the reserved step is never refunded on refusal).
        match tracker.try_consume(&c) {
            BudgetDecision::Exhausted(e) => assert_eq!(e.scope, BudgetExhaustion::Attempts),
            other => panic!("expected attempt exhaustion, got {other:?}"),
        }
        // Refunds never underflow the counter below zero.
        assert_eq!(tracker.peek(&c, false).calls_used, 0);
    }

    #[test]
    fn active_run_ceiling_is_enforced() {
        let tracker = BudgetTracker::new(limits(10, 10), 900, false);
        let _ = tracker.try_consume(&ctx("r1", None));
        let _ = tracker.try_consume(&ctx("r2", None));
        // create a tracker with ceiling 2 via a fresh instance to keep test simple
        let small = BudgetTracker::new(
            BudgetLimits {
                max_calls_per_run: 10,
                max_calls_per_page: 10,
                max_active_runs: 2,
            },
            900,
            false,
        );
        let _ = small.try_consume(&ctx("a", None));
        let _ = small.try_consume(&ctx("b", None));
        assert!(matches!(
            small.try_consume(&ctx("c", None)),
            BudgetDecision::Exhausted(BudgetExhausted {
                scope: BudgetExhaustion::ActiveRuns,
                ..
            })
        ));
        let _ = &tracker;
    }

    #[tokio::test]
    async fn cache_hit_does_not_consume_step_and_epoch_isolation_works() {
        let tracker = BudgetTracker::new(limits(10, 10), 900, true);
        let mut c = ctx("r", None);
        let args = "{\"path\":\"src/lib.rs\"}";
        assert!(tracker.cache_get(&c, "read_file", args).await.is_none());
        tracker
            .cache_put(&c, "read_file", args, serde_json::json!({"lines": []}))
            .await;
        assert!(tracker.cache_get(&c, "read_file", args).await.is_some());
        c.snapshot_epoch = Some(7);
        assert!(tracker.cache_get(&c, "read_file", args).await.is_none());
        // peek without any consume reports zero used.
        let state = tracker.peek(&c, true);
        assert_eq!(state.calls_used, 0);
        assert!(state.cached);
    }
}
