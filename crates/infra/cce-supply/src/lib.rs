//! Shared supply domain for gateway pushes and server ingest.
//!
//! This crate owns pure supply logic without network or filesystem access:
//! content fingerprinting, chunk hashing, compression negotiation, manifest
//! versioning, extension pre-filtering, logical path normalization, and the
//! conversion from supply filter arguments into a project profile patch.
//! Gateway and server entries both delegate here so the two sides cannot drift.

use cce_config::ProjectAppConfig;

/// Hash one chunk of raw bytes for per-chunk verification.
pub fn chunk_hash(bytes: &[u8]) -> String {
    cce_utils::hash::calculate_hash(bytes)
}

/// Optionally compress chunk bytes with zstd.
///
/// Compression stays off unless explicitly enabled. Incompressible or empty
/// input is returned raw so the wire stays a single whole-file shape.
pub fn maybe_compress(bytes: &[u8], compress: bool) -> (Vec<u8>, bool) {
    if !compress || bytes.is_empty() {
        return (bytes.to_vec(), false);
    }
    match zstd::encode_all(bytes, 3) {
        Ok(encoded) if (encoded.len() as u64) < (bytes.len() as u64) => (encoded, true),
        _ => (bytes.to_vec(), false),
    }
}

/// Decompress chunk bytes when negotiation marked them compressed.
pub fn decompress_chunk(bytes: &[u8], compressed: bool) -> Result<Vec<u8>, String> {
    if !compressed {
        return Ok(bytes.to_vec());
    }
    zstd::decode_all(bytes).map_err(|e| format!("chunk decompression failed: {e}"))
}

/// Number of fixed-size chunks needed for a byte size.
pub fn total_chunks_for_size(size: u64, chunk_bytes: usize) -> u32 {
    let chunk_bytes = chunk_bytes.max(1) as u64;
    size.div_ceil(chunk_bytes).max(1).min(u32::MAX as u64) as u32
}

/// Stable manifest version for a file set.
///
/// The same identity set yields the same version so interrupted pushes resume.
/// Any content change yields a new version.
pub fn manifest_version_for_entries(entries: &[(&str, u64, Option<&str>)]) -> u64 {
    let mut sorted: Vec<(&str, u64, Option<&str>)> = entries.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut hash: u64 = 0xcbf29ce484222325;
    for (identity, size, content_hash) in sorted {
        for byte in identity.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for byte in size.to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        if let Some(content_hash) = content_hash {
            for byte in content_hash.as_bytes() {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    hash.max(1)
}

/// Whether the current file matches a baseline fingerprint.
///
/// Hashed entries compare by hash only. Hash-less entries fall back to size
/// plus modification time comparison.
pub fn fingerprints_match(
    previous_hash: Option<&str>,
    previous_size: u64,
    previous_mtime: i64,
    current_hash: Option<&str>,
    current_size: u64,
    current_mtime: i64,
) -> bool {
    if previous_hash.is_some() || current_hash.is_some() {
        return previous_hash == current_hash;
    }
    previous_size == current_size && previous_mtime == current_mtime
}

/// Normalize a logical project path without touching the filesystem.
///
/// Trims whitespace and trailing separators. Empty results collapse to root
/// so gateway logical roots stay comparable without canonicalization.
pub fn normalize_logical_path(raw: &str) -> String {
    let logical = raw.trim();
    if logical.is_empty() {
        return String::new();
    }
    let trimmed = logical.trim_end_matches('/').trim_end_matches('\\');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

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
///
/// Empty input yields an empty list meaning no include restriction. Non-empty
/// input maps each extension to a glob suffix so the scanner and the gateway
/// pre-filter describe the same file universe without duplicating the format.
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
/// suffix after the scan. The explicit constructor keeps the empty literal
/// out of call sites.
pub fn gateway_include_patterns() -> Vec<String> {
    Vec::new()
}

/// Classification of one scanned entry for supply purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplyFileDecision {
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
///
/// The caller supplies already-observed size, text-ness, and the normalized
/// relative identity. The ingest bound stays with the caller; this function
/// only compares.
pub fn classify_supply_file(
    size: u64,
    is_text: bool,
    relative_path: &str,
    wanted: &[String],
    max_bytes: u64,
) -> SupplyFileDecision {
    if size > max_bytes {
        return SupplyFileDecision::Oversize;
    }
    if !is_text {
        return SupplyFileDecision::Binary;
    }
    if !extension_matches(relative_path, wanted) {
        return SupplyFileDecision::Filtered;
    }
    SupplyFileDecision::Keep
}

/// Whether a relative path passes the supply pre-filter.
///
/// An empty wanted list accepts all text files. The remote project profile
/// stays authoritative for parsing; this pre-filter only reduces transfer.
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

/// Build a project profile patch from supply filter arguments.
///
/// Explicit gateway arguments become a one-shot profile patch so the stored
/// profile stays the single source of truth instead of a second filter.
pub fn profile_patch_from_supply_filter(
    extensions: Vec<String>,
    exclude_dirs: Vec<String>,
    respect_gitignore: Option<bool>,
    ignore_patterns: Vec<String>,
) -> ProjectAppConfig {
    let extensions_opt = if extensions.is_empty() {
        None
    } else {
        Some(extensions)
    };
    let exclude_opt = if exclude_dirs.is_empty() {
        None
    } else {
        Some(exclude_dirs)
    };
    let ignore_opt = if ignore_patterns.is_empty() {
        None
    } else {
        Some(ignore_patterns)
    };
    ProjectAppConfig::from_scan_lists(extensions_opt, exclude_opt, respect_gitignore, ignore_opt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_hash_is_stable() {
        assert_eq!(chunk_hash(b"hello"), chunk_hash(b"hello"));
        assert!(chunk_hash(b"hello") != chunk_hash(b"world"));
    }

    #[test]
    fn compression_defaults_off_and_shrinks_repetitive_bytes() {
        let repetitive = vec![b'a'; 4096];
        let (plain, compressed) = maybe_compress(&repetitive, false);
        assert!(!compressed);
        assert_eq!(plain.len(), repetitive.len());
        let (encoded, compressed) = maybe_compress(&repetitive, true);
        assert!(compressed);
        assert!(encoded.len() < repetitive.len());
    }

    #[test]
    fn manifest_version_is_stable_for_same_entries() {
        let entries = vec![("src/main.rs", 12u64, Some("abc"))];
        let first = manifest_version_for_entries(&entries);
        let second = manifest_version_for_entries(&entries);
        assert_eq!(first, second);
        assert!(first >= 1);
        let changed = vec![("src/main.rs", 12u64, Some("abd"))];
        assert!(manifest_version_for_entries(&changed) != first);
    }

    #[test]
    fn fingerprint_matches_by_hash_then_size_mtime() {
        assert!(fingerprints_match(Some("h"), 1, 1, Some("h"), 99, 99));
        assert!(!fingerprints_match(Some("a"), 1, 1, Some("b"), 1, 1));
        assert!(fingerprints_match(None, 10, 7, None, 10, 7));
        assert!(!fingerprints_match(None, 10, 7, None, 11, 7));
    }

    #[test]
    fn logical_path_normalization_trims_separators() {
        assert_eq!(normalize_logical_path("  /srv/repo/ "), "/srv/repo");
        assert_eq!(normalize_logical_path(""), "");
        assert_eq!(normalize_logical_path("/"), "/");
    }

    #[test]
    fn extension_filter_parses_and_matches() {
        let wanted = parse_extension_filter("rs, .PY ,");
        assert_eq!(wanted, vec!["rs".to_string(), "py".to_string()]);
        assert!(extension_matches("src/main.rs", &wanted));
        assert!(!extension_matches("docs/readme.md", &wanted));
        let empty: Vec<String> = Vec::new();
        assert!(extension_matches("anything.bin", &empty));
    }

    #[test]
    fn scan_patterns_derive_from_plain_lists() {
        let extensions = vec!["rs".to_string(), ".PY".to_string(), " ".to_string()];
        assert_eq!(
            scan_include_patterns(&extensions),
            vec!["*.rs".to_string(), "*.py".to_string()]
        );
        let empty: Vec<String> = Vec::new();
        assert!(scan_include_patterns(&empty).is_empty());
        assert!(gateway_include_patterns().is_empty());
        let exclude = vec!["target".to_string(), " ".to_string()];
        assert_eq!(scan_exclude_patterns(&exclude), vec!["target".to_string()]);
    }

    #[test]
    fn supply_classification_covers_keep_and_drops() {
        let wanted = vec!["rs".to_string()];
        assert_eq!(
            classify_supply_file(10, true, "src/main.rs", &wanted, 100),
            SupplyFileDecision::Keep
        );
        assert_eq!(
            classify_supply_file(200, true, "src/main.rs", &wanted, 100),
            SupplyFileDecision::Oversize
        );
        assert_eq!(
            classify_supply_file(10, false, "src/main.rs", &wanted, 100),
            SupplyFileDecision::Binary
        );
        assert_eq!(
            classify_supply_file(10, true, "docs/readme.md", &wanted, 100),
            SupplyFileDecision::Filtered
        );
    }
}
