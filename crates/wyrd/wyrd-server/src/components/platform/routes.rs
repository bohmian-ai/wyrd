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

use axum::Json;
use axum::extract::{Path, State};
use secrecy::{ExposeSecret, SecretString};
use wyrd_auth::platform_sessions::{
    DEFAULT_PLATFORM_TOKEN_TTL_MINUTES, PlatformSessionError, PlatformSessions,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::SecretBearer;
use wyrd_spec::auth::{
    CreateTenantRequest, CreateTenantResponse, PlatformTokenRequest, PlatformTokenResponse,
    ProvisionedTenant, ProvisionedTenantAdmin, RecoverTenantAdminRequest, SetTenantStatusRequest,
    TenantListResponse,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};

use crate::components::auth::PlatformCaller;
use crate::components::platform::provisioning::{ProvisionError, TenantProvisioning};
use crate::components::platform::recovery::TenantRecovery;
use crate::http::error::{WyrdErrorResponse, internal_failure};
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Build the authenticated platform control-plane routes.
///
/// Merged at the top level, not under `/v1`: see this module's documentation
/// for why the two planes have separate entries.
pub fn platform_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_tenant, list_tenants))
        .routes(routes!(inspect_tenant))
        .routes(routes!(set_tenant_status))
        .routes(routes!(recover_tenant_admin))
}

/// Build the anonymous platform credential-exchange route.
///
/// Merged outside the authenticated nests, like the tenant plane's
/// `/auth/token`: a caller presenting a credential has no session yet, so this
/// route cannot sit behind a session requirement.
pub fn platform_auth_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(platform_token))
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
        (status = 401, description = "Credential rejected, indistinguishably for every cause \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    // No session exists yet at this operation, so it clears the document-wide
    // requirement instead of inheriting it.
    security(()),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, request))]
async fn platform_token(
    State(state): State<AppState>,
    request_id: Option<axum::Extension<wyrd_spec::request_id::RequestId>>,
    Json(request): Json<PlatformTokenRequest>,
) -> Result<Json<PlatformTokenResponse>, WyrdErrorResponse> {
    let fallback_request_id: String;
    let req_id = match request_id.as_ref() {
        Some(axum::Extension(id)) => id.as_str(),
        None => {
            fallback_request_id = uuid::Uuid::new_v4().to_string();
            &fallback_request_id
        }
    };
    let Some(operator) = state.postgres.operator_pool() else {
        return Err(not_configured());
    };
    let Some(issuing_key) = state.auth.issuing_key.clone() else {
        return Err(not_configured());
    };

    let sessions = PlatformSessions::new(operator, issuing_key);
    let presented = SecretString::from(request.credential.expose().to_owned());
    match sessions.exchange(&presented, req_id).await {
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
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Tenant administrative recovery not granted \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No active tenant to recover, indistinguishably for every \
          cause (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
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
    let recovery = TenantRecovery::new(operator);
    let conn = state
        .postgres
        .tenant_conn(request.tenant_id)
        .await
        .map_err(|_| not_configured())?;

    recovery
        .recover(&caller, request.tenant_id, conn)
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
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Tenant creation not granted (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 409, description = "Slug already in use by a live tenant (WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
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
    let provisioning = TenantProvisioning::new(operator);

    // Two phases because the tenant id is only settled by the directory claim:
    // a resumed attempt adopts the failed attempt's id. The connection is
    // acquired here, at the boundary that owns pool composition, and the
    // acquisition result is handed on so provisioning can retire the row it
    // already committed if the tenant boundary is unreachable.
    let tenant_id = provisioning
        .claim(&caller, &request)
        .await
        .map_err(provision_error)?;
    let conn = state.postgres.tenant_conn(tenant_id).await;

    provisioning
        .provision(&caller, tenant_id, request, conn)
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

/// List the tenant directory, newest first.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the platform
/// plane is unconfigured, or the read fails.
#[utoipa::path(
    get,
    path = "/platform/tenants",
    responses(
        (status = 200, description = "Every live tenant, in every lifecycle state",
         body = TenantListResponse),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Tenant reading not granted (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn list_tenants(
    State(state): State<AppState>,
    caller: PlatformCaller,
) -> Result<Json<TenantListResponse>, WyrdErrorResponse> {
    directory(&state)?
        .list(&caller)
        .await
        .map(Json)
        .map_err(provision_error)
}

/// Read one tenant's directory row.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, no such tenant
/// exists, the platform plane is unconfigured, or the read fails.
#[utoipa::path(
    get,
    path = "/platform/tenants/{tenant_id}",
    params(("tenant_id" = String, Path, description = "Tenant to inspect")),
    responses(
        (status = 200, description = "The tenant's directory row", body = ProvisionedTenant),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Tenant reading not granted (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No such tenant, indistinguishably for every cause \
          (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn inspect_tenant(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Path(tenant_id): Path<DataTenantId>,
) -> Result<Json<ProvisionedTenant>, WyrdErrorResponse> {
    directory(&state)?
        .inspect(&caller, tenant_id)
        .await
        .map(Json)
        .map_err(provision_error)
}

/// Suspend a tenant or restore it.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized, the status is
/// not a settable one, the tenant is not in the state the transition requires,
/// the platform plane is unconfigured, or the write fails.
#[utoipa::path(
    put,
    path = "/platform/tenants/{tenant_id}/status",
    params(("tenant_id" = String, Path, description = "Tenant to transition")),
    request_body = SetTenantStatusRequest,
    responses(
        (status = 200, description = "The tenant now holds the requested status"),
        (status = 400, description = "Status is neither active nor suspended (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Tenant suspension not granted (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "Tenant is not in the state this transition requires \
          (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 500, description = "A platform store read or write failed, or the platform \
          decision could not be audited (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn set_tenant_status(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Path(tenant_id): Path<DataTenantId>,
    Json(request): Json<SetTenantStatusRequest>,
) -> Result<(), WyrdErrorResponse> {
    // Only the two states an operator may assert are settable. `provisioning`
    // and `failed` describe what provisioning observed, and letting an operator
    // declare them would contradict the record.
    let suspended = match request.status.as_str() {
        "suspended" => true,
        "active" => false,
        other => {
            return Err(WyrdErrorResponse::from(WyrdError::Validation {
                message: "tenant status must be active or suspended".to_owned(),
                details: serde_json::json!({ "field": "status", "value": other }),
            }));
        }
    };

    directory(&state)?
        .set_suspended(&caller, tenant_id, suspended)
        .await
        .map_err(provision_error)
}

/// Bind the tenant directory owner to this request, or report it unconfigured.
///
/// # Errors
/// Returns the unconfigured-plane error when the deployment has no operator
/// connection.
fn directory(state: &AppState) -> Result<TenantProvisioning, WyrdErrorResponse> {
    state
        .postgres
        .operator_pool()
        .map(TenantProvisioning::new)
        .ok_or_else(not_configured)
}
