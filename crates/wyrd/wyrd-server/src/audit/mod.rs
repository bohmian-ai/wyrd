//! Data-plane audit threading for the HTTP, MCP, and gRPC handlers.
//!
//! Every audited operation (register/install, query, RBAC deny, administration)
//! stages one `AuditEvent` on the process audit outbox
//! ([`vala_sql::audit_outbox::AuditOutbox`], held as `AppState::audit_outbox`).
//! The attribution is derived from the resolved [`Caller`]:
//! `principal_id`/`principal_kind`/`card_ref` come straight off the `Principal`
//! and `request_id` off the caller.
//!
//! Permissions block; audits do not. The permission check completes before the
//! operation proceeds or refuses, and its decision is staged as soon as it is
//! known, allowed and denied alike. Staging never waits for, or fails on, the
//! audit commit: the outbox commits in the background, a failed commit is
//! logged, counted on `outbox_write_failures_total{outbox="audit"}`, and
//! retried only once Postgres confirms it aborted. An event can be lost only on
//! abrupt process loss, at the graceful-shutdown deadline, or when Postgres no
//! longer holds the status of its failed commit.

pub mod publication;

use wyrd_runtime::Permission;
use wyrd_spec::auth::{PLATFORM_AUDIT_PRINCIPAL, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
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
        credential_id: caller.principal.credential_id,
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
        credential_id: None,
        permission: permission.to_owned(),
        outcome,
        detail: None,
    }
}

/// Evaluate one receiving permission and stage the verdict exactly once.
///
/// This is the shared owner of the "decide, stage the decision, then act"
/// boundary. The verdict is staged on the process audit outbox before the
/// caller proceeds or is refused, so an allowed operation is never unaudited and
/// a refusal is never silent, and the request never waits for the audit commit.
///
/// Use [`authorize_recording_denial`] instead when the allowed row must carry
/// an operation-specific detail before it is staged.
///
/// # Errors
/// Returns the mapped denial error when the principal lacks `required`.
pub fn authorize(
    state: &AppState,
    caller: &Caller,
    required: &Permission,
    operation: &str,
    resource: &str,
) -> Result<(), WyrdError> {
    let allowed = authorize_recording_denial(state, caller, required, operation, resource)?;
    state.audit_outbox.stage(caller.data_tenant_id, allowed);
    Ok(())
}

/// Evaluate one receiving permission, stage a denial, and hand back the allowed row.
///
/// A denial is staged here because it carries no operation detail. The
/// returned `Allowed` event is *not* yet staged: the caller enriches it with
/// its operation-specific detail and MUST stage it on `state.audit_outbox`
/// before performing the operation, so every evaluated allowance is recorded
/// whether or not the operation later succeeds.
///
/// # Errors
/// Returns the mapped denial error when the principal lacks `required`.
pub fn authorize_recording_denial(
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
        state.audit_outbox.stage(caller.data_tenant_id, denied);
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
/// issuers, workload bindings, and principal revocation. A denial is staged
/// here; the returned `Allowed` event lets the handler attach its
/// operation-specific detail and MUST be staged on `state.audit_outbox` before
/// the handler performs the operation.
///
/// The verdict comes from the configured `PermissionCheck` through
/// [`authorize_recording_denial`], so the audited decision and the response
/// follow the same runtime owner. `action` is the existing human-readable
/// intent kept so the public refusal message stays exactly what it was.
///
/// # Errors
/// Returns [`WyrdError::PermissionDeniedRbac`] when the configured checker denies
/// `service_accounts:write`.
pub fn authorize_service_accounts_write(
    state: &AppState,
    caller: &Caller,
    action: &str,
    operation: &str,
    resource: &str,
) -> Result<AuditEvent, WyrdError> {
    let required = Permission::service_accounts_write();
    authorize_recording_denial(state, caller, &required, operation, resource).map_err(|error| {
        match error {
            WyrdError::PermissionDeniedRbac { .. } => WyrdError::PermissionDeniedRbac {
                message: format!("{required} permission required to {action}"),
                details: serde_json::json!({ "required": required.to_string() }),
            },
            other => other,
        }
    })
}
