//! Platform-scope human identity routes.
//!
//! Configuring the deployment's one OIDC connection, registering the humans who
//! may sign in through it, and the login pair itself. Like the rest of the
//! platform plane these sit outside the `/v1` nest, and the two login routes are
//! anonymous by necessity: someone signing in has no session yet.
//!
//! Federated login is additive. Nothing here can take the global credential
//! away, so removing the connection or losing the provider leaves the
//! deployment administrable.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;
use wyrd_auth::pg_resolvers::{client_auth_label, seal_platform_client_secret};
use wyrd_auth::platform_authz::{PlatformAuthorization, PlatformAuthzError};
use wyrd_auth::platform_login::{PlatformLogin, PlatformLoginError};
use wyrd_auth_oidc::ClientAuth;
use wyrd_runtime::Permission;
use wyrd_spec::auth::LoginInitResponse;
use wyrd_spec::auth::{
    ConfigurePlatformOidcRequest, PlatformCallbackRequest, PlatformClientAuth,
    PlatformLoginRequest, PlatformOidcConnectionView, PlatformTokenResponse, PrincipalId,
    PrincipalKindTag, RegisterPlatformAdminRequest, RegisterPlatformAdminResponse, SecretBearer,
};
use wyrd_spec::error::WyrdError;
use wyrd_sql::queries::platform::identity::{
    delete_platform_oidc_connection, insert_platform_identity, platform_oidc_connection,
    upsert_platform_oidc_connection,
};
use wyrd_sql::queries::platform::principals::insert_platform_principal;
use wyrd_sql::{OperatorPool, SqlError};

use crate::components::auth::PlatformCaller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Claim mapping the platform connection uses.
///
/// Fixed rather than configurable: the platform plane matches a pre-registered
/// administrator on their email and pins the standard `sub`. There is no
/// group-to-role mapping here at all, because a platform principal's authority
/// comes from its grant and must never be assertable by a provider.
fn platform_claim_mapping() -> serde_json::Value {
    serde_json::json!({
        "subject": ["sub"],
        "email": ["email"],
        "groups": null,
    })
}

/// Build the authenticated platform identity routes.
pub fn platform_identity_router() -> Router<AppState> {
    Router::new()
        .route(
            "/platform/oidc/connection",
            get(read_connection)
                .put(configure_connection)
                .delete(remove_connection),
        )
        .route("/platform/admins", post(register_admin))
}

/// Build the anonymous platform login routes.
///
/// Separate from the authenticated router because a human signing in has no
/// platform session yet — requiring one would make federated login impossible
/// for exactly the people it exists for.
pub fn platform_login_router() -> Router<AppState> {
    Router::new()
        .route("/auth/platform/login", post(begin_login))
        .route("/auth/platform/callback", post(complete_login))
}

/// Resolve the platform boundary, or report the plane unconfigured.
///
/// # Errors
/// Returns an internal error when no operator connection is configured.
fn operator(state: &AppState) -> Result<OperatorPool, WyrdErrorResponse> {
    state.postgres.operator_pool().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "platform control plane is not configured".to_owned(),
            details: serde_json::json!({ "plane": "platform" }),
        })
    })
}

/// Authorize one platform identity operation, recording the decision.
///
/// Reuses the platform plane's single audited authorization entry point, so a
/// decision here is recorded in the transaction that made it exactly as a
/// tenant-lifecycle decision is.
///
/// # Errors
/// Returns a permission error when the grant does not cover `required`, and an
/// internal error when the decision cannot be recorded — in which case nothing
/// is performed.
async fn authorize(
    pool: &OperatorPool,
    caller: &PlatformCaller,
    required: &Permission,
) -> Result<(), WyrdErrorResponse> {
    let authz = PlatformAuthorization::new(pool.clone());
    let tx = authz
        .authorize(
            &caller.context,
            required,
            caller.request_id.as_str(),
            caller.credential_id,
            None,
        )
        .await
        .map_err(platform_authz_error)?;
    tx.commit().await.map_err(|error| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "platform authorization could not be committed".to_owned(),
            details: serde_json::json!({ "error": error.to_string() }),
        })
    })
}

/// Install or replace the deployment's platform OIDC connection.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the client
/// secret cannot be sealed, or the write fails.
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn configure_connection(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Json(request): Json<ConfigurePlatformOidcRequest>,
) -> Result<Json<PlatformOidcConnectionView>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    authorize(&pool, &caller, &Permission::platform_identity_write()).await?;

    let client_auth = match request.client_auth {
        PlatformClientAuth::SecretBasic { secret } => {
            ClientAuth::SecretBasic(SecretString::from(secret))
        }
        PlatformClientAuth::SecretPost { secret } => {
            ClientAuth::SecretPost(SecretString::from(secret))
        }
        PlatformClientAuth::Public => ClientAuth::Public,
    };
    let sealed = seal_platform_client_secret(&client_auth, state.auth.sealing_key.as_deref())
        .map_err(|error| {
            // A secret-bearing connection with no sealing key fails closed
            // rather than being stored in the clear.
            WyrdErrorResponse::from(WyrdError::Validation {
                message: "platform client secret could not be sealed".to_owned(),
                details: serde_json::json!({ "reason": error.to_string() }),
            })
        })?;

    upsert_platform_oidc_connection(
        &pool,
        &request.issuer_url,
        &request.jwks_uri,
        &request.expected_audience,
        &request.client_id,
        client_auth_label(&client_auth),
        &platform_claim_mapping(),
        request.jwks_ttl_secs,
        sealed.as_deref(),
    )
    .await
    .map_err(store_error)?;

    Ok(Json(PlatformOidcConnectionView {
        issuer_url: request.issuer_url,
        jwks_uri: request.jwks_uri,
        expected_audience: request.expected_audience,
        client_id: request.client_id,
        client_auth: client_auth_label(&client_auth).to_owned(),
        jwks_ttl_secs: request.jwks_ttl_secs,
    }))
}

/// Read the configured connection, with its secret omitted.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, no connection
/// is configured, or the read fails.
#[tracing::instrument(level = "info", skip(state, caller))]
async fn read_connection(
    State(state): State<AppState>,
    caller: PlatformCaller,
) -> Result<Json<PlatformOidcConnectionView>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    authorize(&pool, &caller, &Permission::platform_identity_read()).await?;

    let row = platform_oidc_connection(&pool)
        .await
        .map_err(store_error)?
        .ok_or_else(|| {
            WyrdErrorResponse::from(WyrdError::NotFound {
                message: "no platform OIDC connection is configured".to_owned(),
                details: serde_json::json!({}),
            })
        })?;

    // The sealed secret is read from the row and deliberately not projected.
    Ok(Json(PlatformOidcConnectionView {
        issuer_url: row.issuer_url,
        jwks_uri: row.jwks_uri,
        expected_audience: row.expected_audience,
        client_id: row.client_id,
        client_auth: row.client_auth,
        jwks_ttl_secs: row.jwks_ttl_secs,
    }))
}

/// Remove the platform OIDC connection.
///
/// Platform principals and their grants are untouched. Federated login stops
/// working; the global credential does not, which is the whole reason removal
/// is safe to expose.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized or the delete
/// fails.
#[tracing::instrument(level = "info", skip(state, caller))]
async fn remove_connection(
    State(state): State<AppState>,
    caller: PlatformCaller,
) -> Result<axum::http::StatusCode, WyrdErrorResponse> {
    let pool = operator(&state)?;
    authorize(&pool, &caller, &Permission::platform_identity_write()).await?;

    if delete_platform_oidc_connection(&pool)
        .await
        .map_err(store_error)?
    {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(WyrdErrorResponse::from(WyrdError::NotFound {
            message: "no platform OIDC connection is configured".to_owned(),
            details: serde_json::json!({}),
        }))
    }
}

/// Pre-register a human platform administrator.
///
/// The principal is created with no credential at all: this human will
/// authenticate by federated identity, and their subject is pinned on first
/// login. Only a caller who already holds platform authority reaches here,
/// which is what stops a login from manufacturing platform authority.
///
/// The registration carries no grant. Authority is granted separately and
/// deliberately, so registering someone is never the same act as empowering
/// them.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, no connection
/// is configured, the name or claim is already registered, or a write fails.
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn register_admin(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Json(request): Json<RegisterPlatformAdminRequest>,
) -> Result<Json<RegisterPlatformAdminResponse>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    authorize(&pool, &caller, &Permission::platform_identity_write()).await?;

    // Registering against no connection would create a principal that can never
    // sign in, so the connection is required first.
    let connection = platform_oidc_connection(&pool)
        .await
        .map_err(store_error)?
        .ok_or_else(|| {
            WyrdErrorResponse::from(WyrdError::Validation {
                message: "configure the platform OIDC connection before registering an \
                          administrator"
                    .to_owned(),
                details: serde_json::json!({}),
            })
        })?;

    let principal_id = Uuid::now_v7();
    insert_platform_principal(&pool, principal_id, PrincipalKindTag::User, &request.name)
        .await
        .map_err(taken_or_store)?;
    insert_platform_identity(
        &pool,
        principal_id,
        &connection.issuer_url,
        &request.match_claim,
    )
    .await
    .map_err(taken_or_store)?;

    Ok(Json(RegisterPlatformAdminResponse {
        principal_id: PrincipalId::new(principal_id),
    }))
}

/// Build the platform login service from server state.
///
/// # Errors
/// Returns an internal error when the platform plane, the token verifier, or
/// the signing key is not configured.
fn login_service(state: &AppState) -> Result<PlatformLogin, WyrdErrorResponse> {
    let pool = operator(state)?;
    let issuing_key = state.auth.issuing_key.clone().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "platform session issuance is not configured".to_owned(),
            details: serde_json::json!({ "plane": "platform" }),
        })
    })?;
    let verifier = state.auth.token_verifier.clone().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "token verification is not configured".to_owned(),
            details: serde_json::json!({ "plane": "platform" }),
        })
    })?;
    let sessions = std::sync::Arc::new(wyrd_auth::platform_sessions::PlatformSessions::new(
        pool.clone(),
        issuing_key,
    ));
    Ok(PlatformLogin::new(
        pool,
        state.auth.sealing_key.clone(),
        verifier,
        sessions,
    ))
}

/// Begin a platform federated login.
///
/// # Errors
/// Returns a stable Wyrd error when federated login is not configured, the
/// provider cannot be reached, or the login state cannot be persisted.
#[tracing::instrument(level = "info", skip(state, request))]
async fn begin_login(
    State(state): State<AppState>,
    Json(request): Json<PlatformLoginRequest>,
) -> Result<Json<LoginInitResponse>, WyrdErrorResponse> {
    login_service(&state)?
        .begin(request.redirect_uri)
        .await
        .map(Json)
        .map_err(login_error)
}

/// Complete a platform federated login and return a platform session.
///
/// # Errors
/// Returns a stable Wyrd error when the state is missing or replayed, the
/// provider cannot be reached, or the identity is not accepted.
#[tracing::instrument(level = "info", skip(state, request))]
async fn complete_login(
    State(state): State<AppState>,
    Json(request): Json<PlatformCallbackRequest>,
) -> Result<Json<PlatformTokenResponse>, WyrdErrorResponse> {
    let token = login_service(&state)?
        .complete(SecretString::from(request.code), &request.state)
        .await
        .map_err(login_error)?;

    Ok(Json(PlatformTokenResponse {
        access_token: SecretBearer::new(token.expose_secret().to_owned()),
        token_type: "Bearer".to_owned(),
        expires_in: u64::try_from(
            wyrd_auth::platform_sessions::DEFAULT_PLATFORM_TOKEN_TTL_MINUTES * 60,
        )
        .unwrap_or(900),
    }))
}

/// Project a platform authorization failure onto the public catalog.
fn platform_authz_error(error: PlatformAuthzError) -> WyrdErrorResponse {
    match error {
        PlatformAuthzError::Denied { .. } => {
            WyrdErrorResponse::from(WyrdError::PermissionDeniedRbac {
                message: "not authorized to administer platform identity".to_owned(),
                details: serde_json::json!({ "permission": "platform_identity:write" }),
            })
        }
        PlatformAuthzError::AuditUnavailable(reason) => {
            WyrdErrorResponse::from(WyrdError::Internal {
                message: "platform authorization could not be audited".to_owned(),
                details: serde_json::json!({ "error": reason.to_string() }),
            })
        }
        PlatformAuthzError::Transaction(error) => WyrdErrorResponse::from(WyrdError::Internal {
            message: "platform authorization failed".to_owned(),
            details: serde_json::json!({ "error": error.to_string() }),
        }),
    }
}

/// Project a login failure onto the public catalog.
///
/// Every identity rejection collapses to one unauthenticated error, so a caller
/// learns whether it got in and never whether a subject is registered.
fn login_error(error: PlatformLoginError) -> WyrdErrorResponse {
    match error {
        PlatformLoginError::NotConfigured => WyrdErrorResponse::from(WyrdError::NotFound {
            message: "platform federated login is not configured".to_owned(),
            details: serde_json::json!({}),
        }),
        PlatformLoginError::NotAccepted | PlatformLoginError::InvalidState => {
            WyrdErrorResponse::from(WyrdError::Unauthenticated {
                message: "federated identity was not accepted".to_owned(),
                details: serde_json::json!({ "plane": "platform" }),
            })
        }
        PlatformLoginError::ProviderUnavailable(_) | PlatformLoginError::ConnectionUnusable => {
            WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
                message: "identity provider unavailable".to_owned(),
                details: serde_json::json!({ "retry_after_seconds": 1 }),
            })
        }
        PlatformLoginError::Store(error) => WyrdErrorResponse::from(WyrdError::Internal {
            message: "platform login failed".to_owned(),
            details: serde_json::json!({ "error": error.to_string() }),
        }),
        PlatformLoginError::Session(reason) => WyrdErrorResponse::from(WyrdError::Internal {
            message: "platform session could not be issued".to_owned(),
            details: serde_json::json!({ "error": reason }),
        }),
    }
}

/// Distinguish an already-registered name or claim from a store failure.
fn taken_or_store(error: SqlError) -> WyrdErrorResponse {
    match error {
        SqlError::UniqueViolation { .. } => WyrdErrorResponse::from(WyrdError::Conflict {
            message: "that name or matching claim is already registered".to_owned(),
            details: serde_json::json!({}),
        }),
        other => store_error(other),
    }
}

/// Project a platform store failure onto the public catalog.
fn store_error(error: SqlError) -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Internal {
        message: "platform identity store failed".to_owned(),
        details: serde_json::json!({ "error": error.to_string() }),
    })
}
