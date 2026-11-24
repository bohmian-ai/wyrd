//! `/platform/browser` — the platform console's browser-session boundary.
//!
//! A registered platform operator signs in to the platform console and keeps
//! one opaque session in a host-only Secure, HttpOnly, SameSite=Lax cookie
//! named [`PLATFORM_SESSION_COOKIE`] on `Path=/platform`. Rust alone creates,
//! resolves and revokes it
//! ([`wyrd_auth::platform_sessions::PlatformSessions`]); these handlers only
//! move the cookie. The cookie is never a token: no API token, bootstrap key
//! or provider token reaches the browser. Tenant routes read only
//! `wyrd_session_*` cookies and the platform extractor reads only this one,
//! so neither plane's cookie authenticates the other. Every cookie-only
//! mutation requires this deployment's exact `Origin`, and every response
//! is `no-store`.

use std::sync::Arc;

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use secrecy::{ExposeSecret as _, SecretString};
use url::Url;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wyrd_auth::platform_login::PlatformBrowserLogin;
use wyrd_auth::platform_sessions::{
    PLATFORM_BROWSER_WINDOWS, PlatformBrowserSession, PlatformSessionError, PlatformSessions,
};
use wyrd_spec::auth::{
    LoginInitResponse, PlatformBrowserSessionView, PlatformCallbackRequest,
    PlatformPasswordLoginRequest, PlatformPasswordReauthRequest, PlatformSignInMethods,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::request_id::RequestId;

use crate::auth::browser::{
    cookie_header, cookie_pairs, request_id, require_same_origin, with_cookies,
};
use crate::auth::source::RequestSource;
use crate::auth::{RequestOrigin, auth_not_configured};
use crate::components::auth::PlatformCaller;
use crate::components::platform::identity::{login_error, login_service};
use crate::components::platform::social::admit_sign_in;
use crate::http::error::{WyrdErrorResponse, body_rejection};
use crate::state::AppState;

/// Name of the platform console's session cookie, distinct from every
/// tenant `wyrd_session_*` cookie.
pub(crate) const PLATFORM_SESSION_COOKIE: &str = "wyrd_platform_session";

/// Path the platform cookies are scoped to; Rust's plane checks still
/// decide every request.
const PLATFORM_COOKIE_PATH: &str = "/platform";

/// Build the platform console browser-session routes.
pub fn platform_console_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(password_login))
        .routes(routes!(session))
        .routes(routes!(logout))
        .routes(routes!(oidc_login))
        .routes(routes!(oidc_callback))
        .routes(routes!(password_reauth))
        .routes(routes!(oidc_reauth))
        .routes(routes!(methods))
}

/// `GET /platform/browser/methods` — the sign-in methods the signed-out
/// platform entry may offer.
///
/// `password` is available wherever platform sessions are configured;
/// `oidc` is true exactly when a platform OIDC connection exists, so
/// `POST /platform/browser/oidc/login` would not answer `404`. Needs no
/// session and reveals nothing about which operators exist.
///
/// # Errors
/// `500 WYRD_SPEC_500_INTERNAL` when the platform store read fails.
#[utoipa::path(
    get,
    path = "/platform/browser/methods",
    responses(
        (status = 200, description = "The methods the platform entry may offer",
          body = PlatformSignInMethods),
        (status = 500, description = "The platform store read failed \
          (WYRD_SPEC_500_INTERNAL)", body = WyrdProblem)
    ),
    security(()),
    tag = "Platform"
)]
#[tracing::instrument(level = "debug", skip_all)]
async fn methods(State(state): State<AppState>) -> Result<Response, WyrdErrorResponse> {
    let oidc = match state.auth.platform_login.as_ref() {
        Some(login) => login.configured().await.map_err(login_error)?,
        None => false,
    };
    Ok(with_cookies(
        StatusCode::OK,
        Vec::new(),
        Json(PlatformSignInMethods {
            password: platform_sessions(&state).is_ok(),
            oidc,
        }),
    ))
}

/// Name of the cookie binding a platform OIDC browser login to the browser
/// that began it.
const LOGIN_BINDING_COOKIE: &str = "wyrd_platform_login";

/// Lifetime of the login binding cookie, matching the login state's.
const LOGIN_BINDING_MAX_AGE: u64 = 300;

/// The fixed console page the platform provider returns a browser login to.
const CONSOLE_CALLBACK_PATH: &str = "/platform/ui/callback";

/// The value of the platform session cookie a request presents, if any.
pub(crate) fn platform_cookie(headers: &HeaderMap) -> Option<&str> {
    cookie_pairs(headers)
        .find_map(|(name, value)| (name == PLATFORM_SESSION_COOKIE).then_some(value))
        .filter(|value| !value.is_empty())
}

/// The platform session owner over this server's operator pool.
///
/// # Errors
/// Returns `WYRD_SPEC_500_INTERNAL` when the platform plane or the signing
/// key is not configured.
pub(crate) fn platform_sessions(state: &AppState) -> Result<PlatformSessions, WyrdErrorResponse> {
    let pool = state
        .postgres
        .operator_pool()
        .ok_or_else(auth_not_configured)?;
    let key = state
        .auth
        .issuing_key
        .clone()
        .ok_or_else(auth_not_configured)?;
    Ok(PlatformSessions::new(
        pool,
        key,
        Arc::clone(state.bifrost.scribe_outbox()) as _,
    ))
}

/// `Set-Cookie` installing a new platform session cookie, living as long as
/// the session's absolute deadline.
fn set_platform_cookie(origin: &Url, value: &str) -> HeaderValue {
    cookie_header(
        origin,
        PLATFORM_SESSION_COOKIE,
        value,
        PLATFORM_COOKIE_PATH,
        PLATFORM_BROWSER_WINDOWS.absolute.as_secs(),
    )
}

/// `Set-Cookie` removing the platform session cookie.
fn clear_platform_cookie(origin: &Url) -> HeaderValue {
    cookie_header(origin, PLATFORM_SESSION_COOKIE, "", PLATFORM_COOKIE_PATH, 0)
}

/// Project a platform session failure: every refusal is one
/// unauthenticated answer and a store failure is unavailable.
pub(crate) fn session_error(error: PlatformSessionError) -> WyrdErrorResponse {
    match error {
        PlatformSessionError::Invalid => WyrdErrorResponse::from(WyrdError::Unauthenticated {
            message: "platform sign-in not accepted".to_owned(),
            details: serde_json::json!({ "plane": "platform" }),
        }),
        error => {
            tracing::warn!(%error, "platform browser session store failed");
            WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
                message: "platform sessions are unavailable".to_owned(),
                details: serde_json::json!({ "plane": "platform" }),
            })
        }
    }
}

/// `POST /platform/browser/password/login` — sign a platform `user` in to
/// the platform console with its platform-user password.
///
/// The start is counted in its source's sign-in window first. Every
/// unusable name/password pair, including a bootstrap key presented as a
/// password, is refused alike and sets no cookie. Success sets a freshly
/// generated session cookie and returns its safe metadata.
///
/// # Errors
/// `403 WYRD_AUTH_403_ORIGIN_REFUSED` without this deployment's exact
/// `Origin`; `401 WYRD_AUTH_401_UNAUTHENTICATED` for every refusal;
/// `429 WYRD_AUTH_429_SIGN_IN_ATTEMPTS` while the source's window is
/// exhausted; `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` when the store fails.
#[utoipa::path(
    post,
    path = "/platform/browser/password/login",
    request_body = PlatformPasswordLoginRequest,
    responses(
        (status = 200, description = "Signed in; a fresh platform session cookie is set",
          body = PlatformBrowserSessionView,
          headers(("Set-Cookie" = String, description = "The platform session cookie"))),
        (status = 401, description = "Name or password not accepted \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Origin absent or foreign \
          (WYRD_AUTH_403_ORIGIN_REFUSED)", body = WyrdProblem),
        (status = 429, description = "Too many sign-in starts from this source \
          (WYRD_AUTH_429_SIGN_IN_ATTEMPTS)", body = WyrdProblem,
          headers(("Retry-After" = u64, description = "Seconds until the window ends"))),
        (status = 503, description = "The store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Platform"
)]
#[tracing::instrument(level = "debug", skip_all)]
async fn password_login(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    id: Option<Extension<RequestId>>,
    RequestSource(source): RequestSource,
    request: Result<Json<PlatformPasswordLoginRequest>, JsonRejection>,
) -> Result<Response, WyrdErrorResponse> {
    let Json(request) = request.map_err(|rejection| body_rejection(&rejection))?;
    require_same_origin(&headers, &origin)?;
    admit_sign_in(&state, source).await?;
    let created = platform_sessions(&state)?
        .password_login(
            &request.username,
            &request.password.into_secret_string(),
            request_id(id).as_str(),
        )
        .await
        .map_err(session_error)?;
    Ok(with_cookies(
        StatusCode::OK,
        vec![set_platform_cookie(&origin, created.cookie.expose_secret())],
        Json(created.session.view),
    ))
}

/// `GET /platform/browser/session` — the platform session this browser
/// presents.
///
/// Resolving records activity, which moves the idle deadline but never the
/// absolute one.
///
/// # Errors
/// `401 WYRD_AUTH_401_UNAUTHENTICATED` without a live platform session
/// cookie; `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` when the store fails.
#[utoipa::path(
    get,
    path = "/platform/browser/session",
    responses(
        (status = 200, description = "The live platform session", body = PlatformBrowserSessionView),
        (status = 401, description = "No live platform session \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 503, description = "The store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Platform"
)]
#[tracing::instrument(level = "debug", skip_all)]
async fn session(caller: PlatformCaller) -> Result<Response, WyrdErrorResponse> {
    let Some(session) = caller.browser else {
        return Err(session_error(PlatformSessionError::Invalid));
    };
    Ok(with_cookies(StatusCode::OK, Vec::new(), Json(session.view)))
}

/// `POST /platform/browser/logout` — end this browser's platform session.
///
/// Revokes only the session the cookie names and clears the cookie; a
/// missing or dead cookie revokes nothing and still clears.
///
/// # Errors
/// `403 WYRD_AUTH_403_ORIGIN_REFUSED` without this deployment's exact
/// `Origin`; `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` when the store fails.
#[utoipa::path(
    post,
    path = "/platform/browser/logout",
    responses(
        (status = 204, description = "Session revoked and its cookie cleared",
          headers(("Set-Cookie" = String, description = "The cleared platform cookie"))),
        (status = 403, description = "Origin absent or foreign \
          (WYRD_AUTH_403_ORIGIN_REFUSED)", body = WyrdProblem),
        (status = 503, description = "The store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Platform"
)]
#[tracing::instrument(level = "debug", skip_all)]
async fn logout(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
) -> Result<Response, WyrdErrorResponse> {
    require_same_origin(&headers, &origin)?;
    if let Some(cookie) = platform_cookie(&headers) {
        platform_sessions(&state)?
            .logout_browser(cookie)
            .await
            .map_err(session_error)?;
    }
    Ok(with_cookies(
        StatusCode::NO_CONTENT,
        vec![clear_platform_cookie(&origin)],
        (),
    ))
}

/// `POST /platform/browser/oidc/login` — begin a platform console sign-in
/// through the platform OIDC connection.
///
/// The start is counted in its source's sign-in window first. The provider
/// returns to this deployment's fixed console callback page; a
/// platform-scoped `HttpOnly` binding cookie ties the login to this browser.
///
/// # Errors
/// `403 WYRD_AUTH_403_ORIGIN_REFUSED` without this deployment's exact
/// `Origin`; `404 WYRD_SPEC_404_NOT_FOUND` when no connection is configured;
/// `429 WYRD_AUTH_429_SIGN_IN_ATTEMPTS` while the source's window is
/// exhausted; `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` when the provider is
/// unavailable.
#[utoipa::path(
    post,
    path = "/platform/browser/oidc/login",
    responses(
        (status = 200, description = "Login begun; a login binding cookie is set",
          body = LoginInitResponse,
          headers(("Set-Cookie" = String, description = "The login binding cookie"))),
        (status = 403, description = "Origin absent or foreign \
          (WYRD_AUTH_403_ORIGIN_REFUSED)", body = WyrdProblem),
        (status = 404, description = "No platform OIDC connection \
          (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 429, description = "Too many sign-in starts from this source \
          (WYRD_AUTH_429_SIGN_IN_ATTEMPTS)", body = WyrdProblem,
          headers(("Retry-After" = u64, description = "Seconds until the window ends"))),
        (status = 503, description = "The provider is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Platform"
)]
#[tracing::instrument(level = "debug", skip_all)]
async fn oidc_login(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    RequestSource(source): RequestSource,
) -> Result<Response, WyrdErrorResponse> {
    require_same_origin(&headers, &origin)?;
    admit_sign_in(&state, source).await?;
    begin_console_login(&state, &origin, None).await
}

/// `POST /platform/browser/oidc/callback` — finish a platform console
/// sign-in with the provider's return, from the browser that began it.
///
/// Success clears the binding cookie. A sign-in sets a freshly generated
/// platform session cookie anchored to the connection's current revision; a
/// re-authentication, presented with the session it began from, refreshes
/// that session's proof and sets no new cookie. A missing or foreign
/// binding, a replay, an unregistered subject, another operator's or an old
/// authentication, or a connection replaced since begin changes nothing.
///
/// # Errors
/// `403 WYRD_AUTH_403_ORIGIN_REFUSED` without this deployment's exact
/// `Origin`; `401 WYRD_AUTH_401_UNAUTHENTICATED` for every refusal;
/// `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` when the provider is unavailable.
#[utoipa::path(
    post,
    path = "/platform/browser/oidc/callback",
    request_body = PlatformCallbackRequest,
    responses(
        (status = 200, description = "Signed in; a fresh platform session cookie is set",
          body = PlatformBrowserSessionView,
          headers(("Set-Cookie" = String, description = "The platform session cookie"))),
        (status = 401, description = "Identity not accepted \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Origin absent or foreign \
          (WYRD_AUTH_403_ORIGIN_REFUSED)", body = WyrdProblem),
        (status = 503, description = "The provider is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Platform"
)]
#[tracing::instrument(level = "debug", skip_all)]
async fn oidc_callback(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    id: Option<Extension<RequestId>>,
    request: Result<Json<PlatformCallbackRequest>, JsonRejection>,
) -> Result<Response, WyrdErrorResponse> {
    let Json(request) = request.map_err(|rejection| body_rejection(&rejection))?;
    require_same_origin(&headers, &origin)?;
    let binding = cookie_pairs(&headers)
        .find_map(|(name, value)| (name == LOGIN_BINDING_COOKIE).then_some(value))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| session_error(PlatformSessionError::Invalid))?;
    // A re-authentication is finished by the session it began from; a
    // sign-in needs none, so an absent or dead cookie is simply no session.
    let presented = match platform_cookie(&headers) {
        Some(cookie) => match platform_sessions(&state)?.resolve_browser(cookie).await {
            Ok(session) => Some(session),
            Err(PlatformSessionError::Invalid) => None,
            Err(error) => return Err(session_error(error)),
        },
        None => None,
    };
    let clear_binding = cookie_header(&origin, LOGIN_BINDING_COOKIE, "", PLATFORM_COOKIE_PATH, 0);
    match login_service(&state)?
        .complete_browser(
            SecretString::from(request.code),
            &request.state,
            request.iss.as_deref(),
            binding,
            presented.as_ref(),
            request_id(id).as_str(),
        )
        .await
        .map_err(login_error)?
    {
        PlatformBrowserLogin::SignedIn(created) => Ok(with_cookies(
            StatusCode::OK,
            vec![
                set_platform_cookie(&origin, created.cookie.expose_secret()),
                clear_binding,
            ],
            Json(created.session.view),
        )),
        PlatformBrowserLogin::Reauthenticated => {
            let session = presented.ok_or_else(|| session_error(PlatformSessionError::Invalid))?;
            Ok(with_cookies(
                StatusCode::OK,
                vec![clear_binding],
                Json(refreshed(session)),
            ))
        }
    }
}

/// The metadata of `session` once its proof has just been refreshed.
fn refreshed(session: PlatformBrowserSession) -> PlatformBrowserSessionView {
    PlatformBrowserSessionView {
        recent: true,
        ..session.view
    }
}

/// The platform browser session a console caller presents.
///
/// # Errors
/// `401 WYRD_AUTH_401_UNAUTHENTICATED` when the caller authenticated with a
/// platform token instead.
fn browser_session(caller: PlatformCaller) -> Result<PlatformBrowserSession, WyrdErrorResponse> {
    caller
        .browser
        .ok_or_else(|| session_error(PlatformSessionError::Invalid))
}

/// `POST /platform/browser/password/reauth` — prove the signed-in platform
/// user's own password again, refreshing this session's proof for
/// destructive platform changes.
///
/// Counted in the source's sign-in window. Only the session's own
/// principal's password is checked, so another operator's password or a
/// bootstrap key refreshes nothing.
///
/// # Errors
/// `401 WYRD_AUTH_401_UNAUTHENTICATED` without a live platform session or
/// when the password is not accepted; `403 WYRD_AUTH_403_ORIGIN_REFUSED`
/// without this deployment's exact `Origin`; `429
/// WYRD_AUTH_429_SIGN_IN_ATTEMPTS` while the window is exhausted; `503
/// WYRD_AUTH_503_VERIFY_UNAVAILABLE` when the store fails.
#[utoipa::path(
    post,
    path = "/platform/browser/password/reauth",
    request_body = PlatformPasswordReauthRequest,
    responses(
        (status = 200, description = "Proof refreshed", body = PlatformBrowserSessionView),
        (status = 401, description = "No live session, or password not accepted \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Origin absent or foreign \
          (WYRD_AUTH_403_ORIGIN_REFUSED)", body = WyrdProblem),
        (status = 429, description = "Too many sign-in starts from this source \
          (WYRD_AUTH_429_SIGN_IN_ATTEMPTS)", body = WyrdProblem,
          headers(("Retry-After" = u64, description = "Seconds until the window ends"))),
        (status = 503, description = "The store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Platform"
)]
#[tracing::instrument(level = "debug", skip_all)]
async fn password_reauth(
    State(state): State<AppState>,
    caller: PlatformCaller,
    RequestSource(source): RequestSource,
    request: Result<Json<PlatformPasswordReauthRequest>, JsonRejection>,
) -> Result<Response, WyrdErrorResponse> {
    let Json(request) = request.map_err(|rejection| body_rejection(&rejection))?;
    let session = browser_session(caller)?;
    admit_sign_in(&state, source).await?;
    platform_sessions(&state)?
        .reauthenticate_password(&session, &request.password.into_secret_string())
        .await
        .map_err(session_error)?;
    Ok(with_cookies(
        StatusCode::OK,
        Vec::new(),
        Json(refreshed(session)),
    ))
}

/// `POST /platform/browser/oidc/reauth` — begin a forced-fresh platform IdP
/// re-authentication of this session.
///
/// Counted in the source's sign-in window. The provider is asked for a new
/// login (`prompt=login`, `max_age=0`) and returns to the console callback,
/// which this session must then present with the binding cookie set here.
///
/// # Errors
/// `401 WYRD_AUTH_401_UNAUTHENTICATED` without a live platform session;
/// `403 WYRD_AUTH_403_ORIGIN_REFUSED` without this deployment's exact
/// `Origin`; `404 WYRD_SPEC_404_NOT_FOUND` without a connection; `429
/// WYRD_AUTH_429_SIGN_IN_ATTEMPTS` while the window is exhausted; `503
/// WYRD_AUTH_503_VERIFY_UNAVAILABLE` when the provider is unavailable.
#[utoipa::path(
    post,
    path = "/platform/browser/oidc/reauth",
    responses(
        (status = 200, description = "Re-authentication begun; a login binding cookie is set",
          body = LoginInitResponse,
          headers(("Set-Cookie" = String, description = "The login binding cookie"))),
        (status = 401, description = "No live platform session \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "Origin absent or foreign \
          (WYRD_AUTH_403_ORIGIN_REFUSED)", body = WyrdProblem),
        (status = 404, description = "No platform OIDC connection \
          (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 429, description = "Too many sign-in starts from this source \
          (WYRD_AUTH_429_SIGN_IN_ATTEMPTS)", body = WyrdProblem,
          headers(("Retry-After" = u64, description = "Seconds until the window ends"))),
        (status = 503, description = "The provider is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Platform"
)]
#[tracing::instrument(level = "debug", skip_all)]
async fn oidc_reauth(
    State(state): State<AppState>,
    caller: PlatformCaller,
    RequestOrigin(origin): RequestOrigin,
    RequestSource(source): RequestSource,
) -> Result<Response, WyrdErrorResponse> {
    let session = browser_session(caller)?;
    admit_sign_in(&state, source).await?;
    begin_console_login(&state, &origin, Some(&session)).await
}

/// Begin a console OIDC login returning to the fixed console callback,
/// re-authenticating `session` when given, and set its binding cookie.
///
/// # Errors
/// The login owner's refusals, projected by [`login_error`].
async fn begin_console_login(
    state: &AppState,
    origin: &Url,
    session: Option<&PlatformBrowserSession>,
) -> Result<Response, WyrdErrorResponse> {
    let redirect = origin
        .join(CONSOLE_CALLBACK_PATH)
        .map_err(|_| auth_not_configured())?;
    let begun = login_service(state)?
        .begin_browser(redirect.to_string(), session)
        .await
        .map_err(login_error)?;
    Ok(with_cookies(
        StatusCode::OK,
        vec![cookie_header(
            origin,
            LOGIN_BINDING_COOKIE,
            begun.binding.expose_secret(),
            PLATFORM_COOKIE_PATH,
            LOGIN_BINDING_MAX_AGE,
        )],
        Json(begun.init),
    ))
}
