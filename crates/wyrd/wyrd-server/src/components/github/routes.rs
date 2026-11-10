//! HTTP projection of the tenant GitHub connection.
//!
//! Five tenant operations under `/v1/integrations/github`, authenticated like
//! every other tenant route (a browser session reaches setup and callback),
//! and one webhook that authenticates by GitHub's signature instead. Each
//! handler parses its typed input and delegates to [`GithubConnections`].

use axum::Json;
use axum::body::Bytes;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use secrecy::ExposeSecret as _;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wyrd_spec::error::WyrdProblem;
use wyrd_spec::github_connection::{
    GithubCallbackQuery, GithubConnectRequest, GithubConnectResponse, GithubConnectionView,
    GithubSetupQuery,
};

use super::service::{GithubConnections, TRANSACTION_TTL, not_configured};
use crate::auth::browser::{cookie_header, cookie_pairs};
use crate::components::auth::Caller;
use crate::http::error::{WyrdErrorResponse, body_rejection, query_rejection};
use crate::state::AppState;

/// Cookie binding a connection transaction to the browser that reached setup.
const BINDING_COOKIE: &str = "wyrd_github_binding";
/// The only path the binding cookie is sent to.
const CALLBACK_PATH: &str = "/v1/integrations/github/callback";

/// Build the tenant-authenticated GitHub routes for the `/v1` group.
pub fn github_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_connection, disconnect))
        .routes(routes!(connect))
        .routes(routes!(setup))
        .routes(routes!(callback))
}

/// Build the signature-authenticated GitHub webhook route for the `/v1`
/// group.
pub fn github_webhook_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(webhook))
}

/// Begin a connection transaction and return GitHub's install URL for it.
///
/// # Errors
/// Returns a stable Wyrd error for a body that is not empty, missing
/// `identity_connections:write`, no configured App, or a failed write.
#[utoipa::path(
    post,
    path = "/integrations/github/connect",
    request_body = GithubConnectRequest,
    responses(
        (status = 201, description = "Transaction begun", body = GithubConnectResponse),
        (status = 400, description = "The body is not an empty object \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 401, description = "The request carried no usable credential \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 503, description = "No GitHub App is configured, or the registry is \
          unavailable (WYRD_GITHUB_503_NOT_CONFIGURED, \
          WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "GitHub"
)]
#[tracing::instrument(
    skip(state, caller, body),
    fields(operation = "github_connection.connect")
)]
async fn connect(
    State(state): State<AppState>,
    caller: Caller,
    body: Result<Json<GithubConnectRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<GithubConnectResponse>), WyrdErrorResponse> {
    let Json(GithubConnectRequest {}) = body.map_err(|rejection| body_rejection(&rejection))?;
    let receipt = GithubConnections::new(&state).connect(&caller).await?;
    Ok((StatusCode::CREATED, Json(receipt)))
}

/// Read the tenant's redacted GitHub connection.
///
/// # Errors
/// Returns a stable Wyrd error for missing `identity_connections:write`, no
/// configured App, or a failed read.
#[utoipa::path(
    get,
    path = "/integrations/github",
    responses(
        (status = 200, description = "The redacted connection", body = GithubConnectionView),
        (status = 401, description = "The request carried no usable credential \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 503, description = "No GitHub App is configured, or the registry is \
          unavailable (WYRD_GITHUB_503_NOT_CONFIGURED, \
          WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "GitHub"
)]
#[tracing::instrument(skip(state, caller), fields(operation = "github_connection.read"))]
async fn get_connection(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<GithubConnectionView>, WyrdErrorResponse> {
    Ok(Json(GithubConnections::new(&state).get(&caller).await?))
}

/// Fence the tenant's GitHub work and release its installation.
///
/// # Errors
/// Returns a stable Wyrd error for missing `identity_connections:write`, no
/// configured App, no association, or a failed write.
#[utoipa::path(
    delete,
    path = "/integrations/github",
    responses(
        (status = 200, description = "Released", body = GithubConnectionView),
        (status = 401, description = "The request carried no usable credential \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks identity_connections:write \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 404, description = "No installation is associated \
          (WYRD_GITHUB_404_NOT_CONNECTED)", body = WyrdProblem),
        (status = 503, description = "No GitHub App is configured, or the registry is \
          unavailable (WYRD_GITHUB_503_NOT_CONFIGURED, \
          WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "GitHub"
)]
#[tracing::instrument(
    skip(state, caller),
    fields(operation = "github_connection.disconnect")
)]
async fn disconnect(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Json<GithubConnectionView>, WyrdErrorResponse> {
    Ok(Json(
        GithubConnections::new(&state).disconnect(&caller).await?,
    ))
}

/// GitHub's setup redirect: bind the transaction to this signed-in browser
/// and redirect to GitHub's OAuth authorization.
///
/// # Errors
/// Returns a stable Wyrd error for a malformed query, missing
/// `identity_connections:write`, a transaction this browser session cannot
/// bind, no configured App, or a failed write.
#[utoipa::path(
    get,
    path = "/integrations/github/setup",
    params(
        ("installation_id" = Option<u64>, Query, description = "Installation GitHub reports; a hint"),
        ("setup_action" = Option<String>, Query, description = "GitHub's setup action; informational"),
        ("state" = Option<String>, Query, description = "Transaction state when begun from Wyrd")
    ),
    responses(
        (status = 302, description = "Redirect to GitHub's OAuth authorization"),
        (status = 400, description = "The query is malformed \
          (WYRD_SPEC_400_VALIDATION, WYRD_GITHUB_400_REQUEST_INVALID)", body = WyrdProblem),
        (status = 401, description = "The browser has no Wyrd session \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks identity_connections:write, or \
          the transaction is not valid for this session \
          (WYRD_PERMISSION_403_DENIED_RBAC, WYRD_GITHUB_403_CONNECTION_REFUSED)",
          body = WyrdProblem),
        (status = 503, description = "No GitHub App is configured, or the registry is \
          unavailable (WYRD_GITHUB_503_NOT_CONFIGURED, \
          WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "GitHub"
)]
#[tracing::instrument(
    skip(state, caller, query),
    fields(operation = "github_connection.setup")
)]
async fn setup(
    State(state): State<AppState>,
    caller: Caller,
    query: Result<Query<GithubSetupQuery>, QueryRejection>,
) -> Result<Response, WyrdErrorResponse> {
    let Query(query) = query.map_err(|rejection| query_rejection(&rejection))?;
    let redirect = GithubConnections::new(&state)
        .setup(&caller, &query)
        .await?;
    let app = state.github.as_deref().ok_or_else(not_configured)?;
    let location = HeaderValue::from_str(redirect.authorize_url.as_str())
        .map_err(|_| super::app::unavailable("the authorization URL"))?;
    let cookie = cookie_header(
        &app.settings().public_url,
        BINDING_COOKIE,
        redirect.binding.expose_secret(),
        CALLBACK_PATH,
        TRANSACTION_TTL.as_secs(),
    );
    Ok((
        StatusCode::FOUND,
        [(header::LOCATION, location), (header::SET_COOKIE, cookie)],
    )
        .into_response())
}

/// GitHub's OAuth callback: verify the authorization and installation and
/// associate it with the signed-in tenant.
///
/// # Errors
/// Returns a stable Wyrd error for a malformed query, missing
/// `identity_connections:write`, a refused or conflicting association, no
/// configured App, an unreachable GitHub, or a failed write.
#[utoipa::path(
    get,
    path = "/integrations/github/callback",
    params(
        ("code" = Option<String>, Query, description = "Transient user authorization code"),
        ("state" = Option<String>, Query, description = "The transaction's one-use state"),
        ("installation_id" = Option<u64>, Query, description = "Installation GitHub reports; a hint"),
        ("setup_action" = Option<String>, Query, description = "GitHub's setup action; informational"),
        ("error" = Option<String>, Query, description = "OAuth error when authorization was denied")
    ),
    responses(
        (status = 200, description = "Associated", body = GithubConnectionView),
        (status = 400, description = "The query is malformed \
          (WYRD_SPEC_400_VALIDATION, WYRD_GITHUB_400_REQUEST_INVALID)", body = WyrdProblem),
        (status = 401, description = "The browser has no Wyrd session \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "The principal lacks identity_connections:write, or \
          a connection check refused (WYRD_PERMISSION_403_DENIED_RBAC, \
          WYRD_GITHUB_403_CONNECTION_REFUSED)", body = WyrdProblem),
        (status = 409, description = "The connection changed, the tenant holds another \
          installation, or another tenant holds this one \
          (WYRD_GITHUB_409_CONNECTION_CONFLICT)", body = WyrdProblem),
        (status = 503, description = "No GitHub App is configured, GitHub is unreachable, \
          or the registry is unavailable (WYRD_GITHUB_503_NOT_CONFIGURED, \
          WYRD_GITHUB_503_UNAVAILABLE, WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE)",
          body = WyrdProblem)
    ),
    tag = "GitHub"
)]
#[tracing::instrument(
    skip(state, caller, headers, query),
    fields(operation = "github_connection.callback")
)]
async fn callback(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    query: Result<Query<GithubCallbackQuery>, QueryRejection>,
) -> Result<Response, WyrdErrorResponse> {
    let Query(query) = query.map_err(|rejection| query_rejection(&rejection))?;
    let mut bindings = cookie_pairs(&headers).filter(|(name, _)| *name == BINDING_COOKIE);
    let binding = match (bindings.next(), bindings.next()) {
        (Some((_, value)), None) => Some(value),
        _ => None,
    };
    let view = GithubConnections::new(&state)
        .callback(&caller, &query, binding)
        .await?;
    let app = state.github.as_deref().ok_or_else(not_configured)?;
    let cleared = cookie_header(
        &app.settings().public_url,
        BINDING_COOKIE,
        "",
        CALLBACK_PATH,
        0,
    );
    Ok(([(header::SET_COOKIE, cleared)], Json(view)).into_response())
}

/// One GitHub webhook delivery, authenticated by `X-Hub-Signature-256`.
///
/// # Errors
/// Returns a stable Wyrd error for a missing or wrong signature, a body that
/// is not an event, no configured App, or a failed write.
#[utoipa::path(
    post,
    path = "/integrations/github/webhook",
    request_body(content = String, description = "GitHub's event payload, signed"),
    responses(
        (status = 202, description = "Accepted"),
        (status = 400, description = "The body is not a GitHub event \
          (WYRD_GITHUB_400_REQUEST_INVALID)", body = WyrdProblem),
        (status = 401, description = "The signature does not verify \
          (WYRD_GITHUB_401_WEBHOOK_REFUSED)", body = WyrdProblem),
        (status = 503, description = "No GitHub App is configured, or the registry is \
          unavailable (WYRD_GITHUB_503_NOT_CONFIGURED, \
          WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE)", body = WyrdProblem)
    ),
    tag = "GitHub"
)]
#[tracing::instrument(
    skip(state, headers, body),
    fields(operation = "github_connection.webhook")
)]
async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, WyrdErrorResponse> {
    let header_str = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    GithubConnections::new(&state)
        .webhook(
            header_str("x-github-event"),
            header_str("x-hub-signature-256"),
            &body,
        )
        .await?;
    Ok(StatusCode::ACCEPTED)
}
