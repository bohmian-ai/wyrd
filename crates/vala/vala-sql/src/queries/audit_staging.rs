//! Tenant-scoped writes and reads for audit staging:
//! `vala.audit_chain_head` and `vala.audit_staging`.
//!
//! `append_audit` runs in the audited operation's own [`TenantConn`]
//! transaction: it advances the per-tenant chain head under a `FOR UPDATE`
//! lock, computes the SHA256 entry hash in Rust (this module owns the canonical
//! encoding), inserts the append-only row, and bumps the head — all so the audit
//! row commits atomically with the operation it records.
// raw-query grep allowlist: audit staging tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sha2::{Digest, Sha256};
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome, audit_detail_canonical_json};
use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::row_types::audit_staging::AuditStagingRow;

/// Append one hash-chained audit row for the connection's tenant, returning its `seq`.
///
/// Advances `vala.audit_chain_head` under `FOR UPDATE` so concurrent appends for
/// the same tenant serialize into a gapless sequence, then inserts the row into
/// `vala.audit_staging`. Runs inside the caller's transaction so the decision
/// record is durable exactly when — and only when — the authorization decision
/// that produced it is.
///
/// Every statement names [`TenantConn::data_tenant_id`] explicitly rather than
/// leaning on the row-level-security policy to supply it. Under the application
/// role the two agree and nothing changes; the explicit predicate is what makes
/// the append correct on the operator boundary too, where row-level security is
/// bypassed and an unqualified `FOR UPDATE` would lock — and an unqualified
/// `UPDATE` would rewrite — every tenant's chain head. Platform-plane decisions
/// stage through that boundary under `DataTenantId::SYSTEM_OWNER`, which is
/// why this is the one canonical append for both planes.
///
/// # Errors
/// Returns [`SqlError`] when any statement fails or an RLS policy rejects a row.
pub async fn append_audit(conn: &mut TenantConn<'_>, event: &AuditEvent) -> Result<i64, SqlError> {
    let data_tenant_id = conn.data_tenant_id().as_uuid();
    let conn = &mut **conn.transaction();
    sqlx::query(
        r#"
        INSERT INTO vala.audit_chain_head (data_tenant_id)
        VALUES ($1)
        ON CONFLICT (data_tenant_id) DO NOTHING
        "#,
    )
    .bind(data_tenant_id)
    .execute(&mut *conn)
    .await
    .map_err(SqlError::from)?;

    let (last_seq, prev_hash): (i64, Vec<u8>) = sqlx::query_as(
        r#"
        SELECT last_seq, head_hash
          FROM vala.audit_chain_head
         WHERE data_tenant_id = $1
        FOR UPDATE
        "#,
    )
    .bind(data_tenant_id)
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
             credential_id, permission, outcome, detail)
        VALUES ($15, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14)
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
    .bind(event.credential_id)
    .bind(event.permission.as_str())
    .bind(outcome_str(event.outcome))
    .bind(detail.as_deref())
    .bind(data_tenant_id)
    .execute(&mut *conn)
    .await
    .map_err(SqlError::from)?;

    sqlx::query(
        r#"
        UPDATE vala.audit_chain_head
           SET last_seq = $1, head_hash = $2, updated_at = now()
         WHERE data_tenant_id = $3
        "#,
    )
    .bind(seq)
    .bind(entry_hash.as_slice())
    .bind(data_tenant_id)
    .execute(&mut *conn)
    .await
    .map_err(SqlError::from)?;

    Ok(seq)
}

/// Read a bounded page of audit rows for one tenant-bound resource.
///
/// The caller supplies the last observed sequence number. RLS is the only
/// tenant boundary: the policy on `vala.audit_staging` supplies the
/// `data_tenant_id` equality the covering `(data_tenant_id, resource, seq)`
/// index is planned against, so no statement repeats it.
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
               credential_id, permission, outcome, detail, created_at
          FROM vala.audit_staging
         WHERE resource = $1
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
               credential_id, permission, outcome, detail, created_at
          FROM vala.audit_staging
         ORDER BY seq
         LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// One frozen contiguous audit range owed to retained history.
///
/// Both bounds are inclusive. The range is derived under tenant serialization
/// and persisted as `vala.audit_chain_head.publishing_seq_hi`, so every
/// concurrent or restarted publisher that observes the same in-flight bound
/// projects the same rows and therefore the same batch identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditPublicationRange {
    /// First sequence number owed, always `published_seq + 1`.
    pub seq_lo: i64,
    /// Frozen inclusive upper bound of the batch currently owed.
    pub seq_hi: i64,
}

/// Freeze, or reuse, the one contiguous range this tenant owes retained history.
///
/// The chain head is locked `FOR UPDATE NOWAIT` so two publishers cannot
/// establish two different bounds. An existing `publishing_seq_hi` is reused verbatim —
/// that is what makes a competing or restarted publisher derive the identical
/// batch identity — and is otherwise established from the bounded staging
/// prefix above the watermark. Returns `None` when the tenant has never
/// appended an audit row or owes nothing, in which case the caller has no work
/// this cycle.
///
/// The caller commits the short transaction before performing any Scribe IO:
/// the bound is durable progress state, not a lease, so no owner token,
/// deadline, or audit-chain lock travels with it.
///
/// Lock acquisition never waits. When another transaction holds this tenant's
/// chain head, the locked read fails immediately with PostgreSQL `55P03`
/// (`lock_not_available`) before any publication state changes: no bound is
/// frozen and no staged row is read. The publisher logs that failure and
/// retries the unchanged tenant on a later sweep, so a contended tenant never
/// holds a sweep open and delays only itself.
///
/// # Errors
/// Returns [`SqlError`] when the locked read or the bound update fails, when
/// another transaction holds the tenant's chain head (lock contention is this
/// error, never `None`), or when RLS rejects the tenant's own chain head.
pub async fn freeze_publication_range(
    conn: &mut TenantConn<'_>,
    limit: i64,
) -> Result<Option<AuditPublicationRange>, SqlError> {
    let frozen: Option<(i64, Option<i64>, bool)> = sqlx::query_as(
        r#"
        WITH head AS (
            SELECT published_seq, publishing_seq_hi
              FROM vala.audit_chain_head
            FOR UPDATE NOWAIT
        )
        SELECT head.published_seq + 1,
               COALESCE(
                   head.publishing_seq_hi,
                   (SELECT max(prefix.seq)
                      FROM (
                          SELECT seq
                            FROM vala.audit_staging
                           WHERE seq > head.published_seq
                           ORDER BY seq
                           LIMIT $1
                      ) prefix)
               ),
               head.publishing_seq_hi IS NOT NULL
          FROM head
        "#,
    )
    .bind(limit)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    let Some((seq_lo, Some(seq_hi), already_frozen)) = frozen else {
        return Ok(None);
    };
    if !already_frozen {
        sqlx::query(
            r#"
            UPDATE vala.audit_chain_head
               SET publishing_seq_hi = $1, updated_at = now()
            "#,
        )
        .bind(seq_hi)
        .execute(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
    }
    Ok(Some(AuditPublicationRange { seq_lo, seq_hi }))
}

/// Read exactly the staged rows of one frozen inclusive audit range.
///
/// Reading the frozen range rather than a fresh bounded prefix is what keeps a
/// growing staging tail out of an in-flight batch: rows above `seq_hi` are
/// invisible here and wait for the next batch.
///
/// # Errors
/// Returns [`SqlError`] when the range query fails or RLS rejects the read.
pub async fn list_publication_range(
    conn: &mut TenantConn<'_>,
    range: AuditPublicationRange,
) -> Result<Vec<AuditStagingRow>, SqlError> {
    sqlx::query_as::<_, AuditStagingRow>(
        r#"
        SELECT data_tenant_id, seq, entry_hash, prev_hash, request_id, trace_id,
               operation, resource, card_ref, principal_id, principal_kind,
               credential_id, permission, outcome, detail, created_at
          FROM vala.audit_staging
         WHERE seq BETWEEN $1 AND $2
         ORDER BY seq
        "#,
    )
    .bind(range.seq_lo)
    .bind(range.seq_hi)
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Settle one durably published range: advance the watermark, release the
/// matching in-flight bound, and delete every staged row through the watermark.
///
/// The caller MUST have observed a durable `vala.system.audit_log` publication
/// for the whole range first. All three effects run in the caller's tenant
/// transaction, so the watermark never advances without the deletion and the
/// bound is never released without the watermark.
///
/// Both the advance and the release are guarded so a stale completion is inert:
/// `GREATEST` refuses to move the watermark backwards, and the bound is cleared
/// only when it still equals `seq_hi`. A publisher whose acknowledgement was
/// lost therefore replays the identical range into Scribe's batch fence and
/// settles it again, removing whatever survives and reporting it, rather than
/// losing or duplicating a retained event or clearing a newer batch's bound.
///
/// # Errors
/// Returns [`SqlError`] when the chain-head update or the delete fails, or RLS
/// rejects the range.
pub async fn settle_publication(conn: &mut TenantConn<'_>, seq_hi: i64) -> Result<u64, SqlError> {
    sqlx::query(
        r#"
        UPDATE vala.audit_chain_head
           SET published_seq = GREATEST(published_seq, $1),
               publishing_seq_hi = CASE
                   WHEN publishing_seq_hi = $1 THEN NULL
                   ELSE publishing_seq_hi
               END,
               updated_at = now()
        "#,
    )
    .bind(seq_hi)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    let result = sqlx::query(
        r#"
        DELETE FROM vala.audit_staging
         WHERE seq <= (
               SELECT published_seq
                 FROM vala.audit_chain_head
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
///
/// The credential segment is encoded like every other optional column —
/// present or absent — so one preimage covers every row the chain holds.
fn entry_hash(
    prev_hash: &[u8],
    seq: i64,
    event: &AuditEvent,
    card_ref: Option<&str>,
    detail: Option<&str>,
) -> [u8; 32] {
    // Hyphenated lowercase, the same spelling the column renders, so a retained
    // row reproduces its own hash from what it stores.
    let credential = event.credential_id.map(|id| id.to_string());
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
    push_opt(&mut buf, credential.as_deref());
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

/// The staged entry hash: exactly which fields enter it, and in what order.
#[cfg(test)]
mod tests {
    use sha2::{Digest as _, Sha256};
    use uuid::Uuid;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};

    use super::{entry_hash, push_opt, push_str};

    /// One decision, with or without the credential that authenticated it.
    fn event(credential_id: Option<Uuid>) -> AuditEvent {
        AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "platform.authz".to_owned(),
            resource: "platform:tenants".to_owned(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::nil()),
            principal_kind: PrincipalKindTag::Service,
            credential_id,
            permission: "tenants:write".to_owned(),
            outcome: AuditOutcome::Allowed,
            detail: None,
        }
    }

    /// The preimage a reader reproduces from the columns a row stores.
    ///
    /// Spelled out independently of [`entry_hash`] so the encoding the chain
    /// commits to is pinned by a second statement of it rather than by the
    /// implementation agreeing with itself.
    fn reader_preimage(prev_hash: &[u8], seq: i64, event: &AuditEvent) -> [u8; 32] {
        let mut buf = Vec::new();
        buf.extend_from_slice(prev_hash);
        buf.extend_from_slice(&seq.to_be_bytes());
        push_str(&mut buf, event.request_id.as_str());
        push_opt(&mut buf, event.trace_id.as_deref());
        push_str(&mut buf, &event.operation);
        push_str(&mut buf, &event.resource);
        push_opt(&mut buf, None);
        buf.extend_from_slice(event.principal_id.as_uuid().as_bytes());
        push_str(&mut buf, event.principal_kind.as_str());
        push_str(&mut buf, &event.permission);
        push_str(&mut buf, "allowed");
        push_opt(&mut buf, None);
        push_opt(
            &mut buf,
            event.credential_id.map(|id| id.to_string()).as_deref(),
        );
        Sha256::digest(&buf).into()
    }

    /// A credential-free row reproduces its stored hash from its own columns.
    #[test]
    fn a_credential_free_decision_reproduces_its_stored_hash() {
        let event = event(None);
        let prev_hash = [7_u8; 32];
        assert_eq!(
            entry_hash(&prev_hash, 42, &event, None, None),
            reader_preimage(&prev_hash, 42, &event),
            "an absent credential is encoded, not omitted"
        );
    }

    /// A credential-bearing row reproduces its stored hash from its own
    /// columns.
    #[test]
    fn a_credential_bearing_decision_reproduces_its_stored_hash() {
        let event = event(Some(uuid::Uuid::from_u128(1)));
        let prev_hash = [7_u8; 32];
        assert_eq!(
            entry_hash(&prev_hash, 42, &event, None, None),
            reader_preimage(&prev_hash, 42, &event),
            "the named credential is encoded in the preimage"
        );
    }

    /// A row that names a credential commits to it, so the two are distinct.
    #[test]
    fn a_credential_bearing_decision_commits_to_the_credential() {
        let credential = uuid::Uuid::from_u128(1);
        let prev_hash = [7_u8; 32];
        assert_ne!(
            entry_hash(&prev_hash, 42, &event(Some(credential)), None, None),
            entry_hash(&prev_hash, 42, &event(None), None, None),
            "the credential segment is part of what the chain commits to"
        );
    }
}
