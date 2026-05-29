//! Retry policy and `Retry-After` parsing for provider HTTP calls.

use std::time::{Duration, SystemTime};

use rand::Rng;
use reqwest::StatusCode;

/// Jittered exponential retry policy for provider calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum number of send attempts, including the initial attempt.
    pub max_attempts: u32,
    /// Base exponential backoff.
    pub initial_backoff: Duration,
    /// Maximum computed backoff.
    pub max_backoff: Duration,
    /// Whether to apply full jitter to computed waits.
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(2),
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// Creates a policy with jitter disabled for deterministic tests.
    pub fn without_jitter(mut self) -> Self {
        self.jitter = false;
        self
    }

    /// Returns whether another attempt should be made after `attempt_index`.
    pub const fn should_retry(&self, attempt_index: u32, status: StatusCode) -> bool {
        attempt_index + 1 < self.max_attempts && is_retryable_status(status)
    }

    /// Computes backoff for an attempt index, optionally honoring `Retry-After`.
    pub fn wait_for_attempt(
        &self,
        attempt_index: u32,
        retry_after: Option<&str>,
        now: SystemTime,
    ) -> Duration {
        if attempt_index == 0 {
            return Duration::ZERO;
        }
        if let Some(value) = retry_after.and_then(|value| parse_retry_after(value, now)) {
            return value.min(self.max_backoff);
        }

        let exponent = attempt_index.saturating_sub(1).min(16);
        let multiplier = 1_u32.checked_shl(exponent).unwrap_or(u32::MAX);
        let base = self.initial_backoff.saturating_mul(multiplier);
        let capped = base.min(self.max_backoff);
        if !self.jitter || capped.is_zero() {
            return capped;
        }

        let max_ms = capped.as_millis();
        let jitter_ms = rand::rng().random_range(0..=max_ms);
        let millis = u64::try_from(jitter_ms).unwrap_or(u64::MAX);
        Duration::from_millis(millis)
    }
}

/// Returns whether a status is retryable for provider calls.
pub const fn is_retryable_status(status: StatusCode) -> bool {
    status.as_u16() == 408 || status.as_u16() == 429 || status.as_u16() >= 500
}

/// Parses `Retry-After` as seconds or an HTTP date relative to `now`.
pub fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    let date = httpdate::parse_http_date(value.trim()).ok()?;
    match date.duration_since(now) {
        Ok(duration) => Some(duration),
        Err(_) => Some(Duration::ZERO),
    }
}
