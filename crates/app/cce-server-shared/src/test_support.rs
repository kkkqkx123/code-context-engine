//! Helpers shared by the crate's unit tests.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Create a fresh directory for one test's data files.
///
/// Engines open the local vector store and the metadata database eagerly, so
/// each test config needs a data directory of its own; a shared path makes
/// concurrently running tests race on the same files.
pub(crate) fn unique_test_dir() -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "cce_test_{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("create test data dir");
    dir
}
