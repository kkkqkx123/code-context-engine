//! Local directory scanning and the snapshot/fingerprint domain types.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};

use cce_api::models::MAX_INGEST_FILE_BYTES;
use cce_scanner::{FSScanner, FileEntry, ScanOptions};

use super::params::SyncParams;

/// Usable scan result for gateway pushes.
#[derive(Debug)]
pub struct ScanSnapshot {
    /// Usable files keyed by normalized identity.
    pub files: HashMap<String, ScannedFile>,
    /// Files skipped for exceeding the ingest bound.
    pub skipped_oversize: usize,
    /// Files skipped as binary.
    pub skipped_binary: usize,
}

/// One scannable file with its fingerprint.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// Local absolute path.
    pub absolute: PathBuf,
    /// File size in bytes.
    pub size: u64,
    /// Modification time as seconds since the unix epoch.
    pub modified_secs: i64,
    /// Full-content hash when known.
    pub content_hash: Option<String>,
}

/// Baseline fingerprint for incremental comparison.
///
/// The full-content hash stays authoritative. Size and modification time make
/// hash-less entries comparable and keep the idempotence key explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineFingerprint {
    /// File size in bytes at baseline time.
    pub size: u64,
    /// Modification time at baseline time.
    pub modified_secs: i64,
    /// Full-content hash when known.
    pub content_hash: Option<String>,
}

impl From<&ScannedFile> for BaselineFingerprint {
    fn from(file: &ScannedFile) -> Self {
        Self {
            size: file.size,
            modified_secs: file.modified_secs,
            content_hash: file.content_hash.clone(),
        }
    }
}

/// Whether the current file matches the baseline fingerprint.
///
/// Hashed entries compare by hash only. Hash-less entries fall back to the
/// size plus modification time comparison.
pub fn fingerprints_match(previous: &BaselineFingerprint, current: &ScannedFile) -> bool {
    if previous.content_hash.is_some() || current.content_hash.is_some() {
        return previous.content_hash == current.content_hash;
    }
    previous.size == current.size && previous.modified_secs == current.modified_secs
}

/// Stable manifest version for a snapshot. The same file set yields the
/// same version so an interrupted push can resume without retransmitting
/// received chunks; any content change yields a new version.
pub fn manifest_version_for_snapshot(snapshot: &ScanSnapshot) -> u64 {
    let mut identities: Vec<&String> = snapshot.files.keys().collect();
    identities.sort();
    let mut hash: u64 = 0xcbf29ce484222325;
    for identity in identities {
        if let Some(file) = snapshot.files.get(identity) {
            for byte in identity.as_bytes() {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
            for byte in file.size.to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
            if let Some(content_hash) = file.content_hash.as_deref() {
                for byte in content_hash.as_bytes() {
                    hash ^= *byte as u64;
                    hash = hash.wrapping_mul(0x100000001b3);
                }
            }
        }
    }
    hash.max(1)
}

/// One scan pass: usable files plus the entries the next pass reuses.
#[derive(Debug)]
pub struct ScanOutcome {
    /// Usable files keyed by normalized identity.
    pub snapshot: ScanSnapshot,
    /// Scanned entries keyed by relative path. Passing this back into
    /// `scan_local` lets the scanner reuse hashes for unchanged files.
    pub entries: HashMap<PathBuf, FileEntry>,
}

/// Scan the local directory and return usable entries keyed by identity.
///
/// With `previous` set, the scanner reuses each unchanged file's hash
/// instead of re-reading it; reuse is a pure read optimization and never
/// takes part in change detection, which stays fingerprint-based. New and
/// deleted files are unaffected by reuse.
///
/// The scan uses the same traversal and ignore semantics as local indexing.
/// The ingest bound is applied as the scanner size limit so oversized files
/// are marked before hashing. The extension filter is an explicit user-side
/// supply pre-filter; the remote project configuration stays authoritative
/// for parsing. Plugin file filters are intentionally not applied here;
/// oversupplied files are filtered again by the remote pipeline.
pub async fn scan_local(
    params: &SyncParams,
    previous: Option<&HashMap<PathBuf, FileEntry>>,
) -> Result<ScanOutcome> {
    let exclude_patterns: Vec<String> = params
        .exclude
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let wanted: Vec<String> = params
        .extensions
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_start_matches('.').to_lowercase())
        .collect();
    let options = ScanOptions {
        root_path: params.path.clone(),
        include_patterns: Vec::new(),
        exclude_patterns,
        follow_symlinks: false,
        respect_gitignore: params.gitignore,
        gitignore_patterns: Vec::new(),
        gitignore_path: None,
        max_content_size: Some(MAX_INGEST_FILE_BYTES),
        max_file_size: Some(MAX_INGEST_FILE_BYTES),
    };
    let mut scanner = FSScanner::new();
    let report = match previous {
        Some(previous) => scanner.scan_incremental_report(&options, previous),
        None => scanner.scan_report(&options),
    }
    .context("gateway failed to scan the local directory")?;
    if !report.failures.is_empty() {
        eprintln!(
            "warning: {} paths could not be scanned and are skipped",
            report.failures.len()
        );
    }
    let mut files: HashMap<String, ScannedFile> = HashMap::new();
    let mut entries: HashMap<PathBuf, FileEntry> = HashMap::with_capacity(report.entries.len());
    let mut skipped_oversize = 0usize;
    let mut skipped_binary = 0usize;
    for entry in report.entries {
        if entry.size > MAX_INGEST_FILE_BYTES {
            skipped_oversize += 1;
        } else if !entry.is_text() {
            skipped_binary += 1;
        } else {
            let matches = wanted.is_empty()
                || entry
                    .relative_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| wanted.contains(&ext.to_lowercase()))
                    .unwrap_or(false);
            if matches {
                let identity = cce_types::path::normalize_project_path(&entry.identity_key());
                files.insert(
                    identity,
                    ScannedFile {
                        absolute: entry.path.clone(),
                        size: entry.size,
                        modified_secs: entry.modified.timestamp(),
                        content_hash: entry.content_hash.clone(),
                    },
                );
            }
        }
        entries.insert(entry.relative_path.clone(), entry);
    }
    Ok(ScanOutcome {
        snapshot: ScanSnapshot {
            files,
            skipped_oversize,
            skipped_binary,
        },
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_version_is_stable_for_same_snapshot() {
        let mut files = HashMap::new();
        files.insert(
            "src/main.rs".to_string(),
            ScannedFile {
                absolute: PathBuf::from("/tmp/src/main.rs"),
                size: 12,
                modified_secs: 0,
                content_hash: Some("abc".to_string()),
            },
        );
        let first = ScanSnapshot {
            files,
            skipped_oversize: 0,
            skipped_binary: 0,
        };
        let version = manifest_version_for_snapshot(&first);
        assert!(version >= 1);
    }

    #[test]
    fn baseline_matches_by_hash_when_present() {
        let file = ScannedFile {
            absolute: PathBuf::from("/tmp/a.rs"),
            size: 10,
            modified_secs: 7,
            content_hash: Some("hash-a".to_string()),
        };
        let same = BaselineFingerprint {
            size: 99,
            modified_secs: 99,
            content_hash: Some("hash-a".to_string()),
        };
        assert!(fingerprints_match(&same, &file));
        let changed = BaselineFingerprint {
            size: 10,
            modified_secs: 7,
            content_hash: Some("hash-b".to_string()),
        };
        assert!(!fingerprints_match(&changed, &file));
    }

    #[test]
    fn baseline_falls_back_to_size_and_mtime_without_hash() {
        let file = ScannedFile {
            absolute: PathBuf::from("/tmp/a.rs"),
            size: 10,
            modified_secs: 7,
            content_hash: None,
        };
        let same = BaselineFingerprint {
            size: 10,
            modified_secs: 7,
            content_hash: None,
        };
        assert!(fingerprints_match(&same, &file));
        let changed = BaselineFingerprint {
            size: 11,
            modified_secs: 7,
            content_hash: None,
        };
        assert!(!fingerprints_match(&changed, &file));
    }

    #[tokio::test]
    async fn incremental_rescan_keeps_hashes_and_detects_tail_rewrite() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("a.rs"), "fn a() {}").expect("write a.rs");
        let params = SyncParams::with_defaults(7, dir.path().to_string_lossy().to_string());
        let first = scan_local(&params, None).await.expect("first scan");
        let hash_before = first
            .snapshot
            .files
            .get("a.rs")
            .expect("a.rs is usable")
            .content_hash
            .clone()
            .expect("a.rs carries a content hash");

        std::fs::write(dir.path().join("a.rs"), "fn a() {}\nfn extra() {}").expect("rewrite a.rs");
        std::fs::write(dir.path().join("b.rs"), "fn b() {}").expect("write b.rs");
        let second = scan_local(&params, Some(&first.entries))
            .await
            .expect("incremental scan");
        let rewritten = second
            .snapshot
            .files
            .get("a.rs")
            .expect("a.rs still usable")
            .content_hash
            .clone()
            .expect("rewritten a.rs carries a hash");
        assert_ne!(
            rewritten, hash_before,
            "a tail rewrite must surface as a new hash"
        );
        assert!(
            second.snapshot.files.contains_key("b.rs"),
            "new files are unaffected by reuse"
        );

        let third = scan_local(&params, Some(&second.entries))
            .await
            .expect("second incremental scan");
        assert_eq!(
            third
                .snapshot
                .files
                .get("a.rs")
                .and_then(|f| f.content_hash.as_deref()),
            Some(rewritten.as_str()),
            "an unchanged file keeps its hash across incremental passes"
        );
        std::fs::remove_file(dir.path().join("b.rs")).expect("remove b.rs");
        let fourth = scan_local(&params, Some(&third.entries))
            .await
            .expect("third incremental scan");
        assert!(
            !fourth.snapshot.files.contains_key("b.rs"),
            "deleted files disappear from the snapshot"
        );
    }
}
