//! Tenant-scoped writes and reads for the audit outbox:
//! `vala.audit_chain_head` and `vala.audit_staging`.
//!
//! `append_audit` runs in the audited operation's own [`TenantConn`]
//! transaction: it advances the per-tenant chain head under a `FOR UPDATE`
//! lock, computes the SHA256 entry hash in Rust (this module owns the canonical
//! encoding), inserts the append-only row, and bumps the head — all so the audit
//! row commits atomically with the operation it records.
// raw-query grep allowlist: audit outbox tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome, audit_detail_canonical_json};
use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::row_types::audit_staging::AuditStagingRow;

/// Append one hash-chained audit row for the current tenant, returning its `seq`.
///
/// Advances `vala.audit_chain_head` under `FOR UPDATE` so concurrent appends for
/// the same tenant serialize into a gapless sequence, then inserts the row into
/// `vala.audit_staging`. Runs inside the caller's transaction so the decision
/// record is durable exactly when — and only when — the authorization decision
/// that produced it is.
///
/// # Errors
/// Returns [`SqlError`] when any statement fails or an RLS policy rejects a row.
pub async fn append_audit(conn: &mut TenantConn<'_>, event: &AuditEvent) -> Result<i64, SqlError> {
    append_audit_connection(conn.transaction(), event).await
}

/// Implements canonical audit encoding for an already tenant-bound connection.
///
/// # Errors
/// Returns [`SqlError`] when chain locking, hashing persistence, or RLS fails.
async fn append_audit_connection(
    conn: &mut PgConnection,
    event: &AuditEvent,
) -> Result<i64, SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.audit_chain_head (data_tenant_id)
        VALUES (wyrd.current_tenant())
        ON CONFLICT (data_tenant_id) DO NOTHING
        "#,
    )
    .execute(&mut *conn)
    .await
    .map_err(SqlError::from)?;

    let (last_seq, prev_hash): (i64, Vec<u8>) = sqlx::query_as(
        r#"
        SELECT last_seq, head_hash
          FROM vala.audit_chain_head
         WHERE data_tenant_id = wyrd.current_tenant()
        FOR UPDATE
        "#,
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(SqlError::from)?;

    let seq = last_seq + 1;
    let card_ref = event.card_ref.as_ref().map(ToString::to_string);
    let detail = event.detail.as_ref().map(audit_detail_canonical_json);
    let entry_hash = entry_hash(
        &prev_hash,
        seq,
        event,
        card_ref.as_deref(),
        detail.as_deref(),
    );

    sqlx::query(
        r#"
        INSERT INTO vala.audit_staging
            (data_tenant_id, seq, prev_hash, entry_hash, request_id, trace_id,
             operation, resource, card_ref, principal_id, principal_kind,
             permission, outcome, detail)
        VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13)
        "#,
    )
    .bind(seq)
    .bind(prev_hash.as_slice())
    .bind(entry_hash.as_slice())
    .bind(event.request_id.as_str())
    .bind(event.trace_id.as_deref())
    .bind(event.operation.as_str())
    .bind(event.resource.as_str())
    .bind(card_ref.as_deref())
    .bind(event.principal_id.as_uuid())
    .bind(event.principal_kind.as_str())
    .bind(event.permission.as_str())
    .bind(outcome_str(event.outcome))
    .bind(detail.as_deref())
    .execute(&mut *conn)
    .await
    .map_err(SqlError::from)?;

    sqlx::query(
        r#"
        UPDATE vala.audit_chain_head
           SET last_seq = $1, head_hash = $2, updated_at = now()
         WHERE data_tenant_id = wyrd.current_tenant()
        "#,
    )
    .bind(seq)
    .bind(entry_hash.as_slice())
    .execute(&mut *conn)
    .await
    .map_err(SqlError::from)?;

    Ok(seq)
}

/// Append one audit row on a caller-provided tenant-scoped transaction.
///
/// The caller owns the [`TenantConn`] lifetime and transaction commit, so this
/// primitive can participate in the audited operation's transaction.
///
/// # Errors
/// Returns [`SqlError`] when appending the audit row fails.
pub async fn record_audit(conn: &mut TenantConn<'_>, event: &AuditEvent) -> Result<i64, SqlError> {
    append_audit(conn, event).await
}

/// Read a bounded page of audit rows for one tenant-bound resource.
///
/// The caller supplies the last observed sequence number. RLS remains the
/// tenant boundary; the explicit current-tenant predicate keeps the query
/// aligned with the covering `(data_tenant_id, resource, seq)` index.
///
/// # Errors
/// Returns [`SqlError`] when the page query fails.
pub async fn list_audit_events_for_resource(
    conn: &mut TenantConn<'_>,
    resource: &str,
    after_seq: i64,
    limit: i64,
) -> Result<Vec<AuditStagingRow>, SqlError> {
    sqlx::query_as::<_, AuditStagingRow>(
        r#"
        SELECT data_tenant_id, seq, entry_hash, prev_hash, request_id, trace_id,
               operation, resource, card_ref, principal_id, principal_kind,
               permission, outcome, detail, created_at
          FROM vala.audit_staging
         WHERE data_tenant_id = wyrd.current_tenant()
           AND resource = $1
           AND seq > $2
         ORDER BY seq
         LIMIT $3
        "#,
    )
    .bind(resource)
    .bind(after_seq)
    .bind(limit)
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Read the oldest bounded run of unpublished audit rows for the current tenant.
///
/// Retirement removes a published prefix, so the lowest surviving `seq` is
/// always the next event owed to `vala.system.audit_log`. The page is ordered
/// and bounded, which is exactly the contiguous shape
/// `project_audit_rows` validates before a shipment is built.
///
/// # Errors
/// Returns [`SqlError`] when the page query fails or RLS rejects the read.
pub async fn list_publication_batch(
    conn: &mut TenantConn<'_>,
    limit: i64,
) -> Result<Vec<AuditStagingRow>, SqlError> {
    sqlx::query_as::<_, AuditStagingRow>(
        r#"
        SELECT data_tenant_id, seq, entry_hash, prev_hash, request_id, trace_id,
               operation, resource, card_ref, principal_id, principal_kind,
               permission, outcome, detail, created_at
          FROM vala.audit_staging
         WHERE data_tenant_id = wyrd.current_tenant()
         ORDER BY seq
         LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Advance the tenant watermark to `seq_hi` and delete every staged row through
/// it, returning the rows actually drained.
///
/// The caller MUST have observed a durable `vala.system.audit_log` publication
/// for the whole range first. Both statements run in the caller's tenant
/// transaction, so the watermark never advances without the deletion and the
/// deletion never outruns the watermark. The watermark only ever moves forward,
/// which makes the call idempotent: a replay after an uncertain outcome removes
/// whatever survives and reports it, so a lost acknowledgement costs one
/// replayed shipment rather than a lost or duplicated audit event.
///
/// # Errors
/// Returns [`SqlError`] when the watermark update or the delete fails, or RLS
/// rejects the range.
pub async fn drain_through_watermark(
    conn: &mut TenantConn<'_>,
    seq_hi: i64,
) -> Result<u64, SqlError> {
    sqlx::query(
        r#"
        UPDATE vala.audit_chain_head
           SET published_seq = GREATEST(published_seq, $1), updated_at = now()
         WHERE data_tenant_id = wyrd.current_tenant()
        "#,
    )
    .bind(seq_hi)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    let result = sqlx::query(
        r#"
        DELETE FROM vala.audit_staging
         WHERE data_tenant_id = wyrd.current_tenant()
           AND seq <= (
               SELECT published_seq
                 FROM vala.audit_chain_head
                WHERE data_tenant_id = wyrd.current_tenant()
           )
        "#,
    )
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(result.rows_affected())
}

/// Compute `entry_hash = SHA256(canonical(prev_hash, seq, event))`.
///
/// The canonical encoding is a length-prefixed concatenation owned here, so the
/// chain is reproducible from the stored columns alone. `card_ref` is passed as
/// its already-canonicalized string to avoid recomputing it.
fn entry_hash(
    prev_hash: &[u8],
    seq: i64,
    event: &AuditEvent,
    card_ref: Option<&str>,
    detail: Option<&str>,
) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(prev_hash);
    buf.extend_from_slice(&seq.to_be_bytes());
    push_str(&mut buf, event.request_id.as_str());
    push_opt(&mut buf, event.trace_id.as_deref());
    push_str(&mut buf, &event.operation);
    push_str(&mut buf, &event.resource);
    push_opt(&mut buf, card_ref);
    buf.extend_from_slice(event.principal_id.as_uuid().as_bytes());
    push_str(&mut buf, event.principal_kind.as_str());
    push_str(&mut buf, &event.permission);
    push_str(&mut buf, outcome_str(event.outcome));
    push_opt(&mut buf, detail);
    Sha256::digest(&buf).into()
}

fn push_str(buf: &mut Vec<u8>, value: &str) {
    buf.extend_from_slice(&(value.len() as u64).to_be_bytes());
    buf.extend_from_slice(value.as_bytes());
}

fn push_opt(buf: &mut Vec<u8>, value: Option<&str>) {
    match value {
        None => buf.push(0),
        Some(value) => {
            buf.push(1);
            push_str(buf, value);
        }
    }
}

/// Canonical durable spelling of one authorization outcome.
///
/// Used identically by the stored `outcome` column and the hash preimage, so a
/// retained row reproduces its own `entry_hash` from what it stores.
fn outcome_str(outcome: AuditOutcome) -> &'static str {
    match outcome {
        AuditOutcome::Allowed => "allowed",
        AuditOutcome::Denied => "denied",
    }
}
