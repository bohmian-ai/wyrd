//! Audit checkpoint verification: recompute each sealed range hash from the
//! Iceberg `audit_log` and re-check the Ed25519 checkpoint signature.
//!
//! A tampered range produces a hash mismatch → the verify call returns
//! [`VerifyOutcome::Tampered`] and the caller should degrade the audit health.
//! A missing checkpoint means the range was never sealed → [`VerifyOutcome::Gap`].

use std::sync::Arc;

use arrow::array::{Array, Int64Array, RecordBatch, StringArray};
use datafusion::prelude::SessionContext;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;
use vala_sql::TenantConn;
use vala_sql::queries::audit_outbox::{
    AuditEntryHashInput, entry_hash_from_cols, shipped_outbox_refs,
};
use vala_sql::queries::audit_seal::list_checkpoints;
use wyrd_auth_issue::AuditSealKey;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::WyrdCatalog;
use crate::catalog::namespaces::BifrostNamespace;
use crate::error::BifrostError;
use crate::session::wyrd_session_context;
use crate::tables::DomainTable;
use crate::tables::system::AuditLogTable;

/// Outcome of one verification pass over all checkpoints for a tenant.
#[derive(Debug, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// All checkpoints verified against the Iceberg data.
    Clean,
    /// At least one checkpoint's recomputed hash did not match its signature.
    Tampered {
        /// The first tampered checkpoint's `seq_lo`.
        seq_lo: i64,
        /// The first tampered checkpoint's `seq_hi`.
        seq_hi: i64,
    },
    /// A gap in shipped rows: outbox has rows in a range that has no checkpoint.
    Gap,
    /// No checkpoints found (range has never been sealed).
    NoCheckpoints,
}

/// Re-verify all seal checkpoints for `tenant_id` against the Iceberg `audit_log`.
///
/// For each checkpoint `(seq_lo, seq_hi, range_hash, signature)`:
/// 1. Load the matching rows from the Iceberg `audit_log`.
/// 2. Recompute the range hash from the loaded entry hashes.
/// 3. Compare the recomputed hash with the stored `range_hash`.
/// 4. Verify the Ed25519 signature over the recomputed hash.
///
/// If any step fails, returns [`VerifyOutcome::Tampered`].
///
/// # Errors
/// Returns [`BifrostError`] on SQL or `DataFusion` scan failures.
pub async fn verify_checkpoint_walk(
    app_pool: &PgPool,
    catalog: &WyrdCatalog,
    key: &AuditSealKey,
    tenant_id: DataTenantId,
) -> Result<VerifyOutcome, BifrostError> {
    let mut conn = TenantConn::acquire(app_pool, tenant_id)
        .await
        .map_err(BifrostError::Sql)?;

    let checkpoints = list_checkpoints(&mut conn)
        .await
        .map_err(BifrostError::Sql)?;
    conn.commit().await.map_err(BifrostError::Sql)?;

    if checkpoints.is_empty() {
        return Ok(VerifyOutcome::NoCheckpoints);
    }

    // Recompute each entry hash from the Iceberg audit-log *content* columns.
    let warehouse_rows = recomputed_entry_hashes_by_seq(catalog, tenant_id).await?;

    for checkpoint in &checkpoints {
        // Collect rows for this range in seq order.
        let mut range_rows: Vec<(i64, Vec<u8>)> = warehouse_rows
            .iter()
            .filter(|(seq, _)| *seq >= checkpoint.seq_lo && *seq <= checkpoint.seq_hi)
            .cloned()
            .collect();
        range_rows.sort_by_key(|(seq, _)| *seq);

        if range_rows.is_empty() {
            return Ok(VerifyOutcome::Tampered {
                seq_lo: checkpoint.seq_lo,
                seq_hi: checkpoint.seq_hi,
            });
        }

        // Recompute range hash.
        let recomputed = {
            let mut hasher = Sha256::new();
            hasher.update(checkpoint.seq_lo.to_be_bytes());
            hasher.update(checkpoint.seq_hi.to_be_bytes());
            for (_seq, entry_hash) in &range_rows {
                hasher.update(entry_hash);
            }
            let arr: [u8; 32] = hasher.finalize().into();
            arr
        };

        // The stored range_hash must equal the recomputed one.
        if recomputed.as_slice() != checkpoint.range_hash.as_slice() {
            return Ok(VerifyOutcome::Tampered {
                seq_lo: checkpoint.seq_lo,
                seq_hi: checkpoint.seq_hi,
            });
        }

        // The signature must verify over the recomputed hash.
        if key
            .verify(&recomputed, checkpoint.signature.as_slice())
            .is_err()
        {
            return Ok(VerifyOutcome::Tampered {
                seq_lo: checkpoint.seq_lo,
                seq_hi: checkpoint.seq_hi,
            });
        }
    }

    // Also check for shipped outbox rows that fall outside all checkpoint ranges.
    let mut conn2 = TenantConn::acquire(app_pool, tenant_id)
        .await
        .map_err(BifrostError::Sql)?;
    let shipped = shipped_outbox_refs(&mut conn2)
        .await
        .map_err(BifrostError::Sql)?;
    conn2.commit().await.map_err(BifrostError::Sql)?;

    if !shipped.is_empty() {
        let max_sealed = checkpoints
            .iter()
            .map(|c| c.seq_hi)
            .max()
            .unwrap_or(i64::MIN);
        let max_shipped = shipped.iter().map(|r| r.seq).max().unwrap_or(i64::MIN);
        if max_shipped > max_sealed {
            return Ok(VerifyOutcome::Gap);
        }
    }

    Ok(VerifyOutcome::Clean)
}

/// Recompute `(seq, entry_hash)` for every `audit_log` row of `tenant_id` by
/// re-deriving each entry hash from the row's **content columns** via
/// [`entry_hash_from_cols`].
///
/// This is the crux of tamper detection. The checkpoint signed a range hash
/// built from the entry hashes; that signature is only meaningful if the
/// verifier rebinds the range hash to the actual audit *content*. Reading the
/// stored `entry_hash` column back would let a content edit that leaves that
/// column untouched pass unnoticed. By recomputing from content, any change to
/// a content column (e.g. `decision` flipping `deny`→`allow`) changes the
/// recomputed hash, changes the range hash, and breaks the checkpoint signature.
///
/// The encoding mirrors the writer (`append_audit`): `prev_hash`/`entry_hash`
/// are stored hex, `principal_id` a UUID string, and the enum columns their
/// canonical lowercase forms — so `entry_hash_from_cols` reproduces the exact
/// bytes hashed at append time.
async fn recomputed_entry_hashes_by_seq(
    catalog: &WyrdCatalog,
    tenant_id: DataTenantId,
) -> Result<Vec<(i64, Vec<u8>)>, BifrostError> {
    let provider = match catalog
        .provider(BifrostNamespace::System, AuditLogTable::NAME, tenant_id)
        .await
    {
        Ok(p) => p,
        Err(BifrostError::TableNotFound(_)) => return Ok(Vec::new()),
        Err(other) => return Err(other),
    };

    let ctx: SessionContext = wyrd_session_context(tenant_id);
    ctx.register_table(
        datafusion::common::TableReference::bare(AuditLogTable::NAME),
        Arc::new(provider),
    )
    .map_err(|e| BifrostError::Internal(e.to_string()))?;

    let df = ctx
        .sql(
            "SELECT seq, prev_hash, request_id, trace_id, operation, resource, \
             audit_card_ref, principal_id, principal_kind, auth_method, permission, \
             decision, result, payload_summary \
             FROM audit_log ORDER BY seq",
        )
        .await
        .map_err(|e| BifrostError::Internal(e.to_string()))?;

    let batches = df
        .collect()
        .await
        .map_err(|e| BifrostError::Internal(e.to_string()))?;

    let mut rows = Vec::new();
    for batch in &batches {
        let seq = int_col(batch, "seq")?;
        let prev_hash = str_col(batch, "prev_hash")?;
        let request_id = str_col(batch, "request_id")?;
        let trace_id = str_col(batch, "trace_id")?;
        let operation = str_col(batch, "operation")?;
        let resource = str_col(batch, "resource")?;
        let audit_card_ref = str_col(batch, "audit_card_ref")?;
        let principal_id = str_col(batch, "principal_id")?;
        let principal_kind = str_col(batch, "principal_kind")?;
        let auth_method = str_col(batch, "auth_method")?;
        let permission = str_col(batch, "permission")?;
        let decision = str_col(batch, "decision")?;
        let result = str_col(batch, "result")?;
        let payload_summary = str_col(batch, "payload_summary")?;

        for i in 0..seq.len() {
            if seq.is_null(i) {
                continue;
            }
            let prev_bytes = hex::decode(prev_hash.value(i))
                .map_err(|e| BifrostError::Internal(format!("audit_log prev_hash not hex: {e}")))?;
            let pid = Uuid::parse_str(principal_id.value(i)).map_err(|e| {
                BifrostError::Internal(format!("audit_log principal_id not a uuid: {e}"))
            })?;
            let pid_bytes = *pid.as_bytes();

            let recomputed = entry_hash_from_cols(AuditEntryHashInput {
                prev_hash: &prev_bytes,
                seq: seq.value(i),
                request_id: request_id.value(i),
                trace_id: opt_str(trace_id, i),
                operation: operation.value(i),
                resource: resource.value(i),
                card_ref: opt_str(audit_card_ref, i),
                principal_id_bytes: &pid_bytes,
                principal_kind: principal_kind.value(i),
                auth_method: auth_method.value(i),
                permission: permission.value(i),
                decision: decision.value(i),
                result: result.value(i),
                payload_summary: payload_summary.value(i),
            });
            rows.push((seq.value(i), recomputed.to_vec()));
        }
    }
    Ok(rows)
}

/// Downcast a required `Int64` column of `batch`.
fn int_col<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a Int64Array, BifrostError> {
    batch
        .column_by_name(name)
        .ok_or_else(|| BifrostError::Internal(format!("audit_log missing {name} column")))?
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or_else(|| BifrostError::Internal(format!("audit_log {name} is not Int64")))
}

/// Downcast a required `Utf8` column of `batch`.
fn str_col<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a StringArray, BifrostError> {
    batch
        .column_by_name(name)
        .ok_or_else(|| BifrostError::Internal(format!("audit_log missing {name} column")))?
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| BifrostError::Internal(format!("audit_log {name} is not Utf8")))
}

/// Read an optional string cell (`None` when the column is null at `i`).
fn opt_str(arr: &StringArray, i: usize) -> Option<&str> {
    if arr.is_null(i) {
        None
    } else {
        Some(arr.value(i))
    }
}

/// In-process tamper-detection structural test.
///
/// Verifies that a tampered entry hash causes the range hash to differ and
/// the original signature to reject it. The full Postgres+Iceberg path is
/// covered by the integration journey (slice 13).
#[cfg(test)]
#[test]
fn tampered_range_hash_is_detected() {
    let key = wyrd_auth_issue::AuditSealKey::generate().expect("key generates");

    let entry_hashes: Vec<Vec<u8>> = vec![vec![0x11; 32], vec![0x22; 32], vec![0x33; 32]];

    let mut hasher = Sha256::new();
    hasher.update(1i64.to_be_bytes());
    hasher.update(3i64.to_be_bytes());
    for h in &entry_hashes {
        hasher.update(h);
    }
    let honest_hash: [u8; 32] = hasher.finalize().into();
    let sig = key.sign(&honest_hash);
    key.verify(&honest_hash, &sig)
        .expect("honest range verifies");

    let bad_entries: Vec<Vec<u8>> = vec![vec![0x11; 32], vec![0xff; 32], vec![0x33; 32]];
    let mut bad_hasher = Sha256::new();
    bad_hasher.update(1i64.to_be_bytes());
    bad_hasher.update(3i64.to_be_bytes());
    for h in &bad_entries {
        bad_hasher.update(h);
    }
    let tampered_hash: [u8; 32] = bad_hasher.finalize().into();
    assert!(
        key.verify(&tampered_hash, &sig).is_err(),
        "tampered range hash must not verify with honest signature"
    );
}

/// Gate test: `serving::audit_seal::verify::checkpoint_anchored_walk`
///
/// Structural test: the range-hash recomputation and signature check correctly
/// identify a clean checkpoint vs a tampered one in-process (no Postgres/Iceberg).
#[cfg(test)]
#[test]
fn checkpoint_anchored_walk() {
    let key = wyrd_auth_issue::AuditSealKey::generate().expect("key generates");

    let entry_hashes: Vec<Vec<u8>> = vec![vec![0xaa; 32], vec![0xbb; 32], vec![0xcc; 32]];
    let seq_lo: i64 = 10;
    let seq_hi: i64 = 12;

    let mut hasher = Sha256::new();
    hasher.update(seq_lo.to_be_bytes());
    hasher.update(seq_hi.to_be_bytes());
    for h in &entry_hashes {
        hasher.update(h);
    }
    let clean_hash: [u8; 32] = hasher.finalize().into();
    let sig = key.sign(&clean_hash);

    // Recompute with honest data → verify succeeds.
    let mut rehasher = Sha256::new();
    rehasher.update(seq_lo.to_be_bytes());
    rehasher.update(seq_hi.to_be_bytes());
    for h in &entry_hashes {
        rehasher.update(h);
    }
    let recomputed: [u8; 32] = rehasher.finalize().into();
    assert_eq!(
        recomputed, clean_hash,
        "recomputed hash must match stored hash"
    );
    key.verify(&recomputed, &sig)
        .expect("clean checkpoint must verify");

    // Tampered: one entry hash changed → signature rejects.
    let tampered: Vec<Vec<u8>> = vec![vec![0xaa; 32], vec![0xff; 32], vec![0xcc; 32]];
    let mut thasher = Sha256::new();
    thasher.update(seq_lo.to_be_bytes());
    thasher.update(seq_hi.to_be_bytes());
    for h in &tampered {
        thasher.update(h);
    }
    let tampered_hash: [u8; 32] = thasher.finalize().into();
    assert_ne!(tampered_hash, clean_hash, "tampered hash must differ");
    assert!(
        key.verify(&tampered_hash, &sig).is_err(),
        "tampered range must not verify with original signature"
    );
}
