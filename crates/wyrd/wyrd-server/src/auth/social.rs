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
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use wyrd_spec::auth::{CallbackQuery, LoginMethodsQuery, LoginMethodsResponse, SocialProvider};
use wyrd_spec::error::{WyrdError, WyrdProblem};

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
/// short page the operator can close.
///
/// # Errors
/// Returns `401` when the transaction is unknown, expired, replayed or fails
/// verification, `404` for an unknown provider, and `503` when the provider
/// or store is unavailable.
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
        (status = 400, description = "Malformed provider return (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "Unknown, expired, replayed or unverifiable sign-in \
          (WYRD_AUTH_401_INVALID_TOKEN)", body = WyrdProblem),
        (status = 404, description = "Not a shared provider (WYRD_SPEC_404_NOT_FOUND)", body = WyrdProblem),
        (status = 503, description = "The provider or store is unavailable \
          (WYRD_AUTH_503_DISCOVERY_UNAVAILABLE, WYRD_AUTH_503_VERIFY_UNAVAILABLE)", body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "info", skip(state, query))]
pub async fn callback(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, WyrdErrorResponse> {
    let provider = SocialProvider::parse(&name).ok_or_else(|| {
        WyrdErrorResponse::from(WyrdError::NotFound {
            message: "no such social sign-in provider".to_owned(),
            details: serde_json::json!({ "provider": name }),
        })
    })?;
    let response = query.response()?;
    if social(&state)?
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
    Err(WyrdErrorResponse::from(WyrdError::InvalidToken {
        message: "this sign-in has expired or was already used; start again".to_owned(),
        details: serde_json::json!({}),
    }))
}
