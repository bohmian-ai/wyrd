//! `POST /v1/principals/{id}/revoke` — bump `tokens_not_before` to now().

use axum::extract::{Path, State};
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::error::WyrdError;

use crate::audit;
use crate::auth::revocation_listener::notify_principal_revoked;
use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
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
/// # Errors
/// Returns [`WyrdError::PermissionDeniedRbac`] when the caller lacks
/// `service_accounts:write`, [`WyrdError::AuditUnavailable`] when the decision
/// cannot be recorded, and an internal error when the revocation transaction
/// cannot be acquired or committed.
#[utoipa::path(
    post,
    path = "/v1/principals/{principal_id}/revoke",
    params(("principal_id" = String, Path, description = "Principal whose tokens stop working")),
    responses(
        (status = 200, description = "Outstanding tokens revoked, effective on the next request"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Tenant principal administration required"),
        (status = 404, description = "No such principal in the caller's tenant")
    ),
    tag = "Principals"
)]
pub async fn revoke_principal(
    State(state): State<AppState>,
    caller: Caller,
    Path(target_id): Path<PrincipalId>,
) -> Result<(), WyrdErrorResponse> {
    let tenant = caller.principal.tenant_id;

    let decision = audit::authorize_service_accounts_write(
        &state,
        &caller,
        "revoke principals",
        "auth.principal.revoke",
        &format!("principal:{target_id}"),
    )
    .await
    .map_err(WyrdErrorResponse::from)?;

    audit::record_audit(state.postgres.vala_pool(), tenant, &decision)
        .await
        .map_err(WyrdErrorResponse::from)?;
    let mut conn = acquire_conn(&state, tenant).await?;
    let kind = revoke_principal_in_conn(&mut conn, target_id, tenant)
        .await
        .map_err(WyrdErrorResponse::from)?;
    conn.commit().await.map_err(internal_error)?;

    fan_out_notify(&state, tenant, kind, target_id).await;

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

fn internal_error(e: impl std::fmt::Display) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Internal {
        message: e.to_string(),
        details: serde_json::Value::Null,
    })
}
