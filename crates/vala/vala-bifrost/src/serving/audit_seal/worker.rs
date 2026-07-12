//! Audit-seal worker: seal a contiguous `[seq_lo, seq_hi]` range of shipped
//! audit rows.
//!
//! Sealing is idempotent under the per-tenant seal lease: if two pods race, at
//! most one will commit the checkpoint; the other will find the `ON CONFLICT DO
//! NOTHING` upsert a no-op and exit cleanly.
//!
//! The range hash is computed by hashing the ordered entry hashes from the
//! shipped outbox rows, then signed with the dedicated `AuditSealKey`. The
//! resulting `(seq_lo, seq_hi, range_hash, signature)` is persisted in
//! `vala.audit_seal_checkpoints`.
//!
//! Pruning the sealed+shipped outbox rows is a separate step (a role-scoped
//! DELETE under the `vala_audit_seal` role) and is not performed here yet;
//! sealing establishes the tamper-evident checkpoint that lets the pruned
//! history be re-verified from the Iceberg `audit_log`.

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use vala_sql::TenantConn;
use vala_sql::queries::audit_outbox::shipped_outbox_refs;
use vala_sql::queries::audit_seal::upsert_checkpoint;
use wyrd_auth_issue::AuditSealKey;
use wyrd_spec::ids::DataTenantId;

use crate::error::BifrostError;

/// Tick interval for the audit-seal worker (60 s).
pub const SEAL_TICK_INTERVAL_SECS: u64 = 60;

/// Outcome of one seal pass.
#[derive(Debug, Default)]
pub struct SealOutcome {
    /// Number of rows included in the sealed range.
    pub rows_sealed: usize,
    /// First `seq` in the sealed range; `None` when nothing to seal.
    pub seq_lo: Option<i64>,
    /// Last `seq` in the sealed range; `None` when nothing to seal.
    pub seq_hi: Option<i64>,
    /// Whether the seal was skipped (no shipped rows, or already sealed).
    pub skipped: bool,
}

/// Seal all shipped audit rows for `tenant_id` into one signed checkpoint.
///
/// Steps:
/// 1. Fetch all shipped outbox `(seq, entry_hash)` pairs.
/// 2. Compute `range_hash = SHA256(seq_lo BE || seq_hi BE || concat(entry_hashes in seq order))`.
/// 3. Sign `range_hash` with `key`.
/// 4. Persist the checkpoint via idempotent upsert.
///
/// Returns [`SealOutcome`] describing what was sealed.
///
/// # Panics
/// Panics if `refs` is somehow non-empty but `.first()` or `.last()` returns `None`
/// — an invariant that cannot occur when `refs.is_empty()` is checked above.
///
/// # Errors
/// Returns [`BifrostError`] when any SQL query or signing operation fails.
pub async fn seal_shipped_range(
    app_pool: &PgPool,
    key: &AuditSealKey,
    tenant_id: DataTenantId,
) -> Result<SealOutcome, BifrostError> {
    let mut conn = TenantConn::acquire(app_pool, tenant_id)
        .await
        .map_err(BifrostError::Sql)?;

    let refs = shipped_outbox_refs(&mut conn)
        .await
        .map_err(BifrostError::Sql)?;
    conn.commit().await.map_err(BifrostError::Sql)?;

    if refs.is_empty() {
        return Ok(SealOutcome {
            skipped: true,
            ..Default::default()
        });
    }

    let seq_lo = refs.first().expect("non-empty").seq;
    let seq_hi = refs.last().expect("non-empty").seq;
    let rows_sealed = refs.len();

    let range_hash = compute_range_hash(seq_lo, seq_hi, &refs);
    let signature = key.sign(&range_hash);

    let mut conn2 = TenantConn::acquire(app_pool, tenant_id)
        .await
        .map_err(BifrostError::Sql)?;
    upsert_checkpoint(&mut conn2, seq_lo, seq_hi, &range_hash, &signature)
        .await
        .map_err(BifrostError::Sql)?;
    conn2.commit().await.map_err(BifrostError::Sql)?;

    Ok(SealOutcome {
        rows_sealed,
        seq_lo: Some(seq_lo),
        seq_hi: Some(seq_hi),
        skipped: false,
    })
}

/// Compute the range hash for `[seq_lo, seq_hi]` over the given outbox refs.
///
/// `range_hash = SHA256(seq_lo_be8 || seq_hi_be8 || entry_hash[0] || ... || entry_hash[n-1])`
///
/// The `seq_lo`/`seq_hi` bounds are included in the hash so a different range
/// of entry hashes does not accidentally collide.
#[must_use]
pub fn compute_range_hash(
    seq_lo: i64,
    seq_hi: i64,
    refs: &[vala_sql::queries::audit_outbox::ShippedOutboxRef],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(seq_lo.to_be_bytes());
    hasher.update(seq_hi.to_be_bytes());
    for r in refs {
        hasher.update(&r.entry_hash);
    }
    hasher.finalize().into()
}

#[cfg(test)]
use vala_sql::queries::audit_outbox::ShippedOutboxRef;

/// Gate test: `serving::audit_seal::worker::seal_then_verify`
///
/// Verifies that `compute_range_hash` + `AuditSealKey::sign/verify` round-trip
/// correctly, and that tampering an entry hash or seq bounds breaks the signature.
#[cfg(test)]
#[test]
fn seal_then_verify() {
    let key = AuditSealKey::generate().expect("key generates");

    let refs = vec![
        ShippedOutboxRef {
            seq: 1,
            entry_hash: vec![0xaa; 32],
        },
        ShippedOutboxRef {
            seq: 2,
            entry_hash: vec![0xbb; 32],
        },
    ];
    let range_hash = compute_range_hash(1, 2, &refs);
    let sig = key.sign(&range_hash);

    // Round-trip: verify with same key succeeds.
    key.verify(&range_hash, &sig)
        .expect("valid seal signature must verify");

    // Tampered range (different entry hash for seq 2) must fail.
    let tampered = vec![
        ShippedOutboxRef {
            seq: 1,
            entry_hash: vec![0xaa; 32],
        },
        ShippedOutboxRef {
            seq: 2,
            entry_hash: vec![0xcc; 32], // changed
        },
    ];
    let tampered_hash = compute_range_hash(1, 2, &tampered);
    assert!(
        key.verify(&tampered_hash, &sig).is_err(),
        "tampered entry hash must not verify"
    );

    // Different seq bounds also change the hash.
    let wrong_bounds_hash = compute_range_hash(1, 3, &refs);
    assert_ne!(
        wrong_bounds_hash, range_hash,
        "different seq_hi changes the range hash"
    );
}
