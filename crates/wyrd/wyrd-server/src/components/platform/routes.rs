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

use axum::Extension;
use axum::Json;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use secrecy::{ExposeSecret, SecretString};
use wyrd_auth::platform_sessions::{
    DEFAULT_PLATFORM_TOKEN_TTL_MINUTES, PlatformSessionError, PlatformSessions,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    CreateTenantRequest, CreateTenantResponse, ExchangeTokenType, OAuthErrorCode,
    OAuthErrorResponse, ProvisionedTenant, ProvisionedTenantAdmin, RecoverTenantAdminRequest,
    SetTenantStatusRequest, TenantListResponse,
};
use wyrd_spec::auth::{SecretBearer, TokenRequest, TokenResponse, TokenType};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::request_id::RequestId;

use crate::auth::oauth::{OAuthError, OAuthForm, TOKEN_EXCHANGE, no_store};
use crate::components::auth::PlatformCaller;
use crate::components::platform::provisioning::{ProvisionError, TenantProvisioning};
use crate::components::platform::recovery::TenantRecovery;
use crate::http::error::{WyrdErrorResponse, internal_failure, path_rejection};
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

/// Exchange a platform credential for a short-lived session through the
/// RFC 8693 token exchange.
///
/// The one route that reads credential material. Every other platform route
/// takes the session this mints, so no served surface after this point handles
/// a secret. The form carries the credential as an API-key `subject_token`
/// and nothing else; the reply is the RFC 8693 §2.2.1 token response.
///
/// # Errors
/// Answers the RFC 6749 §5.2 body: `invalid_request` for a malformed
/// request and, per RFC 8693 §2.2.2, for every credential rejection,
/// indistinguishably; `invalid_target` for an unsupported `audience`;
/// `unsupported_grant_type` for another grant; and
/// `server_error` when the platform store or its audit fails.
#[utoipa::path(
    post,
    path = "/auth/platform/token",
    request_body(content = TokenRequest, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Short-lived platform session", body = TokenResponse),
        (status = 400, description = "`invalid_request`, including every credential \
          rejection, `invalid_target`, or `unsupported_grant_type`", body = OAuthErrorResponse),
        (status = 500, description = "`server_error`: a platform store read or write \
          failed, or the platform decision could not be audited", body = OAuthErrorResponse)
    ),
    // No session exists yet at this operation, so it clears the document-wide
    // requirement instead of inheriting it.
    security(()),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip_all)]
async fn platform_token(
    State(state): State<AppState>,
    request_id: Option<Extension<RequestId>>,
    form: OAuthForm,
) -> Result<Response, OAuthError> {
    match form.get("grant_type") {
        Some(TOKEN_EXCHANGE) => {}
        Some(_) => return Err(OAuthError(OAuthErrorCode::UnsupportedGrantType)),
        None => return Err(OAuthError(OAuthErrorCode::InvalidRequest)),
    }
    let TokenRequest::TokenExchange {
        subject_token,
        subject_token_type: ExchangeTokenType::ApiKey,
        actor_token: None,
        actor_token_type: None,
        audience: None,
    } = form.token_request()?
    else {
        return Err(OAuthError(OAuthErrorCode::InvalidRequest));
    };
    let request_id = request_id.map_or_else(RequestId::now_v7, |Extension(id)| id);
    let (Some(operator), Some(issuing_key)) = (
        state.postgres.operator_pool(),
        state.auth.issuing_key.clone(),
    ) else {
        return Err(OAuthError::from(not_configured().0));
    };

    let sessions = PlatformSessions::new(operator, issuing_key);
    let presented = SecretString::from(subject_token.expose().to_owned());
    match sessions.exchange(&presented, request_id.as_str()).await {
        Ok(session) => Ok(no_store(
            StatusCode::OK,
            Json(TokenResponse {
                issued_token_type: Some(ExchangeTokenType::AccessToken),
                ..platform_session_response(session.token.expose_secret())
            }),
        )),
        Err(PlatformSessionError::Invalid) => {
            Err(OAuthError::request(WyrdError::Unauthenticated {
                message: "invalid platform credential".to_owned(),
                details: serde_json::json!({ "plane": "platform" }),
            }))
        }
        Err(error) => Err(OAuthError::from(internal_failure(
            "platform session could not be issued",
            &error,
        ))),
    }
}

/// The RFC 6749 §5.1 response carrying a freshly minted platform session.
pub(crate) fn platform_session_response(session: &str) -> TokenResponse {
    TokenResponse {
        access_token: SecretBearer::new(session.to_owned()),
        token_type: TokenType::Bearer,
        expires_in: u64::try_from(DEFAULT_PLATFORM_TOKEN_TTL_MINUTES * 60).unwrap_or(900),
        refresh_token: None,
        issued_token_type: None,
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
    params(("tenant_id" = DataTenantId, Path, description = "Tenant to inspect")),
    responses(
        (status = 200, description = "The tenant's directory row", body = ProvisionedTenant),
        (status = 400, description = "A path identifier is not a valid UUID \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
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
    tenant_id: Result<Path<DataTenantId>, PathRejection>,
) -> Result<Json<ProvisionedTenant>, WyrdErrorResponse> {
    let Path(tenant_id) = tenant_id.map_err(|rejection| path_rejection(&rejection))?;
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
    params(("tenant_id" = DataTenantId, Path, description = "Tenant to transition")),
    request_body = SetTenantStatusRequest,
    responses(
        (status = 200, description = "The tenant now holds the requested status"),
        (status = 400, description = "A path identifier is not a valid UUID, or status is \
          neither active nor suspended (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
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
    tenant_id: Result<Path<DataTenantId>, PathRejection>,
    Json(request): Json<SetTenantStatusRequest>,
) -> Result<(), WyrdErrorResponse> {
    let Path(tenant_id) = tenant_id.map_err(|rejection| path_rejection(&rejection))?;
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
