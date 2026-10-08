//! The OAuth authorization endpoint and authorization server metadata.
//!
//! `GET /auth/authorize` answers RFC 6749 §4.1.1 authorization requests from
//! the registered `wyrd-ui` client: once the client and its exact registered
//! redirect URI are matched, the tenant's provider sign-in begins, and the
//! common callback later redirects back with a single-use code or an RFC 6749
//! §4.1.2.1 error ([`client_redirect`]). The one extension parameter,
//! `tenant`, is pre-login routing context only. RFC 8414 metadata describes
//! these endpoints at `/.well-known/oauth-authorization-server`.

use axum::Json;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use url::{Url, form_urlencoded};
use wyrd_spec::auth::{
    AbsoluteUrl, AuthorizationServerMetadata, ClientAuthorization, OAuthClientId, OAuthErrorCode,
};
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::ids::TenantSlug;

use crate::auth::auth_not_configured;
use crate::auth::cli_login::page;
use crate::auth::oauth::{GRANT_TYPES, OAuthError, OAuthForm};
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Path of `wyrd-ui`'s registered redirect URI under the public origin.
const UI_REDIRECT_PATH: &str = "/login/callback";

/// `GET /auth/authorize` — begin a sign-in for an OAuth client (RFC 6749
/// §4.1.1, RFC 7636 §4.3).
///
/// Only `wyrd-ui` uses the authorization code grant, so `client_id` must
/// name it once and `redirect_uri` must appear once and equal its registered
/// `{public_origin}/login/callback` exactly (RFC 6749 §3.1.2); otherwise the
/// browser is shown an error page and never redirected. After that, any
/// other repeated parameter, a missing `response_type`, `code_challenge`, or
/// `tenant`, a challenge method other than `S256`, or a refused sign-in
/// redirects back with the RFC 6749 §4.1.2.1 error, carrying `state` only
/// when the request sent exactly one. A valid request
/// redirects (`303`) to the tenant's provider sign-in, bound to the client,
/// redirect URI, challenge, and `state`. No request header selects the
/// tenant, connection, or redirect, and initiation appends no audit event:
/// it evaluates no principal permission.
///
/// # Errors
/// Returns problem JSON when auth or the public origin is not configured.
#[utoipa::path(
    get,
    path = "/auth/authorize",
    params(
        ("response_type" = String, Query, description = "`code`"),
        ("client_id" = String, Query, description = "`wyrd-ui`"),
        ("redirect_uri" = String, Query, description = "The client's registered redirect URI"),
        ("code_challenge" = String, Query, description = "RFC 7636 S256 code challenge"),
        ("code_challenge_method" = String, Query, description = "`S256`"),
        ("state" = Option<String>, Query, description = "Opaque client state, echoed back"),
        ("tenant" = String, Query, description = "The tenant's route key")
    ),
    responses(
        (status = 303, description = "Sign in at the tenant's provider, or return to the \
          client's redirect URI with an RFC 6749 §4.1.2.1 `error`",
          headers(("Location" = String, description = "Provider or client redirect URI"))),
        (status = 400, description = "An unknown client or unregistered redirect URI is \
          shown an HTML page; a deployment without a public origin answers \
          WYRD_SPEC_400_VALIDATION",
          content((String = "text/html"), (WyrdProblem = "application/problem+json"))),
        (status = 500, description = "Auth is not configured (WYRD_SPEC_500_INTERNAL)",
          body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn authorize(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
) -> Result<Response, WyrdErrorResponse> {
    let connections = state
        .auth
        .human_connections
        .as_ref()
        .ok_or_else(auth_not_configured)?;
    let registered = ui_redirect_uri(connections.require_callback()?);
    let query = query.unwrap_or_default();
    if unique_param(&query, "client_id").as_deref() != Some(OAuthClientId::WyrdUi.as_str())
        || unique_param(&query, "redirect_uri").as_deref() != Some(registered.as_str())
    {
        return Ok(page(
            StatusCode::BAD_REQUEST,
            "<h1>Sign-in request refused</h1><p>The application or its return address is not \
             registered with this Wyrd deployment.</p>",
        ));
    }
    let mut authorization = ClientAuthorization {
        client: OAuthClientId::WyrdUi,
        redirect_uri: registered,
        code_challenge: String::new(),
        state: unique_param(&query, "state"),
    };
    let Ok(form) = OAuthForm::parse(query.as_bytes()) else {
        return Ok(client_redirect(
            &authorization,
            &[("error", error_name(OAuthErrorCode::InvalidRequest))],
        ));
    };
    form.get("code_challenge")
        .unwrap_or_default()
        .clone_into(&mut authorization.code_challenge);
    let refused = |code| {
        Ok(client_redirect(
            &authorization,
            &[("error", error_name(code))],
        ))
    };
    match form.get("response_type") {
        Some("code") => {}
        Some(_) => return refused(OAuthErrorCode::UnsupportedResponseType),
        None => return refused(OAuthErrorCode::InvalidRequest),
    }
    let Some(tenant) = form
        .get("tenant")
        .and_then(|tenant| TenantSlug::new(tenant.to_owned()).ok())
    else {
        return refused(OAuthErrorCode::InvalidRequest);
    };
    if form.get("code_challenge_method") != Some("S256")
        || !is_s256_challenge(&authorization.code_challenge)
    {
        return refused(OAuthErrorCode::InvalidRequest);
    }
    match connections.authorize(&tenant, authorization.clone()).await {
        Ok(url) => Ok(Redirect::to(url.as_str()).into_response()),
        Err(error) => refused(OAuthError::authorization(error)),
    }
}

/// `GET /.well-known/oauth-authorization-server` — RFC 8414 §3 metadata.
///
/// # Errors
/// Returns problem JSON when auth or the public origin is not configured.
#[utoipa::path(
    get,
    path = "/.well-known/oauth-authorization-server",
    responses(
        (status = 200, description = "Authorization server metadata",
          body = AuthorizationServerMetadata),
        (status = 400, description = "No public origin is configured \
          (WYRD_SPEC_400_VALIDATION)", body = WyrdProblem),
        (status = 500, description = "Auth is not configured (WYRD_SPEC_500_INTERNAL)",
          body = WyrdProblem)
    ),
    security(()),
    tag = "Auth"
)]
pub async fn metadata(
    State(state): State<AppState>,
) -> Result<Json<AuthorizationServerMetadata>, WyrdErrorResponse> {
    let callback = state
        .auth
        .human_connections
        .as_ref()
        .ok_or_else(auth_not_configured)?
        .require_callback()?;
    let origin = callback.origin().ascii_serialization();
    let url = |path: &str| {
        AbsoluteUrl::new(format!("{origin}{path}")).map_err(|error| {
            WyrdErrorResponse::from(WyrdError::Internal {
                message: format!("the public origin does not form an endpoint URL: {error}"),
                details: serde_json::json!({}),
            })
        })
    };
    let strings = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();
    let auth_methods = strings(&["client_secret_basic", "none"]);
    Ok(Json(AuthorizationServerMetadata {
        issuer: url("")?,
        authorization_endpoint: url("/auth/authorize")?,
        token_endpoint: url("/auth/token")?,
        device_authorization_endpoint: url("/auth/device_authorization")?,
        revocation_endpoint: url("/auth/revoke")?,
        response_types_supported: strings(&["code"]),
        grant_types_supported: strings(&GRANT_TYPES),
        token_endpoint_auth_methods_supported: Vec::clone(&auth_methods),
        revocation_endpoint_auth_methods_supported: auth_methods,
        code_challenge_methods_supported: strings(&["S256"]),
    }))
}

/// The value of the parameter `name` in the form-encoded `query` when it is
/// sent exactly once with a value; `None` when it is absent or repeated.
/// Parameters without a value are omitted, as [`OAuthForm`] omits them.
fn unique_param(query: &str, name: &str) -> Option<String> {
    let mut values = form_urlencoded::parse(query.as_bytes())
        .filter(|(key, value)| key == name && !value.is_empty())
        .map(|(_, value)| value.into_owned());
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

/// `wyrd-ui`'s registered redirect URI under the origin of the deployment's
/// configured provider callback.
pub(crate) fn ui_redirect_uri(callback: &Url) -> String {
    format!(
        "{}{UI_REDIRECT_PATH}",
        callback.origin().ascii_serialization()
    )
}

/// Redirect (`303`) to `authorization`'s redirect URI with `params` and the
/// client's `state` (RFC 6749 §4.1.2, §4.1.2.1).
pub(crate) fn client_redirect(
    authorization: &ClientAuthorization,
    params: &[(&str, &str)],
) -> Response {
    let Ok(mut location) = Url::parse(&authorization.redirect_uri) else {
        return WyrdErrorResponse::from(WyrdError::Internal {
            message: "the client's registered redirect URI does not parse".to_owned(),
            details: serde_json::json!({}),
        })
        .into_response();
    };
    {
        let mut query = location.query_pairs_mut();
        query.extend_pairs(params);
        if let Some(state) = &authorization.state {
            query.append_pair("state", state);
        }
    }
    Redirect::to(location.as_str()).into_response()
}

/// The wire name of an OAuth error code.
pub(crate) fn error_name(code: OAuthErrorCode) -> &'static str {
    match code {
        OAuthErrorCode::InvalidRequest => "invalid_request",
        OAuthErrorCode::InvalidClient => "invalid_client",
        OAuthErrorCode::InvalidGrant => "invalid_grant",
        OAuthErrorCode::UnauthorizedClient => "unauthorized_client",
        OAuthErrorCode::UnsupportedGrantType => "unsupported_grant_type",
        OAuthErrorCode::UnsupportedResponseType => "unsupported_response_type",
        OAuthErrorCode::InvalidTarget => "invalid_target",
        OAuthErrorCode::AccessDenied => "access_denied",
        OAuthErrorCode::AuthorizationPending => "authorization_pending",
        OAuthErrorCode::SlowDown => "slow_down",
        OAuthErrorCode::ExpiredToken => "expired_token",
        OAuthErrorCode::ServerError => "server_error",
        OAuthErrorCode::TemporarilyUnavailable => "temporarily_unavailable",
    }
}

/// Whether `challenge` has the form of an S256 code challenge: 43
/// unpadded base64url characters (RFC 7636 §4.2).
fn is_s256_challenge(challenge: &str) -> bool {
    challenge.len() == 43
        && challenge
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Authorization requests against a mock provider, proving the client and
/// redirect URI are matched exactly and request headers take no part in
/// tenant, connection, or redirect selection.
#[cfg(test)]
mod pg_tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth::connections::HumanConnections;
    use wyrd_auth_oidc::ScreenedHttp;
    use wyrd_dev_fixtures::pg::{PgFixture, seed_active_human_connection};
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use crate::components::auth::ServerAuth;
    use crate::state::AppState;

    /// A well-formed S256 challenge.
    const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

    /// App state over `fixture` whose tenant's Active connection points at a
    /// mock provider, with a fixed public origin.
    ///
    /// # Panics
    /// Panics when the fixture cannot be seeded or the state cannot be built.
    async fn authorize_state(fixture: &PgFixture, provider: &MockServer) -> AppState {
        let issuer = provider.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/authorize"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
                "response_types_supported": ["code"],
                "subject_types_supported": ["public"],
                "id_token_signing_alg_values_supported": ["EdDSA"],
            })))
            .mount(provider)
            .await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "keys": [] })),
            )
            .mount(provider)
            .await;
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection seeds");
        conn.commit().await.expect("seed commits");
        sqlx::query(
            "UPDATE wyrd.auth_human_connections SET issuer_url = $1 WHERE connection_id = $2",
        )
        .bind(&issuer)
        .bind(binding.connection_id)
        .execute(&fixture.superuser_pool().expect("superuser pool opens"))
        .await
        .expect("connection points at the mock provider");
        let origin = Url::parse("https://wyrd.example.com").expect("origin parses");
        let tempdir = tempfile::tempdir().expect("authorize storage tempdir");
        let signer = LocalSigner::new(tempdir.keep()).expect("local signer creates");
        let state = crate::test_support::test_app_state(
            Arc::new(crate::postgres::ServerPostgres::from_parts(
                fixture.wyrd_postgres().clone(),
                fixture.vala_postgres().clone(),
            )),
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        );
        let audit = Arc::clone(&state.audit_outbox);
        state.with_auth(ServerAuth {
            human_connections: Some(HumanConnections::new(
                fixture.wyrd_postgres().clone(),
                None,
                ScreenedHttp::allowing_internal(),
                Some(&origin),
                audit,
            )),
            ..ServerAuth::default()
        })
    }

    /// GET `/auth/authorize?{query}` with hostile `Host` and forwarded
    /// headers through the handler alone; returns the status and `Location`.
    ///
    /// # Panics
    /// Panics when the request cannot be built or answered.
    async fn get_authorize(state: AppState, query: &str) -> (StatusCode, Option<Url>) {
        let response = axum::Router::new()
            .route("/auth/authorize", axum::routing::get(super::authorize))
            .with_state(state)
            .oneshot(
                Request::get(format!("/auth/authorize?{query}"))
                    .header(header::HOST, "attacker.example.net")
                    .header("x-forwarded-host", "evil.example.org")
                    .header("x-forwarded-proto", "http")
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router answers");
        let location = response
            .headers()
            .get(header::LOCATION)
            .map(|value| Url::parse(value.to_str().expect("ascii")).expect("location parses"));
        (response.status(), location)
    }

    /// The query of a `wyrd-ui` authorization request for `tenant`, with
    /// `extra` parameters appended.
    fn request(tenant: &str, extra: &str) -> String {
        format!(
            "response_type=code&client_id=wyrd-ui\
             &redirect_uri=https%3A%2F%2Fwyrd.example.com%2Flogin%2Fcallback\
             &code_challenge={CHALLENGE}&code_challenge_method=S256&state=xyz\
             &tenant={tenant}{extra}"
        )
    }

    /// The query parameter `name` of `url`.
    fn param(url: &Url, name: &str) -> Option<String> {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    }

    /// A valid request sends the browser to the tenant's provider with the
    /// deployment's configured callback, whatever `Host` or forwarded headers
    /// it carried.
    ///
    /// # Panics
    /// Panics when the request is refused or the provider URL carries another
    /// `redirect_uri`.
    #[tokio::test]
    async fn authorize_ignores_request_headers() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let state = authorize_state(&fixture, &provider).await;

        let (status, location) = get_authorize(state, &request(fixture.tenant_slug(), "")).await;

        assert_eq!(status, StatusCode::SEE_OTHER);
        let location = location.expect("redirects");
        assert!(location.as_str().starts_with(&provider.uri()));
        assert_eq!(
            param(&location, "redirect_uri").as_deref(),
            Some("https://wyrd.example.com/auth/callback")
        );
    }

    /// An unknown tenant redirects back to the client with `access_denied`
    /// and its `state`, and a missing challenge with `invalid_request`.
    ///
    /// # Panics
    /// Panics when either is answered differently.
    #[tokio::test]
    async fn refusals_redirect_back_with_rfc_errors() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let state = authorize_state(&fixture, &provider).await;

        let (status, location) = get_authorize(state.clone(), &request("no-such-tenant", "")).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let location = location.expect("redirects");
        assert!(
            location
                .as_str()
                .starts_with("https://wyrd.example.com/login/callback?")
        );
        assert_eq!(param(&location, "error").as_deref(), Some("access_denied"));
        assert_eq!(param(&location, "state").as_deref(), Some("xyz"));

        let plain = request(fixture.tenant_slug(), "").replace("S256", "plain");
        let (_, location) = get_authorize(state, &plain).await;
        assert_eq!(
            param(&location.expect("redirects"), "error").as_deref(),
            Some("invalid_request")
        );
    }

    /// Once the client and redirect URI are each sent once and registered, a
    /// repeated non-binding parameter redirects back with `invalid_request`,
    /// echoing `state` only when it is unambiguous (RFC 6749 §4.1.2.1).
    ///
    /// # Panics
    /// Panics when a duplicate is not redirected back with `invalid_request`
    /// or `state` is echoed from an ambiguous request.
    #[tokio::test]
    async fn duplicate_parameters_redirect_back_to_the_registered_client() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let state = authorize_state(&fixture, &provider).await;
        let tenant = fixture.tenant_slug().to_owned();

        for (query, echoed) in [
            (request(&tenant, "&state=again"), None),
            (request(&tenant, "&code_challenge_method=S256"), Some("xyz")),
            (request(&tenant, &format!("&tenant={tenant}")), Some("xyz")),
        ] {
            let (status, location) = get_authorize(state.clone(), &query).await;
            assert_eq!(status, StatusCode::SEE_OTHER, "{query}");
            let location = location.expect("redirects");
            assert!(
                location
                    .as_str()
                    .starts_with("https://wyrd.example.com/login/callback?"),
                "{query}"
            );
            assert_eq!(
                param(&location, "error").as_deref(),
                Some("invalid_request"),
                "{query}"
            );
            assert_eq!(param(&location, "state").as_deref(), echoed, "{query}");
        }
    }

    /// An unregistered, missing, or repeated redirect URI or client is never
    /// redirected to.
    ///
    /// # Panics
    /// Panics when the request redirects.
    #[tokio::test]
    async fn an_unregistered_redirect_is_never_followed() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let provider = MockServer::start().await;
        let state = authorize_state(&fixture, &provider).await;
        let tenant = fixture.tenant_slug().to_owned();

        for query in [
            request(&tenant, "").replace("wyrd.example.com", "evil.example.org"),
            request(&tenant, "").replace("wyrd-ui", "wyrd-cli"),
            request(&tenant, "&client_id=wyrd-ui"),
            request(
                &tenant,
                "&redirect_uri=https%3A%2F%2Fwyrd.example.com%2Flogin%2Fcallback",
            ),
            request(&tenant, "").replace("&client_id=wyrd-ui", ""),
        ] {
            let (status, location) = get_authorize(state.clone(), &query).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
            assert!(location.is_none(), "{query}");
        }
    }
}
