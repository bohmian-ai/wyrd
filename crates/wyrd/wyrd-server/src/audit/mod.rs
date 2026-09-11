//! Data-plane audit threading for the C2 handlers (S3.C5).
//!
//! Every audited HTTP data-plane operation (register/install, query, RBAC deny)
//! appends one hash-chained `AuditEvent` row into the
//! transactional `vala.audit_staging`. The attribution is derived from the
//! resolved [`Caller`]: `principal_id`/`principal_kind`/`card_ref` come straight
//! off the `Principal`, `request_id` off the caller, and `auth_method` is `Jwt`
//! because this surface is reached only through the HTTP JWT-bearer flow
//! (internal record writes audit as `Internal` down the ingest path).
//!
//! A same-tx append (register) is threaded directly on the operation's
//! `TenantConn`; a standalone append (query, RBAC deny) uses
//! [`record_audit`], which owns its own short transaction. Either way a failed
//! append is fail-closed: the enclosing op is refused with
//! `WYRD_VALA_500_AUDIT_UNAVAILABLE`.

pub mod publication;

use sqlx::PgPool;
use vala_sql::TenantConn;
use vala_sql::queries::audit_staging::append_audit;
use wyrd_spec::auth::{PLATFORM_AUDIT_PRINCIPAL, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::BifrostError as ValaError;
use wyrd_spec::vala::api::{AuditDetail, AuditEvent, AuditOutcome};

use crate::components::auth::Caller;

/// Build a data-plane [`AuditEvent`] attributed to the HTTP caller.
///
/// `card_ref` is the writer-identity card (`None` for a `User` principal), never
/// a per-row data column (Decision E). The row states what the boundary decided
/// about `permission`, never whether the admitted operation later succeeded.
#[must_use]
pub fn audit_event(
    caller: &Caller,
    operation: &str,
    resource: &str,
    permission: &str,
    outcome: AuditOutcome,
) -> AuditEvent {
    AuditEvent {
        request_id: caller.request_id.clone(),
        trace_id: None,
        operation: operation.to_owned(),
        resource: resource.to_owned(),
        card_ref: caller.principal.card_ref().cloned(),
        principal_id: caller.principal.id,
        principal_kind: caller.principal.kind.tag(),
        permission: permission.to_owned(),
        outcome,
        detail: delegation_detail(caller),
    }
}

/// Project the caller's verified delegation chain into a durable detail.
///
/// Operations reached through this builder carry no operation-specific detail
/// of their own, so an attribution-only detail is the one place their audit row
/// can record who was acting for whom. A nondelegated caller keeps `None`, which
/// is exactly the encoding every such row had before delegation attribution
/// existed, so historical rows and new nondelegated rows hash identically.
///
/// Callers that already build an operation-specific detail must fold the chain
/// into that detail instead of calling this; overwriting a read decision with
/// an attribution-only detail would lose the decision.
fn delegation_detail(caller: &Caller) -> Option<AuditDetail> {
    if caller.delegation_chain.is_empty() {
        return None;
    }
    Some(AuditDetail::DelegationAttribution {
        delegation_chain: wyrd_runtime::audit_delegation_chain(&caller.delegation_chain),
    })
}

/// Build an [`AuditEvent`] for a pre-authentication attempt, attributed to
/// [`PLATFORM_AUDIT_PRINCIPAL`] (a reserved well-known service actor). Used
/// when no `Caller` is available — e.g. `GET /auth/login` before OIDC resolve.
///
/// The outcome states whether the attempt was admitted or refused.
#[must_use]
pub fn audit_event_unauthenticated(
    request_id: RequestId,
    operation: &str,
    resource: &str,
    permission: &str,
    outcome: AuditOutcome,
) -> AuditEvent {
    AuditEvent {
        request_id,
        trace_id: None,
        operation: operation.to_owned(),
        resource: resource.to_owned(),
        card_ref: None,
        principal_id: PLATFORM_AUDIT_PRINCIPAL,
        principal_kind: PrincipalKindTag::Service,
        permission: permission.to_owned(),
        outcome,
        detail: None,
    }
}

/// Map an audit-append failure to the fail-closed public code, never leaking the
/// underlying SQL/connection detail across the boundary.
pub fn audit_unavailable(error: impl std::fmt::Display) -> WyrdError {
    tracing::error!(error = %error, "audit append failed; refusing operation");
    ValaError::AuditUnavailable {
        detail: "audit append failed".to_owned(),
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

/// Appends one owned audit event in its own tenant-scoped transaction.
///
/// This adapter keeps event and pool ownership inside transport futures that
/// must remain `Send`; durability and fail-closed behavior match
/// [`record_audit`].
///
/// # Errors
///
/// Returns [`WyrdError::AuditUnavailable`] when acquiring, appending, or
/// committing fails.
pub async fn record_audit_owned(
    pool: PgPool,
    tenant: DataTenantId,
    event: AuditEvent,
) -> Result<(), WyrdError> {
    tokio::spawn(async move {
        let mut conn = TenantConn::acquire(&pool, tenant)
            .await
            .map_err(audit_unavailable)?;
        append_audit(&mut conn, &event)
            .await
            .map_err(audit_unavailable)?;
        conn.commit().await.map_err(audit_unavailable)?;
        Ok(())
    })
    .await
    .map_err(audit_unavailable)?
}
