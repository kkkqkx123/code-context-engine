//! Verified reads of scanned files.
//!
//! The scanner hashes raw bytes while discovering files; indexing must then
//! observe exactly those bytes. Reading, verifying against the scan-phase
//! fingerprint and decoding therefore live next to [`FileEntry`] rather than
//! in the indexing layer, which would otherwise re-derive the scanner's hash
//! domain.

use std::path::Path;

use cce_types::error::ParseError;
use cce_types::error::common::IoError;

use crate::FileEntry;

/// Check raw file bytes against the scan-phase content hash.
///
/// The scanner hashes the raw bytes of the full file, so the full-content
/// hash is the only domain: a match proves the bytes still correspond to the
/// scanned snapshot.
fn raw_bytes_match_scan_hash(bytes: &[u8], expected: &str) -> bool {
    cce_utils::hash::calculate_hash(bytes) == expected
}

/// Read a file, verify its raw bytes against the scan-phase content hash,
/// then decode to UTF-8 with automatic encoding detection.
///
/// This closes the scan→process race: without the verification, a file
/// modified between the scanning and processing phases would be parsed into
/// data that no longer matches the recorded hashes, silently poisoning
/// checkpoints and the hot-update change baseline. Pass `None` when no scan
/// baseline exists (event-driven reads); verification is skipped in that
/// case.
pub async fn read_verified_utf8(
    path: &Path,
    expected_hash: Option<&str>,
) -> Result<String, ParseError> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| ParseError::Io(IoError::from(e)))?;
    if let Some(expected) = expected_hash
        && !raw_bytes_match_scan_hash(&bytes, expected)
    {
        return Err(ParseError::content_changed(format!(
            "content of '{}' changed between scan and processing (scan-time hash {expected} no longer matches); a re-scan is required",
            path.display()
        )));
    }
    cce_utils::file::decode_bytes_to_utf8(&bytes, path)
        .map_err(|e| ParseError::encoding(format!("{}: {e}", path.display())))
}

/// Read a scanned file reusing its scan-phase fingerprint.
///
/// When the on-disk size and modification time still match the scan-phase
/// entry, the content cannot have changed without updating the fingerprint,
/// so the file is decoded directly without recomputing the full-content
/// hash. Any fingerprint mismatch falls back to hash verification, which
/// reports drift explicitly instead of silently indexing stale content.
pub async fn read_verified_utf8_for_entry(entry: &FileEntry) -> Result<String, ParseError> {
    let path = &entry.path;
    let fresh = tokio::fs::metadata(path)
        .await
        .map(|metadata| {
            let size_matches = metadata.len() == entry.size;
            let mtime_matches = metadata
                .modified()
                .map(|modified| chrono::DateTime::<chrono::Utc>::from(modified) == entry.modified)
                .unwrap_or(false);
            size_matches && mtime_matches
        })
        .unwrap_or(false);
    if fresh {
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| ParseError::Io(IoError::from(e)))?;
        return cce_utils::file::decode_bytes_to_utf8(&bytes, path)
            .map_err(|e| ParseError::encoding(format!("{}: {e}", path.display())));
    }
    read_verified_utf8(path, entry.content_hash.as_deref()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full-content hash domain (small files) must verify.
    #[test]
    fn raw_bytes_match_scan_hash_full_domain() {
        let bytes = b"small file body".to_vec();
        let full = cce_utils::hash::calculate_hash(&bytes);
        assert!(raw_bytes_match_scan_hash(&bytes, &full));
    }

    /// Only the full-content hash verifies: a stale prefix-window hash (the
    /// retired partial-hash domain) must be rejected as drift.
    #[test]
    fn raw_bytes_match_scan_hash_rejects_prefix_hash() {
        let big = vec![b'a'; 1024 * 1024 + 128];
        let prefix = cce_utils::hash::calculate_hash_with_limit(&big, Some(1024 * 1024));
        assert!(!raw_bytes_match_scan_hash(&big, &prefix));
        assert!(raw_bytes_match_scan_hash(
            &big,
            &cce_utils::hash::calculate_hash(&big)
        ));
    }

    /// Verified read accepts content whose raw bytes still hash to the
    /// scan-time value and decodes it with encoding detection.
    #[tokio::test]
    async fn read_verified_utf8_accepts_matching_hash() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("sample.txt");
        let source = "hello verified world";
        std::fs::write(&path, source).expect("write file");
        let hash = cce_utils::hash::calculate_hash(source.as_bytes());

        let content = read_verified_utf8(&path, Some(&hash))
            .await
            .expect("matching hash must decode");
        assert_eq!(content, source);
    }

    /// Content drifted inside the scan→process window must fail explicitly
    /// instead of silently producing data inconsistent with recorded hashes.
    #[tokio::test]
    async fn read_verified_utf8_rejects_drifted_content() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("drifted.txt");
        std::fs::write(&path, "current on-disk content").expect("write file");
        let stale = cce_utils::hash::calculate_hash(b"content at scan time");

        let error = read_verified_utf8(&path, Some(&stale))
            .await
            .expect_err("drifted content must fail verification");
        assert!(matches!(error, ParseError::ContentChanged(_)));
        assert!(
            error
                .to_string()
                .contains("changed between scan and processing")
        );
    }

    /// Without a scan baseline the check is skipped (event-driven reads).
    #[tokio::test]
    async fn read_verified_utf8_skips_check_without_baseline() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("no_baseline.txt");
        std::fs::write(&path, "any content").expect("write file");

        let content = read_verified_utf8(&path, None)
            .await
            .expect("missing baseline must skip verification");
        assert_eq!(content, "any content");
    }

    /// Entry-based read reuses the scan fingerprint without recomputing the
    /// content hash when size and mtime are unchanged.
    #[tokio::test]
    async fn read_verified_utf8_for_entry_reuses_fingerprint() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("fresh.txt");
        let source = "hello fingerprint reuse";
        std::fs::write(&path, source).expect("write file");
        let metadata = std::fs::metadata(&path).expect("stat file");
        let entry = FileEntry {
            path: path.clone(),
            relative_path: std::path::PathBuf::from("fresh.txt"),
            size: metadata.len(),
            modified: metadata.modified().expect("mtime").into(),
            content_hash: Some(cce_utils::hash::calculate_hash(source.as_bytes())),
            language_info: None,
        };

        let content = read_verified_utf8_for_entry(&entry)
            .await
            .expect("fresh fingerprint must decode");
        assert_eq!(content, source);
    }

    /// Entry-based read falls back to hash verification and reports drift
    /// when the fingerprint no longer matches.
    #[tokio::test]
    async fn read_verified_utf8_for_entry_reports_drift() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("drifted_entry.txt");
        std::fs::write(&path, "current on-disk content").expect("write file");
        let metadata = std::fs::metadata(&path).expect("stat file");
        let stale_modified = chrono::DateTime::<chrono::Utc>::from(std::time::UNIX_EPOCH);
        assert_ne!(
            chrono::DateTime::<chrono::Utc>::from(metadata.modified().expect("mtime")),
            stale_modified
        );
        let entry = FileEntry {
            path: path.clone(),
            relative_path: std::path::PathBuf::from("drifted_entry.txt"),
            size: 1,
            modified: stale_modified,
            content_hash: Some(cce_utils::hash::calculate_hash(b"content at scan time")),
            language_info: None,
        };

        let error = read_verified_utf8_for_entry(&entry)
            .await
            .expect_err("drifted content must fail verification");
        assert!(matches!(error, ParseError::ContentChanged(_)));
    }

    /// Non-UTF-8 files verify in the raw-byte domain before decoding; a GBK
    /// document must pass the hash check and decode through encoding detection.
    #[tokio::test]
    async fn read_verified_utf8_decodes_non_utf8_after_raw_check() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("gbk.txt");
        // GBK bytes of a Chinese greeting (decodes to the assertion below).
        let gbk: Vec<u8> = vec![0xC4, 0xE3, 0xBA, 0xC3, 0xCA, 0xC0, 0xBD, 0xE7];
        std::fs::write(&path, &gbk).expect("write file");
        let hash = cce_utils::hash::calculate_hash(&gbk);

        let content = read_verified_utf8(&path, Some(&hash))
            .await
            .expect("raw-byte hash must match for non-UTF-8 files");
        assert_eq!(content, "你好世界");
    }
}
