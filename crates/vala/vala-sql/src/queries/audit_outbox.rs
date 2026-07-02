//! Tenant-scoped writes + cross-tenant relay claim for the audit outbox:
//! `vala.audit_chain_head` and `vala.audit_outbox`.
//!
//! `append_audit` runs in the audited operation's own [`TenantConn`]
//! transaction: it advances the per-tenant chain head under a `FOR UPDATE`
//! lock, computes the SHA256 entry hash in Rust (this module owns the canonical
//! encoding), inserts the append-only row, and bumps the head — all so the audit
//! row commits atomically with the operation it records. `mark_audit_shipped`
//! flips a contiguous `seq` range to shipped under the tenant bind. The relay
//! discovers work across every tenant via the SECURITY DEFINER
//! `claim_unshipped_audit` routine.
// raw-query grep allowlist: audit outbox tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sha2::{Digest, Sha256};
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};
use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::row_types::audit_outbox::AuditOutboxRow;

/// Append one hash-chained audit row for the current tenant, returning its `seq`.
///
/// Advances `vala.audit_chain_head` under `FOR UPDATE` so concurrent appends for
/// the same tenant serialize into a gapless sequence, then inserts the row into
/// `vala.audit_outbox`. Runs inside the caller's transaction so the audit row is
/// durable exactly when — and only when — the audited operation commits.
///
/// # Errors
/// Returns [`SqlError`] when any statement fails or an RLS policy rejects a row.
pub async fn append_audit(
    conn: &mut TenantConn<'_>,
    event: &AuditEvent,
) -> Result<i64, SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.audit_chain_head (data_tenant_id)
        VALUES (wyrd.current_tenant())
        ON CONFLICT (data_tenant_id) DO NOTHING
        "#,
    )
    .execute(&mut **conn.transaction())
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
    .fetch_one(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    let seq = last_seq + 1;
    let card_ref = event.card_ref.as_ref().map(ToString::to_string);
    let entry_hash = entry_hash(&prev_hash, seq, event, card_ref.as_deref());

    sqlx::query(
        r#"
        INSERT INTO vala.audit_outbox
            (data_tenant_id, seq, prev_hash, entry_hash, request_id, trace_id,
             operation, resource, card_ref, principal_id, principal_kind,
             auth_method, permission, decision, result, payload_summary)
        VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15)
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
    .bind(event.principal_kind.tag().as_str())
    .bind(auth_method_str(event.auth_method))
    .bind(event.permission.as_str())
    .bind(decision_str(event.decision))
    .bind(result_str(event.result))
    .bind(event.payload_summary.as_str())
    .execute(&mut **conn.transaction())
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
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(seq)
}

/// Mark a contiguous `seq` range shipped for the current tenant.
///
/// Stamps `ship_batch_id` / `shipped_at` on every still-unshipped row in
/// `[seq_lo, seq_hi]`; already-shipped rows are skipped, so re-marking a range
/// after a partial relay failure is idempotent. Returns the number of rows
/// transitioned.
///
/// # Errors
/// Returns [`SqlError`] when the update fails.
pub async fn mark_audit_shipped(
    conn: &mut TenantConn<'_>,
    seq_lo: i64,
    seq_hi: i64,
    ship_batch_id: &[u8; 16],
) -> Result<u64, SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE vala.audit_outbox
           SET shipped = true, shipped_at = now(), ship_batch_id = $3
         WHERE data_tenant_id = wyrd.current_tenant()
           AND seq BETWEEN $1 AND $2
           AND NOT shipped
        "#,
    )
    .bind(seq_lo)
    .bind(seq_hi)
    .bind(ship_batch_id.as_slice())
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(result.rows_affected())
}

/// Claim up to `limit` unshipped audit rows across all tenants for relay.
///
/// Calls the SECURITY DEFINER `vala.claim_unshipped_audit` routine (owned by
/// `vala_audit_relay`, BYPASSRLS) so the relay sees pending work regardless of
/// the connection's tenant bind. Rows are ordered by `(data_tenant_id, seq)`;
/// the relay groups by tenant, ships one batch per tenant, then marks each range
/// shipped under that tenant's bind.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn claim_unshipped_audit(
    conn: &mut TenantConn<'_>,
    limit: i32,
) -> Result<Vec<AuditOutboxRow>, SqlError> {
    sqlx::query_as::<_, AuditOutboxRow>("SELECT * FROM vala.claim_unshipped_audit($1)")
        .bind(limit)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)
}

/// Compute `entry_hash = SHA256(canonical(prev_hash, seq, event))`.
///
/// The canonical encoding is a length-prefixed concatenation owned here, so the
/// chain is reproducible from the stored columns alone. `card_ref` is passed as
/// its already-canonicalized string to avoid recomputing it.
fn entry_hash(prev_hash: &[u8], seq: i64, event: &AuditEvent, card_ref: Option<&str>) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(prev_hash);
    buf.extend_from_slice(&seq.to_be_bytes());
    push_str(&mut buf, event.request_id.as_str());
    push_opt(&mut buf, event.trace_id.as_deref());
    push_str(&mut buf, &event.operation);
    push_str(&mut buf, &event.resource);
    push_opt(&mut buf, card_ref);
    buf.extend_from_slice(event.principal_id.as_uuid().as_bytes());
    push_str(&mut buf, event.principal_kind.tag().as_str());
    push_str(&mut buf, auth_method_str(event.auth_method));
    push_str(&mut buf, &event.permission);
    push_str(&mut buf, decision_str(event.decision));
    push_str(&mut buf, result_str(event.result));
    push_str(&mut buf, &event.payload_summary);
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

fn auth_method_str(method: AuthMethod) -> &'static str {
    match method {
        AuthMethod::Jwt => "jwt",
        AuthMethod::Internal => "internal",
    }
}

fn decision_str(decision: AuditDecision) -> &'static str {
    match decision {
        AuditDecision::Allow => "allow",
        AuditDecision::Deny => "deny",
    }
}

fn result_str(result: AuditResult) -> &'static str {
    match result {
        AuditResult::Success => "success",
        AuditResult::Failure => "failure",
    }
}
