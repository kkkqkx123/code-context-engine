//! Operation execution state tracking
//!
//! Provides real-time state tracking for operations including:
//! - Execution time and throughput
//! - Progress percentage and ETA
//! - File and batch processing statistics

use std::time::Instant;

/// Operation execution state
#[derive(Debug, Clone, Default)]
pub struct OperationState {
    /// Unique operation identifier
    pub operation_id: String,
    /// Operation type name
    pub operation_type: String,
    /// Operation start time
    pub start_time: Option<Instant>,
    /// Operation end time
    pub end_time: Option<Instant>,
    /// Total number of files to process
    pub total_files: usize,
    /// Number of files successfully processed
    pub processed_files: usize,
    /// Number of files with processing failures
    pub failed_files: usize,
    /// Current batch number
    pub current_batch: u32,
    /// Total number of batches
    pub total_batches: u32,
}

impl OperationState {
    /// Create new state tracker
    pub fn new(operation_id: String, operation_type: String) -> Self {
        Self {
            operation_id,
            operation_type,
            start_time: None,
            end_time: None,
            total_files: 0,
            processed_files: 0,
            failed_files: 0,
            current_batch: 0,
            total_batches: 0,
        }
    }

    /// Start operation timing
    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
    }

    /// End operation timing
    pub fn end(&mut self) {
        self.end_time = Some(Instant::now());
    }

    /// Get elapsed time in seconds
    pub fn elapsed_secs(&self) -> Option<f64> {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => Some(end.duration_since(start).as_secs_f64()),
            (Some(start), None) => Some(Instant::now().duration_since(start).as_secs_f64()),
            _ => None,
        }
    }

    /// Calculate files per second throughput
    pub fn throughput_files_per_sec(&self) -> Option<f64> {
        if let Some(elapsed) = self.elapsed_secs() {
            if elapsed > 0.0 {
                return Some(self.processed_files as f64 / elapsed);
            }
        }
        None
    }

    /// Calculate progress percentage (0.0-100.0)
    pub fn progress_percentage(&self) -> f32 {
        if self.total_files == 0 {
            0.0
        } else {
            (self.processed_files as f32 / self.total_files as f32) * 100.0
        }
    }

    /// Estimate remaining time in seconds
    pub fn estimated_remaining_secs(&self) -> Option<f64> {
        if self.total_files == 0 {
            return None;
        }

        let throughput = self.throughput_files_per_sec()?;
        if throughput <= 0.0 {
            return None;
        }

        let remaining = self.total_files - self.processed_files;
        if remaining == 0 {
            return Some(0.0);
        }

        Some((remaining as f64 / throughput).ceil())
    }

    /// Update progress with file processing
    pub fn record_file_processed(&mut self, success: bool) {
        if success {
            self.processed_files += 1;
        } else {
            self.failed_files += 1;
        }
    }

    /// Update batch progress
    pub fn set_batch_progress(&mut self, current: u32, total: u32) {
        self.current_batch = current;
        self.total_batches = total;
    }

    /// Generate readable state snapshot
    pub fn snapshot(&self) -> OperationStateSnapshot {
        let elapsed = self.elapsed_secs().unwrap_or(0.0);
        let progress_percent = self.progress_percentage();
        let throughput = self.throughput_files_per_sec();
        let eta_secs = self.estimated_remaining_secs();

        OperationStateSnapshot {
            operation_id: self.operation_id.clone(),
            operation_type: self.operation_type.clone(),
            elapsed_secs: elapsed,
            progress_percent,
            processed_files: self.processed_files,
            total_files: self.total_files,
            failed_files: self.failed_files,
            throughput_files_per_sec: throughput,
            eta_secs,
            current_batch: self.current_batch,
            total_batches: self.total_batches,
        }
    }
}

/// Snapshot of operation state at a point in time
#[derive(Debug, Clone)]
pub struct OperationStateSnapshot {
    /// Operation identifier
    pub operation_id: String,
    /// Operation type
    pub operation_type: String,
    /// Elapsed time in seconds
    pub elapsed_secs: f64,
    /// Progress percentage (0-100)
    pub progress_percent: f32,
    /// Files successfully processed
    pub processed_files: usize,
    /// Total files to process
    pub total_files: usize,
    /// Files with processing failures
    pub failed_files: usize,
    /// Files per second throughput
    pub throughput_files_per_sec: Option<f64>,
    /// Estimated remaining time in seconds
    pub eta_secs: Option<f64>,
    /// Current batch number
    pub current_batch: u32,
    /// Total number of batches
    pub total_batches: u32,
}

impl OperationStateSnapshot {
    /// Format metrics as human-readable string
    pub fn format_summary(&self) -> String {
        let eta_str = self
            .eta_secs
            .map(|e| format!("ETA: {:.0}s", e))
            .unwrap_or_else(|| "ETA: N/A".to_string());

        let throughput_str = self
            .throughput_files_per_sec
            .map(|t| format!("{:.2} files/sec", t))
            .unwrap_or_else(|| "N/A".to_string());

        format!(
            "Operation: {} ({})\n  Progress: {:.1}% ({}/{})\n  Duration: {:.1}s\n  Throughput: {}\n  Failed: {}\n  {}",
            self.operation_id,
            self.operation_type,
            self.progress_percent,
            self.processed_files,
            self.total_files,
            self.elapsed_secs,
            throughput_str,
            self.failed_files,
            eta_str
        )
    }

    /// Format metrics with batch information
    pub fn format_with_batch(&self) -> String {
        let mut summary = self.format_summary();
        if self.total_batches > 0 {
            summary.push_str(&format!(
                "\n  Batch: {}/{}",
                self.current_batch, self.total_batches
            ));
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_creation() {
        let state = OperationState::new("op1".to_string(), "full_index".to_string());
        assert_eq!(state.operation_id, "op1");
        assert_eq!(state.operation_type, "full_index");
        assert_eq!(state.progress_percentage(), 0.0);
    }

    #[test]
    fn test_progress_calculation() {
        let mut state = OperationState::new("op1".to_string(), "full_index".to_string());
        state.total_files = 100;
        state.processed_files = 50;

        assert_eq!(state.progress_percentage(), 50.0);
    }

    #[test]
    fn test_zero_files_progress() {
        let state = OperationState::new("op1".to_string(), "full_index".to_string());
        assert_eq!(state.progress_percentage(), 0.0);
    }
}
