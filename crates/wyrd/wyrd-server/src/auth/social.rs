//! Anonymous social sign-in routes: the provider-specific callbacks and the
//! sign-in method listing.
//!
//! Each shared provider returns to its own fixed callback, the URL the
//! operator registered in the provider console. The callback consumes the
//! transaction its `state` names before any provider IO and finishes it for
//! the purpose that transaction recorded.
//!
//! The method listing is the projection a sign-in page renders. It discloses
//! only what a visitor may already try; an unknown tenant reads exactly like a
//! tenant without company SSO.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use secrecy::ExposeSecret as _;
use wyrd_auth::callback::LoginCompletion;
use wyrd_auth::social::{SIGNUP_EMAIL_TTL, TenantLogin};
use wyrd_spec::auth::{
    BrowserLoginResponse, CreateTenantRequest, ProofStatus, ProofStatusResponse,
    SignupBeginRequest, SignupCompleteRequest, SignupCompleteResponse, SignupVerifyRequest,
};
use wyrd_spec::auth::{CallbackQuery, LoginMethodsQuery, LoginMethodsResponse, SocialProvider};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::request_id::RequestId;

use crate::auth::RequestOrigin;
use crate::auth::browser::{
    LOGIN_COOKIE, LOGIN_COOKIE_MAX_AGE_SECONDS, SOCIAL_LOGIN_COOKIE_PATH, browser_sessions,
    clear_login_cookie, cookie_header, cookie_pairs, login_binding, login_refused, request_id,
    require_same_origin, see_other, set_session_cookie, signed_in, with_cookies,
};
use crate::auth::source::RequestSource;
use crate::components::platform::provisioning::HumanOwner;
use crate::components::platform::routes::{directory, provision_error};
use crate::components::platform::social::social;
use crate::http::error::{WyrdErrorResponse, body_rejection};
use crate::state::AppState;

/// List the sign-in methods Rust currently allows for a tenant's sign-in page.
///
/// # Errors
/// Returns `503` when the platform or tenant store is unavailable.
#[utoipa::path(
    get,
    path = "/auth/browser/methods",
    params(LoginMethodsQuery),
    responses(
        (status = 200, description = "The methods a sign-in page may offer", body = LoginMethodsResponse),
        (status = 503, description = "The store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip(state))]
pub async fn methods(
    State(state): State<AppState>,
    Query(query): Query<LoginMethodsQuery>,
) -> Result<Json<LoginMethodsResponse>, WyrdErrorResponse> {
    Ok(Json(
        available_methods(&state, query.tenant.as_ref()).await?,
    ))
}

/// The methods Rust allows now on `tenant`'s sign-in page, or the root's;
/// the one report every login, enrollment, reset and creation path checks
/// before it acts. An unknown tenant reads as one without company SSO or
/// policy, so the report reveals nothing about which tenants exist.
///
/// # Errors
/// Returns `503` when the platform or tenant store is unavailable.
pub(crate) async fn available_methods(
    state: &AppState,
    tenant: Option<&wyrd_spec::TenantSlug>,
) -> Result<LoginMethodsResponse, WyrdErrorResponse> {
    let tenant = match (tenant, &state.auth.human_connections) {
        (None, _) => None,
        (Some(_), None) => Some(TenantLogin::default()),
        (Some(slug), Some(connections)) => {
            match state.postgres.wyrd().resolve_tenant_slug(slug).await {
                Ok(Some(tenant)) => Some(connections.tenant_login(tenant).await?),
                Ok(None) => Some(TenantLogin::default()),
                Err(error) => {
                    tracing::error!(error = %error, "tenant slug resolution failed");
                    return Err(WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
                        message: "sign-in methods are temporarily unavailable".to_owned(),
                        details: serde_json::json!({ "retry_after_seconds": 1 }),
                    }));
                }
            }
        }
    };
    Ok(social(state)?.login_methods(tenant).await?)
}

/// Finish a sign-in the shared provider returned to its fixed callback.
///
/// An operator provider test marks its exact revision tested and answers a
/// short page the operator can close. A tenant sign-in completes only for the
/// browser that began it
/// ([`wyrd_auth::browser_sessions::BrowserSessions::complete_social`]); a refusal
/// sends the browser back to the tenant's sign-in page, with account guidance
/// when the provider identity belongs to no User there.
///
/// # Errors
/// Returns `400` when the state names no pending sign-in, `401` when a
/// provider test fails verification, `404` for an unknown provider, and
/// `503` when the provider or store is unavailable.
#[utoipa::path(
    get,
    path = "/auth/social/{provider}/callback",
    params(
        ("provider" = SocialProvider, Path, description = "`google` or `github`"),
        ("code" = Option<String>, Query, description = "Authorization code from the provider; \
          exactly one of `code` or `error` is present"),
        ("error" = Option<String>, Query, description = "The provider's RFC 6749 §4.1.2.1 error code"),
        ("state" = String, Query, description = "Opaque sign-in state Wyrd generated at initiation"),
        ("iss" = Option<String>, Query, description = "RFC 9207 authorization-response issuer")
    ),
    responses(
        (status = 200, description = "The provider test passed", content_type = "text/html", body = String),
        (status = 303, description = "A tenant sign-in was refused; back to its sign-in page"),
        (status = 400, description = "Malformed return, or a state naming no pending sign-in \
          (WYRD_SPEC_400_VALIDATION, WYRD_AUTH_400_INVALID_STATE)", body = WyrdProblem),
        (status = 401, description = "A provider test failed verification \
          (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 404, description = "Not a shared provider (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The provider or store is unavailable \
          (WYRD_AUTH_503_DISCOVERY_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "info", skip(state, origin, headers, query, id))]
pub async fn callback(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    Path(name): Path<String>,
    headers: HeaderMap,
    id: Option<Extension<RequestId>>,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, WyrdErrorResponse> {
    let provider = SocialProvider::parse(&name).ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::NotFound {
            message: "no such social sign-in provider".to_owned(),
            details: serde_json::json!({ "provider": name }),
        })
    })?;
    let response = query.response()?;
    let social = social(&state)?;
    if social
        .complete_test(provider, response, &query.state, query.iss.as_deref())
        .await?
    {
        let mut page = Html(format!(
            "<!doctype html><title>Wyrd</title><p>The {provider} sign-in test passed. \
             Return to Wyrd to enable it.</p>"
        ))
        .into_response();
        page.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        *page.status_mut() = StatusCode::OK;
        return Ok(page);
    }
    match social
        .prove_signup(provider, &query, login_binding(&headers).as_deref())
        .await
    {
        Ok(None) => {}
        Ok(Some(completion)) => {
            return Ok(see_other(
                SIGNUP_FORM,
                vec![
                    cookie_header(
                        &origin,
                        SIGNUP_COOKIE,
                        completion.expose_secret(),
                        SIGNUP_COOKIE_PATH,
                        SIGNUP_COOKIE_MAX_AGE_SECONDS,
                    ),
                    clear_login_cookie(&origin, SOCIAL_LOGIN_COOKIE_PATH),
                ],
            ));
        }
        Err(error) if error.status() >= 500 => return Err(WyrdErrorResponse::from(error)),
        Err(error) => {
            tracing::info!(code = error.code(), "workspace signup proof refused");
            return Ok(see_other(
                "/signup?signup=failed",
                vec![clear_login_cookie(&origin, SOCIAL_LOGIN_COOKIE_PATH)],
            ));
        }
    }
    let request_id = id.map_or_else(RequestId::now_v7, |Extension(id)| id);
    match browser_sessions(&state)?
        .complete_social(
            social,
            provider,
            &query,
            login_binding(&headers).as_deref(),
            &request_id.to_string(),
        )
        .await?
    {
        Some(LoginCompletion::SignedIn(created)) => {
            signed_in(&state, &origin, &created, SOCIAL_LOGIN_COOKIE_PATH).await
        }
        Some(LoginCompletion::Refused { tenant_id, error }) => {
            login_refused(&state, &origin, tenant_id, error, SOCIAL_LOGIN_COOKIE_PATH).await
        }
        Some(LoginCompletion::DeviceApproved | LoginCompletion::ConnectionTested) | None => {
            Err(WyrdErrorResponse::from(WyrdError::InvalidState {
                message: "this sign-in has expired or was already used; start again".to_owned(),
                details: serde_json::json!({}),
            }))
        }
    }
}

/// The UI page a proven signup continues on to name its workspace.
const SIGNUP_FORM: &str = "/signup/workspace";

/// Name of the cookie holding a proven signup's completion secret.
const SIGNUP_COOKIE: &str = "wyrd_signup";

/// The only path the completion cookie is sent to.
const SIGNUP_COOKIE_PATH: &str = "/auth/signup";

/// Lifetime of the completion cookie: the proof's completion window.
const SIGNUP_COOKIE_MAX_AGE_SECONDS: u64 = 300;

/// `POST /auth/signup` — begin creating a workspace by proving, in this
/// browser, a mailbox or a shared social provider identity.
///
/// A social start sets the sealed `wyrd_login` round-trip cookie the
/// provider callback requires and returns the provider authorization URL. An
/// email start mails the address a one-use link, sets the sealed
/// `wyrd_signup_email` flow cookie, and answers `202 check_email`. Nothing is
/// stored or created.
///
/// # Errors
/// `403 WYRD_AUTH_403_ORIGIN_REFUSED` without this deployment's exact
/// `Origin` or `403 WYRD_AUTH_403_METHOD_UNAVAILABLE` when signup, local
/// signup or the provider is not available; `503` when the provider or store
/// is unavailable.
#[utoipa::path(
    post,
    path = "/auth/signup",
    request_body = SignupBeginRequest,
    responses(
        (status = 200, description = "Social signup begun; the login-transaction cookie is set",
          body = BrowserLoginResponse,
          headers(("Set-Cookie" = String, description = "The `wyrd_login` transaction cookie"))),
        (status = 202, description = "Email signup begun; the proof was mailed and the flow \
          cookie is set", body = ProofStatusResponse,
          headers(("Set-Cookie" = String, description = "The `wyrd_signup_email` flow cookie"))),
        (status = 400, description = "The address is not an email address \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 403, description = "Origin absent or foreign, or signup or the provider is \
          not available (WYRD_AUTH_403_ORIGIN_REFUSED, WYRD_AUTH_403_METHOD_UNAVAILABLE)",
          body = WyrdProblem),
        (status = 429, description = "Too many signup starts from this source \
          (WYRD_AUTH_429_SIGNUP_ATTEMPTS)", body = WyrdProblem,
          headers(("Retry-After" = u64, description = "Seconds until the window ends"))),
        (status = 503, description = "The identity provider or store is unavailable, or the \
          request's source cannot be determined (WYRD_AUTH_503_DISCOVERY_UNAVAILABLE, \
          WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn signup(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    RequestSource(source): RequestSource,
    request: Result<Json<SignupBeginRequest>, JsonRejection>,
) -> Result<Response, WyrdErrorResponse> {
    let Json(request) = request.map_err(|rejection| body_rejection(&rejection))?;
    require_same_origin(&headers, &origin)?;
    let provider = match request {
        SignupBeginRequest::Social { provider } => provider,
        SignupBeginRequest::Email { email } => {
            let mail = state.auth.application_mail.as_ref().ok_or_else(|| {
                WyrdErrorResponse::from(WyrdError::Internal {
                    message: "platform control plane is not configured".to_owned(),
                    details: serde_json::json!({ "plane": "platform" }),
                })
            })?;
            // A provider attempt this browser holds may need this mailbox
            // as its verified contact.
            let mut completions = cookie_pairs(&headers).filter(|(name, _)| *name == SIGNUP_COOKIE);
            let completion = match (completions.next(), completions.next()) {
                (Some((_, value)), None) => Some(value),
                _ => None,
            };
            let flow = social(&state)?
                .begin_email_signup(mail, &email, &origin, source, completion)
                .await?;
            return Ok(with_cookies(
                StatusCode::ACCEPTED,
                vec![cookie_header(
                    &origin,
                    SIGNUP_EMAIL_COOKIE,
                    flow.expose_secret(),
                    SIGNUP_COOKIE_PATH,
                    SIGNUP_EMAIL_TTL.as_secs(),
                )],
                Json(ProofStatusResponse {
                    status: ProofStatus::CheckEmail,
                }),
            ));
        }
    };
    let begun = social(&state)?
        .begin_signup(provider, &origin, source)
        .await?;
    Ok(with_cookies(
        StatusCode::OK,
        vec![cookie_header(
            &origin,
            LOGIN_COOKIE,
            begun.binding.expose_secret(),
            SOCIAL_LOGIN_COOKIE_PATH,
            LOGIN_COOKIE_MAX_AGE_SECONDS,
        )],
        Json(BrowserLoginResponse {
            authorization_url: begun.authorization_url,
        }),
    ))
}

/// Name of the cookie holding an email signup's sealed flow.
const SIGNUP_EMAIL_COOKIE: &str = "wyrd_signup_email";

/// `POST /auth/signup/verify` — prove the mailbox an email signup message
/// was sent to, in the browser that began it.
///
/// The browser presents its `wyrd_signup_email` flow cookie and the proof
/// from the message. The proof is recorded once and the response sets the
/// `wyrd_signup` completion cookie; the same browser repeating the request
/// after a lost response gets a fresh completion cookie for the same proof.
///
/// # Errors
/// `401 WYRD_AUTH_401_ONE_TIME_CODE_INVALID` for a missing flow cookie or a
/// wrong or spent proof; `400` once expired; `403
/// WYRD_AUTH_403_METHOD_UNAVAILABLE` when signup or password is disabled.
#[utoipa::path(
    post,
    path = "/auth/signup/verify",
    request_body = SignupVerifyRequest,
    responses(
        (status = 200, description = "The mailbox is proven; the completion cookie is set",
          body = ProofStatusResponse,
          headers(("Set-Cookie" = String, description = "The `wyrd_signup` completion cookie"))),
        (status = 400, description = "Invalid body, or the proof expired \
          (WYRD_SPEC_400_VALIDATION, WYRD_AUTH_400_INVALID_STATE)", body = WyrdProblem),
        (status = 401, description = "No flow in this browser, or a wrong or spent proof \
          (WYRD_AUTH_401_ONE_TIME_CODE_INVALID)", body = WyrdProblem),
        (status = 403, description = "Origin absent or foreign, or signup or password is \
          disabled (WYRD_AUTH_403_ORIGIN_REFUSED, WYRD_AUTH_403_METHOD_UNAVAILABLE)",
          body = WyrdProblem),
        (status = 503, description = "The store is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn signup_verify(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    request: Result<Json<SignupVerifyRequest>, JsonRejection>,
) -> Result<Response, WyrdErrorResponse> {
    let Json(request) = request.map_err(|rejection| body_rejection(&rejection))?;
    require_same_origin(&headers, &origin)?;
    let mut flows = cookie_pairs(&headers).filter(|(name, _)| *name == SIGNUP_EMAIL_COOKIE);
    let flow = match (flows.next(), flows.next()) {
        (Some((_, value)), None) => Some(value),
        _ => None,
    };
    let completion = social(&state)?
        .verify_email_signup(flow, &request.proof)
        .await?;
    Ok(with_cookies(
        StatusCode::OK,
        vec![
            cookie_header(
                &origin,
                SIGNUP_COOKIE,
                completion.expose_secret(),
                SIGNUP_COOKIE_PATH,
                SIGNUP_COOKIE_MAX_AGE_SECONDS,
            ),
            cookie_header(&origin, SIGNUP_EMAIL_COOKIE, "", SIGNUP_COOKIE_PATH, 0),
        ],
        Json(ProofStatusResponse {
            status: ProofStatus::Proven,
        }),
    ))
}

/// `POST /auth/signup/complete` — create the workspace a proven signup will
/// administer, and sign its creator into it.
///
/// The browser presents the `wyrd_signup` completion cookie its provider
/// return set. Signup and the provider are rechecked and the attempt is
/// counted against the shared source and identity windows before the
/// existing provisioner claims the slug, initializes the tenant with the
/// creator as its `admin` User and promotes it. The response sets the new
/// workspace's session cookie and carries no key or platform authority. A
/// completed attempt never creates another workspace; a creator whose
/// response was lost signs in to it.
///
/// # Errors
/// `400` for an invalid body or slug, or a missing or expired proof; `401`
/// when signup or the provider changed; `409` when the slug is taken, the
/// attempt already created its workspace or another completion holds it;
/// `429 WYRD_AUTH_429_SIGNUP_ATTEMPTS` while a window is exhausted; `503`
/// when admission state is unavailable; `500` when provisioning fails.
#[utoipa::path(
    post,
    path = "/auth/signup/complete",
    request_body = SignupCompleteRequest,
    responses(
        (status = 200, description = "The workspace exists and its session cookie is set",
          body = SignupCompleteResponse,
          headers(("Set-Cookie" = String, description = "The workspace session cookie"))),
        (status = 400, description = "Invalid body or slug, or no live signup proof \
          (WYRD_SPEC_400_VALIDATION, WYRD_AUTH_400_INVALID_STATE)", body = WyrdProblem),
        (status = 401, description = "Signup or the provider changed after proof \
          (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 403, description = "Origin absent or foreign \
          (WYRD_AUTH_403_ORIGIN_REFUSED)", body = WyrdProblem),
        (status = 409, description = "Slug taken, workspace already created, or creation \
          in progress (WYRD_SPEC_409_CONFLICT)", body = WyrdProblem),
        (status = 429, description = "Too many workspace signups \
          (WYRD_AUTH_429_SIGNUP_ATTEMPTS)", body = WyrdProblem,
          headers(("Retry-After" = u64, description = "Seconds until the window ends"))),
        (status = 500, description = "Provisioning failed (WYRD_SPEC_500_INTERNAL)",
          body = WyrdProblem),
        (status = 503, description = "Admission state is unavailable \
          (WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "info", skip_all)]
pub async fn signup_complete(
    State(state): State<AppState>,
    RequestOrigin(origin): RequestOrigin,
    headers: HeaderMap,
    RequestSource(source): RequestSource,
    id: Option<Extension<RequestId>>,
    request: Result<Json<SignupCompleteRequest>, JsonRejection>,
) -> Result<Response, WyrdErrorResponse> {
    let Json(request) = request.map_err(|rejection| body_rejection(&rejection))?;
    require_same_origin(&headers, &origin)?;
    let mut completions = cookie_pairs(&headers).filter(|(name, _)| *name == SIGNUP_COOKIE);
    let completion = match (completions.next(), completions.next()) {
        (Some((_, value)), None) => value.to_owned(),
        _ => {
            return Err(WyrdErrorResponse::from(WyrdError::InvalidState {
                message: "no workspace signup is in progress in this browser; start again"
                    .to_owned(),
                details: serde_json::json!({}),
            }));
        }
    };
    let social = social(&state)?;
    let sessions = browser_sessions(&state)?;
    let provisioning = directory(&state)?;
    let creation = social
        .admit_signup(&completion, request.local_credentials.as_ref(), source)
        .await?;
    // The challenge runs after admission, so refused traffic costs no
    // Siteverify call, and before the claim, so a refusal creates nothing.
    if let Err(error) = social
        .require_challenge(
            request.challenge_token.as_deref(),
            origin.host_str().unwrap_or_default(),
            source,
        )
        .await
    {
        social.finish_signup(creation.attempt, false).await?;
        return Err(WyrdErrorResponse::from(error));
    }
    if let Some((_, claimed)) = &creation.claimed
        && claimed != request.slug.as_str()
    {
        social.finish_signup(creation.attempt, false).await?;
        return Err(WyrdErrorResponse::from(WyrdError::Conflict {
            message: format!("this signup is creating workspace {claimed}; retry with that name"),
            details: serde_json::json!({ "field": "slug", "tenant": claimed }),
        }));
    }
    let owner = HumanOwner {
        attempt: creation.attempt,
        credential: &creation.credential,
        creator_key: &creation.creator_key,
    };
    let tenant_request = CreateTenantRequest {
        slug: request.slug.clone(),
        display_name: request.display_name,
    };
    let provisioned = match provisioning.claim_for_signup(&owner, &tenant_request).await {
        Ok(tenant) => {
            let conn = state.postgres.tenant_conn(tenant).await;
            provisioning
                .provision_for_signup(tenant, owner, conn)
                .await
                .map(|owner| (tenant, owner))
        }
        Err(error) => Err(error),
    };
    // The attempt is settled before the session is opened: an active
    // workspace is never created twice, and a lost session is recovered by
    // signing in to it.
    social
        .finish_signup(creation.attempt, provisioned.is_ok())
        .await?;
    let (tenant, (user_id, identity_id)) = provisioned.map_err(provision_error)?;
    let created = sessions
        .open_signup_session(
            tenant,
            user_id,
            identity_id,
            &creation,
            request_id(id).as_str(),
        )
        .await?;
    Ok(with_cookies(
        StatusCode::OK,
        vec![
            set_session_cookie(&origin, &request.slug, &created.cookie),
            cookie_header(&origin, SIGNUP_COOKIE, "", SIGNUP_COOKIE_PATH, 0),
        ],
        Json(SignupCompleteResponse {
            tenant: request.slug,
        }),
    ))
}
