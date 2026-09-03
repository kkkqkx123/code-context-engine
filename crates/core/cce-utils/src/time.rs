//! Time utility functions for consistent timestamp handling
//!
//! This module provides unified time-related utilities to ensure:
//! - Consistent timestamp precision (milliseconds)
//! - Optimized performance using SystemTime
//! - Proper error handling

use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Time-related errors
#[derive(Error, Debug)]
pub enum TimeError {
    #[error("System time is before UNIX epoch")]
    TimeBeforeEpoch,
}

/// Get current timestamp in milliseconds since UNIX epoch
///
/// This is the preferred function for obtaining timestamps for:
/// - Database records
/// - Metrics collection
/// - API responses
///
/// # Performance
///
/// Uses `SystemTime` instead of `chrono` for better performance in hot paths.
///
/// # Returns
///
/// Current timestamp in milliseconds as `u64`. Returns 0 if system time is
/// before UNIX epoch (which should never happen in practice).
///
/// # Example
///
/// ```
/// use cce_core::utils::time::current_timestamp_ms;
///
/// let timestamp = current_timestamp_ms();
/// assert!(timestamp > 0);
/// ```
pub fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Get current timestamp in seconds since UNIX epoch
///
/// Use this when millisecond precision is not required.
///
/// # Returns
///
/// Current timestamp in seconds as `u64`. Returns 0 if system time is
/// before UNIX epoch.
pub fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Get current timestamp in milliseconds with error handling
///
/// Use this when you need to handle the rare case of system time being
/// before UNIX epoch.
///
/// # Errors
///
/// Returns `TimeError::TimeBeforeEpoch` if system time is before UNIX epoch.
///
/// # Example
///
/// ```
/// use cce_core::utils::time::current_timestamp_ms_checked;
///
/// match current_timestamp_ms_checked() {
///     Ok(ts) => println!("Current timestamp: {}", ts),
///     Err(e) => eprintln!("Time error: {}", e),
/// }
/// ```
pub fn current_timestamp_ms_checked() -> Result<u64, TimeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .map_err(|_| TimeError::TimeBeforeEpoch)
}

/// Get current timestamp in seconds with error handling
///
/// # Errors
///
/// Returns `TimeError::TimeBeforeEpoch` if system time is before UNIX epoch.
pub fn current_timestamp_secs_checked() -> Result<u64, TimeError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| TimeError::TimeBeforeEpoch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_timestamp_ms() {
        let ts = current_timestamp_ms();
        assert!(ts > 0);
        // Should be reasonable value (after year 2020)
        assert!(ts > 1577836800000); // Jan 1, 2020 in ms
    }

    #[test]
    fn test_current_timestamp_secs() {
        let ts = current_timestamp_secs();
        assert!(ts > 0);
        // Should be reasonable value (after year 2020)
        assert!(ts > 1577836800); // Jan 1, 2020 in seconds
    }

    #[test]
    fn test_current_timestamp_ms_checked() {
        let result = current_timestamp_ms_checked();
        assert!(result.is_ok());
        let ts = result.unwrap();
        assert!(ts > 1577836800000);
    }

    #[test]
    fn test_timestamp_consistency() {
        let ms = current_timestamp_ms();
        let secs = current_timestamp_secs();
        // Milliseconds should be approximately seconds * 1000
        let expected_ms = secs * 1000;
        // Allow up to 1 second difference due to timing
        assert!(ms >= expected_ms && ms < expected_ms + 1000);
    }
}
