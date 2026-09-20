//! Platform-scope credential administration.
//!
//! The deployment's root credential is established at initialization and, until
//! this surface existed, could never be rotated: issuing was internal, and
//! listing and revoking were reachable only from tests. An operator whose root
//! credential leaked had no move inside Wyrd.
//!
//! These three routes close that. They mirror the tenant plane's credential
//! surface exactly — issue returns the plaintext once, listing never returns
//! secret material, and revocation is per credential — but run on the platform
//! boundary and are authorized by their own permission, so rotating a credential
//! is not the same authority as registering who may sign in.
//!
//! There is deliberately no MCP projection: issuance returns a secret, and a
//! tool result is transcript material.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{Duration, Utc};
use secrecy::ExposeSecret;
use uuid::Uuid;
use wyrd_auth::platform_authz::{
    PlatformAuthorization, platform_credential_resource, platform_principal_resource,
};
use wyrd_auth::platform_credentials::{PlatformCredentialError, issue_platform_credential};
use wyrd_runtime::Permission;
use wyrd_spec::auth::{
    CredentialListResponse, CredentialMetadata, IssuePlatformCredentialRequest, IssuedCredential,
    SecretBearer,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_sql::queries::platform::credentials::{
    PlatformCredentialMetadataRow, list_platform_credentials, platform_credential_by_id,
    revoke_platform_credential,
};

use crate::components::auth::PlatformCaller;
use crate::components::platform::identity::{authorize, authorize_read, commit_decision, operator};
use crate::http::error::{WyrdErrorResponse, internal_failure};
use crate::state::AppState;

/// Build the platform credential-administration routes.
///
/// Nested under the principal they belong to, like the tenant plane's, so a
/// credential id alone can never address a credential across principals.
pub fn platform_credentials_router() -> Router<AppState> {
    Router::new()
        .route(
            "/platform/admins/{principal_id}/credentials",
            get(list_credentials).post(issue_credential),
        )
        .route(
            "/platform/admins/{principal_id}/credentials/{credential_id}",
            axum::routing::delete(revoke_credential),
        )
}

/// Mint a credential for an existing platform principal.
///
/// The plaintext is in the response and nowhere else: it is never persisted,
/// logged, or traced, and no later read can recover it.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the decision
/// cannot be audited — in which case nothing is minted — the principal does not
/// exist, or the write fails.
#[utoipa::path(
    post,
    path = "/platform/admins/{principal_id}/credentials",
    params(("principal_id" = String, Path, description = "Platform principal to issue for")),
    request_body = IssuePlatformCredentialRequest,
    responses(
        (status = 200, description = "Credential issued, plaintext returned once",
         body = IssuedCredential),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform credential administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn issue_credential(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Path(principal_id): Path<Uuid>,
    Json(request): Json<IssuePlatformCredentialRequest>,
) -> Result<Json<IssuedCredential>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    // The handle must outlive the transaction it lends out.
    let authz = PlatformAuthorization::new(pool.clone());
    let mut decision = authorize(
        &authz,
        &caller,
        &Permission::platform_credential_write(),
        &platform_principal_resource(principal_id),
    )
    .await?;

    let expires_at = request
        .expires_in_days
        .map(|days| Utc::now() + Duration::days(i64::from(days)));
    // The credential and the allowance permitting it commit together: a secret
    // that outlived a failed decision would be usable authority nothing
    // recorded granting.
    let issued = issue_platform_credential(&mut decision, principal_id, expires_at)
        .await
        .map_err(credential_error)?;
    commit_decision(decision).await?;

    Ok(Json(IssuedCredential {
        id: issued.id.to_string(),
        credential: SecretBearer::new(issued.credential.secret.expose_secret().to_owned()),
    }))
}

/// List a platform principal's credential metadata, newest first.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the decision
/// cannot be audited, or the read fails.
#[utoipa::path(
    get,
    path = "/platform/admins/{principal_id}/credentials",
    params(("principal_id" = String, Path, description = "Platform principal whose credentials to list")),
    responses(
        (status = 200, description = "Non-secret credential metadata, newest first",
         body = CredentialListResponse),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform credential administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn list_credentials(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Path(principal_id): Path<Uuid>,
) -> Result<Json<CredentialListResponse>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    authorize_read(
        &pool,
        &caller,
        &Permission::platform_credential_read(),
        &platform_principal_resource(principal_id),
    )
    .await?;

    let rows = list_platform_credentials(&pool, principal_id)
        .await
        .map_err(|error| {
            WyrdErrorResponse::from(internal_failure(
                "platform credentials could not be listed",
                &error,
            ))
        })?;

    Ok(Json(CredentialListResponse {
        credentials: rows.into_iter().map(metadata).collect(),
    }))
}

/// Retire one platform credential.
///
/// Revocation takes effect on the next request rather than at the next token
/// expiry: a platform session names the credential that minted it, and session
/// verification reads that credential's state every time. So a retired
/// credential's live sessions stop working immediately, and there is no epoch
/// to advance.
///
/// The credential must belong to the principal the path names, so a credential
/// id alone cannot retire some other principal's credential. A probe for one
/// that does not still commits its authorization decision, because that attempt
/// is exactly what an operator needs to find in the audit log.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the decision
/// cannot be audited, the credential is not this principal's or is already
/// retired, or the write fails.
#[utoipa::path(
    delete,
    path = "/platform/admins/{principal_id}/credentials/{credential_id}",
    params(
        ("principal_id" = String, Path, description = "Platform principal that owns the credential"),
        ("credential_id" = String, Path, description = "Credential to retire")
    ),
    responses(
        (status = 204, description = "Credential retired"),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform credential administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No live credential for this platform principal \
          (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn revoke_credential(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Path((principal_id, credential_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, WyrdErrorResponse> {
    let pool = operator(&state)?;
    // The handle must outlive the transaction it lends out.
    let authz = PlatformAuthorization::new(pool.clone());
    let mut decision = authorize(
        &authz,
        &caller,
        &Permission::platform_credential_write(),
        &platform_credential_resource(credential_id),
    )
    .await?;

    let owned = platform_credential_by_id(&pool, credential_id)
        .await
        .map_err(|error| {
            WyrdErrorResponse::from(internal_failure(
                "platform credential could not be read",
                &error,
            ))
        })?
        .is_some_and(|row| row.principal_id == principal_id);
    if !owned {
        return Err(not_found());
    }

    if revoke_platform_credential(&mut decision, credential_id)
        .await
        .map_err(|error| {
            WyrdErrorResponse::from(internal_failure(
                "platform credential could not be revoked",
                &error,
            ))
        })?
    {
        commit_decision(decision).await?;
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

/// Project one stored credential row onto the shared wire metadata.
///
/// The same shape the tenant plane returns: an operator rotating either plane's
/// credentials reads one format, and nothing secret is in it.
fn metadata(row: PlatformCredentialMetadataRow) -> CredentialMetadata {
    CredentialMetadata {
        id: row.id.to_string(),
        prefix: row.prefix,
        created_at: row.created_at.to_rfc3339(),
        expires_at: row.expires_at.map(|at| at.to_rfc3339()),
        revoked_at: row.revoked_at.map(|at| at.to_rfc3339()),
        last_used_at: row.last_used_at.map(|at| at.to_rfc3339()),
    }
}

/// The one refusal an unknown or already-retired credential earns.
///
/// Deliberately identical for both, so a caller cannot use the difference to
/// learn which credential ids exist on the platform plane.
fn not_found() -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::NotFound {
        message: "no live credential for this platform principal".to_owned(),
        details: serde_json::json!({}),
    })
}

/// Project an issuance failure onto the public catalog.
///
/// Every cause is internal: the caller supplied a principal id and an optional
/// lifetime, and a hashing, runtime, or store failure tells them nothing they
/// can act on while naming infrastructure.
fn credential_error(error: PlatformCredentialError) -> WyrdErrorResponse {
    WyrdErrorResponse::from(internal_failure(
        "platform credential could not be issued",
        &error,
    ))
}
