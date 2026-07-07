//! Data-plane audit threading for the C2 handlers (S3.C5).
//!
//! Every audited HTTP data-plane op (register/install, sync query, async
//! submit/status, RBAC deny) appends one hash-chained `AuditEvent` row into the
//! transactional `vala.audit_outbox`. The attribution is derived from the
//! resolved [`Caller`]: `principal_id`/`principal_kind`/`card_ref` come straight
//! off the `Principal`, `request_id` off the caller, and `auth_method` is `Jwt`
//! because this surface is reached only through the HTTP JWT-bearer flow
//! (internal record writes audit as `Internal` down the ingest path).
//!
//! A same-tx append (async submit/status, register) is threaded directly on the
//! operation's `TenantConn`; a standalone append (sync query, RBAC deny) uses
//! [`record_audit`], which owns its own short transaction. Either way a failed
//! append is fail-closed: the enclosing op is refused with
//! `WYRD_VALA_500_AUDIT_UNAVAILABLE`.

use sqlx::PgPool;
use vala_sql::TenantConn;
use vala_sql::queries::audit_outbox::append_audit;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::BifrostError as ValaError;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

use crate::components::auth::Caller;

/// Build a data-plane [`AuditEvent`] attributed to the HTTP caller.
///
/// `card_ref` is the writer-identity card (`None` for a `User` principal), never
/// a per-row data column (Decision E). `auth_method` is `Jwt`: this handler
/// surface authenticates via the HTTP bearer flow.
#[must_use]
pub fn audit_event(
    caller: &Caller,
    operation: &str,
    resource: &str,
    permission: &str,
    decision: AuditDecision,
    result: AuditResult,
    payload_summary: &str,
) -> AuditEvent {
    AuditEvent {
        request_id: caller.request_id.clone(),
        trace_id: None,
        operation: operation.to_owned(),
        resource: resource.to_owned(),
        card_ref: caller.principal.card_ref().cloned(),
        principal_id: caller.principal.id,
        principal_kind: caller.principal.kind.tag(),
        auth_method: AuthMethod::Jwt,
        permission: permission.to_owned(),
        decision,
        result,
        payload_summary: payload_summary.to_owned(),
    }
}

/// Map an audit-append failure to the fail-closed public code, never leaking the
/// underlying SQL/connection detail across the boundary.
pub fn audit_unavailable(error: impl std::fmt::Display) -> WyrdError {
    tracing::error!(error = %error, "audit outbox append failed; refusing operation");
    ValaError::AuditUnavailable {
        detail: "audit outbox append failed".to_owned(),
    }
    .into()
}

/// Append one audit row on an existing operation transaction (same-tx path).
///
/// The row commits exactly when the caller commits `conn`, so it is durable iff
/// the audited operation is.
///
/// # Errors
/// Returns [`WyrdError::AuditUnavailable`] when the append fails.
pub async fn append_on(conn: &mut TenantConn<'_>, event: &AuditEvent) -> Result<(), WyrdError> {
    append_audit(conn, event).await.map_err(audit_unavailable)?;
    Ok(())
}

/// Append one audit row in its own tenant-scoped transaction (standalone path).
///
/// # Errors
/// Returns [`WyrdError::AuditUnavailable`] when acquiring the connection, the
/// append, or the commit fails.
pub async fn record_audit(
    pool: &PgPool,
    tenant: DataTenantId,
    event: &AuditEvent,
) -> Result<(), WyrdError> {
    let mut conn = TenantConn::acquire(pool, tenant)
        .await
        .map_err(audit_unavailable)?;
    append_audit(&mut conn, event)
        .await
        .map_err(audit_unavailable)?;
    conn.commit().await.map_err(audit_unavailable)?;
    Ok(())
}
