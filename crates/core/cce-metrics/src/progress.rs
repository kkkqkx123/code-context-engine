//! Simplified progress tracking for indexing operations

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Thread-safe progress tracker for indexing operations
pub struct ProgressTracker {
    total_files: AtomicUsize,
    scanned_files: AtomicUsize,
    processed_files: AtomicUsize,
    error_count: AtomicUsize,
    current_file: Mutex<Option<PathBuf>>,
    start_time: Instant,
}

impl ProgressTracker {
    pub fn new(total: usize) -> Self {
        Self {
            total_files: AtomicUsize::new(total),
            scanned_files: AtomicUsize::new(0),
            processed_files: AtomicUsize::new(0),
            error_count: AtomicUsize::new(0),
            current_file: Mutex::new(None),
            start_time: Instant::now(),
        }
    }

    pub fn set_total(&self, total: usize) {
        self.total_files.store(total, Ordering::Relaxed);
    }

    pub fn increment_scanned(&self) {
        self.scanned_files.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_processed(&self) {
        self.processed_files.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_current_file(&self, path: &std::path::Path) {
        let mut current = self
            .current_file
            .lock()
            .expect("progress tracker current_file lock poisoned");
        *current = Some(path.to_path_buf());
    }

    pub fn clear_current_file(&self) {
        let mut current = self
            .current_file
            .lock()
            .expect("progress tracker current_file lock poisoned");
        *current = None;
    }

    pub fn get_progress(&self) -> ProgressSnapshot {
        let total = self.total_files.load(Ordering::Relaxed);
        let scanned = self.scanned_files.load(Ordering::Relaxed);
        let processed = self.processed_files.load(Ordering::Relaxed);
        let errors = self.error_count.load(Ordering::Relaxed);
        let current_file = self
            .current_file
            .lock()
            .expect("progress tracker current_file lock poisoned")
            .clone();
        let elapsed = self.start_time.elapsed();

        ProgressSnapshot {
            total_files: total,
            scanned_files: scanned,
            processed_files: processed,
            error_count: errors,
            current_file,
            elapsed,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn reset(&self) {
        self.scanned_files.store(0, Ordering::Relaxed);
        self.processed_files.store(0, Ordering::Relaxed);
        self.error_count.store(0, Ordering::Relaxed);
        let mut current = self
            .current_file
            .lock()
            .expect("progress tracker current_file lock poisoned");
        *current = None;
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new(0)
    }
}

/// A point-in-time snapshot of indexing progress
#[derive(Debug, Clone)]
pub struct ProgressSnapshot {
    pub total_files: usize,
    pub scanned_files: usize,
    pub processed_files: usize,
    pub error_count: usize,
    pub current_file: Option<PathBuf>,
    pub elapsed: Duration,
}

impl ProgressSnapshot {
    pub fn scan_percentage(&self) -> f64 {
        if self.total_files == 0 {
            return 0.0;
        }
        (self.scanned_files as f64 / self.total_files as f64) * 100.0
    }

    pub fn process_percentage(&self) -> f64 {
        if self.total_files == 0 {
            return 0.0;
        }
        (self.processed_files as f64 / self.total_files as f64) * 100.0
    }

    pub fn estimated_remaining(&self) -> Option<Duration> {
        if self.processed_files == 0 || self.elapsed.as_secs_f64() == 0.0 {
            return None;
        }

        let rate = self.processed_files as f64 / self.elapsed.as_secs_f64();
        let remaining_files = self.total_files.saturating_sub(self.processed_files);
        let remaining_secs = remaining_files as f64 / rate;

        Some(Duration::from_secs_f64(remaining_secs))
    }

    pub fn format_progress(&self) -> String {
        let pct = self.process_percentage();
        let base = format!(
            "{:.1}% ({}/{})",
            pct, self.processed_files, self.total_files
        );

        if let Some(ref file) = self.current_file {
            format!("{} - {}", base, file.display())
        } else {
            base
        }
    }

    pub fn is_complete(&self) -> bool {
        self.processed_files + self.error_count >= self.total_files && self.total_files > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_progress_tracker_basic() {
        let tracker = ProgressTracker::new(100);

        assert_eq!(tracker.get_progress().total_files, 100);
        assert_eq!(tracker.get_progress().processed_files, 0);
        assert_eq!(tracker.get_progress().scan_percentage(), 0.0);

        tracker.increment_scanned();
        tracker.increment_processed();

        assert_eq!(tracker.get_progress().scanned_files, 1);
        assert_eq!(tracker.get_progress().processed_files, 1);
        assert_eq!(tracker.get_progress().process_percentage(), 1.0);
    }

    #[test]
    fn test_progress_tracker_concurrent() {
        let tracker = Arc::new(ProgressTracker::new(1000));
        let mut handles = vec![];

        for _ in 0..10 {
            let t = Arc::clone(&tracker);
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    t.increment_processed();
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(tracker.get_progress().processed_files, 1000);
    }

    #[test]
    fn test_progress_snapshot_estimation() {
        let tracker = ProgressTracker::new(100);

        for _ in 0..10 {
            tracker.increment_processed();
        }

        thread::sleep(Duration::from_millis(100));

        let snapshot = tracker.get_progress();
        assert!(snapshot.estimated_remaining().is_some());

        let remaining = snapshot.estimated_remaining().unwrap();
        assert!(remaining.as_secs_f64() > snapshot.elapsed.as_secs_f64() * 5.0);
    }

    #[test]
    fn test_progress_snapshot_formatting() {
        let tracker = ProgressTracker::new(100);
        tracker.increment_processed();
        tracker.set_current_file(std::path::Path::new("src/main.rs"));

        let snapshot = tracker.get_progress();
        let formatted = snapshot.format_progress();

        assert!(formatted.contains("1.0%"));
        assert!(formatted.contains("1/100"));
        assert!(formatted.contains("main.rs"));
    }

    #[test]
    fn test_progress_is_complete() {
        let tracker = ProgressTracker::new(10);

        assert!(!tracker.get_progress().is_complete());

        for _ in 0..10 {
            tracker.increment_processed();
        }

        assert!(tracker.get_progress().is_complete());
    }

    #[test]
    fn test_progress_with_errors() {
        let tracker = ProgressTracker::new(10);

        for _ in 0..8 {
            tracker.increment_processed();
        }

        for _ in 0..2 {
            tracker.increment_error();
        }

        let snapshot = tracker.get_progress();
        assert!(snapshot.is_complete());
        assert_eq!(snapshot.error_count, 2);
    }

    #[test]
    fn test_reset() {
        let tracker = ProgressTracker::new(100);

        tracker.increment_scanned();
        tracker.increment_processed();
        tracker.increment_error();

        tracker.reset();

        let snapshot = tracker.get_progress();
        assert_eq!(snapshot.scanned_files, 0);
        assert_eq!(snapshot.processed_files, 0);
        assert_eq!(snapshot.error_count, 0);
        assert_eq!(snapshot.total_files, 100);
    }

    #[test]
    fn test_progress_tracker_default() {
        let tracker = ProgressTracker::default();
        let progress = tracker.get_progress();

        assert_eq!(progress.total_files, 0);
        assert_eq!(progress.process_percentage(), 0.0);
    }

    #[test]
    fn test_progress_tracker_set_total() {
        let tracker = ProgressTracker::new(50);
        tracker.set_total(100);

        let progress = tracker.get_progress();
        assert_eq!(progress.total_files, 100);
    }

    #[test]
    fn test_progress_tracker_error_handling() {
        let tracker = ProgressTracker::new(10);

        for _ in 0..5 {
            tracker.increment_processed();
        }

        for _ in 0..3 {
            tracker.increment_error();
        }

        let progress = tracker.get_progress();
        assert_eq!(progress.processed_files, 5);
        assert_eq!(progress.error_count, 3);
        assert!(!progress.is_complete());
    }

    #[test]
    fn test_progress_tracker_completion_with_errors() {
        let tracker = ProgressTracker::new(10);

        for _ in 0..7 {
            tracker.increment_processed();
        }

        for _ in 0..3 {
            tracker.increment_error();
        }

        let progress = tracker.get_progress();
        assert!(progress.is_complete());
    }

    #[test]
    fn test_progress_snapshot_scan_percentage() {
        let snapshot = ProgressSnapshot {
            total_files: 100,
            scanned_files: 50,
            processed_files: 25,
            error_count: 0,
            current_file: None,
            elapsed: Duration::from_secs(10),
        };

        assert_eq!(snapshot.scan_percentage(), 50.0);
        assert_eq!(snapshot.process_percentage(), 25.0);
    }

    #[test]
    fn test_progress_snapshot_zero_total() {
        let snapshot = ProgressSnapshot {
            total_files: 0,
            scanned_files: 0,
            processed_files: 0,
            error_count: 0,
            current_file: None,
            elapsed: Duration::from_secs(0),
        };

        assert_eq!(snapshot.scan_percentage(), 0.0);
        assert_eq!(snapshot.process_percentage(), 0.0);
        assert!(snapshot.estimated_remaining().is_none());
    }

    #[test]
    fn test_progress_snapshot_formatting_without_current_file() {
        let snapshot = ProgressSnapshot {
            total_files: 100,
            processed_files: 25,
            scanned_files: 50,
            error_count: 0,
            current_file: None,
            elapsed: Duration::from_secs(10),
        };

        let formatted = snapshot.format_progress();
        assert!(formatted.contains("25.0%"));
        assert!(formatted.contains("25/100"));
        assert!(!formatted.contains("-"));
    }

    #[test]
    fn test_progress_snapshot_estimation_edge_cases() {
        let snapshot = ProgressSnapshot {
            total_files: 100,
            processed_files: 0,
            scanned_files: 0,
            error_count: 0,
            current_file: None,
            elapsed: Duration::from_secs(10),
        };

        assert!(snapshot.estimated_remaining().is_none());

        let snapshot2 = ProgressSnapshot {
            total_files: 100,
            processed_files: 10,
            scanned_files: 20,
            error_count: 0,
            current_file: None,
            elapsed: Duration::from_secs(0),
        };

        assert!(snapshot2.estimated_remaining().is_none());
    }
}
