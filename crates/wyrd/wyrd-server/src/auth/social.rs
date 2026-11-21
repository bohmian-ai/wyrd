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
use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use wyrd_auth::callback::LoginCompletion;
use wyrd_spec::auth::{CallbackQuery, LoginMethodsQuery, LoginMethodsResponse, SocialProvider};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::request_id::RequestId;

use crate::auth::RequestOrigin;
use crate::auth::browser::{
    SOCIAL_LOGIN_COOKIE_PATH, browser_sessions, login_binding, login_refused, signed_in,
};
use crate::components::platform::social::social;
use crate::http::error::WyrdErrorResponse;
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
    let company_sso = match (&query.tenant, &state.auth.human_connections) {
        (Some(slug), Some(connections)) => {
            match state.postgres.wyrd().resolve_tenant_slug(slug).await {
                Ok(Some(tenant)) => connections.active_connection(tenant).await?.is_some(),
                Ok(None) => false,
                Err(error) => {
                    tracing::error!(error = %error, "tenant slug resolution failed");
                    return Err(WyrdErrorResponse::from(WyrdError::AuthVerifyUnavailable {
                        message: "sign-in methods are temporarily unavailable".to_owned(),
                        details: serde_json::json!({ "retry_after_seconds": 1 }),
                    }));
                }
            }
        }
        _ => false,
    };
    Ok(Json(social(&state)?.login_methods(company_sso).await?))
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
