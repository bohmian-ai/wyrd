//! Platform management of shared social sign-in, deployment login settings
//! and the application-mail relay.
//!
//! A platform administrator registers the deployment's one Google client and
//! one GitHub.com client, proves each stored revision with a real provider
//! sign-in, and only then enables it. The same platform identity permissions
//! that govern the deployment's own OIDC connection govern these routes, and
//! every decision is staged on the process audit outbox. Tenant company SSO
//! keeps its own connection slot; nothing here reads or writes it. The SMTP
//! relay is governed the same way; tenant administrators send invitations
//! through it but can neither read nor change it.
//!
//! Behavior lives in [`wyrd_auth::social::SocialSignIn`]; these handlers only
//! authorize, derive the public origin and project the result.

use axum::Json;
use axum::extract::{Path, State};
use std::sync::Arc;
use wyrd_auth::mail::ApplicationMail;
use wyrd_auth::platform_authz::PlatformAuthorization;
use wyrd_auth::social::SocialSignIn;
use wyrd_runtime::Permission;
use wyrd_spec::auth::{
    ConfigureMailRelayRequest, ConfigureSocialProviderRequest, EnableSocialProviderRequest,
    LoginSettings, MailSettings, MailTestRequest, MailTestResponse, SocialProvider,
    SocialProviderTestRequest, SocialProviderTestResponse, SocialProviderView,
    SocialProvidersResponse,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};

use super::identity::{authorize, authorize_read, commit_decision, decide, operator};
use crate::auth::RequestOrigin;
use crate::components::auth::PlatformCaller;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Audited resource for a decision about the shared social providers.
const SOCIAL_PROVIDERS_RESOURCE: &str = "platform:social_providers";

/// Audited resource for a decision about the deployment login settings.
const LOGIN_SETTINGS_RESOURCE: &str = "platform:login_settings";

/// Audited resource for a decision about the application-mail relay.
const MAIL_RESOURCE: &str = "platform:mail";

/// Build the authenticated platform social sign-in routes.
pub fn platform_social_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_providers))
        .routes(routes!(configure_provider))
        .routes(routes!(test_provider))
        .routes(routes!(enable_provider))
        .routes(routes!(read_login_settings, write_login_settings))
        .routes(routes!(read_mail, write_mail))
        .routes(routes!(test_mail))
}

/// The process social sign-in owner.
///
/// # Errors
/// Returns an internal error when the platform plane is not configured.
pub(crate) fn social(state: &AppState) -> Result<&SocialSignIn, WyrdErrorResponse> {
    state.auth.social_sign_in.as_ref().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "platform control plane is not configured".to_owned(),
            details: serde_json::json!({ "plane": "platform" }),
        })
    })
}

/// Admit one unauthenticated sign-in start from `source` before it records
/// anything (see [`SocialSignIn::admit_sign_in`]).
///
/// # Errors
/// Returns [`WyrdError::SignInAttempts`] while the source's window is
/// exhausted, and [`WyrdError::AuthVerifyUnavailable`] when admission cannot
/// run, including without the platform plane.
pub(crate) async fn admit_sign_in(
    state: &AppState,
    source: Option<std::net::IpAddr>,
) -> Result<(), WyrdError> {
    let social =
        state
            .auth
            .social_sign_in
            .as_ref()
            .ok_or_else(|| WyrdError::AuthVerifyUnavailable {
                message: "sign-in starts cannot be admitted without the platform plane".to_owned(),
                details: serde_json::json!({ "reason": "admission_unavailable" }),
            })?;
    social.admit_sign_in(source).await
}

/// The process application-mail owner.
///
/// # Errors
/// Returns an internal error when the platform plane is not configured.
fn mail(state: &AppState) -> Result<&ApplicationMail, WyrdErrorResponse> {
    state.auth.application_mail.as_ref().ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::Internal {
            message: "platform control plane is not configured".to_owned(),
            details: serde_json::json!({ "plane": "platform" }),
        })
    })
}

/// Parse the `{provider}` path segment.
///
/// # Errors
/// Returns `404` for anything but a shared provider's name.
fn provider(name: &str) -> Result<SocialProvider, WyrdErrorResponse> {
    SocialProvider::parse(name).ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::NotFound {
            message: "no such social sign-in provider".to_owned(),
            details: serde_json::json!({ "provider": name }),
        })
    })
}

/// Read every shared provider, configured or not, with no secret.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized or the read
/// fails.
#[utoipa::path(
    get,
    path = "/platform/social/providers",
    responses(
        (status = 200, description = "Google and GitHub.com, with their registration callbacks and \
          current revision state, never a secret", body = SocialProvidersResponse),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity read required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 503, description = "The platform store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn list_providers(
    State(state): State<AppState>,
    caller: PlatformCaller,
    RequestOrigin(origin): RequestOrigin,
) -> Result<Json<SocialProvidersResponse>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    authorize_read(
        &pool,
        state.bifrost.scribe_outbox(),
        &caller,
        &Permission::platform_identity_read(),
        SOCIAL_PROVIDERS_RESOURCE,
    )
    .await?;
    Ok(Json(social(&state)?.views(&origin).await?))
}

/// Store a shared provider's OAuth client as a new untested, disabled
/// revision.
///
/// # Errors
/// Returns `400` for an empty client or a deployment that cannot seal its
/// secret, and a stable Wyrd error when the caller is unauthorized or the
/// write fails.
#[utoipa::path(
    put,
    path = "/platform/social/providers/{provider}",
    params(("provider" = SocialProvider, Path, description = "`google` or `github`")),
    request_body = ConfigureSocialProviderRequest,
    responses(
        (status = 200, description = "Stored as a new untested, disabled revision", body = SocialProviderView),
        (status = 400, description = "Empty client id or secret, or no sealing key \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "Not a shared provider (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The platform store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn configure_provider(
    State(state): State<AppState>,
    caller: PlatformCaller,
    RequestOrigin(origin): RequestOrigin,
    Path(name): Path<String>,
    Json(request): Json<ConfigureSocialProviderRequest>,
) -> Result<Json<SocialProviderView>, WyrdErrorResponse> {
    caller.require_recent_proof()?;
    let provider = provider(&name)?;
    let pool = operator(&state)?;
    let authz =
        PlatformAuthorization::new(pool.clone(), Arc::clone(state.bifrost.scribe_outbox()) as _);
    // Sealing waits on the key service, so the owner opens the write
    // transaction only after it.
    decide(
        &authz,
        &caller,
        &Permission::platform_identity_write(),
        SOCIAL_PROVIDERS_RESOURCE,
    )?;
    let view = social(&state)?
        .configure(provider, &request, &origin)
        .await?;
    Ok(Json(view))
}

/// Begin a real provider sign-in that tests exactly the named revision.
///
/// The operator's browser follows the returned URL; the provider returns to
/// the provider's social callback, which marks that revision tested.
///
/// # Errors
/// Returns `404` for an unconfigured provider, `409` for a revision that is
/// no longer current, `503` when the provider cannot be discovered, and a
/// stable Wyrd error when the caller is unauthorized.
#[utoipa::path(
    post,
    path = "/platform/social/providers/{provider}/test",
    params(("provider" = SocialProvider, Path, description = "`google` or `github`")),
    request_body = SocialProviderTestRequest,
    responses(
        (status = 200, description = "Sign in at the provider to finish the test", body = SocialProviderTestResponse),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "Provider not configured (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 409, description = "The provider was reconfigured (WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 503, description = "The provider or platform store is unavailable \
          (WYRD_AUTH_503_DISCOVERY_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn test_provider(
    State(state): State<AppState>,
    caller: PlatformCaller,
    RequestOrigin(origin): RequestOrigin,
    Path(name): Path<String>,
    Json(request): Json<SocialProviderTestRequest>,
) -> Result<Json<SocialProviderTestResponse>, WyrdErrorResponse> {
    let provider = provider(&name)?;
    let pool = operator(&state)?;
    authorize_read(
        &pool,
        state.bifrost.scribe_outbox(),
        &caller,
        &Permission::platform_identity_write(),
        SOCIAL_PROVIDERS_RESOURCE,
    )
    .await?;
    let authorization_url = social(&state)?
        .begin_test(
            provider,
            request.expected_revision,
            caller.context.principal_id().as_uuid(),
            &origin,
        )
        .await?;
    Ok(Json(SocialProviderTestResponse { authorization_url }))
}

/// Enable or disable exactly the named revision of a shared provider.
///
/// # Errors
/// Returns `404` for an unconfigured provider, `409` for a revision that is
/// not current or, when enabling, not tested, and a stable Wyrd error when the
/// caller is unauthorized or the write fails.
#[utoipa::path(
    put,
    path = "/platform/social/providers/{provider}/enabled",
    params(("provider" = SocialProvider, Path, description = "`google` or `github`")),
    request_body = EnableSocialProviderRequest,
    responses(
        (status = 200, description = "The provider's new state", body = SocialProviderView),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "Provider not configured (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 409, description = "Revision not current, or untested \
          (WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 503, description = "The platform store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn enable_provider(
    State(state): State<AppState>,
    caller: PlatformCaller,
    RequestOrigin(origin): RequestOrigin,
    Path(name): Path<String>,
    Json(request): Json<EnableSocialProviderRequest>,
) -> Result<Json<SocialProviderView>, WyrdErrorResponse> {
    caller.require_recent_proof()?;
    let provider = provider(&name)?;
    let pool = operator(&state)?;
    let authz =
        PlatformAuthorization::new(pool.clone(), Arc::clone(state.bifrost.scribe_outbox()) as _);
    let mut decision = authorize(
        &authz,
        &caller,
        &Permission::platform_identity_write(),
        SOCIAL_PROVIDERS_RESOURCE,
    )
    .await?;
    let view = social(&state)?
        .set_enabled(&mut decision, provider, request, &origin)
        .await?;
    commit_decision(decision).await?;
    Ok(Json(view))
}

/// Read the deployment login settings.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized or the read
/// fails.
#[utoipa::path(
    get,
    path = "/platform/login-settings",
    responses(
        (status = 200, description = "Password and public-signup availability", body = LoginSettings),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity read required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 503, description = "The platform store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn read_login_settings(
    State(state): State<AppState>,
    caller: PlatformCaller,
) -> Result<Json<LoginSettings>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    authorize_read(
        &pool,
        state.bifrost.scribe_outbox(),
        &caller,
        &Permission::platform_identity_read(),
        LOGIN_SETTINGS_RESOURCE,
    )
    .await?;
    Ok(Json(social(&state)?.settings().await?))
}

/// Replace the deployment login settings.
///
/// # Errors
/// Returns a stable Wyrd error when the body is malformed, the caller is
/// unauthorized or the write fails.
#[utoipa::path(
    put,
    path = "/platform/login-settings",
    request_body = LoginSettings,
    responses(
        (status = 200, description = "The stored settings", body = LoginSettings),
        (status = 400, description = "The body is malformed or the workspace cap is below one \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 503, description = "The platform store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn write_login_settings(
    State(state): State<AppState>,
    caller: PlatformCaller,
    settings: Result<Json<LoginSettings>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<LoginSettings>, WyrdErrorResponse> {
    caller.require_recent_proof()?;
    let Json(settings) =
        settings.map_err(|rejection| crate::http::error::body_rejection(&rejection))?;
    let pool = operator(&state)?;
    let authz =
        PlatformAuthorization::new(pool.clone(), Arc::clone(state.bifrost.scribe_outbox()) as _);
    // Sealing a challenge secret waits on the key service, so the owner
    // opens the write transaction only after it.
    decide(
        &authz,
        &caller,
        &Permission::platform_identity_write(),
        LOGIN_SETTINGS_RESOURCE,
    )?;
    social(&state)?.put_settings(&settings).await?;
    Ok(Json(settings))
}

/// Read the application-mail relay, never its password.
///
/// # Errors
/// Returns a stable Wyrd error when the caller is unauthorized or the read
/// fails.
#[utoipa::path(
    get,
    path = "/platform/mail",
    responses(
        (status = 200, description = "The SMTP relay, or `null` when Wyrd emails nothing", body = MailSettings),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity read required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 503, description = "The platform store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller))]
async fn read_mail(
    State(state): State<AppState>,
    caller: PlatformCaller,
) -> Result<Json<MailSettings>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    authorize_read(
        &pool,
        state.bifrost.scribe_outbox(),
        &caller,
        &Permission::platform_identity_read(),
        MAIL_RESOURCE,
    )
    .await?;
    Ok(Json(mail(&state)?.settings().await?))
}

/// Replace the application-mail relay; the password is sealed and never
/// returned.
///
/// # Errors
/// Returns `400` for an invalid relay and a stable Wyrd error when the caller
/// is unauthorized or the write fails.
#[utoipa::path(
    put,
    path = "/platform/mail",
    request_body = ConfigureMailRelayRequest,
    responses(
        (status = 200, description = "The stored relay, without its password", body = MailSettings),
        (status = 400, description = "Empty host, zero port, invalid sender, a password without \
          a username, or no sealing key (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 503, description = "The platform store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn write_mail(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Json(request): Json<ConfigureMailRelayRequest>,
) -> Result<Json<MailSettings>, WyrdErrorResponse> {
    caller.require_recent_proof()?;
    let pool = operator(&state)?;
    let authz =
        PlatformAuthorization::new(pool.clone(), Arc::clone(state.bifrost.scribe_outbox()) as _);
    // Sealing the password waits on the key service, so the owner opens the
    // write transaction only after it.
    decide(
        &authz,
        &caller,
        &Permission::platform_identity_write(),
        MAIL_RESOURCE,
    )?;
    let settings = mail(&state)?.configure(&request).await?;
    Ok(Json(settings))
}

/// Send one test message through the relay. Acceptance means the relay took
/// it, not that it reached an inbox.
///
/// # Errors
/// Returns `400` for an invalid recipient, `409` when no relay is configured,
/// and a stable Wyrd error when the caller is unauthorized.
#[utoipa::path(
    post,
    path = "/platform/mail/test",
    request_body = MailTestRequest,
    responses(
        (status = 200, description = "Whether the relay accepted the message", body = MailTestResponse),
        (status = 400, description = "Not an email address (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "Platform session required (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Platform identity administration required \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 409, description = "No relay is configured (WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 503, description = "The platform store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "Platform"
)]
#[tracing::instrument(level = "info", skip(state, caller, request))]
async fn test_mail(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Json(request): Json<MailTestRequest>,
) -> Result<Json<MailTestResponse>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    let authz =
        PlatformAuthorization::new(pool.clone(), Arc::clone(state.bifrost.scribe_outbox()) as _);
    let decision = authorize(
        &authz,
        &caller,
        &Permission::platform_identity_write(),
        MAIL_RESOURCE,
    )
    .await?;
    commit_decision(decision).await?;
    Ok(Json(mail(&state)?.test(&request.recipient).await?))
}
