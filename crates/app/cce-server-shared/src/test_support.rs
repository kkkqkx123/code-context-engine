//! Helpers shared by the crate's unit tests.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Scratch directory for one test's data files, removed on scope exit.
///
/// Engines open the local vector store and the metadata database eagerly, so
/// each test config needs a data directory of its own; a shared path makes
/// concurrently running tests race on the same files. Dropping the guard
/// removes the whole directory, including vector and WAL files.
pub(crate) struct TestDir(PathBuf);

impl TestDir {
    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Create a fresh directory for one test's data files.
pub(crate) fn unique_test_dir() -> TestDir {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "cce_test_{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("create test data dir");
    TestDir(dir)
}
