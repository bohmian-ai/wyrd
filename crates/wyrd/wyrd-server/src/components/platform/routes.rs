//! Platform control-plane routes.
//!
//! Every route here takes [`PlatformCaller`], so a tenant identity cannot reach
//! them: the extractor is the only producer of a platform-scoped context and it
//! accepts only a platform session. Authorization and its audit record happen
//! inside each operation, in the transaction that performs it.
//!
//! These routes sit outside the `/v1` nest deliberately. That nest is
//! default-deny on *tenant* access tokens, so a platform session would be
//! refused there before reaching a handler — and a tenant token must never be
//! accepted here. Two planes, two entries.

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use secrecy::{ExposeSecret, SecretString};
use wyrd_auth::platform_sessions::{
    DEFAULT_PLATFORM_TOKEN_TTL_MINUTES, PlatformSessionError, PlatformSessions,
};
use wyrd_spec::auth::SecretBearer;
use wyrd_spec::auth::{
    CreateTenantRequest, CreateTenantResponse, PlatformTokenRequest, PlatformTokenResponse,
    ProvisionedTenantAdmin, RecoverTenantAdminRequest,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};

use crate::components::auth::PlatformCaller;
use crate::components::platform::provisioning::{ProvisionError, TenantProvisioning};
use crate::components::platform::recovery::TenantRecovery;
use crate::http::error::{WyrdErrorResponse, internal_failure};
use crate::state::AppState;

/// Build the authenticated platform control-plane routes.
///
/// Merged at the top level, not under `/v1`: see this module's documentation
/// for why the two planes have separate entries.
pub fn platform_router() -> Router<AppState> {
    Router::new()
        .route("/platform/tenants", post(create_tenant))
        .route(
            "/platform/tenants/admin/credentials",
            post(recover_tenant_admin),
        )
}

/// Build the anonymous platform credential-exchange route.
///
/// Merged outside the authenticated nests, like the tenant plane's
/// `/auth/token`: a caller presenting a credential has no session yet, so this
/// route cannot sit behind a session requirement.
pub fn platform_auth_router() -> Router<AppState> {
    Router::new().route("/auth/platform/token", post(platform_token))
}

/// Exchange a platform credential for a short-lived session.
///
/// The one route that reads credential material. Every other platform route
/// takes the session this mints, so no served surface after this point handles
/// a secret.
///
/// # Errors
/// Returns an unauthenticated error for every credential rejection, so no
/// caller can distinguish which condition failed.
#[utoipa::path(
    post,
    path = "/auth/platform/token",
    request_body = PlatformTokenRequest,
    responses(
        (status = 200, description = "Short-lived platform session", body = PlatformTokenResponse),
        (status = 401, description = "Credential rejected, indistinguishably for every cause", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, request))]
async fn platform_token(
    State(state): State<AppState>,
    Json(request): Json<PlatformTokenRequest>,
) -> Result<Json<PlatformTokenResponse>, WyrdErrorResponse> {
    let Some(operator) = state.postgres.operator_pool() else {
        return Err(not_configured());
    };
    let Some(issuing_key) = state.auth.issuing_key.clone() else {
        return Err(not_configured());
    };

    let sessions = PlatformSessions::new(operator, issuing_key);
    let presented = SecretString::from(request.credential.expose().to_owned());
    match sessions.exchange(&presented).await {
        Ok(session) => Ok(Json(PlatformTokenResponse {
            access_token: SecretBearer::new(session.token.expose_secret().to_owned()),
            token_type: "Bearer".to_owned(),
            expires_in: u64::try_from(DEFAULT_PLATFORM_TOKEN_TTL_MINUTES * 60).unwrap_or(900),
        })),
        Err(PlatformSessionError::Invalid) => {
            Err(WyrdErrorResponse::from(WyrdError::Unauthenticated {
                message: "invalid platform credential".to_owned(),
                details: serde_json::json!({ "plane": "platform" }),
            }))
        }
        Err(error) => Err(WyrdErrorResponse::from(internal_failure(
            "platform session could not be issued",
            &error,
        ))),
    }
}

/// Restore administrative access to a tenant that has lost it.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the tenant has
/// no administrative principal, or a write fails.
#[utoipa::path(
    post,
    path = "/platform/tenants/admin/credentials",
    request_body = RecoverTenantAdminRequest,
    responses(
        (status = 200, description = "Replacement credential for the tenant's existing administrator",
         body = ProvisionedTenantAdmin),
        (status = 401, description = "Platform session required", body = WyrdProblem),
        (status = 403, description = "Tenant administrative recovery not granted", body = WyrdProblem),
        (status = 404, description = "No active tenant to recover, indistinguishably for every cause", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn recover_tenant_admin(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Json(request): Json<RecoverTenantAdminRequest>,
) -> Result<Json<ProvisionedTenantAdmin>, WyrdErrorResponse> {
    let Some(operator) = state.postgres.operator_pool() else {
        return Err(not_configured());
    };
    let recovery = TenantRecovery::new(operator, state.postgres.wyrd().clone());

    recovery
        .recover(&caller, request.tenant_id)
        .await
        .map(Json)
        .map_err(provision_error)
}

/// The platform plane has no operator connection or signing key configured.
fn not_configured() -> WyrdErrorResponse {
    WyrdErrorResponse::from(WyrdError::Internal {
        message: "platform control plane is not configured".to_owned(),
        details: serde_json::json!({ "plane": "platform" }),
    })
}

/// Provision a tenant and return its one-time administrative credential.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the slug is
/// taken, the platform plane is unconfigured, or provisioning fails.
#[utoipa::path(
    post,
    path = "/platform/tenants",
    request_body = CreateTenantRequest,
    responses(
        (status = 200, description = "Provisioned tenant and its one-time administrative credential",
         body = CreateTenantResponse),
        (status = 401, description = "Platform session required", body = WyrdProblem),
        (status = 403, description = "Tenant creation not granted", body = WyrdProblem),
        (status = 409, description = "Slug already in use by a live tenant", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn create_tenant(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Json(request): Json<CreateTenantRequest>,
) -> Result<Json<CreateTenantResponse>, WyrdErrorResponse> {
    let Some(operator) = state.postgres.operator_pool() else {
        return Err(not_configured());
    };
    let provisioning = TenantProvisioning::new(operator, state.postgres.wyrd().clone());

    provisioning
        .provision(&caller, request)
        .await
        .map(Json)
        .map_err(provision_error)
}

/// Project a provisioning failure onto the stable public error catalog.
///
/// A denial and a taken slug are caller-correctable and say so; everything else
/// is internal, because the caller can do nothing with a store failure's
/// details and they may name infrastructure.
fn provision_error(error: ProvisionError) -> WyrdErrorResponse {
    match error {
        ProvisionError::Denied => WyrdErrorResponse::from(WyrdError::PermissionDeniedRbac {
            message: "not authorized to provision tenants".to_owned(),
            details: serde_json::json!({ "permission": "tenants:write" }),
        }),
        ProvisionError::SlugTaken => WyrdErrorResponse::from(WyrdError::Conflict {
            message: "tenant slug is already in use".to_owned(),
            details: serde_json::json!({ "field": "slug" }),
        }),
        ProvisionError::TenantUnavailable => WyrdErrorResponse::from(WyrdError::NotFound {
            message: "no active tenant to act on".to_owned(),
            details: serde_json::json!({ "resource": "tenant" }),
        }),
        ProvisionError::AuditUnavailable(reason) | ProvisionError::Store(reason) => {
            WyrdErrorResponse::from(internal_failure("tenant provisioning failed", &reason))
        }
    }
}
