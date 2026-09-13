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

use std::fmt::Display;

use sqlx::PgPool;
use vala_sql::TenantConn;
use vala_sql::queries::audit_staging::append_audit;
use wyrd_runtime::Permission;
use wyrd_spec::auth::{PLATFORM_AUDIT_PRINCIPAL, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::BifrostError as ValaError;
use wyrd_spec::vala::api::{AuditDetail, AuditEvent, AuditOutcome};

use crate::components::auth::Caller;
use crate::http::error::permission_deny_reason_to_wyrd;
use crate::state::AppState;

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
pub fn audit_unavailable(error: impl Display) -> WyrdError {
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

/// Evaluate one receiving permission and audit the verdict exactly once.
///
/// This is the shared owner of the "audit the decision, then act" boundary. The
/// verdict is appended in its own tenant transaction before the caller proceeds
/// or is refused, so an allowed operation cannot run unaudited and a refusal
/// cannot be silent. An append failure fails closed: the operation is refused
/// with [`WyrdError::AuditUnavailable`] even when the verdict was `Allow`.
///
/// Use [`authorize_recording_denial`] instead when the allowed path already owns
/// a transaction that the allowed row must commit with.
///
/// # Errors
/// Returns the mapped denial error when the principal lacks `required`, and
/// [`WyrdError::AuditUnavailable`] when the decision row cannot be persisted.
pub async fn authorize(
    state: &AppState,
    caller: &Caller,
    required: &Permission,
    operation: &str,
    resource: &str,
) -> Result<(), WyrdError> {
    let denial = authorize_recording_denial(state, caller, required, operation, resource).await?;
    record_audit(state.postgres.vala_pool(), caller.data_tenant_id, &denial).await
}

/// Evaluate one receiving permission, audit a denial, and hand back the allowed row.
///
/// Denials are recorded standalone because there is no operation transaction to
/// join. The returned `Allowed` event is *not* yet durable: the caller MUST
/// [`append_on`] it inside the transaction that performs the operation, so the
/// decision and its effect commit together.
///
/// # Errors
/// Returns the mapped denial error when the principal lacks `required`, and
/// [`WyrdError::AuditUnavailable`] when the denial row cannot be persisted.
pub async fn authorize_recording_denial(
    state: &AppState,
    caller: &Caller,
    required: &Permission,
    operation: &str,
    resource: &str,
) -> Result<AuditEvent, WyrdError> {
    if let Err(reason) = state
        .authz
        .permission_check
        .check(&caller.principal, required)
        .into_result()
    {
        let denied = audit_event(
            caller,
            operation,
            resource,
            &required.to_string(),
            AuditOutcome::Denied,
        );
        record_audit(state.postgres.vala_pool(), caller.data_tenant_id, &denied).await?;
        return Err(permission_deny_reason_to_wyrd(reason));
    }
    Ok(audit_event(
        caller,
        operation,
        resource,
        &required.to_string(),
        AuditOutcome::Allowed,
    ))
}

/// Evaluate and audit the `service_accounts:write` gate for one admin operation.
///
/// Credential administration shares one permission across issuance, trusted
/// issuers, workload bindings, and principal revocation, and every one of those
/// handlers owns a tenant transaction. A denial is recorded standalone here; the
/// returned `Allowed` event MUST be committed standalone with [`record_audit`]
/// **before** the handler opens its transaction, so the decision survives a
/// failed, rolled-back, or not-found administrative write.
///
/// The verdict comes from the configured `PermissionCheck` through
/// [`authorize_recording_denial`], so the audited decision, the response, and
/// the effect all follow the same runtime owner. `action` is the existing
/// human-readable intent kept so the public refusal message stays exactly what
/// it was.
///
/// # Errors
/// Returns [`WyrdError::PermissionDeniedRbac`] when the configured checker denies
/// `service_accounts:write`, and [`WyrdError::AuditUnavailable`] when the denial
/// row cannot be persisted.
pub async fn authorize_service_accounts_write(
    state: &AppState,
    caller: &Caller,
    action: &str,
    operation: &str,
    resource: &str,
) -> Result<AuditEvent, WyrdError> {
    let required = Permission::service_accounts_write();
    authorize_recording_denial(state, caller, &required, operation, resource)
        .await
        .map_err(|error| match error {
            WyrdError::PermissionDeniedRbac { .. } => WyrdError::PermissionDeniedRbac {
                message: format!("{required} permission required to {action}"),
                details: serde_json::json!({ "required": required.to_string() }),
            },
            other => other,
        })
}
