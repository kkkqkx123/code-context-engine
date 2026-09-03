//! Utility functions for SQLite repository operations.
//!
//! This module provides common utility functions used across different
//! repository implementations.

use chrono::Utc;

/// Get the current Unix timestamp.
///
/// This function returns the current time as a Unix timestamp (seconds since epoch).
/// It's commonly used for setting created_at and updated_at timestamps.
pub fn current_timestamp() -> i64 {
    Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_timestamp() {
        let now = current_timestamp();
        assert!(now > 0);

        let chrono_now = Utc::now().timestamp();
        assert!((chrono_now - now).abs() <= 1);
    }
}
