use std::time::Duration;

/// Policy for retrying failed operations.
pub trait Retry: Send + Sync {
    fn max_retries(&self) -> u32;
    fn delay_for_attempt(&self, attempt: u32) -> Duration;
    fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_retries()
    }
}

/// Exponential backoff with a cap.
#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    max_retries: u32,
    base_delay: Duration,
    max_delay: Duration,
}

impl ExponentialBackoff {
    pub fn new(max_retries: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_retries,
            base_delay,
            max_delay,
        }
    }
}

impl Retry for ExponentialBackoff {
    fn max_retries(&self) -> u32 {
        self.max_retries
    }

    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let multiplier = 2u64.saturating_pow(attempt);
        let delay = self.base_delay.saturating_mul(multiplier as u32);
        delay.min(self.max_delay)
    }
}

/// No retry — fail immediately.
#[derive(Debug, Clone)]
pub struct NoRetry;

impl Retry for NoRetry {
    fn max_retries(&self) -> u32 {
        0
    }
    fn delay_for_attempt(&self, _: u32) -> Duration {
        Duration::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_backoff_delays() {
        let policy = ExponentialBackoff::new(3, Duration::from_millis(100), Duration::from_secs(5));
        assert_eq!(policy.max_retries(), 3);
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(400));
        // Capped at max
        assert!(policy.delay_for_attempt(100) <= Duration::from_secs(5));
    }

    #[test]
    fn no_retry_policy() {
        let policy = NoRetry;
        assert_eq!(policy.max_retries(), 0);
        assert!(!policy.should_retry(0));
    }

    #[test]
    fn should_retry_returns_false_at_max() {
        let policy = ExponentialBackoff::new(3, Duration::from_millis(100), Duration::from_secs(5));
        assert!(policy.should_retry(0));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
    }
}
