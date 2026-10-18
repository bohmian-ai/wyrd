//! Declaration idempotency-key ownership.

use uuid::Uuid;

/// Mint one opaque key for one logical declaration attempt.
#[must_use]
pub(crate) fn mint() -> String {
    format!("card-declaration-{}", Uuid::now_v7())
}
