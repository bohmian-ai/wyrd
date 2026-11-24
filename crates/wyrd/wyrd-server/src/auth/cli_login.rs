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
use wyrd_auth::browser_sessions::BrowserSession;
use wyrd_auth::cli_logins::{CliLogins, DEVICE_VERIFICATION_PATH};
use wyrd_spec::auth::{
    DeviceAuthorization, DeviceAuthorizationRequest, OAuthClientId, OAuthErrorCode,
    OAuthErrorResponse, TokenRevocationRequest,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::TenantSlug;
use wyrd_spec::request_id::RequestId;
use wyrd_sql::row_types::auth::HumanLoginAnchor;

use crate::auth::browser::{browser_sessions, session_cookies};
use crate::auth::oauth::{ClientForm, OAuthError, OAuthForm, no_store};
use crate::auth::source::RequestSource;
use crate::auth::{RequestOrigin, auth_not_configured};
use crate::components::platform::social::admit_sign_in;
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
            .tenant_issuer(state.bifrost.scribe_outbox())
            .ok_or_else(missing)?,
    ))
}

/// `POST /auth/device_authorization` — begin a CLI device login (RFC 8628
/// §3.1).
///
/// The public client `wyrd-cli` identifies itself with `client_id`; another
/// client is `unauthorized_client`. Like `POST /auth/browser/login` it
/// appends no audit event: it evaluates no principal permission, and it is
/// first counted in its source's sign-in window, so a refused start records
/// no device authorization.
///
/// # Errors
/// Answers the RFC 6749 §5.2 body for a malformed request, an unidentified
/// or other client, and every refusal of [`CliLogins::authorize`]: an
/// unknown tenant or one without SSO is `invalid_request`. An exhausted
/// sign-in window answers `429` `temporarily_unavailable` with
/// `Retry-After`; a start that cannot be admitted is `503`
/// `temporarily_unavailable`.
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
        (status = 429, description = "`temporarily_unavailable`: too many sign-in starts \
          from this source", body = OAuthErrorResponse,
          headers(("Retry-After" = u64, description = "Seconds until the window ends"))),
        (status = 500, description = "`server_error`", body = OAuthErrorResponse),
        (status = 503, description = "`temporarily_unavailable`", body = OAuthErrorResponse)
    ),
    // No session exists yet at this operation: a client names itself with
    // `client_id` and needs nothing.
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn device_authorization(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    RequestSource(source): RequestSource,
    form: OAuthForm,
) -> Result<Response, OAuthError> {
    let OAuthClientId::WyrdCli = crate::auth::oauth::require_client(&headers, &form)?;
    let request: DeviceAuthorizationRequest = form.decode()?;
    match admit_sign_in(&state, source).await {
        Ok(()) => {}
        Err(error @ WyrdError::SignInAttempts { .. }) => return Ok(slow_start(&error)),
        Err(error) => return Err(OAuthError::request(error)),
    }
    let device = cli_logins(&state)?
        .authorize(&request.tenant, &origin)
        .await
        .map_err(OAuthError::request)?;
    Ok(no_store(StatusCode::OK, Json(device)))
}

/// The `429` RFC 6749 §5.2 refusal of a device start whose source's
/// sign-in window is exhausted (`error`): `temporarily_unavailable` with its
/// Wyrd code and `Retry-After` set to the whole seconds until the window
/// ends.
fn slow_start(error: &WyrdError) -> Response {
    tracing::info!(wyrd_code = error.code(), %error, "device start refused");
    let mut response = no_store(
        StatusCode::TOO_MANY_REQUESTS,
        Json(OAuthErrorResponse {
            error: OAuthErrorCode::TemporarilyUnavailable,
            error_description: None,
            code: Some(error.code().to_owned()),
        }),
    );
    if let WyrdError::SignInAttempts { details, .. } = error
        && let Some(seconds) = details["retry_after_seconds"].as_u64()
    {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
    }
    response
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
             <input type=\"hidden\" name=\"tenant\" value=\"{tenant}\"><label for=\"user_code\">Code\
             </label><input id=\"user_code\" name=\"user_code\" value=\"{user_code}\" autocomplete=\"off\" \
             required><button class=\"primary\" name=\"decision\" value=\"approve\">Approve and sign in\
             </button><button name=\"decision\" value=\"deny\">Deny</button></form>",
            tenant = query.tenant.as_str(),
        ),
    )
}

/// `POST /auth/device` — approve or deny a user code (RFC 8628 §3.3).
///
/// Only a form posted from the origin the request arrived at is accepted, so
/// another site cannot approve its own code in the person's browser. Approve
/// with a recent password or provider session for the tenant records that
/// session's human anchor and answers `200`; without one it redirects
/// (`303`) to the tenant's provider sign-in, bound to the code's device
/// authorization. Deny ends the CLI's login.
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
        (status = 200, description = "The code was approved with the recent session, or denied",
          content_type = "text/html", body = String),
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
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    Form(form): Form<DeviceForm>,
) -> Result<Response, WyrdErrorResponse> {
    let logins = cli_logins(&state)?;
    if headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        != Some(&*origin.origin().ascii_serialization())
    {
        return Ok(page(
            StatusCode::FORBIDDEN,
            "<h1>Request refused</h1><p>Open the link your terminal shows and try again.</p>",
        ));
    }
    let result = match form.decision {
        DeviceDecision::Approve => match signed_in(&state, &headers, &form.tenant).await? {
            Some((session, anchor)) => logins
                .approve_signed_in(
                    &form.tenant,
                    &form.user_code,
                    session.verified.principal.tenant_id,
                    session.verified.principal.id,
                    anchor,
                )
                .await
                .map(|()| {
                    page(
                        StatusCode::OK,
                        "<h1>Login approved</h1><p>Return to your terminal. You can close this \
                         page.</p>",
                    )
                }),
            None => logins
                .approve(&form.tenant, &form.user_code, &origin)
                .await
                .map(|url| Redirect::to(url.as_str()).into_response()),
        },
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

/// The recent human browser session the request presents for
/// `tenant_route_key`, with the human anchor it was created through, which is
/// fresh proof to approve a device login.
///
/// Only a password or provider session created in the last five minutes
/// counts; a recovery session, an older session, a cookie for another tenant
/// and a cookie that no longer resolves all yield `None`, so the approval
/// falls back to the provider sign-in.
///
/// # Errors
/// Returns `WYRD_SPEC_500_INTERNAL` when no browser-session owner is
/// configured.
async fn signed_in(
    state: &AppState,
    headers: &HeaderMap,
    tenant_route_key: &TenantSlug,
) -> Result<Option<(BrowserSession, HumanLoginAnchor)>, WyrdErrorResponse> {
    let Some(cookie) = session_cookies(headers)
        .into_iter()
        .find_map(|(label, value)| (label.as_ref() == Some(tenant_route_key)).then_some(value))
    else {
        return Ok(None);
    };
    let sessions = browser_sessions(state)?;
    Ok(sessions
        .resolve(&cookie)
        .await
        .ok()
        .filter(|session| session.recent)
        .and_then(|session| session.anchor.map(|anchor| (session, anchor))))
}

/// The Wyrd mark, inline so a device page needs no asset route.
const LOGO: &str = "<svg width=\"28\" height=\"28\" viewBox=\"0 0 100 100\" aria-hidden=\"true\">\
    <path d=\"M42 92 L30 92 L6 8 L22 8 Z\" fill=\"#7a8cff\" stroke=\"#0b0c12\" stroke-width=\"2\"/>\
    <path d=\"M58 92 L70 92 L94 8 L78 8 Z\" fill=\"#7a8cff\" stroke=\"#0b0c12\" stroke-width=\"2\"/>\
    <path d=\"M44 8 h12 v84 h-12 z\" fill=\"#c5f23b\" stroke=\"#0b0c12\" stroke-width=\"2\"/></svg>";

/// The sign-in entry card's look (wyrd-ui `Entry.svelte` and `app.css`, with the
/// brand theme's light and dark tokens), inlined so the server-rendered device
/// pages match every other sign-in page.
const ENTRY_STYLE: &str = ":root{--bg:#f5f6f8;--surface:#fff;--text:#101014;--muted:#62626d;\
    --border:#d9dde5;--declare:#4d5ef0;--declare-ink:#fff;--weight-strong:700;\
    --font-sans:\"Familjen Grotesk\",system-ui,sans-serif;\
    --font-mono:\"Fragment Mono\",ui-monospace,monospace;color-scheme:light dark}\
    @media (prefers-color-scheme: dark){:root{--bg:#0b0c12;--surface:#12141c;--text:#e6e4da;\
    --muted:#8f92a5;--border:#252838;--declare:#7a8cff;--declare-ink:#0b0c12;--weight-strong:600}}\
    *{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text);font-family:var(--font-sans)}\
    .entry{min-height:100dvh;display:grid;place-items:center;padding:64px 24px}\
    .card{width:100%;max-width:420px;display:grid;gap:20px;padding:32px;background:var(--surface);\
    border:2px solid var(--border);border-top:4px solid var(--declare);border-radius:2px}\
    header{display:flex;align-items:center;justify-content:center;gap:10px}\
    .wordmark{font:600 18px var(--font-sans)}\
    .product{font:var(--weight-strong) 10px var(--font-mono);border:2px solid var(--border);border-radius:2px;\
    padding:4px 12px;background:var(--declare);color:var(--declare-ink)}\
    h1{margin:0;font:var(--weight-strong) 24px var(--font-sans);text-align:center}\
    p{margin:0;font:12px/18px var(--font-sans);color:var(--muted);text-align:center}\
    form{display:grid;gap:10px}label{font:var(--weight-strong) 11px var(--font-mono)}\
    input,button{min-height:32px;border:1px solid var(--border);border-radius:2px;padding:12px 16px;\
    background:var(--surface);color:var(--text);font:13px var(--font-sans);width:100%}\
    input{padding:6px 10px;font:12px var(--font-mono)}button{cursor:pointer}\
    button.primary{border-color:var(--declare);background:var(--declare);color:var(--declare-ink);\
    font-weight:var(--weight-strong)}:focus-visible{outline:2px solid var(--declare);outline-offset:3px}\
    @media (max-width:767px){.entry{padding:32px 16px}.card{padding:28px 20px}}";

/// A device page with `body` inside the branded sign-in card, which may not be framed.
pub(crate) fn page(status: StatusCode, body: &str) -> Response {
    let mut response = (
        status,
        Html(format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
             <title>Wyrd CLI login</title><style>{ENTRY_STYLE}</style></head><body>\
             <main class=\"entry\"><div class=\"card\"><header>{LOGO}\
             <span class=\"wordmark\">Wyrd</span><span class=\"product\">WYRD</span></header>\
             {body}</div></main></body></html>"
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
/// The client identifies itself as for the token endpoint, with
/// `client_id`. Revoking a token revokes
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
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn revoke(
    State(state): State<AppState>,
    request_id: Option<Extension<RequestId>>,
    headers: HeaderMap,
    form: OAuthForm,
) -> Result<Response, OAuthError> {
    let client = crate::auth::oauth::require_client(&headers, &form)?;
    let request: TokenRevocationRequest = form.decode()?;
    let request_id = request_id.map_or_else(RequestId::now_v7, |Extension(id)| id);
    cli_logins(&state)?
        .revoke(&request.token, client, request_id.as_str())
        .await?;
    Ok(no_store(StatusCode::OK, ()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every device page renders inside the branded sign-in card, in light and dark.
    #[tokio::test]
    async fn device_pages_render_in_the_branded_entry_card() {
        let response = page(StatusCode::OK, "<h1>Login approved</h1>");
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("an in-memory body reads");
        let html = String::from_utf8(bytes.to_vec()).expect("the page is UTF-8");
        assert!(html.contains("<main class=\"entry\"><div class=\"card\">"));
        assert!(html.contains("<span class=\"wordmark\">Wyrd</span>"));
        assert!(html.contains("<h1>Login approved</h1>"));
        assert!(html.contains("--declare:#4d5ef0"));
        assert!(html.contains("@media (prefers-color-scheme: dark)"));
    }
}
