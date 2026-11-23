//! Process-level resource probes used by index stage reporting.

/// Current resident set size in KiB, read from the Linux process status file.
///
/// Returns `None` on platforms or containers where the file is unavailable.
/// Stage reports treat `None` as an unsampled value instead of guessing.
pub fn rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        let Some(value) = line.strip_prefix("VmRSS:") else {
            continue;
        };
        let kib = value.split_whitespace().next()?.parse::<u64>().ok()?;
        return Some(kib);
    }
    None
}
