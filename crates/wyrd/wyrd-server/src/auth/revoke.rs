//! `POST /v1/principals/{id}/revoke` — bump `tokens_not_before` to now().

use axum::Json;
use axum::extract::{Path, State};
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalKindTag, RevokePrincipalRequest};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::vala::{AuditDetail, RevocationReason};

use crate::audit;
use crate::auth::revocation_listener::notify_principal_revoked;
use crate::components::auth::Caller;
use crate::http::error::{WyrdErrorResponse, internal_failure};
use crate::state::AppState;
use wyrd_auth::revoke::revoke_principal_in_conn;
use wyrd_sql::TenantConn;

/// Revoke one principal's outstanding tokens under the caller's tenant.
///
/// Revocation is an administrative authorization boundary: the
/// `service_accounts:write` verdict is audited for both outcomes. The allowed
/// row commits on its own before the `tokens_not_before` bump, so the decision
/// stays durable even when the revocation fails.
///
/// The request body is the contract, not decoration. Its `principal_kind`
/// selects the table the id is resolved in, and its `reason` is folded into
/// the audited decision, because the operator's justification is the one part
/// of a revocation that cannot be reconstructed afterwards. The reason is
/// screened by [`RevocationReason`] first, so a pasted credential is refused
/// rather than persisted.
///
/// # Errors
/// Returns [`WyrdError::PermissionDeniedRbac`] when the caller lacks
/// `service_accounts:write`, [`WyrdError::MissingRequiredField`] when the
/// reason is empty, oversized, or secret-like, [`WyrdError::PrincipalNotFound`]
/// when no principal of that kind exists in the tenant,
/// [`WyrdError::AuditUnavailable`] when the decision cannot be recorded, and an
/// internal error when the revocation transaction cannot be acquired or
/// committed.
#[utoipa::path(
    post,
    path = "/v1/principals/{principal_id}/revoke",
    params(("principal_id" = String, Path, description = "Principal whose tokens stop working")),
    request_body = RevokePrincipalRequest,
    responses(
        (status = 200, description = "Outstanding tokens revoked, effective on the next request"),
        (status = 400, description = "Missing, oversized, or secret-like revocation reason", body = WyrdProblem),
        (status = 401, description = "Authentication required", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required", body = WyrdProblem),
        (status = 404, description = "No such principal of that kind in the caller's tenant", body = WyrdProblem),
        (status = 503, description = "Revocation decision could not be audited", body = WyrdProblem)
    ),
    tag = "Principals"
)]
pub async fn revoke_principal(
    State(state): State<AppState>,
    caller: Caller,
    Path(target_id): Path<PrincipalId>,
    Json(request): Json<RevokePrincipalRequest>,
) -> Result<(), WyrdErrorResponse> {
    let tenant = caller.principal.tenant_id;
    let reason = RevocationReason::new(request.reason).map_err(|error| {
        WyrdErrorResponse::from(WyrdError::MissingRequiredField {
            message: format!("revocation reason is not recordable: {error}"),
            details: serde_json::json!({ "field": "reason" }),
        })
    })?;

    let mut decision = audit::authorize_service_accounts_write(
        &state,
        &caller,
        "revoke principals",
        "auth.principal.revoke",
        &format!("principal:{target_id}"),
    )
    .await
    .map_err(WyrdErrorResponse::from)?;
    decision.detail = Some(AuditDetail::PrincipalRevocation {
        principal_id: target_id,
        principal_kind: request.principal_kind,
        reason,
        delegation_chain: wyrd_runtime::audit_delegation_chain(&caller.delegation_chain),
    });

    let mut conn = acquire_conn(&state, tenant).await?;
    audit::append_on(&mut conn, &decision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    revoke_principal_in_conn(&mut conn, target_id, request.principal_kind, tenant)
        .await
        .map_err(WyrdErrorResponse::from)?;
    conn.commit().await.map_err(internal_error)?;

    fan_out_notify(&state, tenant, request.principal_kind, target_id).await;

    Ok(())
}

async fn acquire_conn(
    state: &AppState,
    tenant: DataTenantId,
) -> Result<TenantConn<'_>, WyrdErrorResponse> {
    state
        .postgres
        .tenant_conn(tenant)
        .await
        .map_err(internal_error)
}

/// Send the cross-pod revocation NOTIFY. Best-effort; failure does not undo the DB write.
async fn fan_out_notify(
    state: &AppState,
    tenant: DataTenantId,
    kind: PrincipalKindTag,
    id: PrincipalId,
) {
    if let Err(e) = notify_principal_revoked(state.postgres.app_pool(), tenant, kind, id).await {
        tracing::warn!(
            error = %e,
            "revocation NOTIFY failed; the epoch write is durable, TTL will enforce it"
        );
    }
}

/// Refuse a revocation request with a stable message, logging the real cause.
///
/// Reuses the server's one internal-failure constructor so the source's
/// `Display` — a SQL error, a pool timeout — reaches the trace and never the
/// problem body.
fn internal_error(cause: impl std::fmt::Display) -> WyrdErrorResponse {
    WyrdErrorResponse::from(internal_failure("principal revocation failed", &cause))
}
