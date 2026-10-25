//! Scan-time file filtering helpers.
//!
//! Pure helpers shared by the gateway and the server so the two sides cannot
//! drift on extension pre-filtering, scan pattern derivation, and per-file
//! classification.

/// Parse a comma-separated extension filter into lowercase suffixes.
pub fn parse_extension_filter(csv: &str) -> Vec<String> {
    csv.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_start_matches('.').to_lowercase())
        .collect()
}

/// Parse a comma-separated exclude filter into trimmed entries.
pub fn parse_exclude_filter(csv: &str) -> Vec<String> {
    csv.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Build scanner include patterns from an extension list.
pub fn scan_include_patterns(extensions: &[String]) -> Vec<String> {
    extensions
        .iter()
        .map(|ext| ext.trim())
        .filter(|ext| !ext.is_empty())
        .map(|ext| format!("*.{}", ext.trim_start_matches('.').to_lowercase()))
        .collect()
}

/// Build scanner exclude patterns from an exclude directory list.
pub fn scan_exclude_patterns(exclude_dirs: &[String]) -> Vec<String> {
    exclude_dirs
        .iter()
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

/// Gateway scanner uses no include restriction: the remote pipeline stays
/// authoritative for parsing while the transport side only pre-filters by
/// suffix after the scan.
pub fn gateway_include_patterns() -> Vec<String> {
    Vec::new()
}

/// Classification of one scanned entry for filter purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanFileDecision {
    /// Usable file passing size, text, and suffix checks.
    Keep,
    /// Larger than the ingest bound.
    Oversize,
    /// Detected binary content.
    Binary,
    /// Rejected by the suffix pre-filter.
    Filtered,
}

/// Classify one scanned entry without touching the filesystem.
pub fn classify_scan_file(
    size: u64,
    is_text: bool,
    relative_path: &str,
    wanted: &[String],
    max_bytes: u64,
) -> ScanFileDecision {
    if size > max_bytes {
        return ScanFileDecision::Oversize;
    }
    if !is_text {
        return ScanFileDecision::Binary;
    }
    if !extension_matches(relative_path, wanted) {
        return ScanFileDecision::Filtered;
    }
    ScanFileDecision::Keep
}

/// Whether a relative path passes the suffix pre-filter.
pub fn extension_matches(relative_path: &str, wanted: &[String]) -> bool {
    if wanted.is_empty() {
        return true;
    }
    let ext = std::path::Path::new(relative_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .unwrap_or_default();
    wanted.contains(&ext)
}