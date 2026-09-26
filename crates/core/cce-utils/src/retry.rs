//! Shared backoff schedule for transient storage-operation retries.

use std::time::{Duration, Instant};

/// Compute the backoff before the retry that follows failed attempt number
/// `attempt` (1-based).
///
/// The base delay doubles per attempt and receives random jitter of up to
/// `jitter_ratio` times the base so concurrent retriers do not synchronize.
/// Returns `None` once the attempt budget is exhausted or the next sleep
/// would cross `deadline` measured from `start`; the caller must then
/// surface the transient error instead of retrying again.
pub fn transient_backoff(
    start: Instant,
    attempt: u32,
    max_attempts: u32,
    initial_delay: Duration,
    deadline: Duration,
    jitter_ratio: f64,
) -> Option<Duration> {
    if attempt == 0 || attempt >= max_attempts {
        return None;
    }
    let base = initial_delay.saturating_mul(1 << (attempt - 1).min(16));
    let delay = base.saturating_add(base.mul_f64(fastrand::f64() * jitter_ratio.max(0.0)));
    if start.elapsed() + delay >= deadline {
        return None;
    }
    Some(delay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_zero_for_exhausted_attempts() {
        let start = Instant::now();
        assert_eq!(
            transient_backoff(
                start,
                2,
                2,
                Duration::from_millis(200),
                Duration::from_secs(10),
                0.3,
            ),
            None
        );
        assert_eq!(
            transient_backoff(
                start,
                3,
                2,
                Duration::from_millis(200),
                Duration::from_secs(10),
                0.3,
            ),
            None
        );
    }

    #[test]
    fn delay_grows_and_respects_deadline() {
        let start = Instant::now();
        let first = transient_backoff(
            start,
            1,
            5,
            Duration::from_millis(200),
            Duration::from_secs(10),
            0.0,
        )
        .expect("first attempt must schedule a retry");
        let second = transient_backoff(
            start,
            2,
            5,
            Duration::from_millis(200),
            Duration::from_secs(10),
            0.0,
        )
        .expect("second attempt must schedule a retry");
        assert_eq!(first, Duration::from_millis(200));
        assert_eq!(second, Duration::from_millis(400));

        assert_eq!(
            transient_backoff(
                start,
                1,
                5,
                Duration::from_millis(200),
                Duration::from_millis(100),
                0.0,
            ),
            None,
            "a delay crossing the total deadline must stop the retries"
        );
    }

    #[test]
    fn jitter_stays_within_ratio() {
        let start = Instant::now();
        for _ in 0..32 {
            let delay = transient_backoff(
                start,
                1,
                5,
                Duration::from_millis(200),
                Duration::from_secs(10),
                0.5,
            )
            .expect("jittered retry must be scheduled");
            assert!(delay >= Duration::from_millis(200) && delay < Duration::from_millis(300));
        }
    }
}
