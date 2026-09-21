//! Constant-cost secret verification shared by both control planes.
//!
//! Argon2 is deliberately expensive, which makes *skipping* it observable. Any
//! path that answers "this credential is invalid" before verifying leaks, by
//! clock, that the presented prefix names no live row — and a caller that can
//! measure that can enumerate live prefixes without ever guessing a secret.
//!
//! So both planes route every presented secret through [`verify_presented`],
//! which verifies against a fixed dummy when there is no row to verify against.
//! Rejecting then costs what accepting costs.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};

use secrecy::SecretString;
use tokio::task::JoinError;
use uuid::Uuid;
use wyrd_auth_issue::{hash_api_key, verify_api_key};

/// The verifier a presented secret is checked against when no row matches.
///
/// Derived once per process from throwaway randomness, so no presented secret
/// can match it and the cost of failing is the cost of succeeding. Computing it
/// lazily rather than per request matters: paying for a fresh Argon2 hash on
/// every rejection would be a denial-of-service amplifier rather than a timing
/// defence.
static DUMMY_VERIFIER: LazyLock<String> = LazyLock::new(|| {
    hash_api_key(&SecretString::from(Uuid::new_v4().to_string()))
        .expect("Argon2 hashing generated randomness cannot fail")
});

/// Count of verifications performed since process start.
///
/// Exists so a test can assert the invariant this module is for — that every
/// invalid condition performs exactly one verification — which is otherwise
/// only observable as a timing difference, and therefore only provable by the
/// flaky measurement the attack itself uses.
static VERIFICATIONS: AtomicU64 = AtomicU64::new(0);

/// Verify `presented` against `stored`, or against the dummy when absent.
///
/// Argon2 runs on the blocking pool: it is CPU-bound by design and would
/// otherwise stall the runtime worker handling the request.
///
/// Returns whether the secret matched. A `None` `stored` always returns
/// `false`, having spent the same work as a real comparison — so the caller
/// must still decide the refusal, and must not skip this call on the paths
/// where it already knows the answer.
///
/// # Errors
/// Returns the join error when the blocking task cannot be run to completion.
pub async fn verify_presented(
    presented: &SecretString,
    stored: Option<&str>,
) -> Result<bool, JoinError> {
    let verifier = stored.map_or_else(|| DUMMY_VERIFIER.clone(), ToOwned::to_owned);
    let candidate = presented.clone();
    let matched =
        tokio::task::spawn_blocking(move || verify_api_key(&candidate, &verifier)).await?;
    VERIFICATIONS.fetch_add(1, Ordering::Relaxed);
    Ok(matched && stored.is_some())
}

/// Read the process-wide verification count.
///
/// See `VERIFICATIONS`. Monotonic and process-wide, so a caller compares two
/// readings around one operation rather than expecting an absolute value.
#[must_use]
pub fn verifications_performed() -> u64 {
    VERIFICATIONS.load(Ordering::Relaxed)
}
