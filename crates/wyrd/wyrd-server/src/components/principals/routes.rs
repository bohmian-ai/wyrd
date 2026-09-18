//! Tenant-scoped principal and credential administration.
//!
//! A tenant administrator manages its own tenant here: creating machine
//! principals for automation, granting them a narrower set of roles, and
//! issuing, listing, and revoking their credentials. Nothing on this surface
//! reaches the tenant directory or another tenant — it runs entirely inside the
//! authenticated caller's tenant, under row-level security.
//!
//! Credentials are separate from identity throughout. Issuing a second
//! credential and revoking the first is a rotation with no gap, and it never
//! touches the principal or its roles.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Duration;
use secrecy::ExposeSecret;
use uuid::Uuid;
use wyrd_auth::issue_api_key::WyrdApiKey;
use wyrd_runtime::{Permission, RoleRef};
use wyrd_spec::auth::{
    CreateServicePrincipalRequest, CreateServicePrincipalResponse, CredentialListResponse,
    CredentialMetadata, IssuedCredential, SecretBearer,
};
use wyrd_spec::error::WyrdError;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    ApiKeyMetadataRow, grant_role_to_service_account, insert_api_key, insert_service_account,
    list_api_key_metadata, revoke_api_key, role_by_name,
};

use crate::components::auth::Caller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Lifetime of a credential issued to tenant automation.
const AUTOMATION_CREDENTIAL_DAYS: i64 = 90;

/// Build the tenant principal-administration routes for the `/v1` group.
pub fn principals_router() -> Router<AppState> {
    Router::new()
        .route("/principals", post(create_service_principal))
        .route(
            "/principals/{principal_id}/credentials",
            get(list_credentials).post(issue_credential),
        )
        .route(
            "/principals/{principal_id}/credentials/{credential_id}",
            axum::routing::delete(revoke_credential),
        )
}

/// Refuse a caller that lacks tenant principal-management authority.
///
/// # Errors
/// Returns [`WyrdError::PermissionDeniedRbac`] when the permission is not held.
fn require_principal_admin(caller: &Caller) -> Result<(), WyrdErrorResponse> {
    if caller
        .principal
        .effective_permissions
        .contains(&Permission::service_accounts_write())
    {
        return Ok(());
    }
    Err(WyrdErrorResponse::from(WyrdError::PermissionDeniedRbac {
        message: "caller lacks tenant principal administration".to_owned(),
        details: serde_json::json!({ "required": "service_accounts:write" }),
    }))
}

/// Open a transaction scoped to the caller's tenant.
///
/// Tenant identity comes from the verified principal, never the request, so no
/// caller can steer this at another tenant.
///
/// # Errors
/// Returns an internal error when the connection cannot be opened.
async fn tenant_conn<'a>(
    state: &'a AppState,
    caller: &Caller,
) -> Result<TenantConn<'a>, WyrdErrorResponse> {
    TenantConn::acquire(state.postgres.app_pool(), caller.data_tenant_id)
        .await
        .map_err(|error| {
            WyrdErrorResponse::from(WyrdError::Internal {
                message: "tenant connection unavailable".to_owned(),
                details: serde_json::json!({ "error": error.to_string() }),
            })
        })
}

/// Mint a credential for `principal_id` inside the caller's tenant.
///
/// # Errors
/// Returns an internal error when hashing or the insert fails.
async fn mint_credential(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
    created_by: Uuid,
) -> Result<IssuedCredential, WyrdErrorResponse> {
    let plaintext = WyrdApiKey::generate(conn.data_tenant_id());
    let raw = plaintext.secret.clone();
    let key_hash = tokio::task::spawn_blocking(move || wyrd_auth_issue::hash_api_key(&raw))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    let credential_id = Uuid::now_v7();
    insert_api_key(
        conn,
        credential_id,
        principal_id,
        &plaintext.prefix,
        &key_hash,
        created_by,
        Some(chrono::Utc::now() + Duration::days(AUTOMATION_CREDENTIAL_DAYS)),
    )
    .await
    .map_err(internal)?;

    Ok(IssuedCredential {
        id: credential_id.to_string(),
        credential: SecretBearer::new(plaintext.secret.expose_secret().to_owned()),
    })
}

/// Create a Card-free machine principal and issue its first credential.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, a requested
/// role does not exist in the tenant, or a write fails.
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn create_service_principal(
    State(state): State<AppState>,
    caller: Caller,
    Json(request): Json<CreateServicePrincipalRequest>,
) -> Result<Json<CreateServicePrincipalResponse>, WyrdErrorResponse> {
    require_principal_admin(&caller)?;
    for role in &request.roles {
        RoleRef::new(role).map_err(|_| {
            WyrdErrorResponse::from(WyrdError::Validation {
                message: "role name is not valid".to_owned(),
                details: serde_json::json!({ "role": role }),
            })
        })?;
    }

    let mut conn = tenant_conn(&state, &caller).await?;
    let principal_id = Uuid::now_v7();
    insert_service_account(
        &mut conn,
        principal_id,
        "service",
        None,
        &request.name,
        request.description.as_deref(),
        caller.principal.id.as_uuid(),
    )
    .await
    .map_err(internal)?;

    for role in &request.roles {
        let row = role_by_name(&mut conn, role)
            .await
            .map_err(internal)?
            .ok_or_else(|| {
                WyrdErrorResponse::from(WyrdError::Validation {
                    message: "role does not exist in this tenant".to_owned(),
                    details: serde_json::json!({ "role": role }),
                })
            })?;
        grant_role_to_service_account(&mut conn, principal_id, row.id)
            .await
            .map_err(internal)?;
    }

    let issued = mint_credential(&mut conn, principal_id, caller.principal.id.as_uuid()).await?;
    conn.commit().await.map_err(internal)?;

    Ok(Json(CreateServicePrincipalResponse {
        principal_id: wyrd_spec::auth::PrincipalId::new(principal_id),
        credential: issued.credential,
    }))
}

/// Issue an additional credential for an existing principal.
///
/// This is the first half of a rotation: the principal now holds two live
/// credentials, so the old one can be retired once the new one is verified.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized or a write
/// fails.
#[tracing::instrument(level = "info", skip(state, caller))]
async fn issue_credential(
    State(state): State<AppState>,
    caller: Caller,
    Path(principal_id): Path<Uuid>,
) -> Result<Json<IssuedCredential>, WyrdErrorResponse> {
    require_principal_admin(&caller)?;
    let mut conn = tenant_conn(&state, &caller).await?;
    let issued = mint_credential(&mut conn, principal_id, caller.principal.id.as_uuid()).await?;
    conn.commit().await.map_err(internal)?;
    Ok(Json(issued))
}

/// List a principal's credential metadata.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized or the read
/// fails.
#[tracing::instrument(level = "info", skip(state, caller))]
async fn list_credentials(
    State(state): State<AppState>,
    caller: Caller,
    Path(principal_id): Path<Uuid>,
) -> Result<Json<CredentialListResponse>, WyrdErrorResponse> {
    require_principal_admin(&caller)?;
    let mut conn = tenant_conn(&state, &caller).await?;
    let rows = list_api_key_metadata(&mut conn, principal_id)
        .await
        .map_err(internal)?;
    conn.commit().await.map_err(internal)?;

    Ok(Json(CredentialListResponse {
        credentials: rows.into_iter().map(metadata).collect(),
    }))
}

/// Revoke one credential, leaving the principal and its other credentials
/// untouched.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the credential
/// is unknown, or the write fails.
#[tracing::instrument(level = "info", skip(state, caller))]
async fn revoke_credential(
    State(state): State<AppState>,
    caller: Caller,
    Path((_principal_id, credential_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, WyrdErrorResponse> {
    require_principal_admin(&caller)?;
    let mut conn = tenant_conn(&state, &caller).await?;
    let revoked = revoke_api_key(&mut conn, credential_id)
        .await
        .map_err(internal)?;
    conn.commit().await.map_err(internal)?;

    if revoked {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(WyrdErrorResponse::from(WyrdError::NotFound {
            message: "credential not found in this tenant".to_owned(),
            details: serde_json::json!({}),
        }))
    }
}

/// Project a stored credential row onto its non-secret wire metadata.
fn metadata(row: ApiKeyMetadataRow) -> CredentialMetadata {
    CredentialMetadata {
        id: row.id.to_string(),
        prefix: row.prefix,
        created_at: row.created_at.to_rfc3339(),
        expires_at: row.expires_at.map(|at| at.to_rfc3339()),
        revoked_at: row.revoked_at.map(|at| at.to_rfc3339()),
        last_used_at: row.last_used_at.map(|at| at.to_rfc3339()),
    }
}

/// Map any internal failure onto the stable catalog without leaking detail.
fn internal(error: impl std::fmt::Display) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Internal {
        message: "tenant principal administration failed".to_owned(),
        details: serde_json::json!({ "error": error.to_string() }),
    })
}
