use super::paths::storage_path;
use super::staging::*;
use base64::Engine as _;
use cce_api::models::IngestedFile;
use std::time::{Duration, SystemTime};

/// Split `bytes` into two pieces for multi-chunk cases.
fn split_two(bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mid = bytes.len() / 2;
    (bytes[..mid].to_vec(), bytes[mid..].to_vec())
}

/// Build one ingested chunk carrying valid per-chunk hashes.
fn entry(relative: &str, index: u32, total: u32, chunk: &[u8], content_hash: &str) -> IngestedFile {
    IngestedFile {
        relative_path: relative.to_string(),
        content_hash: Some(content_hash.to_string()),
        content_base64: base64::engine::general_purpose::STANDARD.encode(chunk),
        chunk_index: index,
        total_chunks: total,
        chunk_hash: Some(cce_utils::hash::calculate_hash(chunk)),
        compressed: false,
    }
}

#[tokio::test]
async fn zero_version_single_chunk_stages_without_bookkeeping() {
    let mirror = tempfile::tempdir().expect("mirror");
    let staging = tempfile::tempdir().expect("staging");
    let full = b"fn main() {}";
    let content_hash = cce_utils::hash::calculate_hash(full);
    let single = entry("src/main.rs", 0, 1, full, &content_hash);
    let file = decode_group("src/main.rs", &[&single]).expect("decode");
    let staged = stage_file(FileStageParams {
        root: mirror.path(),
        staging_base: staging.path(),
        project_id: 9101,
        manifest_version: 0,
        file: &file,
    })
    .await
    .expect("stage");
    assert!(staged);
    let dest = mirror.path().join(storage_path("src/main.rs"));
    assert_eq!(std::fs::read(&dest).expect("staged file"), full);
    assert_eq!(
        walk_version_staging_in(staging.path(), 9101, 0),
        VersionStagingUsage::default(),
        "version zero stages no chunks for resume"
    );
}

#[tokio::test]
async fn zero_version_requires_whole_file_in_one_batch() {
    let mirror = tempfile::tempdir().expect("mirror");
    let staging = tempfile::tempdir().expect("staging");
    let full = b"only the first half arrives in this batch";
    let content_hash = cce_utils::hash::calculate_hash(full);
    let (first, _second) = split_two(full);
    let partial = entry("lib.rs", 0, 2, &first, &content_hash);
    let file = decode_group("lib.rs", &[&partial]).expect("decode");
    let error = stage_file(FileStageParams {
        root: mirror.path(),
        staging_base: staging.path(),
        project_id: 9102,
        manifest_version: 0,
        file: &file,
    })
    .await
    .expect_err("incomplete version zero batch must fail");
    assert!(error.contains("version zero carries no resume bookkeeping"));
    assert!(!mirror.path().join(storage_path("lib.rs")).exists());
}

#[tokio::test]
async fn versioned_batch_reassembles_and_verifies_whole_file() {
    let mirror = tempfile::tempdir().expect("mirror");
    let staging = tempfile::tempdir().expect("staging");
    let full = b"the quick brown fox jumps over the lazy dog";
    let content_hash = cce_utils::hash::calculate_hash(full);
    let (first, second) = split_two(full);
    let left = entry("lib.rs", 0, 2, &first, &content_hash);
    let right = entry("lib.rs", 1, 2, &second, &content_hash);
    let file = decode_group("lib.rs", &[&left, &right]).expect("decode");
    let staged = stage_file(FileStageParams {
        root: mirror.path(),
        staging_base: staging.path(),
        project_id: 9103,
        manifest_version: 7,
        file: &file,
    })
    .await
    .expect("stage");
    assert!(staged);
    assert_eq!(
        std::fs::read(mirror.path().join(storage_path("lib.rs"))).expect("staged file"),
        full
    );
    assert_eq!(
        walk_version_staging_in(staging.path(), 9103, 7),
        VersionStagingUsage {
            chunks: 2,
            bytes: full.len() as u64
        }
    );
    assert_eq!(
        pending_pieces_in(staging.path(), 9103, 7, &file),
        VersionStagingUsage::default(),
        "re-sending the same pieces costs no extra quota"
    );
}

#[tokio::test]
async fn interrupted_push_resumes_from_staged_chunks() {
    let mirror = tempfile::tempdir().expect("mirror");
    let staging = tempfile::tempdir().expect("staging");
    let full = b"resume across two separate batches";
    let content_hash = cce_utils::hash::calculate_hash(full);
    let (first, second) = split_two(full);
    let left = entry("lib.rs", 0, 2, &first, &content_hash);
    let right = entry("lib.rs", 1, 2, &second, &content_hash);

    let opening = decode_group("lib.rs", &[&left]).expect("decode first batch");
    let staged = stage_file(FileStageParams {
        root: mirror.path(),
        staging_base: staging.path(),
        project_id: 9104,
        manifest_version: 9,
        file: &opening,
    })
    .await
    .expect("stage first batch");
    assert!(!staged, "a partial batch reports an incomplete file");
    assert!(!mirror.path().join(storage_path("lib.rs")).exists());
    assert_eq!(
        received_chunk_indices_in(staging.path(), 9104, 9, &storage_path("lib.rs")),
        vec![0]
    );
    assert_eq!(
        pending_pieces_in(staging.path(), 9104, 9, &opening),
        VersionStagingUsage::default(),
        "the stored chunk is not charged again"
    );

    let closing = decode_group("lib.rs", &[&right]).expect("decode second batch");
    let pending = pending_pieces_in(staging.path(), 9104, 9, &closing);
    assert_eq!(pending.chunks, 1, "only the missing chunk is new");
    let staged = stage_file(FileStageParams {
        root: mirror.path(),
        staging_base: staging.path(),
        project_id: 9104,
        manifest_version: 9,
        file: &closing,
    })
    .await
    .expect("stage second batch");
    assert!(staged, "the resumed file completes");
    assert_eq!(
        std::fs::read(mirror.path().join(storage_path("lib.rs"))).expect("staged file"),
        full
    );
}

#[test]
fn chunk_hash_mismatch_is_rejected_before_staging() {
    let full = b"content";
    let content_hash = cce_utils::hash::calculate_hash(full);
    let mut corrupted = entry("a.rs", 0, 1, full, &content_hash);
    corrupted.chunk_hash = Some("00".repeat(32));
    let error = decode_group("a.rs", &[&corrupted]).expect_err("a corrupted piece must be refused");
    assert!(error.contains("chunk hash mismatch"));
}

#[tokio::test]
async fn whole_file_hash_mismatch_is_rejected_after_reassembly() {
    let mirror = tempfile::tempdir().expect("mirror");
    let staging = tempfile::tempdir().expect("staging");
    let full = b"both pieces decode but the manifest hash is stale";
    let stale_hash = cce_utils::hash::calculate_hash(b"a different file");
    let (first, second) = split_two(full);
    let left = entry("a.rs", 0, 2, &first, &stale_hash);
    let right = entry("a.rs", 1, 2, &second, &stale_hash);
    let file = decode_group("a.rs", &[&left, &right]).expect("decode");
    let error = stage_file(FileStageParams {
        root: mirror.path(),
        staging_base: staging.path(),
        project_id: 9105,
        manifest_version: 3,
        file: &file,
    })
    .await
    .expect_err("a stale whole-file hash must be refused");
    assert!(error.contains("changed between manifest and upload"));
    assert!(!mirror.path().join(storage_path("a.rs")).exists());
}

#[test]
fn group_validation_rejects_inconsistent_pieces() {
    let payload = b"payload";
    let content_hash = cce_utils::hash::calculate_hash(payload);

    let zero_total = entry("a.rs", 0, 0, payload, &content_hash);
    assert!(
        decode_group("a.rs", &[&zero_total])
            .expect_err("zero total must be refused")
            .contains("total chunks must be positive")
    );

    let over_bound = entry("a.rs", 0, max_file_chunks() + 1, payload, &content_hash);
    assert!(
        decode_group("a.rs", &[&over_bound])
            .expect_err("an oversized split must be refused")
            .contains("more than the")
    );

    let first = entry("a.rs", 0, 1, payload, &content_hash);
    let mismatched = entry("a.rs", 1, 2, payload, &content_hash);
    assert!(
        decode_group("a.rs", &[&first, &mismatched])
            .expect_err("totals must agree")
            .contains("while the group started with")
    );

    let out_of_range = entry("a.rs", 2, 2, payload, &content_hash);
    assert!(
        decode_group("a.rs", &[&out_of_range])
            .expect_err("the index must stay in range")
            .contains("exceeds")
    );

    let duplicate_one = entry("a.rs", 0, 1, payload, &content_hash);
    let duplicate_two = entry("a.rs", 0, 1, payload, &content_hash);
    assert!(
        decode_group("a.rs", &[&duplicate_one, &duplicate_two])
            .expect_err("duplicate indices must be refused")
            .contains("more than once")
    );
}

#[test]
fn pending_usage_stays_within_version_bounds() {
    let used = VersionStagingUsage::default();
    let pending = VersionStagingUsage {
        chunks: 1,
        bytes: 7,
    };
    assert!(within_staging_bounds(used, pending));
    assert!(within_staging_bounds(
        VersionStagingUsage {
            chunks: MAX_STAGING_CHUNKS_PER_VERSION - 1,
            bytes: MAX_STAGING_BYTES_PER_VERSION - 7,
        },
        pending
    ));
    assert!(!within_staging_bounds(
        VersionStagingUsage {
            chunks: MAX_STAGING_CHUNKS_PER_VERSION,
            ..VersionStagingUsage::default()
        },
        pending
    ));
    assert!(!within_staging_bounds(
        VersionStagingUsage {
            bytes: MAX_STAGING_BYTES_PER_VERSION,
            ..VersionStagingUsage::default()
        },
        pending
    ));
    assert!(!within_staging_bounds(
        used,
        VersionStagingUsage {
            chunks: MAX_STAGING_CHUNKS_PER_VERSION + 1,
            ..VersionStagingUsage::default()
        }
    ));
}

#[test]
fn sweep_reclaims_expired_versions_and_keeps_fresh_ones() {
    let base = tempfile::tempdir().expect("staging base");
    let project = base.path().join("project-9106");
    let expired = project.join("manifest-1");
    let fresh = project.join("manifest-2");
    for version in [&expired, &fresh] {
        let chunk = version.join("file-1").join("chunk_0");
        std::fs::create_dir_all(chunk.parent().expect("file dir")).expect("version dir");
        std::fs::write(chunk, b"stale").expect("chunk");
    }
    let age = SystemTime::now()
        .checked_sub(INGEST_STAGING_RETENTION + Duration::from_secs(60))
        .expect("clock is far enough from the epoch");
    std::fs::File::open(&expired)
        .expect("expired dir handle")
        .set_modified(age)
        .expect("age the expired version");

    sweep_project_staging_in(base.path(), 9106, INGEST_STAGING_RETENTION);
    assert!(!expired.exists(), "expired version is reclaimed");
    assert!(fresh.exists(), "a fresh version survives the sweep");

    sweep_expired_staging_in(base.path(), INGEST_STAGING_RETENTION);
    assert!(
        fresh.exists(),
        "the startup sweep only removes expired versions"
    );
}

#[tokio::test]
async fn commit_cleanup_removes_every_version_of_one_project() {
    let base = tempfile::tempdir().expect("staging base");
    let mirror = tempfile::tempdir().expect("mirror");
    let full = b"cleanup removes both versions";
    let content_hash = cce_utils::hash::calculate_hash(full);
    let single = entry("a.rs", 0, 1, full, &content_hash);
    for manifest_version in [1u64, 2u64] {
        let file = decode_group("a.rs", &[&single]).expect("decode");
        stage_file(FileStageParams {
            root: mirror.path(),
            staging_base: base.path(),
            project_id: 9107,
            manifest_version,
            file: &file,
        })
        .await
        .expect("stage");
    }
    let sibling = entry("b.rs", 0, 1, full, &content_hash);
    let file = decode_group("b.rs", &[&sibling]).expect("decode");
    stage_file(FileStageParams {
        root: mirror.path(),
        staging_base: base.path(),
        project_id: 9108,
        manifest_version: 1,
        file: &file,
    })
    .await
    .expect("stage sibling");

    clear_project_staging_in(base.path(), 9107);
    assert!(
        !base.path().join("project-9107").exists(),
        "commit cleanup removes every version of the committed project"
    );
    assert!(
        base.path().join("project-9108").exists(),
        "other projects are untouched"
    );
}
