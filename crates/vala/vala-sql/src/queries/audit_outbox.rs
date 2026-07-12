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
pub async fn append_audit(conn: &mut TenantConn<'_>, event: &AuditEvent) -> Result<i64, SqlError> {
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
    .bind(event.principal_kind.as_str())
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

/// Stamp `ship_batch_id` on the claimed `[seq_lo, seq_hi]` range for one tenant.
///
/// Called inside the claim transaction so the batch boundary is durable before the
/// relay even begins shipping. `WHERE ship_batch_id IS NULL` makes repeated calls
/// idempotent — rows already stamped from a prior (crashed) attempt are untouched,
/// preserving their existing batch_id for the recovery path.
///
/// # Errors
/// Returns [`SqlError`] when the update fails.
pub async fn stamp_audit_ship_batch_id(
    conn: &mut TenantConn<'_>,
    tenant_id: uuid::Uuid,
    seq_lo: i64,
    seq_hi: i64,
    batch_id: &[u8; 16],
) -> Result<u64, SqlError> {
    let result = sqlx::query("SELECT vala.stamp_audit_ship_batch_id($1, $2, $3, $4)")
        .bind(tenant_id)
        .bind(seq_lo)
        .bind(seq_hi)
        .bind(batch_id.as_slice())
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
    push_str(&mut buf, event.principal_kind.as_str());
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

/// Column values needed to recompute the SHA256 entry hash.
///
/// Used by the audit-seal verifier to recompute hashes from the Iceberg
/// `audit_log` table and cross-check them against seal checkpoints. The
/// canonical encoding is identical to the private `entry_hash` function.
pub struct AuditEntryHashInput<'a> {
    /// Entry hash of `seq - 1` (all-zeros for seq=1).
    pub prev_hash: &'a [u8],
    /// Monotonically increasing row sequence number.
    pub seq: i64,
    /// Unique request correlation id.
    pub request_id: &'a str,
    /// Optional trace id for distributed tracing.
    pub trace_id: Option<&'a str>,
    /// The operation name (e.g. `cards.create`).
    pub operation: &'a str,
    /// The resource path acted upon.
    pub resource: &'a str,
    /// Optional `CardRef` string for card-scoped operations.
    pub card_ref: Option<&'a str>,
    /// Raw 16-byte principal UUID.
    pub principal_id_bytes: &'a [u8; 16],
    /// The principal kind string (e.g. `user`, `service`).
    pub principal_kind: &'a str,
    /// The authentication method used.
    pub auth_method: &'a str,
    /// The permission that was checked.
    pub permission: &'a str,
    /// The policy decision string (`allow` / `deny`).
    pub decision: &'a str,
    /// The operation result string (`success` / `failure`).
    pub result: &'a str,
    /// Short human-readable summary of the payload.
    pub payload_summary: &'a str,
}

/// Recompute the SHA256 entry hash from the raw stored column bytes.
///
/// The canonical encoding is identical to the private `entry_hash` function
/// so this is the public re-check entry point used by the seal verifier.
#[must_use]
pub fn entry_hash_from_cols(input: AuditEntryHashInput<'_>) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(input.prev_hash);
    buf.extend_from_slice(&input.seq.to_be_bytes());
    push_str(&mut buf, input.request_id);
    push_opt(&mut buf, input.trace_id);
    push_str(&mut buf, input.operation);
    push_str(&mut buf, input.resource);
    push_opt(&mut buf, input.card_ref);
    buf.extend_from_slice(input.principal_id_bytes);
    push_str(&mut buf, input.principal_kind);
    push_str(&mut buf, input.auth_method);
    push_str(&mut buf, input.permission);
    push_str(&mut buf, input.decision);
    push_str(&mut buf, input.result);
    push_str(&mut buf, input.payload_summary);
    Sha256::digest(&buf).into()
}

/// A gap detected between consecutive `seq` values for a tenant.
#[derive(Debug, Clone)]
pub struct SeqGap {
    /// First missing `seq` in the gap.
    pub gap_from: i64,
    /// Last missing `seq` in the gap.
    pub gap_to: i64,
}

/// A hash-chain break: the row at `seq` has a `prev_hash` that does not match
/// the `entry_hash` of `seq - 1`.
#[derive(Debug, Clone)]
pub struct ChainBreak {
    /// The `seq` whose `prev_hash` does not match its predecessor's `entry_hash`.
    pub seq: i64,
}

/// A (seq, entry_hash) pair from a shipped outbox row, used for cross-store parity.
#[derive(Debug, Clone)]
pub struct ShippedOutboxRef {
    /// The sequence number.
    pub seq: i64,
    /// The SHA256 entry hash of this row.
    pub entry_hash: Vec<u8>,
}

/// Find gaps in the per-tenant `seq` sequence (check 1 of 3 — SQL-only).
///
/// Returns at most 100 gaps; a non-empty result indicates rows were lost or
/// never inserted, which is an audit integrity incident.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn check_seq_gaps(conn: &mut TenantConn<'_>) -> Result<Vec<SeqGap>, SqlError> {
    sqlx::query_as::<_, (i64, i64)>(
        r#"
        WITH ordered AS (
            SELECT seq, lag(seq) OVER (ORDER BY seq) AS prev
              FROM vala.audit_outbox
             WHERE data_tenant_id = wyrd.current_tenant()
        )
        SELECT prev + 1 AS gap_from, seq - 1 AS gap_to
          FROM ordered
         WHERE seq - prev > 1
         LIMIT 100
        "#,
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
    .map(|rows| {
        rows.into_iter()
            .map(|(gap_from, gap_to)| SeqGap { gap_from, gap_to })
            .collect()
    })
}

/// Detect hash-chain breaks: rows whose `prev_hash` does not match the
/// `entry_hash` of the preceding row (check 2 of 3 — SQL-only).
///
/// Returns at most 100 breaks; a non-empty result indicates tampering or
/// data corruption.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn check_hash_chain(conn: &mut TenantConn<'_>) -> Result<Vec<ChainBreak>, SqlError> {
    sqlx::query_as::<_, (i64,)>(
        r#"
        WITH chained AS (
            SELECT seq, prev_hash,
                   lag(entry_hash) OVER (ORDER BY seq) AS expected_prev
              FROM vala.audit_outbox
             WHERE data_tenant_id = wyrd.current_tenant()
        )
        SELECT seq
          FROM chained
         WHERE expected_prev IS NOT NULL
           AND prev_hash != expected_prev
         LIMIT 100
        "#,
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
    .map(|rows| rows.into_iter().map(|(seq,)| ChainBreak { seq }).collect())
}

/// Return (seq, entry_hash) for all shipped outbox rows in seq order. Used by
/// the Bifrost reconcile layer (check 3 of 3) to cross-reference against the
/// Iceberg `audit_log` table.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn shipped_outbox_refs(
    conn: &mut TenantConn<'_>,
) -> Result<Vec<ShippedOutboxRef>, SqlError> {
    sqlx::query_as::<_, (i64, Vec<u8>)>(
        r#"
        SELECT seq, entry_hash
          FROM vala.audit_outbox
         WHERE data_tenant_id = wyrd.current_tenant()
           AND shipped = true
         ORDER BY seq
        "#,
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
    .map(|rows| {
        rows.into_iter()
            .map(|(seq, entry_hash)| ShippedOutboxRef { seq, entry_hash })
            .collect()
    })
}
