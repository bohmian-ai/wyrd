//! Tenant-scoped writes and reads for the audit outbox:
//! `vala.audit_chain_head` and `vala.audit_outbox`.
//!
//! `append_audit` runs in the audited operation's own [`TenantConn`]
//! transaction: it advances the per-tenant chain head under a `FOR UPDATE`
//! lock, computes the SHA256 entry hash in Rust (this module owns the canonical
//! encoding), inserts the append-only row, and bumps the head — all so the audit
//! row commits atomically with the operation it records.
// raw-query grep allowlist: audit outbox tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Postgres, Transaction};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, audit_detail_canonical_json,
};
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
    append_audit_connection(conn.transaction(), event).await
}

/// Tenant-bound audit capability for a trusted operator transaction.
///
/// Exposes only canonical audit append bound to one already verified tenant.
/// It owns the verified `tenant` and re-binds the current tenant via
/// [`BIND_CURRENT_TENANT_SQL`](wyrd_sql::tenant_conn::BIND_CURRENT_TENANT_SQL)
/// before every append, re-establishing on the shared operator transaction the
/// RLS boundary a [`TenantConn`] would otherwise provide. It cannot be used as
/// a general SQL executor or masquerade as a tenant connection.
pub struct OperatorAudit<'transaction, 'connection> {
    /// Verified tenant on whose behalf Forge is appending audit evidence.
    tenant: DataTenantId,
    /// Operator transaction shared with the fenced Forge planning workflow.
    transaction: &'transaction mut Transaction<'connection, Postgres>,
}

impl<'transaction, 'connection> OperatorAudit<'transaction, 'connection> {
    /// Binds canonical audit append to one already verified tenant.
    pub fn new(
        tenant: DataTenantId,
        transaction: &'transaction mut Transaction<'connection, Postgres>,
    ) -> Self {
        Self {
            tenant,
            transaction,
        }
    }

    /// Appends one canonical hash-chained event for the bound tenant.
    ///
    /// # Errors
    /// Returns [`SqlError`] when tenant binding, chain locking, hashing
    /// persistence, or row insertion fails.
    ///
    /// # Cancellation
    /// Cancellation leaves the enclosing operator transaction uncommitted, so
    /// its owner can roll back the Forge mutation and audit append together.
    pub async fn append(&mut self, event: &AuditEvent) -> Result<i64, SqlError> {
        sqlx::query(wyrd_sql::tenant_conn::BIND_CURRENT_TENANT_SQL)
            .bind(self.tenant.to_string())
            .execute(&mut **self.transaction)
            .await
            .map_err(SqlError::from)?;
        append_audit_connection(self.transaction, event).await
    }
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
        INSERT INTO vala.audit_outbox
            (data_tenant_id, seq, prev_hash, entry_hash, request_id, trace_id,
             operation, resource, card_ref, principal_id, principal_kind,
             auth_method, permission, decision, result, payload_summary, detail)
        VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16)
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
) -> Result<Vec<AuditOutboxRow>, SqlError> {
    sqlx::query_as::<_, AuditOutboxRow>(
        r#"
        SELECT data_tenant_id, seq, entry_hash, prev_hash, request_id, trace_id,
               operation, resource, card_ref, principal_id, principal_kind,
               auth_method, permission, decision, result, payload_summary, detail, created_at
          FROM vala.audit_outbox
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
) -> Result<Vec<AuditOutboxRow>, SqlError> {
    sqlx::query_as::<_, AuditOutboxRow>(
        r#"
        SELECT data_tenant_id, seq, entry_hash, prev_hash, request_id, trace_id,
               operation, resource, card_ref, principal_id, principal_kind,
               auth_method, permission, decision, result, payload_summary, detail, created_at
          FROM vala.audit_outbox
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

/// Retire the inclusive `seq` range whose audit events are durably published.
///
/// The caller MUST have observed a durable `vala.system.audit_log` publication
/// for the whole range first. Retirement is idempotent: a repeated call after
/// an uncertain outcome removes whatever survives and reports the rows it
/// actually retired, so a lost acknowledgement costs one replayed shipment
/// rather than a lost or duplicated audit event.
///
/// # Errors
/// Returns [`SqlError`] when the delete fails or RLS rejects the range.
pub async fn retire_published(
    conn: &mut TenantConn<'_>,
    seq_lo: i64,
    seq_hi: i64,
) -> Result<u64, SqlError> {
    let result = sqlx::query(
        r#"
        DELETE FROM vala.audit_outbox
         WHERE data_tenant_id = wyrd.current_tenant()
           AND seq BETWEEN $1 AND $2
        "#,
    )
    .bind(seq_lo)
    .bind(seq_hi)
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
    push_str(&mut buf, auth_method_str(event.auth_method));
    push_str(&mut buf, &event.permission);
    push_str(&mut buf, decision_str(event.decision));
    push_str(&mut buf, result_str(event.result));
    push_str(&mut buf, &event.payload_summary);
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

#[cfg(test)]
mod pg_tests {
    //! Database parity proof for the execute-only recovery audit encoder.

    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::{
        auth::{PrincipalId, PrincipalKindTag},
        request_id::RequestId,
        vala::api::{AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod},
    };

    use super::entry_hash;

    /// The SQL definer function produces the exact canonical Rust detail and hash bytes.
    #[tokio::test]
    async fn oracle_recovery_sql_hash_matches_canonical_rust_encoder() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let request_id = RequestId::now_v7();
        let seq: i64 = sqlx::query_scalar(
            "SELECT vala.append_oracle_admission_recovery_audit($1,$2,$3,$4,$5,$6)",
        )
        .bind(request_id.as_str())
        .bind(2_i64)
        .bind(3_i64)
        .bind(5_i64)
        .bind(8_i64)
        .bind(13_i64)
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("recovery append executes");
        let (stored_detail, stored_prev_hash, stored_entry_hash): (String, Vec<u8>, Vec<u8>) =
            sqlx::query_as(
                "SELECT detail,prev_hash,entry_hash FROM vala.audit_outbox \
                 WHERE data_tenant_id=$1 AND seq=$2",
            )
            .bind(uuid::Uuid::nil())
            .bind(seq)
            .fetch_one(&fixture.superuser_pool().await.expect("superuser pool"))
            .await
            .expect("recovery audit reads");
        let detail = AuditDetail::OracleAdmissionRecovery {
            expired_lease_count: 2,
            active_lease_count: 3,
            interactive_slots: 5,
            analytical_slots: 8,
            total_slots: 13,
        };
        let canonical_detail = wyrd_spec::vala::api::audit_detail_canonical_json(&detail);
        let event = AuditEvent::new(
            request_id,
            None,
            "bifrost.oracle.admission_recovery".to_owned(),
            "bifrost.oracle.admission".to_owned(),
            None,
            PrincipalId::new(uuid::Uuid::nil()),
            PrincipalKindTag::Service,
            AuthMethod::Internal,
            "bifrost:oracle".to_owned(),
            AuditDecision::Allow,
            AuditResult::Success,
            "recovered Oracle admission aggregates".to_owned(),
        )
        .with_detail(detail);
        let expected_hash = entry_hash(
            &stored_prev_hash,
            seq,
            &event,
            None,
            Some(&canonical_detail),
        );
        assert_eq!(stored_detail, canonical_detail);
        assert_eq!(stored_entry_hash, expected_hash);
    }
}
