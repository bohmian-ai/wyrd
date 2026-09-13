//! Registration idempotency-key ownership.

use uuid::Uuid;

/// Mint one opaque key for one logical registration attempt.
#[must_use]
pub(crate) fn mint() -> String {
    format!("card-registration-{}", Uuid::now_v7())
}
