//! CLI human login HTTP adapters: the RFC 8628 device authorization endpoint
//! and its verification page, and RFC 7009 token revocation. The device-code
//! token poll is a `POST /auth/token` grant.
//!
//! Every route is anonymous: the CLI holds no Wyrd session yet, or is ending
//! one, and the person approving a code signs in only after approving it.
//! Each handler composes [`CliLogins`] from the server's auth configuration
//! and maps its refusals to the RFC 6749 §5.2 body, or to a plain page for
//! the browser.

use axum::Json;
use axum::extract::{Extension, Form, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use wyrd_auth::cli_logins::{CliLogins, DEVICE_VERIFICATION_PATH};
use wyrd_spec::auth::{
    DeviceAuthorization, DeviceAuthorizationRequest, OAuthClientId, OAuthErrorCode,
    OAuthErrorResponse, TokenRevocationRequest,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::TenantSlug;
use wyrd_spec::request_id::RequestId;

use crate::auth::auth_not_configured;
use crate::auth::oauth::{ClientForm, OAuthError, OAuthForm, no_store};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Compose the CLI login owner from the server's human-connection owner and
/// tenant issuance owner.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when either is not configured.
pub(crate) fn cli_logins(state: &AppState) -> Result<CliLogins, WyrdError> {
    let missing = || auth_not_configured().0;
    Ok(CliLogins::new(
        state.auth.human_connections.clone().ok_or_else(missing)?,
        state
            .auth
            .tenant_issuer(&state.scribe_outbox)
            .ok_or_else(missing)?,
    ))
}

/// `POST /auth/device_authorization` — begin a CLI device login (RFC 8628
/// §3.1).
///
/// The public client `wyrd-cli` identifies itself with `client_id`; another
/// client is `unauthorized_client`. Like `GET /auth/authorize` it appends no
/// audit event: it evaluates no principal permission.
///
/// # Errors
/// Answers the RFC 6749 §5.2 body for a malformed request, an unidentified
/// or other client, and every refusal of [`CliLogins::authorize`]: an
/// unknown tenant or one without SSO is `invalid_request`.
#[utoipa::path(
    post,
    path = "/auth/device_authorization",
    request_body(content = ClientForm<DeviceAuthorizationRequest>,
        content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Device code issued; show `user_code`, open \
          `verification_uri_complete`, and poll `POST /auth/token` with the device_code grant \
          every `interval` seconds", body = DeviceAuthorization),
        (status = 400, description = "RFC 6749 §5.2 refusal: `invalid_request`, including \
          a tenant without SSO, or `unauthorized_client`",
          body = OAuthErrorResponse),
        (status = 401, description = "`invalid_client`", body = OAuthErrorResponse),
        (status = 500, description = "`server_error`", body = OAuthErrorResponse),
        (status = 503, description = "`temporarily_unavailable`", body = OAuthErrorResponse)
    ),
    // No session exists yet at this operation: a public client names itself
    // with `client_id` and needs nothing, a confidential client uses Basic.
    security((), ("oauthClientBasic" = [])),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn device_authorization(
    State(state): State<AppState>,
    headers: HeaderMap,
    form: OAuthForm,
) -> Result<Response, OAuthError> {
    match state.auth.oauth_clients.require(&headers, &form)? {
        OAuthClientId::WyrdCli => {}
        OAuthClientId::WyrdUi => return Err(OAuthError(OAuthErrorCode::UnauthorizedClient)),
    }
    let request: DeviceAuthorizationRequest = form.decode()?;
    let device = cli_logins(&state)?
        .authorize(&request.tenant)
        .await
        .map_err(OAuthError::request)?;
    Ok(no_store(StatusCode::OK, Json(device)))
}

/// Query of the verification page: the tenant, and the user code when the
/// CLI opened `verification_uri_complete`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct DeviceQuery {
    /// The tenant's route key.
    tenant: TenantSlug,
    /// The user code to prefill.
    user_code: Option<String>,
}

/// The person's decision on the verification page.
#[derive(Debug, Clone, Copy, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceDecision {
    /// Approve the code and sign in.
    Approve,
    /// Deny the code; the CLI's login ends.
    Deny,
}

/// Form the verification page posts.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DeviceForm {
    /// The tenant's route key.
    #[schema(value_type = String)]
    tenant: TenantSlug,
    /// The user code the person confirmed.
    user_code: String,
    /// Approve or deny.
    decision: DeviceDecision,
}

/// `GET /auth/device` — the device verification page (RFC 8628 §3.3).
///
/// A static form: the user code (prefilled from the query when it is
/// well-formed), and Approve and Deny buttons that post to the same path.
/// The page may not be framed, so it cannot be clicked through invisibly.
#[utoipa::path(
    get,
    path = "/auth/device",
    params(DeviceQuery),
    responses(
        (status = 200, description = "The verification form", content_type = "text/html",
          body = String)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all, fields(tenant_route_key = %query.tenant))]
pub async fn device_page(Query(query): Query<DeviceQuery>) -> Response {
    // Only a code of user-code characters is echoed, so it needs no escaping;
    // the tenant is a validated slug.
    let user_code = query
        .user_code
        .filter(|code| {
            code.len() <= 16 && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        .unwrap_or_default();
    page(
        StatusCode::OK,
        &format!(
            "<h1>Approve a Wyrd CLI login</h1><p>Approve only if this code is shown in your own \
             terminal.</p><form method=\"post\" action=\"{DEVICE_VERIFICATION_PATH}\">\
             <input type=\"hidden\" name=\"tenant\" value=\"{tenant}\"><label>Code \
             <input name=\"user_code\" value=\"{user_code}\" autocomplete=\"off\" required>\
             </label> <button name=\"decision\" value=\"approve\">Approve and sign in</button> \
             <button name=\"decision\" value=\"deny\">Deny</button></form>",
            tenant = query.tenant.as_str(),
        ),
    )
}

/// `POST /auth/device` — approve or deny a user code (RFC 8628 §3.3).
///
/// Only a form posted from the deployment's own origin is accepted, so
/// another site cannot approve its own code in the person's browser. Approve
/// redirects (`303`) to the tenant's provider sign-in, bound to the code's
/// device authorization; Deny ends the CLI's login.
///
/// # Errors
/// Answers a plain `400` page for an unknown, expired, or denied code, a
/// `403` page for a cross-origin post, and problem JSON for every other
/// refusal of [`CliLogins::approve`] and [`CliLogins::deny`].
#[utoipa::path(
    post,
    path = "/auth/device",
    request_body(content = DeviceForm, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "The code was denied", content_type = "text/html",
          body = String),
        (status = 303, description = "Approved; sign in at the tenant's provider",
          headers(("Location" = String, description = "Provider authorization URL"))),
        (status = 400, description = "The code is unknown, expired, or denied",
          content_type = "text/html", body = String),
        (status = 403, description = "The form was not posted from this deployment's origin",
          content_type = "text/html", body = String),
        (status = 401, description = "SSO login is not available for this tenant \
          (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 503, description = "The identity provider or auth backend is unavailable \
          (WYRD_AUTH_503_DISCOVERY_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)",
          body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all, fields(tenant_route_key = %form.tenant))]
pub async fn device_decision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<DeviceForm>,
) -> Result<Response, WyrdErrorResponse> {
    let logins = cli_logins(&state)?;
    let origin = state
        .auth
        .human_connections
        .as_ref()
        .ok_or_else(auth_not_configured)?
        .require_callback()?
        .origin()
        .ascii_serialization();
    if headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        != Some(&*origin)
    {
        return Ok(page(
            StatusCode::FORBIDDEN,
            "<h1>Request refused</h1><p>Open the link your terminal shows and try again.</p>",
        ));
    }
    let result = match form.decision {
        DeviceDecision::Approve => logins
            .approve(&form.tenant, &form.user_code)
            .await
            .map(|url| Redirect::to(url.as_str()).into_response()),
        DeviceDecision::Deny => logins.deny(&form.tenant, &form.user_code).await.map(|()| {
            page(
                StatusCode::OK,
                "<h1>Login denied</h1><p>The terminal's login was refused. You can close this \
                 page.</p>",
            )
        }),
    };
    match result {
        Err(WyrdError::InvalidState { .. }) => Ok(page(
            StatusCode::BAD_REQUEST,
            "<h1>Code not accepted</h1><p>The code is unknown or expired. Check the code your \
             terminal shows, or start a new login.</p>",
        )),
        other => other.map_err(WyrdErrorResponse::from),
    }
}

/// A plain HTML page with `body`, which may not be framed.
pub(crate) fn page(status: StatusCode, body: &str) -> Response {
    let mut response = (
        status,
        Html(format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>Wyrd CLI login</title>\
             </head><body>{body}</body></html>"
        )),
    )
        .into_response();
    let headers = response.headers_mut();
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("frame-ancestors 'none'"),
    );
    response
}

/// `POST /auth/revoke` — revoke a refresh token (RFC 7009 §2).
///
/// The client identifies itself as for the token endpoint: `wyrd-ui` with
/// its Basic secret, `wyrd-cli` with `client_id`. Revoking a token revokes
/// its whole login; the User's other logins continue. An unknown, malformed,
/// or already revoked token also answers `200` (RFC 7009 §2.2), and
/// `token_type_hint` is ignored: only refresh tokens are revocable. The
/// revocation commits with its audit event under the request id.
///
/// # Errors
/// Answers the RFC 6749 §5.2 body: `invalid_client` for a missing or refused
/// client, `invalid_request` for a malformed request or a token issued to
/// another client, and `temporarily_unavailable` when the store or the audit
/// path fails.
#[utoipa::path(
    post,
    path = "/auth/revoke",
    request_body(content = ClientForm<TokenRevocationRequest>,
        content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "The token's login, if it named one, no longer renews"),
        (status = 400, description = "`invalid_request`", body = OAuthErrorResponse),
        (status = 401, description = "`invalid_client`", body = OAuthErrorResponse),
        (status = 500, description = "`server_error`", body = OAuthErrorResponse),
        (status = 503, description = "`temporarily_unavailable`", body = OAuthErrorResponse)
    ),
    security((), ("oauthClientBasic" = [])),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn revoke(
    State(state): State<AppState>,
    request_id: Option<Extension<RequestId>>,
    headers: HeaderMap,
    form: OAuthForm,
) -> Result<Response, OAuthError> {
    let client = state.auth.oauth_clients.require(&headers, &form)?;
    let request: TokenRevocationRequest = form.decode()?;
    let request_id = request_id.map_or_else(RequestId::now_v7, |Extension(id)| id);
    cli_logins(&state)?
        .revoke(&request.token, client, request_id.as_str())
        .await?;
    Ok(no_store(StatusCode::OK, ()))
}
