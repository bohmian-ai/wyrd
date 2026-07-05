//! `POST /v1/principals/{id}/revoke` — bump `tokens_not_before` to now().

use axum::extract::{Path, State};
use wyrd_auth_verify::PrincipalKindWire;
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;

use crate::auth::AuthenticatedPrincipal;
use crate::auth::require_service_accounts_write;
use crate::auth::revocation_listener::notify_principal_revoked;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    revoke_service_account_principal, revoke_user_principal, service_account_by_id, user_by_id,
};

pub async fn revoke_principal(
    State(state): State<AppState>,
    caller: AuthenticatedPrincipal,
    Path(target_id): Path<PrincipalId>,
) -> Result<(), WyrdErrorResponse> {
    let tenant = caller.principal.tenant_id;

    require_service_accounts_write(&caller.principal, "revoke principals")?;

    let mut conn = acquire_conn(&state, tenant).await?;
    let kind = revoke_in_conn(&mut conn, target_id, tenant).await?;
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

/// Look up `target_id`, write the revocation timestamp, and return the principal kind.
///
/// Returns `PrincipalNotFound` when `target_id` does not exist in the tenant.
async fn revoke_in_conn(
    conn: &mut TenantConn<'_>,
    target_id: PrincipalId,
    tenant: DataTenantId,
) -> Result<PrincipalKindWire, WyrdErrorResponse> {
    let id_uuid = target_id.as_uuid();

    if user_by_id(conn, id_uuid).await.ok().flatten().is_some() {
        revoke_user_principal(conn, id_uuid)
            .await
            .map_err(internal_error)?;
        return Ok(PrincipalKindWire::User);
    }

    if let Some(row) = service_account_by_id(conn, id_uuid).await.ok().flatten() {
        revoke_service_account_principal(conn, id_uuid)
            .await
            .map_err(internal_error)?;
        let kind = if row.principal_kind == "agent" {
            PrincipalKindWire::Agent
        } else {
            PrincipalKindWire::Service
        };
        return Ok(kind);
    }

    Err(WyrdErrorResponse::from(WyrdError::PrincipalNotFound {
        message: format!("principal {target_id} not found in tenant {tenant}"),
        details: serde_json::json!({ "id": target_id.to_string() }),
    }))
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
