//! Extension-route conformance tests for `WyrdServer::merge_http_protected`.
//!
//! Proves that routes merged via `merge_http_protected` receive the same
//! protective edge stack as core `/v1` routes: request-id propagation,
//! panic→500 mapping, body-limit enforcement, and authenticated `Principal`.
//! Also contrasts `merge_http` (unprotected) as a negative baseline.
//!
//! Every case takes its `AppState` from [`WyrdTestServer`], which composes the
//! server through `wyrd_server::boot::compose_bifrost`, so the extension API is
//! exercised against production state rather than a hand-assembled facsimile.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use chrono::Duration;
use std::sync::OnceLock;
use tower::ServiceExt;
use wyrd_auth_verify::TokenPrincipalRef;
use wyrd_runtime::PrincipalId;

use wyrd_server::config::{
    BifrostPeerConfig, BifrostTarget, ScribeRuntimeConfig, WyrdServerConfig,
};
use wyrd_server::state::LimitsConfig;
use wyrd_server::{AppState, WyrdServer};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_testing::WyrdTestServer;

/// Mint a user access token from the composed server's issuing key.
///
/// # Panics
/// Panics when the composed state has no issuing key or minting fails.
fn mint_user_jwt(state: &AppState, tenant: DataTenantId) -> String {
    let principal = TokenPrincipalRef {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKindTag::User,
        tenant_id: tenant,
        card_ref: None,
        card_ref_scope: wyrd_spec::reference::CardRefScope::default(),
    };
    state
        .auth
        .issuing_key
        .as_ref()
        .expect("composed server carries an issuing key")
        .issue_user_access_token(principal, vec![], None, Duration::minutes(5))
        .expect("test jwt mints")
}

/// A handler that always panics, used to prove the catch-panic layer converts
/// an extension-route panic into a problem+json 500 instead of dropping the
/// connection.
///
/// # Panics
/// Always panics.
async fn panicking_handler() -> &'static str {
    panic!("test panic from extension")
}

/// The extension router merged into the server under test: a plain route, a
/// panicking route, and a body echo used to drive the body-limit layer.
fn probe_router() -> Router {
    Router::new()
        .route("/probe/ok", get(|| async { "ok" }))
        .route("/probe/panic", get(panicking_handler))
        .route("/probe/echo", post(|body: String| async move { body }))
}

/// One process-lifetime peer identity shared by every case in this binary.
///
/// A default target is Scribe- and Oracle-bearing, so `WyrdServer::new` refuses
/// to compose without a complete `bifrost.peer` identity. These cases never
/// dial the peer plane — they exercise the HTTP edge stack — so one minted
/// identity, kept alive for the binary, satisfies composition without giving
/// each case its own certificate authority.
static PEER_IDENTITY: OnceLock<(tempfile::TempDir, BifrostPeerConfig)> = OnceLock::new();

/// Returns the shared peer identity, minting it on first use.
///
/// # Panics
/// Panics when peer certificate or ticket material cannot be minted.
fn peer_identity() -> &'static BifrostPeerConfig {
    &PEER_IDENTITY
        .get_or_init(|| {
            let root = tempfile::tempdir().expect("peer material tempdir");
            let config = wyrd_testing::materialize_test_peer_config(root.path(), "merge-http")
                .expect("peer identity mints");
            (root, config)
        })
        .1
}

/// Build the production `WyrdServer` around the supplied composed state.
///
/// # Panics
/// Panics when the state cannot satisfy `WyrdServer::new`.
fn protected_server(state: AppState) -> WyrdServer {
    let mut config = WyrdServerConfig::default();
    config.bifrost.peer = peer_identity().clone();
    WyrdServer::new(config, state).expect("WyrdServer builds with full auth state")
}

/// `attach_request_id` runs before `require_authenticated`, so even a refused
/// request on a merged route carries a request id an operator can correlate.
#[tokio::test]
async fn extension_route_gets_request_id() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let router = protected_server(server.state().clone())
        .merge_http_protected(probe_router())
        .into_http_router();

    let response = router
        .oneshot(
            Request::builder()
                .uri("/probe/ok")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert!(
        response.headers().contains_key("wyrd-request-id"),
        "protected extension route must carry wyrd-request-id; got headers: {:?}",
        response.headers()
    );

    server.shutdown().await.expect("server shuts down");
}

/// A panic inside an extension handler must surface as a problem+json 500
/// through `CatchPanicLayer`, never as a dropped connection.
#[tokio::test]
async fn extension_panic_maps_to_500() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let token = mint_user_jwt(server.state(), server.data_tenant_id());
    let router = protected_server(server.state().clone())
        .merge_http_protected(probe_router())
        .into_http_router();

    let response = router
        .oneshot(
            Request::builder()
                .uri("/probe/panic")
                .header("x-wyrd-access-token", format!("Bearer {token}"))
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds (CatchPanicLayer must catch the panic)");

    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "panicking handler must return 500 via CatchPanicLayer"
    );
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json"),
        "panic response must be application/problem+json WyrdError"
    );

    server.shutdown().await.expect("server shuts down");
}

/// Proves Scribe routes use their frozen ingest cap only while the role is
/// active: on a Scribe-composed server an oversized OTLP request reaches the
/// decoder, while the same request against an Oracle-only server — one with no
/// Scribe role at all — is refused by the global body limit.
#[tokio::test]
async fn pg_scribe_body_limit_is_route_and_role_local() {
    let request_bytes = 201 * 1024 * 1024;
    let scribe_config = ScribeRuntimeConfig {
        ingest_request_bytes: request_bytes,
        ..ScribeRuntimeConfig::default()
    };
    scribe_config
        .validate()
        .expect("supported request above the default validates");

    let server = WyrdTestServer::builder()
        .with_scribe_ingest_limits_for_test(scribe_config.ingest_limits())
        .with_limits_for_test(LimitsConfig {
            body_bytes: 1024,
            ..LimitsConfig::default()
        })
        .start_in_process()
        .await
        .expect("Scribe-composed server starts");

    let transport = server
        .state()
        .bifrost_resources()
        .expect("Scribe role resources")
        .transport_admission();
    assert_eq!(
        transport.message_limit_bytes(),
        request_bytes,
        "resource composition freezes the selected Scribe request maximum"
    );

    let token = mint_user_jwt(server.state(), server.data_tenant_id());
    let router = protected_server(server.state().clone())
        .merge_http_protected(probe_router())
        .into_http_router();

    // Use the Content-Length fast path: no need to stream 1025 bytes.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/probe/echo")
                .header("content-length", "2048")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(
        response.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "body exceeding the limit must return 413"
    );

    let above_default = 200 * 1024 * 1024 + 512 * 1024;
    let scribe_response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/traces")
                .header("content-length", above_default.to_string())
                .header("content-type", "application/json")
                .header("x-wyrd-access-token", format!("Bearer {token}"))
                .body(Body::from("{invalid-json"))
                .expect("Scribe request builds"),
        )
        .await
        .expect("Scribe router responds");
    assert_eq!(
        scribe_response.status(),
        StatusCode::BAD_REQUEST,
        "enabled Scribe route must reach the downstream OTLP decoder"
    );
    let scribe_body = to_bytes(scribe_response.into_body(), usize::MAX)
        .await
        .expect("Scribe decoder response collects");
    let scribe_problem: serde_json::Value =
        serde_json::from_slice(&scribe_body).expect("Scribe decoder response is JSON");
    assert_eq!(
        scribe_problem["code"], "WYRD_VALA_400_OTLP_REQUEST_MALFORMED",
        "selected transport admission must yield the exact downstream decoder contract"
    );
    assert_eq!(
        transport.used_bytes(),
        0,
        "downstream decoder completion releases the selected transport charge"
    );

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let problem: serde_json::Value = serde_json::from_slice(&body).expect("413 body must be JSON");
    assert_eq!(
        problem["code"], "WYRD_SPEC_413_PAYLOAD_TOO_LARGE",
        "error code must be WYRD_SPEC_413_PAYLOAD_TOO_LARGE; got {problem}"
    );

    let oracle_only = WyrdTestServer::builder()
        .with_forge_process_role_for_test(BifrostTarget::Oracle)
        .with_limits_for_test(LimitsConfig {
            body_bytes: 1024,
            ..LimitsConfig::default()
        })
        .start_in_process()
        .await
        .expect("Oracle-only server starts");
    let absent_response = protected_server(oracle_only.state().clone())
        .into_http_router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/traces")
                .header("content-length", above_default.to_string())
                .body(Body::empty())
                .expect("role-absent request builds"),
        )
        .await
        .expect("role-absent router responds");
    assert_eq!(
        absent_response.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "with no Scribe role the global body limit governs /v1/traces"
    );

    oracle_only
        .shutdown()
        .await
        .expect("Oracle-only server shuts down");
    server.shutdown().await.expect("server shuts down");
}

/// An extension route merged as protected inherits the default-deny layer: an
/// unauthenticated call must be refused with the stable 401 code.
#[tokio::test]
async fn extension_requires_auth() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let router = protected_server(server.state().clone())
        .merge_http_protected(probe_router())
        .into_http_router();

    let response = router
        .oneshot(
            Request::builder()
                .uri("/probe/ok")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "extension route without auth must return 401"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let problem: serde_json::Value = serde_json::from_slice(&body).expect("401 body must be JSON");
    assert_eq!(
        problem["code"], "WYRD_AUTH_401_UNAUTHENTICATED",
        "error code must be WYRD_AUTH_401_UNAUTHENTICATED; got {problem}"
    );

    server.shutdown().await.expect("server shuts down");
}

/// The negative baseline: `merge_http` is unprotected by design, so a route
/// merged through it must not acquire the edge stack's request id.
#[tokio::test]
async fn raw_merge_http_has_no_request_id() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let router = protected_server(server.state().clone())
        .merge_http(probe_router().with_state(()))
        .into_http_router();

    let response = router
        .oneshot(
            Request::builder()
                .uri("/probe/ok")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert!(
        !response.headers().contains_key("wyrd-request-id"),
        "raw merge_http must NOT add wyrd-request-id (unprotected by design); got headers: {:?}",
        response.headers()
    );

    server.shutdown().await.expect("server shuts down");
}
