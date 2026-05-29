use std::time::{Duration, SystemTime};

use reqwest::StatusCode;
use skald_providers::retry::{RetryPolicy, is_retryable_status, parse_retry_after};

#[test]
fn retry_policy_classifies_statuses() {
    assert!(is_retryable_status(StatusCode::REQUEST_TIMEOUT));
    assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
    assert!(is_retryable_status(StatusCode::INTERNAL_SERVER_ERROR));
    assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
}

#[test]
fn exponential_backoff_is_capped_without_jitter() {
    let policy = RetryPolicy {
        max_attempts: 4,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(25),
        jitter: false,
    };

    assert_eq!(
        policy.wait_for_attempt(0, None, SystemTime::now()),
        Duration::ZERO
    );
    assert_eq!(
        policy.wait_for_attempt(1, None, SystemTime::now()),
        Duration::from_millis(10)
    );
    assert_eq!(
        policy.wait_for_attempt(3, None, SystemTime::now()),
        Duration::from_millis(25)
    );
}

#[test]
fn retry_after_seconds_and_http_date_parse() {
    let now = SystemTime::now();

    assert_eq!(parse_retry_after("2", now), Some(Duration::from_secs(2)));
    assert!(parse_retry_after("Wed, 21 Oct 2099 07:28:00 GMT", now).is_some());
}
