//! `POST /v1/principals/{id}/revoke` — bump `tokens_not_before` to now().

use axum::extract::{Path, State};
use wyrd_auth_verify::PrincipalKindWire;
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;

use crate::auth::require_service_accounts_write;
use crate::auth::revocation_listener::notify_principal_revoked;
use crate::components::auth::AuthenticatedPrincipal;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;
use wyrd_auth::revoke::revoke_principal_in_conn;
use wyrd_sql::TenantConn;

pub async fn revoke_principal(
    State(state): State<AppState>,
    caller: AuthenticatedPrincipal,
    Path(target_id): Path<PrincipalId>,
) -> Result<(), WyrdErrorResponse> {
    let tenant = caller.principal.tenant_id;

    require_service_accounts_write(&caller.principal, "revoke principals")?;

    let mut conn = acquire_conn(&state, tenant).await?;
    let kind = revoke_principal_in_conn(&mut conn, target_id, tenant)
        .await
        .map_err(WyrdErrorResponse::from)?;
    commit_conn(conn).await?;

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

async fn commit_conn(conn: TenantConn<'_>) -> Result<(), WyrdErrorResponse> {
    conn.commit().await.map_err(internal_error)
}

/// Send the cross-pod revocation NOTIFY. Best-effort; failure does not undo the DB write.
async fn fan_out_notify(
    state: &AppState,
    tenant: DataTenantId,
    kind: PrincipalKindWire,
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
