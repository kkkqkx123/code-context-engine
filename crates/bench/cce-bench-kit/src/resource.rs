//! Linux process resource snapshots used by performance benchmarks.
//!
//! The snapshots read only `/proc` files and never fail a benchmark: a missing
//! counter is reported as `None` so non-Linux hosts can still run comparisons.
//! `VmHWM` is process-wide and monotonic, so both the absolute value and the
//! delta between two snapshots are reported; the delta is only meaningful when
//! the measured phase is the main allocator in the process.

use std::fs;

/// A point-in-time sample of process resource counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceSnapshot {
    /// Current resident set size, in KiB (`VmRSS`).
    pub rss_kb: Option<u64>,
    /// Peak resident set size since process start, in KiB (`VmHWM`).
    pub peak_rss_kb: Option<u64>,
    /// User plus system CPU time, in clock ticks.
    pub cpu_ticks: Option<u64>,
    /// Bytes read from storage (`read_bytes` in `/proc/self/io`).
    pub read_bytes: Option<u64>,
    /// Bytes written to storage (`write_bytes` in `/proc/self/io`).
    pub write_bytes: Option<u64>,
}

/// Difference between two resource snapshots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceDelta {
    pub rss_kb: Option<u64>,
    pub peak_rss_kb: Option<u64>,
    pub cpu_ticks: Option<u64>,
    pub read_bytes: Option<u64>,
    pub write_bytes: Option<u64>,
}

/// Read the current process resource counters.
pub fn resource_snapshot() -> ResourceSnapshot {
    ResourceSnapshot {
        rss_kb: proc_key_value("/proc/self/status", "VmRSS:"),
        peak_rss_kb: proc_key_value("/proc/self/status", "VmHWM:"),
        cpu_ticks: proc_cpu_ticks(),
        read_bytes: proc_key_value("/proc/self/io", "read_bytes:"),
        write_bytes: proc_key_value("/proc/self/io", "write_bytes:"),
    }
}

/// Subtract `before` from `after`, saturating at zero when a counter is reset.
pub fn resource_delta(before: ResourceSnapshot, after: ResourceSnapshot) -> ResourceDelta {
    ResourceDelta {
        rss_kb: delta(before.rss_kb, after.rss_kb),
        peak_rss_kb: delta(before.peak_rss_kb, after.peak_rss_kb),
        cpu_ticks: delta(before.cpu_ticks, after.cpu_ticks),
        read_bytes: delta(before.read_bytes, after.read_bytes),
        write_bytes: delta(before.write_bytes, after.write_bytes),
    }
}

/// Convert clock ticks to milliseconds on the standard Linux tick rate.
pub fn cpu_ms(delta: &ResourceDelta) -> Option<u64> {
    delta
        .cpu_ticks
        .map(|ticks| ticks.saturating_mul(1000).saturating_div(100))
}

fn delta(before: Option<u64>, after: Option<u64>) -> Option<u64> {
    match (before, after) {
        (Some(before), Some(after)) => Some(after.saturating_sub(before)),
        _ => None,
    }
}

fn proc_key_value(path: &str, key: &str) -> Option<u64> {
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

fn proc_cpu_ticks() -> Option<u64> {
    let text = fs::read_to_string("/proc/self/stat").ok()?;
    let tail = text.rfind(')')?;
    let fields: Vec<&str> = text[tail + 1..].split_whitespace().collect();
    let user = fields.get(11)?.parse::<u64>().ok()?;
    let system = fields.get(12)?.parse::<u64>().ok()?;
    Some(user.saturating_add(system))
}
