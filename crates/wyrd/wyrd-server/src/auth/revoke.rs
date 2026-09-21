//! `POST /v1/principals/{id}/revoke` — bump `tokens_not_before` to now().

use axum::Json;
use axum::extract::{Path, State};
use std::fmt::Display;
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::RevokePrincipalRequest;
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::vala::{AuditDetail, RevocationReason};

use crate::audit;
use crate::components::auth::Caller;
use crate::http::error::{WyrdErrorResponse, internal_failure};
use crate::state::AppState;
use wyrd_auth::revoke::revoke_principal_in_conn;
use wyrd_sql::TenantConn;

/// Revoke one principal's outstanding tokens under the caller's tenant.
///
/// Revocation is an administrative authorization boundary: the
/// `service_accounts:write` verdict is audited for both outcomes. A refusal is
/// durable on its own, because a denied attempt is evidence whether or not
/// anything followed it. An allowance is appended to the same transaction as
/// the `tokens_not_before` bump and commits with it, so the record and the
/// effect cannot disagree: there is no committed allowance for a revocation
/// that did not happen.
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
    path = "/principals/{id}/revoke",
    params(("id" = String, Path, description = "Principal whose tokens stop working")),
    request_body = RevokePrincipalRequest,
    responses(
        (status = 200, description = "Outstanding tokens revoked, effective on the next request"),
        (status = 400, description = "Missing, oversized, or secret-like revocation reason \
          (WYRD_VALIDATION_400_MISSING_REQUIRED_FIELD)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable access token \
          (WYRD_AUTH_401_UNAUTHENTICATED, WYRD_AUTH_401_INVALID_TOKEN, \
          WYRD_AUTH_401_TOKEN_EXPIRED, WYRD_AUTH_401_CREDENTIAL_REVOKED)", body = WyrdProblem),
        (status = 403, description = "Tenant principal administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such principal of that kind in the caller's tenant \
          (WYRD_AUTH_404_PRINCIPAL_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A tenant store read or write failed, or the revocation \
          decision could not be audited (WYRD_SPEC_500_INTERNAL, \
          WYRD_VALA_500_AUDIT_UNAVAILABLE)", body = WyrdProblem),
        (status = 503, description = "The revocation store could not vouch for the token (\
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
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

/// Refuse a revocation request with a stable message, logging the real cause.
///
/// Reuses the server's one internal-failure constructor so the source's
/// `Display` — a SQL error, a pool timeout — reaches the trace and never the
/// problem body.
fn internal_error(cause: impl Display) -> WyrdErrorResponse {
    WyrdErrorResponse::from(internal_failure("principal revocation failed", &cause))
}
