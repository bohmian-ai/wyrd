//! Principal revocation domain operations.

use wyrd_auth_verify::PrincipalKindWire;
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    revoke_service_account_principal, revoke_user_principal, service_account_by_id, user_by_id,
};

/// Look up `target_id`, write the revocation timestamp, and return the principal kind.
///
/// Returns `PrincipalNotFound` when `target_id` does not exist in the tenant.
///
/// # Errors
/// Returns a Wyrd error when the target principal is not found or the revocation
/// write fails.
pub async fn revoke_principal_in_conn(
    conn: &mut TenantConn<'_>,
    target_id: PrincipalId,
    tenant: DataTenantId,
) -> Result<PrincipalKindWire, WyrdError> {
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

    Err(WyrdError::PrincipalNotFound {
        message: format!("principal {target_id} not found in tenant {tenant}"),
        details: serde_json::json!({ "id": target_id.to_string() }),
    })
}

fn internal_error(error: impl std::fmt::Display) -> WyrdError {
    WyrdError::Internal {
        message: error.to_string(),
        details: serde_json::Value::Null,
    }
}
