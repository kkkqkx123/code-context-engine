use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tracing::warn;

use super::error::PluginError;

/// A handle that allows the caller to signal a running blocking
/// operation that it should be cancelled.
///
/// `execute_with_timeout_blocking` passes this token to the closure.
/// Long-running FFI calls should check `is_cancelled()` at safe points
/// and return `Err(PluginError::Timeout)` early when possible.
#[derive(Clone, Default)]
pub struct CancellationToken {
    inner: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Create a new token in the non-cancelled state.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Mark the token as cancelled.
    pub fn cancel(&self) {
        self.inner.store(true, Ordering::SeqCst);
    }

    /// Check whether `cancel` has been called.
    pub fn is_cancelled(&self) -> bool {
        self.inner.load(Ordering::SeqCst)
    }
}

/// Run `f` on a dedicated thread with a hard timeout.
///
/// The closure receives a [`CancellationToken`] that is set when the
/// caller has given up waiting. Plugins that perform long FFI work
/// should inspect the token and bail out promptly.
///
/// Note: the thread cannot be forcefully terminated; on timeout it
/// lingers until the FFI call returns naturally.
pub fn execute_with_timeout_blocking<F, T>(
    f: F,
    timeout_ms: u64,
    plugin_id: &str,
    operation: &str,
) -> Result<T, PluginError>
where
    F: FnOnce(&CancellationToken) -> Result<T, PluginError> + Send + 'static,
    T: Send + 'static,
{
    let token = CancellationToken::new();
    let token_for_thread = token.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = f(&token_for_thread);
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            token.cancel();
            warn!(
                "Plugin {} timed out during {} ({}ms)",
                plugin_id, operation, timeout_ms
            );
            Err(PluginError::Timeout)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(PluginError::ExecutionFailed(
            format!("Plugin {plugin_id} thread terminated unexpectedly during {operation}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cancellation_token_starts_uncancelled() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_cancel_takes_effect() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_clone_shares_state() {
        let token = CancellationToken::new();
        let clone = token.clone();
        assert!(!clone.is_cancelled());
        token.cancel();
        // The clone observes the cancellation (cross-thread visibility is
        // exercised by the timeout path below).
        assert!(clone.is_cancelled());
    }

    #[test]
    fn test_execute_with_timeout_blocking_completes() {
        let result = execute_with_timeout_blocking(
            |_token| Ok::<_, PluginError>(42),
            1000,
            "test-plugin",
            "test_op",
        );
        assert_eq!(result.expect("call succeeded"), 42);
    }

    #[test]
    fn test_execute_with_timeout_blocking_timeout_sets_token() {
        let start = std::time::Instant::now();
        let result: Result<(), PluginError> = execute_with_timeout_blocking(
            |token| {
                // Block until the caller gives up waiting; then observe the
                // cancellation token so a long-running plugin can bail out.
                while !token.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(PluginError::Timeout)
            },
            100,
            "test-plugin",
            "test_op",
        );
        assert!(matches!(result, Err(PluginError::Timeout)));
        assert!(start.elapsed() >= Duration::from_millis(90));
    }

    #[test]
    fn test_execute_with_timeout_blocking_disconnected_is_execution_failed() {
        let result: Result<(), PluginError> = execute_with_timeout_blocking(
            |_token| panic!("worker thread dies before sending a result"),
            1000,
            "test-plugin",
            "test_op",
        );
        match result {
            Err(PluginError::ExecutionFailed(msg)) => assert!(msg.contains("test-plugin")),
            other => panic!("expected ExecutionFailed, got: {other:?}"),
        }
    }

    #[test]
    fn test_execute_with_timeout_blocking_error_passthrough() {
        let result: Result<(), PluginError> = execute_with_timeout_blocking(
            |_token| Err(PluginError::LogicError("boom".to_string())),
            1000,
            "test-plugin",
            "test_op",
        );
        assert!(matches!(result, Err(PluginError::LogicError(_))));
    }
}
