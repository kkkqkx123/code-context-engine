//! Listener execution priority levels
//!
//! Defines well-known priority values for event listeners.
//! Lower values execute first.

/// System-critical state updates (must execute first)
pub const SYSTEM_STATE_UPDATE: u32 = 1;

/// Metrics collection and observability
pub const METRICS_COLLECTION: u32 = 2;

/// Standard processing and business logic
pub const STANDARD: u32 = 3;

/// Logging and diagnostics
pub const LOGGING: u32 = 4;

/// External notifications and side effects (must execute last)
pub const NOTIFICATION: u32 = 5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        const _: () = {
            assert!(SYSTEM_STATE_UPDATE < METRICS_COLLECTION);
            assert!(METRICS_COLLECTION < STANDARD);
            assert!(STANDARD < LOGGING);
            assert!(LOGGING < NOTIFICATION);
        };
    }
}
