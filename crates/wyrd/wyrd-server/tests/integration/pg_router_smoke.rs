//! Edge-contract coverage for the composed Wyrd HTTP router.
//!
//! Every case drives [`WyrdTestServer`], which boots the server through
//! `wyrd_server::boot::compose_bifrost` and `build_router`, so the router,
//! middleware stack, and default-deny auth layer under assertion are the
//! production ones. The suite locks the edge behaviour that no individual
//! handler owns: request-id propagation, the 401/413/503 problem+json arms,
//! `/healthz` and `/readyz` shapes, and the guarantee that an unauthenticated
//! caller cannot distinguish a real `/v1` route from a missing one.

use std::sync::Arc;

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use wyrd_server::components::health::{ProbeOutcome, ProbeReason, ReadinessSnapshot};
use wyrd_testing::WyrdTestServer;

/// Build an upload-init request body that is well formed enough to pass the
/// auth layer and reach its handler.
fn upload_init_body() -> axum::body::Body {
    axum::body::Body::from(
        serde_json::json!({
            "card_uid": "018f0000-0000-7000-8000-000000000001",
            "relative_path": "model.bin",
            "expected_sha256": "abc",
            "expected_size_bytes": 1
        })
        .to_string(),
    )
}

/// Collect a response body and parse it as the Wyrd problem+json envelope.
///
/// # Panics
/// Panics when the body cannot be collected or is not valid JSON.
async fn problem_json(response: axum::response::Response) -> serde_json::Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    serde_json::from_slice(&body).expect("problem JSON")
}

/// `/healthz` is a liveness probe: it must answer before auth and must not
/// acquire a request id, so a probe cannot inflate request-id telemetry.
#[tokio::test]
async fn healthz_returns_ok_without_request_id() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(!response.headers().contains_key("wyrd-request-id"));
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    assert_eq!(&body[..], b"ok");

    server.shutdown().await.expect("server shuts down");
}

/// An unauthenticated `/v1` call must be refused as problem+json carrying the
/// stable code and a request id an agent can quote back in a support request.
#[tokio::test]
async fn unauthenticated_v1_request_returns_problem_json() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    assert!(response.headers().contains_key("wyrd-request-id"));

    let problem = problem_json(response).await;
    assert_eq!(problem["code"], "WYRD_AUTH_401_UNAUTHENTICATED");
    assert_eq!(problem["status"], 401);

    server.shutdown().await.expect("server shuts down");
}

/// A real minted token must clear the principal extractor. The handler may
/// still fail downstream; the contract asserted here is only that the edge
/// stack does not reject an authenticated caller.
#[tokio::test]
async fn request_with_real_jwt_passes_extractor() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let user = server
        .bootstrap_user("router-smoke", &["admin"])
        .await
        .expect("user bootstraps");

    let response = server
        .oneshot_authenticated(
            user.jwt()
                .expect("bootstrapped user carries a minted token"),
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .header("content-type", "application/json")
                .body(upload_init_body())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    server.shutdown().await.expect("server shuts down");
}

/// A missing credential header is an authentication failure, not a malformed
/// request: it must map to the 401 arm rather than 400.
#[tokio::test]
async fn request_with_missing_header_returns_401_unauthenticated() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let problem = problem_json(response).await;
    assert_eq!(problem["code"], "WYRD_AUTH_401_UNAUTHENTICATED");

    server.shutdown().await.expect("server shuts down");
}

/// Guards two invariants: `/auth/token` is not behind the default-deny layer
/// (otherwise no client could ever obtain a first token), and auth routes still
/// carry `wyrd-request-id` from `attach_request_id`.
#[tokio::test]
async fn auth_routes_are_reachable_without_credential_and_carry_request_id() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .body(axum::body::Body::from("{}"))
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "/auth/token must be reachable without a credential (public route)"
    );
    assert!(
        response.headers().contains_key("wyrd-request-id"),
        "auth routes sit in the protected router and must carry wyrd-request-id"
    );

    server.shutdown().await.expect("server shuts down");
}

/// When the composed state has no token verifier, every `/v1` request must
/// answer `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` — not 401 and not 500. This
/// locks the third arm of the 400/401/503 auth-error contract and the
/// `retry_after_seconds` hint agents use for backoff.
#[tokio::test]
async fn v1_request_without_verifier_configured_returns_503() {
    let issuer = WyrdTestServer::start_in_process()
        .await
        .expect("token-issuing server starts");
    let user = issuer
        .bootstrap_user("verify-unavailable", &["admin"])
        .await
        .expect("user bootstraps");
    let token = user
        .jwt()
        .expect("bootstrapped user carries a minted token")
        .to_owned();
    issuer.shutdown().await.expect("issuer shuts down");

    let server = WyrdTestServer::builder()
        .without_token_verifier_for_test()
        .start_in_process()
        .await
        .expect("verifier-less server starts");

    let response = server
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .header("x-wyrd-access-token", format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let problem = problem_json(response).await;
    assert_eq!(problem["code"], "WYRD_AUTH_503_VERIFY_UNAVAILABLE");
    assert_eq!(problem["status"], 503);

    server.shutdown().await.expect("server shuts down");
}

/// `/healthz` must never answer as problem+json; Kubernetes treats the body as
/// opaque and a problem envelope here would signal a routing regression.
#[tokio::test]
async fn healthz_returns_ok_without_problem_json() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        !content_type.contains("problem"),
        "/healthz must not return problem+json"
    );

    server.shutdown().await.expect("server shuts down");
}

/// On cold boot the readiness snapshot is all-warmup, so `/readyz` must refuse
/// with 503 problem+json rather than reporting a server that cannot yet serve.
#[tokio::test]
async fn readyz_returns_json_even_on_cold_boot() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    let problem = problem_json(response).await;
    assert_eq!(problem["code"], "WYRD_SERVER_503_NOT_READY");
    assert_eq!(problem["status"], 503);

    server.shutdown().await.expect("server shuts down");
}

/// Publish an all-ok snapshot into the composed state's live readiness cell and
/// assert `/readyz` flips to 200. Guards the success path so a wrong status code
/// or serialization regression cannot silently block Kubernetes readiness.
#[tokio::test]
async fn readyz_returns_ok_when_all_probes_pass() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let ok_probe = ProbeOutcome {
        ok: true,
        reason: ProbeReason::Ok,
        elapsed_ms: 1,
    };
    server.state().readiness.store(Arc::new(ReadinessSnapshot {
        postgres: ok_probe.clone(),
        storage: ok_probe.clone(),
        scribe: ok_probe.clone(),
        oracle: ok_probe.clone(),
        peer: ok_probe,
        forge_coordinator: None,
        forge_worker: None,
        verification: None,
        scribe_fault_is_role_local: false,
    }));

    let response = server
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("JSON");
    assert_eq!(json["status"], "ok");

    server.shutdown().await.expect("server shuts down");
}

/// A body past the configured ceiling must be refused as 413 problem+json by
/// the edge layer, before any handler or credential check consumes it.
#[tokio::test]
async fn oversized_body_returns_413_problem_json() {
    let server = WyrdTestServer::builder()
        .with_limits_for_test(wyrd_server::state::LimitsConfig {
            body_bytes: 10,
            audio_upload_bytes: 26_214_400,
            timeout: std::time::Duration::from_secs(30),
            concurrency: 1024,
        })
        .start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .header("content-type", "application/json")
                .header("x-wyrd-access-token", "Bearer invalid.token.here")
                .body(axum::body::Body::from("a".repeat(100)))
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    let problem = problem_json(response).await;
    assert_eq!(problem["code"], "WYRD_SPEC_413_PAYLOAD_TOO_LARGE");

    server.shutdown().await.expect("server shuts down");
}

/// The default-deny layer wraps the `/v1` fallback, so an unknown `/v1` path
/// with no token is rejected with 401 before `v1_not_found` runs — not 404.
/// This is the "no route-existence oracle" guarantee: an unauthenticated caller
/// cannot distinguish a real route from a missing one.
#[tokio::test]
async fn unknown_v1_path_without_token_returns_401_via_layer() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/this/route/does/not/exist")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    server.shutdown().await.expect("server shuts down");
}

/// Sweep one real route from each mounted group (storage, authz, admin). With
/// no token every one must be rejected by the layer with 401, proving auth is a
/// property of the whole `/v1` nest rather than of any single handler.
#[tokio::test]
async fn representative_v1_routes_without_token_all_return_401() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let routes = [
        ("POST", "/v1/cards/upload/init"),
        ("POST", "/v1/cards/download/init"),
        ("POST", "/v1/authz/check"),
        (
            "POST",
            "/v1/principals/018f0000-0000-7000-8000-000000000001/revoke",
        ),
        ("GET", "/v1/admin/trusted-issuers"),
        ("GET", "/v1/admin/workload-bindings"),
        ("GET", "/v1/admin/gateway/provider-credentials"),
        ("GET", "/v1/admin/gateway/provider-credentials/primary"),
        ("GET", "/v1/admin/gateway/provider-deployments"),
        ("GET", "/v1/admin/gateway/provider-deployments/primary"),
        ("GET", "/v1/admin/gateway/fallback-policy"),
        ("GET", "/v1/admin/gateway/governance-policy"),
        ("GET", "/v1/admin/gateway/capture-policy"),
    ];

    for (method, path) in routes {
        let response = server
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(axum::body::Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} must be rejected with 401 by the default-deny layer"
        );
    }

    server.shutdown().await.expect("server shuts down");
}

/// A minted valid token on a real `/v1` route must pass the default-deny layer.
/// The handler may still fail downstream; the layer itself must not answer 401.
#[tokio::test]
async fn valid_token_is_not_rejected_by_default_deny_layer() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let user = server
        .bootstrap_user("default-deny", &["admin"])
        .await
        .expect("user bootstraps");

    let response = server
        .oneshot_authenticated(
            user.jwt()
                .expect("bootstrapped user carries a minted token"),
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .header("content-type", "application/json")
                .body(upload_init_body())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    server.shutdown().await.expect("server shuts down");
}

/// A composed, API-serving target validates on the production profile only
/// while it carries a token verifier.
///
/// `AppState::production_validate` short-circuits for a target that serves no
/// public API, so the guard can only be observed against a state composed
/// through `wyrd_server::boot::compose_bifrost` — which is exactly what the
/// harness builds.
///
/// # Panics
///
/// Panics when the test server fails to start or shut down, when the composed
/// production state is refused, or when removing its token verifier is not
/// refused as missing.
#[tokio::test]
async fn production_profile_requires_a_token_verifier_on_a_serving_target() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let state = server
        .state()
        .clone()
        .with_deployment_profile(wyrd_server::config::DeploymentProfile::Production);
    state
        .production_validate()
        .expect("a composed serving target with a verifier is production-valid");

    let mut without_verifier = state;
    without_verifier.auth.token_verifier = None;
    assert!(
        matches!(
            without_verifier.production_validate(),
            Err(wyrd_server::state::ProductionValidationError::MissingTokenVerifier)
        ),
        "a production target without a token verifier must be refused"
    );

    server.shutdown().await.expect("server shuts down");
}

/// The removed authorization-check route answers as any unknown route does.
///
/// # Panics
///
/// Panics when the test server fails to start or shut down, the user cannot be
/// bootstrapped, or an authenticated request to the route is answered with
/// anything other than `404`.
#[tokio::test]
async fn authz_check_route_is_not_served() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let user = server
        .bootstrap_user("authz-route-absent", &["admin"])
        .await
        .expect("user bootstraps");

    let response = server
        .oneshot_authenticated(
            user.jwt()
                .expect("bootstrapped user carries a minted token"),
            Request::builder()
                .method("POST")
                .uri("/v1/authz/check")
                .header("content-type", "application/json")
                .body(axum::body::Body::from("{}"))
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    server.shutdown().await.expect("server shuts down");
}

/// A production-valid serving target performs standard RFC 8693 delegation
/// with no preview setting anywhere in its configuration.
///
/// The harness composes the same signing key, concrete verifier, and
/// `TenantTokenIssuer` a production boot does; the test proves that exact
/// state passes `production_validate` under the production profile, and then
/// exchanges a subject and actor token through the real `/auth/token` route.
/// Nothing gates the grant beyond its verifier, permissions, and audit.
///
/// # Panics
///
/// Panics when the production-profile state is refused, a fixture cannot be
/// bootstrapped, or the exchange does not issue a token.
#[tokio::test]
async fn production_valid_target_serves_token_exchange_without_preview() {
    let server = WyrdTestServer::builder()
        .start_in_process()
        .await
        .expect("test server starts");
    server
        .state()
        .clone()
        .with_deployment_profile(wyrd_server::config::DeploymentProfile::Production)
        .production_validate()
        .expect("real verifier, audit, and signing key are production-valid");

    let subject = server
        .bootstrap_service("production-subject", &["editor"])
        .await
        .expect("subject bootstraps");
    let actor = server
        .bootstrap_service("production-actor", &["editor"])
        .await
        .expect("actor bootstraps");
    let subject_jwt = server
        .exchange_api_key(subject.api_key().expect("subject carries a key"))
        .await
        .expect("subject key exchanges");
    let actor_jwt = server
        .exchange_api_key(actor.api_key().expect("actor carries a key"))
        .await
        .expect("actor key exchanges");

    server
        .delegate(
            &subject_jwt,
            &actor_jwt,
            wyrd_spec::auth::TokenAudience::Wyrd,
        )
        .await
        .expect("a production-valid target serves standard token exchange");

    server.shutdown().await.expect("server shuts down");
}

/// Plan payload whose parameters no strategy contract accepts.
///
/// The worker's pre-effect gate terminalizes a claim carrying it before any
/// table fence, catalog load, or object IO, which is exactly what the durable
/// non-success rows assert.
#[cfg(feature = "test-support")]
const MALFORMED_PLAN: &str = r#"{"version":1,"inputs":["a.parquet"],"parameters":{}}"#;

/// Plan payload matching the small-files strategy contract exactly.
///
/// `validate_payload` requires the parameter object to be exactly the
/// live-rewrite kind, so this is the payload a case must seed when it needs the
/// claim to reach the fenced execution path rather than the pre-effect refusal.
#[cfg(feature = "test-support")]
const LIVE_REWRITE_PLAN: &str =
    r#"{"version":1,"inputs":["a.parquet"],"parameters":{"kind":"live_rewrite"}}"#;

/// Diagnostic ceiling for one Forge role transition.
///
/// Every transition this matrix drives is caused by an explicit scheduler pass,
/// worker run, or durable seed, so the wait is a deadlock detector rather than a
/// schedule. It is deliberately far shorter than the production scheduler
/// interval: a wait that could absorb one natural wake would let a missing
/// trigger pass as a slow success.
#[cfg(feature = "test-support")]
const FORGE_READINESS_CEILING: std::time::Duration = std::time::Duration::from_secs(15);

/// Diagnostic ceiling for the readiness loop's first computed snapshot.
#[cfg(feature = "test-support")]
const SNAPSHOT_PUBLICATION_CEILING: std::time::Duration = std::time::Duration::from_secs(5);

/// Drives exactly one production Forge scheduler pass and returns when it ends.
///
/// The scheduler is passive between wakes, so every coordinator transition in
/// this matrix is caused here rather than waited for. Completion is read from
/// the production pass counter, which advances whether the pass planned,
/// stood by, overflowed, or rolled back, so the caller may assert the resulting
/// readiness bit immediately instead of polling it.
///
/// # Panics
///
/// Panics when the requested pass does not complete within
/// [`FORGE_READINESS_CEILING`], which means the scheduler loop is not running or
/// is wedged rather than merely unready.
#[cfg(feature = "test-support")]
async fn drive_scheduler_pass(server: &WyrdTestServer, label: &str) {
    drive_forge_pass(
        server,
        label,
        WyrdTestServer::request_forge_scheduler_pass_for_test,
    )
    .await;
}

/// Requests one leader maintenance pass and waits until the timer loop returns.
///
/// Maintenance runs manifest rewrite, expiry, and cleanup apart from the
/// heartbeat, so it publishes no readiness of its own; the caller drives a
/// heartbeat pass afterwards to observe the coordinator bit.
///
/// # Panics
///
/// Panics when the requested pass does not complete within
/// [`FORGE_READINESS_CEILING`].
#[cfg(feature = "test-support")]
async fn drive_maintenance_pass(server: &WyrdTestServer, label: &str) {
    drive_forge_pass(
        server,
        label,
        WyrdTestServer::request_forge_maintenance_pass_for_test,
    )
    .await;
}

/// Issues one test-trigger request and waits for the pass counter to advance.
///
/// Both the heartbeat and maintenance loops count a returned pass on the same
/// trigger, so one bounded wait serves either request.
///
/// # Panics
///
/// Panics when the requested pass does not complete within
/// [`FORGE_READINESS_CEILING`], which means the loop is not running or is
/// wedged rather than merely unready.
#[cfg(feature = "test-support")]
async fn drive_forge_pass(server: &WyrdTestServer, label: &str, request: fn(&WyrdTestServer)) {
    let before = server.completed_forge_scheduler_passes_for_test();
    request(server);
    tokio::time::timeout(
        FORGE_READINESS_CEILING,
        server.wait_for_forge_scheduler_passes_for_test(before + 1),
    )
    .await
    .unwrap_or_else(|_| {
        let ready = server.state().forge().expect("Forge role").coordinator_readiness().is_ready();
        let completed = server.completed_forge_scheduler_passes_for_test();
        server.state().shutdown_token.cancel();
        panic!("{label}: scheduler timed out; ready={ready}, completed passes={completed}, requested={}", before + 1)
    });
}

/// Waits for the maintenance pass the coordinator runs as soon as it starts.
///
/// The leader maintenance timer's first tick is immediate, so a test that
/// drives its own passes must observe the boot pass first; otherwise the next
/// pass it drives is the boot pass still in flight, and its assertions describe
/// a pass that ran before the test finished arranging the world.
///
/// # Panics
///
/// Panics when the boot pass does not complete within the readiness budget.
#[cfg(feature = "test-support")]
async fn await_boot_scheduler_pass(server: &WyrdTestServer, label: &str) {
    tokio::time::timeout(
        FORGE_READINESS_CEILING,
        server.wait_for_forge_scheduler_passes_for_test(1),
    )
    .await
    .unwrap_or_else(|_| {
        server.state().shutdown_token.cancel();
        panic!("{label}: the coordinator's boot planning pass never completed");
    });
}

/// Polls one Forge role bit until it reaches `expected`.
///
/// The composed in-process server does not run the background readiness loop,
/// so the contract is observed at the same shared bit `/readyz` reads.
///
/// # Panics
///
/// Panics when the bit never reaches `expected` within the budget.
#[cfg(feature = "test-support")]
async fn await_forge_role(
    server: &WyrdTestServer,
    expected: bool,
    select: fn(&wyrd_server::state::Forge) -> vala_bifrost_redux::forge::ForgeRoleReadiness,
    label: &str,
) {
    let readiness = select(server.state().forge().expect("Forge role"));
    if tokio::time::timeout(FORGE_READINESS_CEILING, async {
        while readiness.is_ready() != expected {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .is_err()
    {
        let ready = readiness.is_ready();
        server.state().shutdown_token.cancel();
        panic!(
            "{label}: expected ready={expected}, last ready={ready}, completed passes={}",
            server.completed_forge_scheduler_passes_for_test()
        );
    }
}

/// Publishes one readiness snapshot for a composed server and returns it.
///
/// The production `readiness_loop` is driven for exactly as long as it takes to
/// store its first computed snapshot, so the assertion reads the same probe
/// output `/readyz` serialises rather than a hand-built fixture.
///
/// # Panics
///
/// Panics when no snapshot is published within the budget.
#[cfg(feature = "test-support")]
async fn published_snapshot(server: &WyrdTestServer) -> Arc<ReadinessSnapshot> {
    let stop = server.state().shutdown_token.child_token();
    let mut loop_handle = tokio::spawn(wyrd_server::components::health::readiness_loop(
        server.state().clone(),
        std::time::Duration::from_millis(50),
        std::time::Duration::from_secs(5),
        stop.clone(),
    ));
    let snapshot = tokio::time::timeout(SNAPSHOT_PUBLICATION_CEILING, async {
        loop {
            let snapshot = server.state().readiness.load_full();
            if snapshot.postgres.reason != ProbeReason::Warmup {
                break snapshot;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        stop.cancel();
        loop_handle.abort();
        panic!(
            "readiness publication timed out; postgres={:?}, completed passes={}",
            server.state().readiness.load().postgres,
            server.completed_forge_scheduler_passes_for_test()
        );
    });
    stop.cancel();
    if tokio::time::timeout(SNAPSHOT_PUBLICATION_CEILING, &mut loop_handle)
        .await
        .is_err()
    {
        loop_handle.abort();
        panic!(
            "readiness loop did not stop; postgres={:?}",
            snapshot.postgres
        );
    }
    snapshot
}

/// Every production target projection publishes exactly the Forge checks it runs.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forge_checks_follow_target_selection() {
    for (target, coordinator, worker) in [
        (wyrd_server::config::BifrostTarget::All, true, true),
        (wyrd_server::config::BifrostTarget::Server, true, false),
        (wyrd_server::config::BifrostTarget::Oracle, false, false),
        (wyrd_server::config::BifrostTarget::Scribe, false, false),
        (wyrd_server::config::BifrostTarget::ForgeWorker, false, true),
    ] {
        let server = WyrdTestServer::builder()
            .with_bifrost_target_for_test(target)
            .start_in_process()
            .await
            .expect("targeted server starts");
        let snapshot = published_snapshot(&server).await;
        assert_eq!(
            snapshot.forge_coordinator.is_some(),
            coordinator,
            "{target:?} published the wrong Forge coordinator check"
        );
        assert_eq!(
            snapshot.forge_worker.is_some(),
            worker,
            "{target:?} published the wrong Forge worker check"
        );
        server.shutdown().await.expect("targeted server shuts down");
    }
}

/// Process shutdown closes both Forge role bits before it cancels their token.
///
/// Readiness answers whether this replica can be routed work right now. If the
/// token were cancelled first, both loops would keep advertising themselves for
/// as long as it took them to notice, and the gateway would keep sending
/// maintenance to a replica that is already tearing down. Closing is terminal,
/// so a loop that publishes on its way out cannot reopen routing either.
///
/// # Panics
///
/// Panics when either role fails to reach ready, when a bit survives
/// `begin_shutdown`, when the Forge token is not cancelled, or when joining a
/// loop republishes readiness.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forge_begin_shutdown_closes_readiness_before_cancellation() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let forge = server
        .state()
        .forge()
        .expect("the default target selects Forge");
    let coordinator_ready = forge.coordinator_readiness();
    let worker_ready = forge.worker_readiness();

    let stop = server.state().shutdown_token.child_token();
    let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone())
        .expect("the worker composes");
    let worker_handle = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(worker));
    let scheduler = wyrd_server::boot::spawn_maintenance_scheduler(server.state(), stop.clone())
        .expect("the scheduler composes")
        .expect("the default target selects a coordinator");
    let scheduler_handle = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(scheduler));

    await_forge_role(
        &server,
        true,
        wyrd_server::state::Forge::worker_readiness,
        "worker before shutdown",
    )
    .await;
    drive_scheduler_pass(&server, "coordinator pass before shutdown").await;
    assert!(
        coordinator_ready.is_ready() && worker_ready.is_ready(),
        "both selected roles are ready before shutdown begins"
    );

    // No await between the call and the assertions: the contract is that
    // routing is already closed the moment `begin_shutdown` returns, not that
    // it closes soon afterwards.
    server.state().bifrost.begin_shutdown();
    assert!(
        !coordinator_ready.is_ready(),
        "coordinator routing closes synchronously with shutdown"
    );
    assert!(
        !worker_ready.is_ready(),
        "worker routing closes synchronously with shutdown"
    );
    assert!(
        forge.shutdown_token().is_cancelled(),
        "the Forge token is cancelled only after routing is already closed"
    );

    stop.cancel();
    join_forge_loop(&server, worker_handle)
        .await
        .expect("worker loop ok");
    join_forge_loop(&server, scheduler_handle)
        .await
        .expect("pass loop ok");
    assert!(
        !coordinator_ready.is_ready() && !worker_ready.is_ready(),
        "a closed role never republishes readiness while its loop drains"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// A selected worker publishes ready only after its recovery drain, and clears
/// the bit on shutdown while liveness keeps answering.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forge_worker_readiness_gates_on_recovery() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let readiness = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .worker_readiness();
    assert!(
        !readiness.is_ready(),
        "a worker that has not drained recoverable work is not ready"
    );

    let stop = server.state().shutdown_token.child_token();
    let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone())
        .expect("the worker composes");
    let handle = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(worker));

    await_forge_role(
        &server,
        true,
        wyrd_server::state::Forge::worker_readiness,
        "worker",
    )
    .await;

    stop.cancel();
    join_forge_loop(&server, handle)
        .await
        .expect("worker loop ok");
    assert!(
        !readiness.is_ready(),
        "a stopped worker does not advertise ready"
    );

    let liveness = server
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("liveness answers");
    assert_eq!(
        liveness.status(),
        StatusCode::OK,
        "a draining Forge worker is still live"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// The coordinator publishes ready only once a full fenced pass completes, and
/// its guard removes the bit when the supervised loop stops.
///
/// # Panics
///
/// Panics if the server or scheduler fails to start or stop, or if readiness
/// is reported before the first completed pass or after the loop stops.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forge_coordinator_readiness_follows_a_completed_pass() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let forge = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .coordinator_readiness();
    assert!(
        !forge.is_ready(),
        "a coordinator that has not run a pass is not ready"
    );

    let stop = server.state().shutdown_token.child_token();
    let scheduler = wyrd_server::boot::spawn_maintenance_scheduler(server.state(), stop.clone())
        .expect("the scheduler composes")
        .expect("the default target selects a coordinator");
    let handle = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(scheduler));

    // The boot maintenance pass counts on the same trigger, so settle it
    // first; the driven pass is then the first heartbeat pass.
    await_boot_scheduler_pass(&server, "boot maintenance pass").await;
    drive_scheduler_pass(&server, "first coordinator pass").await;
    assert!(
        forge.is_ready(),
        "a coordinator that completed one fenced pass is ready"
    );

    stop.cancel();
    join_forge_loop(&server, handle)
        .await
        .expect("pass loop ok");
    assert!(
        !forge.is_ready(),
        "a stopped coordinator does not advertise ready"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// A standby pass leaves the coordinator ready, and so does the complete fenced
/// pass that reclaims the fence once the peer's lease lapses.
///
/// Planning is a singleton fenced by a lease, so a second replica that finds a
/// live peer holding it is a healthy standby, not a failed one: it reached the
/// coordination store, its workers keep executing the durable queue, and it
/// takes over when the lease lapses. Scaling out with more `all` replicas
/// depends on every one of them reporting ready.
///
/// # Panics
///
/// Panics when a standby or completed pass publishes the wrong bit.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coordinator_standby_pass_is_ready() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let readiness = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .coordinator_readiness();
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool");

    // A live peer holds the fence, so every pass this replica runs is standby.
    sqlx::query(
        "UPDATE vala.forge_scheduler_state SET owner=$1,fencing_token=fencing_token+1,\
         expires_at=statement_timestamp()+interval '10 minutes',\
         updated_at=statement_timestamp() WHERE singleton",
    )
    .bind(uuid::Uuid::now_v7())
    .execute(&pool)
    .await
    .expect("a peer takes the scheduler fence");

    let stop = server.state().shutdown_token.child_token();
    let scheduler = wyrd_server::boot::spawn_maintenance_scheduler(server.state(), stop.clone())
        .expect("the scheduler composes")
        .expect("the default target selects a coordinator");
    let handle = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(scheduler));
    // Observe the immediate boot pass first, so each driven pass below is the
    // one it requested rather than a pass already in flight.
    await_boot_scheduler_pass(&server, "standby boot pass").await;

    drive_scheduler_pass(&server, "standby pass").await;
    assert!(
        readiness.is_ready(),
        "a standby replica behind a live peer's fence is a ready coordinator"
    );

    // Releasing the fence lets the very next pass complete under this replica.
    sqlx::query(
        "UPDATE vala.forge_scheduler_state SET expires_at=statement_timestamp()-interval '1 second' \
         WHERE singleton",
    )
    .execute(&pool)
    .await
    .expect("expire the peer fence");
    drive_scheduler_pass(&server, "pass after the fence is released").await;
    assert!(
        readiness.is_ready(),
        "the first pass to reclaim the fence completed and is ready"
    );

    stop.cancel();
    join_forge_loop(&server, handle)
        .await
        .expect("pass loop ok");
    server.shutdown().await.expect("test server shuts down");
}

/// Composes one server whose Forge worker observes an injectable production
/// observer, so a startup or recovery boundary can be failed exactly once.
///
/// # Panics
///
/// Panics when the server cannot start.
#[cfg(feature = "test-support")]
async fn server_with_forge_observer() -> (
    WyrdTestServer,
    vala_bifrost_redux::forge::ForgeWorkerCompletionObserver,
) {
    let observer = vala_bifrost_redux::forge::ForgeWorkerCompletionObserver::new();
    let server = WyrdTestServer::builder()
        .with_forge_completion_observer_for_test(observer.clone())
        .start_in_process()
        .await
        .expect("test server starts");
    (server, observer)
}

/// Runs one production Forge worker to completion and reports its result.
///
/// # Panics
///
/// Panics when the worker cannot be composed or its task panics.
#[cfg(feature = "test-support")]
async fn run_forge_worker_once(
    server: &WyrdTestServer,
) -> Result<(), vala_bifrost_redux::forge::ForgeError> {
    let stop = server.state().shutdown_token.child_token();
    let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone())
        .expect("the worker composes");
    let mut handle = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(worker));
    let outcome = tokio::time::timeout(FORGE_READINESS_CEILING, &mut handle).await;
    if outcome.is_err() {
        stop.cancel();
        handle.abort();
        panic!(
            "worker did not return; ready={}",
            server
                .state()
                .forge()
                .expect("Forge role")
                .worker_readiness()
                .is_ready()
        );
    }
    let outcome = outcome.expect("bounded worker").expect("worker joins");
    stop.cancel();
    outcome
}

/// A worker whose durable registration fails returns that error and never
/// publishes ready; the one-shot is consumed, so a respawn recovers.
///
/// # Panics
///
/// Panics when the failed worker publishes ready or the respawned one does not.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_registration_failure_never_publishes_ready() {
    let (server, observer) = Box::pin(server_with_forge_observer()).await;
    let readiness = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .worker_readiness();
    observer.fail_next_registration();
    let error = run_forge_worker_once(&server)
        .await
        .expect_err("the injected registration failure is returned");
    assert!(
        format!("{error}").contains("injected Forge worker registration failure"),
        "the worker returned an unrelated error: {error}"
    );
    assert!(
        !readiness.is_ready(),
        "a worker that never registered advertised ready"
    );
    await_forge_role_while_running(&server, "worker after registration recovers").await;
    server.shutdown().await.expect("test server shuts down");
}

/// A worker whose recovery predicate fails returns that error before the
/// deterministic next-claim barrier and never publishes ready.
///
/// # Panics
///
/// Panics when the failed worker publishes ready or the respawned one does not.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_recovery_failure_never_publishes_ready() {
    let (server, observer) = Box::pin(server_with_forge_observer()).await;
    let readiness = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .worker_readiness();
    observer.fail_next_recovery_predicate();
    run_forge_worker_once(&server)
        .await
        .expect_err("the injected recovery failure is returned");
    assert!(
        !readiness.is_ready(),
        "a worker that has not drained recoverable work advertised ready"
    );
    await_forge_role_while_running(&server, "worker after recovery drains").await;
    server.shutdown().await.expect("test server shuts down");
}

/// Spawns one production worker and waits for it to publish ready, then stops it.
///
/// # Panics
///
/// Panics when readiness is never published or the worker loop returns an error.
#[cfg(feature = "test-support")]
async fn await_forge_role_while_running(server: &WyrdTestServer, label: &str) {
    let stop = server.state().shutdown_token.child_token();
    let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone())
        .expect("the worker composes");
    let handle = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(worker));
    await_forge_role(
        server,
        true,
        wyrd_server::state::Forge::worker_readiness,
        label,
    )
    .await;
    stop.cancel();
    join_forge_loop(server, handle)
        .await
        .expect("worker loop ok");
}

/// Registers one real Bifrost table through the retained production catalog.
///
/// Forge loads a table's snapshot before it reaches any durable write, so a
/// fault this matrix injects has to be raised against a table that really
/// exists rather than a name the catalog does not hold.
///
/// # Panics
///
/// Panics when the catalog refuses the registration.
#[cfg(feature = "test-support")]
async fn register_forge_table(server: &WyrdTestServer, table: &str) {
    server
        .create_bifrost_table_for_test(vala_bifrost_redux::catalog::CreateTableRequest {
            table: vala_bifrost_redux::catalog::TableRef::new(
                vala_bifrost_redux::namespaces::BifrostNamespace::Bifrost,
                table,
            ),
            user_fields: vec![arrow::datatypes::Field::new(
                "value",
                arrow::datatypes::DataType::Int64,
                false,
            )],
            tenant: server.data_tenant_id(),
            physical_layout: None,
        })
        .await
        .expect("the production catalog registers the table");
}

/// Spawns the production maintenance scheduler for one composed server.
///
/// # Panics
///
/// Panics when the scheduler cannot be composed for this target.
#[cfg(feature = "test-support")]
fn spawn_coordinator(
    server: &WyrdTestServer,
    stop: &tokio_util::sync::CancellationToken,
) -> tokio_util::task::AbortOnDropHandle<Result<(), vala_bifrost_redux::forge::ForgeError>> {
    let scheduler = wyrd_server::boot::spawn_maintenance_scheduler(server.state(), stop.clone())
        .expect("the scheduler composes")
        .expect("the default target selects a coordinator");
    tokio_util::task::AbortOnDropHandle::new(tokio::spawn(scheduler))
}

/// Joins a spawned Forge loop within the same readiness diagnostic budget.
///
/// Timeout cancels the server's child work and aborts the retained loop before
/// panicking, so no scheduler or worker is detached from a failed assertion.
///
/// # Errors
/// Returns the production loop's original Forge failure.
/// # Panics
/// Panics on a task panic or timeout with the last role readiness and pass count.
#[cfg(feature = "test-support")]
async fn join_forge_loop(
    server: &WyrdTestServer,
    mut handle: tokio_util::task::AbortOnDropHandle<
        Result<(), vala_bifrost_redux::forge::ForgeError>,
    >,
) -> Result<(), vala_bifrost_redux::forge::ForgeError> {
    let outcome = tokio::time::timeout(FORGE_READINESS_CEILING, &mut handle).await;
    if outcome.is_err() {
        server.state().shutdown_token.cancel();
        handle.abort();
        let forge = server.state().forge().expect("Forge role");
        panic!(
            "Forge join timed out; worker ready={}, coordinator ready={}, completed passes={}",
            forge.worker_readiness().is_ready(),
            forge.coordinator_readiness().is_ready(),
            server.completed_forge_scheduler_passes_for_test()
        );
    }
    outcome
        .expect("bounded Forge join")
        .expect("Forge task joins")
}

/// Publication evidence naming a snapshot the table never retained.
///
/// Reconciliation validates prepared evidence against the live table before it
/// resumes any effect, so an unretained snapshot id is the smallest fixture
/// that reaches that refusal with otherwise well-formed evidence.
#[cfg(feature = "test-support")]
const UNRETAINED_EVIDENCE: &str = r#"{"version":1,"committed_snapshot_id":999999,"committed_metadata_location":"metadata/v9.json","committed_metadata_digest":"digest","cleanup_candidates":[],"deleted_candidate_count":0}"#;

/// Moves one seeded task into a lapsed Prepared attempt awaiting reconciliation.
///
/// A Prepared row with an expired claim is exactly the residue a crashed owner
/// leaves behind, and it is what `claim_prepared_for_reconciliation` takes over.
/// `evidence` is the durable publication evidence the successor must validate.
///
/// # Panics
///
/// Panics when the update does not reach the seeded row.
#[cfg(feature = "test-support")]
async fn prepare_lapsed_attempt(pool: &sqlx::PgPool, task_id: uuid::Uuid, evidence: &str) {
    sqlx::query(
        "UPDATE vala.forge_tasks SET state='prepared',attempt_id=gen_random_uuid(),\
         claimed_by=gen_random_uuid(),claim_expires_at=statement_timestamp()-interval '1 hour',\
         watermark_snapshot_id=4242,watermark_timestamp_ms=0,\
         evidence=$2::jsonb,updated_at=statement_timestamp() WHERE task_id=$1",
    )
    .bind(task_id)
    .bind(evidence)
    .execute(pool)
    .await
    .expect("the seeded task becomes a lapsed Prepared attempt");
}

/// Boots a worker over one lapsed Prepared attempt whose evidence cannot validate.
///
/// Returns the worker's error and its readiness handle so each caller asserts
/// the precedence it owns.
///
/// # Panics
///
/// Panics when the worker does not return within the readiness ceiling, or
/// exits clean over unreconciled durable evidence.
#[cfg(feature = "test-support")]
async fn run_worker_over_invalid_prepared_evidence(
    server: &WyrdTestServer,
    observer: &vala_bifrost_redux::forge::ForgeWorkerCompletionObserver,
    table: &str,
    arm_release_failure: bool,
) -> (String, uuid::Uuid, sqlx::PgPool) {
    register_forge_table(server, table).await;
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool");
    let task_id = seed_ready_forge_task(
        &pool,
        uuid::Uuid::from(server.data_tenant_id()),
        table,
        "small_files",
        LIVE_REWRITE_PLAN,
    )
    .await;
    prepare_lapsed_attempt(&pool, task_id, UNRETAINED_EVIDENCE).await;
    if arm_release_failure {
        observer.fail_next_lease_release();
    }
    let stop = server.state().shutdown_token.child_token();
    let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone())
        .expect("the worker composes");
    let error = tokio::time::timeout(FORGE_READINESS_CEILING, worker)
        .await
        .unwrap_or_else(|_| {
            stop.cancel();
            panic!(
                "prepared recovery did not return; task={task_id}, ready={}, attempts={}",
                server
                    .state()
                    .forge()
                    .expect("Forge role")
                    .worker_readiness()
                    .is_ready(),
                observer.attempts()
            );
        })
        .expect_err("a worker does not exit clean over unreconciled prepared evidence");
    stop.cancel();
    (format!("{error}"), task_id, pool)
}

/// Recovery refuses a Prepared attempt whose evidence no longer validates, and
/// never advertises readiness over it.
///
/// # Panics
///
/// Panics when the refusal is not the validation refusal, readiness is
/// published, or the attempt is settled instead of retained.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn prepared_evidence_validation_failure_never_publishes_ready() {
    let (server, observer) = Box::pin(server_with_forge_observer()).await;
    let readiness = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .worker_readiness();
    let (error, task_id, pool) =
        run_worker_over_invalid_prepared_evidence(&server, &observer, "prepared_validation", false)
            .await;
    assert!(
        error.contains("Prepared evidence snapshot is no longer retained"),
        "recovery returned an unrelated error: {error}"
    );
    assert!(
        !readiness.is_ready(),
        "a worker advertised ready over unreconciled prepared evidence"
    );
    let state: String = sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
        .bind(task_id)
        .fetch_one(&pool)
        .await
        .expect("the retained attempt is readable");
    assert_eq!(
        state, "prepared",
        "invalid evidence was settled instead of retained for a later owner"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// A reconciliation failure stays the reported error when its table-lease
/// release fails too.
///
/// Both failures are fatal, but they are not equally diagnostic: the release
/// failure is downstream of a reconciliation that already refused to proceed,
/// so surfacing the release error would name the wrong cause.
///
/// # Panics
///
/// Panics when the release failure displaces the reconciliation error or
/// readiness survives the pair.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reconciliation_failure_outranks_its_release_failure() {
    let (server, observer) = Box::pin(server_with_forge_observer()).await;
    let readiness = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .worker_readiness();
    let (error, _, _) =
        run_worker_over_invalid_prepared_evidence(&server, &observer, "prepared_release", true)
            .await;
    assert!(
        error.contains("Prepared evidence snapshot is no longer retained"),
        "the release failure displaced the reconciliation error: {error}"
    );
    assert!(
        !error.contains("injected Forge lease release failure"),
        "the secondary release failure was reported as the cause: {error}"
    );
    assert!(
        !observer.lease_release_failure_armed(),
        "the release site was never reached, so nothing outranked anything"
    );
    assert!(
        !readiness.is_ready(),
        "a worker advertised ready after a fatal reconciliation and release pair"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// A worker whose cancelled-claim release fails reports that failure, clears
/// readiness, and retains the claim for durable recovery.
///
/// A clean shutdown drains a just-claimed task back to a claimable state so the
/// pod's active claims reach zero. When that drain fails the claim is still held
/// in durable state by an owner that is no longer running it, so the worker must
/// not report that it drained: it returns the error, clears readiness, and
/// leaves the row for lease-expiry recovery rather than releasing it optimistically.
///
/// # Panics
///
/// Panics when the release failure is swallowed, readiness survives it, or the
/// claim is released despite the failure.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancellation_release_failure_closes_readiness() {
    let (server, observer) = Box::pin(server_with_forge_observer()).await;
    let readiness = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .worker_readiness();
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool");
    let task_id = seed_ready_forge_task(
        &pool,
        uuid::Uuid::from(server.data_tenant_id()),
        "cancelled_release",
        "small_files",
        LIVE_REWRITE_PLAN,
    )
    .await;
    // The barrier holds the worker after its durable claim, so cancellation
    // lands in the pre-execution window this release owns rather than racing it.
    observer.hold_after_claims_for_test(1);
    observer.fail_next_cancelled_claim_release();

    let stop = server.state().shutdown_token.child_token();
    let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone())
        .expect("the worker composes");
    let handle = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(worker));
    if tokio::time::timeout(FORGE_READINESS_CEILING, observer.wait_for_claims_for_test())
        .await
        .is_err()
    {
        stop.cancel();
        handle.abort();
        panic!(
            "claim barrier timed out; ready={}, attempts={}",
            readiness.is_ready(),
            observer.attempts()
        );
    }
    stop.cancel();
    observer.release_claims_for_test();

    let error = tokio::time::timeout(FORGE_READINESS_CEILING, handle)
        .await
        .unwrap_or_else(|_| {
            stop.cancel();
            panic!(
                "cancelled-claim release did not return; ready={}, attempts={}",
                readiness.is_ready(),
                observer.attempts()
            );
        })
        .expect("worker joins")
        .expect_err("a worker that cannot drain its own claim does not exit clean");
    assert!(
        format!("{error}").contains("injected Forge cancelled claim release failure"),
        "the worker returned an unrelated error: {error}"
    );
    assert!(
        !readiness.is_ready(),
        "a worker still holding an undrained claim advertised ready"
    );
    let state: String = sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
        .bind(task_id)
        .fetch_one(&pool)
        .await
        .expect("the retained claim row is readable");
    assert_eq!(
        state, "claimed",
        "the claim was released despite its release failing"
    );
    assert_eq!(
        observer.completed(),
        0,
        "a drained claim was reported as a completion"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// Seeds one claimable Forge task row directly.
///
/// The durable non-success outcome under test is reachable only from a row the
/// planner would not produce today — a row whose plan parameters do not match
/// its strategy — so the row is written directly rather than through an enqueue
/// that would reject it. `parameters` is the exact plan parameter object: the
/// worker's pre-effect gate
/// refuses a claim whose parameters do not match its strategy contract before it
/// acquires any table fence, so a case that needs the fenced path must supply the
/// contract-valid object rather than an empty one.
///
/// # Panics
///
/// Panics when the insert fails.
#[cfg(feature = "test-support")]
async fn seed_ready_forge_task(
    pool: &sqlx::PgPool,
    tenant: uuid::Uuid,
    table_name: &str,
    strategy: &str,
    parameters: &str,
) -> uuid::Uuid {
    let task_id = uuid::Uuid::now_v7();
    let statement = "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,\
         namespace_name,table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,\
         estimated_bytes,state,ready_at,next_eligible_at,updated_at) \
         VALUES ($1,$2,'wyrd-redux','vala.bifrost',$3,$4,4242,$5::jsonb,\
         decode(repeat('33',32),'hex'),1,100,'ready',statement_timestamp()-interval '1 hour',\
         statement_timestamp()-interval '1 hour',statement_timestamp())";
    sqlx::query(statement)
        .bind(task_id)
        .bind(tenant)
        .bind(table_name)
        .bind(strategy)
        .bind(parameters)
        .execute(pool)
        .await
        .expect("seed a claimable Forge task");
    task_id
}

/// Waits until the seeded task leaves its claimable states and reports the
/// durable state it settled in.
///
/// # Panics
///
/// Panics when the row never settles inside the bounded wait.
#[cfg(feature = "test-support")]
async fn await_settled_task_state(
    server: &WyrdTestServer,
    pool: &sqlx::PgPool,
    task_id: uuid::Uuid,
) -> String {
    let mut state = String::from("unobserved");
    let settled = tokio::time::timeout(FORGE_READINESS_CEILING, async {
        loop {
            state = sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(pool)
                .await
                .expect("the seeded task row is readable");
            if !matches!(
                state.as_str(),
                "ready" | "retryable" | "claimed" | "running"
            ) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await;
    if settled.is_err() {
        server.state().shutdown_token.cancel();
        panic!(
            "task {task_id} did not settle; last state={state}, ready={}",
            server
                .state()
                .forge()
                .expect("Forge role")
                .worker_readiness()
                .is_ready()
        );
    }
    state
}

/// A claim that settles durably without producing an effect keeps the worker
/// ready and reports no completion.
///
/// A row whose payload does not match its strategy is a healthy worker outcome:
/// the worker terminalized durable state and produced nothing. Readiness answers
/// whether this replica can take the next claim, so that outcome may not clear it.
///
/// # Panics
///
/// Panics when the row is reported as a completion or clears readiness.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn durably_settled_claims_keep_readiness() {
    // Expired cleanup's exact work is its parameters, so a copied input set is
    // exactly what its payload contract forbids.
    for (table_name, strategy, expected) in [("malformed_payload", "expired_cleanup", "failed")] {
        let (server, observer) = Box::pin(server_with_forge_observer()).await;
        let readiness = server
            .state()
            .forge()
            .expect("the default target selects Forge")
            .worker_readiness();
        let pool = server
            .pg_fixture()
            .superuser_pool()
            .expect("superuser pool");
        let task_id = seed_ready_forge_task(
            &pool,
            uuid::Uuid::from(server.data_tenant_id()),
            table_name,
            strategy,
            MALFORMED_PLAN,
        )
        .await;

        let stop = server.state().shutdown_token.child_token();
        let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone())
            .expect("the worker composes");
        let handle = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(worker));
        await_forge_role(
            &server,
            true,
            wyrd_server::state::Forge::worker_readiness,
            table_name,
        )
        .await;

        let settled = await_settled_task_state(&server, &pool, task_id).await;
        assert_eq!(
            settled, expected,
            "{table_name} settled in an unexpected durable state"
        );
        assert_eq!(
            observer.completed(),
            0,
            "{table_name} produced no effect but was reported as a completion"
        );
        assert!(
            readiness.is_ready(),
            "{table_name} is a healthy outcome and must not clear worker readiness"
        );

        stop.cancel();
        join_forge_loop(&server, handle)
            .await
            .expect("worker loop ok");
        server.shutdown().await.expect("test server shuts down");
    }
}

/// Resolves the physical Iceberg identity the production planner uses.
///
/// # Panics
///
/// Panics when the tenant/table pair cannot be bound.
#[cfg(feature = "test-support")]
fn forge_table_ident(server: &WyrdTestServer, table: &str) -> iceberg::TableIdent {
    vala_bifrost_redux::catalog::TenantTableBinding::resolve((
        server.data_tenant_id(),
        vala_bifrost_redux::catalog::TableRef::new(
            vala_bifrost_redux::namespaces::BifrostNamespace::Bifrost,
            table,
        ),
    ))
    .expect("the seeded table binds")
    .table_ident()
}

/// A maintenance pass whose object store cannot serve one table's manifest
/// list completes and keeps coordinator readiness.
///
/// The leader's maintenance timer isolates per-table failures: an unreadable
/// table is logged and skipped while the pass continues, exactly as the
/// RisingWave Iceberg GC loop treats one table's error. Coordinator readiness
/// answers whether this replica holds or can contest leadership and read
/// promotion debt, so one table's missing object must not drain the replica.
///
/// # Panics
///
/// Panics when the faulted pass does not complete or clears readiness, or when
/// the pass after restoring the exact bytes is not ready.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coordinator_object_store_failure_keeps_readiness() {
    let server = WyrdTestServer::builder()
        .start_bound()
        .await
        .expect("test server starts");
    let readiness = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .coordinator_readiness();
    // The coordinator's first tick is immediate, so the boot pass must be
    // observed before this test arranges anything. Otherwise the pass this
    // test drives below can be that boot pass still in flight — one that ran
    // before the table held any rows — and the promotion loop would count it.
    await_boot_scheduler_pass(&server, "boot pass before object-store fault").await;
    let table = "coordinator_object_store";

    register_forge_table(&server, table).await;
    server
        .seed_bifrost_rows(&format!("vala.bifrost.{table}"), &[1, 2, 3])
        .await
        .expect("the table publishes a real snapshot");
    // The rows are hot until Scribe promotion publishes them, and the manifest
    // list this case removes only exists once a snapshot does.
    server
        .flush_bifrost()
        .await
        .expect("the seeded rows publish as hot files");
    // The coordinator promotes inline, so a driven pass returns only after the
    // promoting attempt settled: a visible snapshot already proves ownership
    // finished.
    let catalog = server.bifrost_catalog();
    let ident = forge_table_ident(&server, table);
    let mut completed = server.completed_forge_scheduler_passes_for_test();
    let location = tokio::time::timeout(FORGE_READINESS_CEILING, async {
        loop {
            drive_scheduler_pass(&server, "promotion pass").await;
            completed = server.completed_forge_scheduler_passes_for_test();
            let loaded = iceberg::Catalog::load_table(catalog.iceberg_catalog().as_ref(), &ident)
                .await
                .expect("the registered table loads");
            if let Some(snapshot) = loaded.metadata().current_snapshot() {
                break snapshot.manifest_list().to_owned();
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        server.state().shutdown_token.cancel();
        panic!(
            "promotion timed out without a snapshot; ready={}, completed passes={completed}",
            readiness.is_ready()
        );
    });
    drive_scheduler_pass(&server, "healthy pass after promotion ownership completes").await;
    assert!(readiness.is_ready(), "the healthy pass did not complete");

    // Retain the exact manifest-list bytes, then remove that exact object. The
    // table is otherwise untouched, so restoring the bytes restores the table.
    let file_io = catalog.file_io();
    let retained = file_io
        .new_input(&location)
        .expect("the manifest list is addressable")
        .read()
        .await
        .expect("the manifest list reads before removal");
    file_io
        .delete(&location)
        .await
        .expect("the manifest list is removable");

    drive_maintenance_pass(&server, "object-store faulted maintenance pass").await;
    drive_scheduler_pass(&server, "heartbeat after the faulted maintenance").await;
    assert!(
        readiness.is_ready(),
        "one table's unreadable manifest list drained the coordinator"
    );

    file_io
        .new_output(&location)
        .expect("the manifest list location is writable")
        .write(retained)
        .await
        .expect("the exact manifest list bytes are restored");
    drive_scheduler_pass(&server, "restoration pass").await;
    assert!(
        readiness.is_ready(),
        "the pass after restoring the manifest list was not ready"
    );

    server.state().shutdown_token.cancel();
    server.shutdown().await.expect("test server shuts down");
}

/// A transient execution failure settles the task retryable and keeps the
/// worker ready.
///
/// A contract-valid rewrite whose table the catalog cannot load is an
/// environment failure, not a defect in the task or in this replica. The
/// durable row must return to `retryable` for a later attempt, the worker must
/// report no completion, and readiness must survive: a replica that unreadied
/// itself over another table's absence would drain the whole pod for one bad
/// row.
///
/// # Panics
///
/// Panics when the task terminalizes instead of retrying, is reported as a
/// completion, or clears readiness.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transient_execution_failure_retries_and_keeps_readiness() {
    let (server, observer) = Box::pin(server_with_forge_observer()).await;
    let readiness = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .worker_readiness();
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool");
    let task_id = seed_ready_forge_task(
        &pool,
        uuid::Uuid::from(server.data_tenant_id()),
        "absent_table",
        "small_files",
        LIVE_REWRITE_PLAN,
    )
    .await;

    let stop = server.state().shutdown_token.child_token();
    let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone())
        .expect("the worker composes");
    let handle = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(worker));
    await_forge_role(
        &server,
        true,
        wyrd_server::state::Forge::worker_readiness,
        "worker",
    )
    .await;

    // Poll for the retryable transition rather than a terminal state: this row
    // deliberately never terminalizes, so a settle-wait would time out on it.
    let mut observed = String::from("unobserved");
    let retried = tokio::time::timeout(FORGE_READINESS_CEILING, async {
        loop {
            observed = sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(&pool)
                .await
                .expect("the seeded task row is readable");
            if observed == "retryable" {
                break;
            }
            assert!(
                !matches!(observed.as_str(), "succeeded" | "failed" | "cancelled"),
                "a transient catalog failure terminalized as {observed}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await;
    if retried.is_err() {
        stop.cancel();
        handle.abort();
        panic!(
            "retry transition timed out; task={task_id}, state={observed}, ready={}",
            readiness.is_ready()
        );
    }
    assert_eq!(
        observer.completed(),
        0,
        "a failed attempt was reported as a completion"
    );
    assert!(
        readiness.is_ready(),
        "a replica unreadied itself over another table's absence"
    );

    stop.cancel();
    join_forge_loop(&server, handle)
        .await
        .expect("worker loop ok");
    server.shutdown().await.expect("test server shuts down");
}

/// A live claim owned by another worker is not this worker's recovery residue
/// and does not delay its readiness.
///
/// Recovery drains what this owner must resolve. A peer's unexpired claim is
/// owned by that peer's own recovery, so treating it as residue would stall
/// every replica behind the slowest one.
///
/// # Panics
///
/// Panics when the foreign claim blocks readiness or is disturbed.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_foreign_claim_does_not_block_readiness() {
    let (server, _observer) = Box::pin(server_with_forge_observer()).await;
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool");
    let task_id = seed_ready_forge_task(
        &pool,
        uuid::Uuid::from(server.data_tenant_id()),
        "foreign_claim",
        "small_files",
        MALFORMED_PLAN,
    )
    .await;
    let peer = uuid::Uuid::now_v7();
    sqlx::query(
        "UPDATE vala.forge_tasks SET state='claimed',claimed_by=$2,attempt_id=$3,\
         claim_expires_at=statement_timestamp()+interval '10 minutes' WHERE task_id=$1",
    )
    .bind(task_id)
    .bind(peer)
    .bind(uuid::Uuid::now_v7())
    .execute(&pool)
    .await
    .expect("a live peer holds the claim");

    await_forge_role_while_running(&server, "worker beside a live foreign claim").await;

    let (state, owner): (String, Option<uuid::Uuid>) =
        sqlx::query_as("SELECT state,claimed_by FROM vala.forge_tasks WHERE task_id=$1")
            .bind(task_id)
            .fetch_one(&pool)
            .await
            .expect("the foreign claim is readable");
    assert_eq!(state, "claimed", "the foreign claim was disturbed");
    assert_eq!(owner, Some(peer), "the foreign claim changed owner");
    server.shutdown().await.expect("test server shuts down");
}

/// A worker whose table-fence release fails after a durable settlement returns
/// that release error, clears readiness, and recovers on respawn.
///
/// The settlement itself already committed, so the release failure is the only
/// unsafe part: the fence is still held by an owner that is no longer running
/// it. Readiness must clear before anything can claim behind that fence, and
/// the stopping worker must take no further work of its own — newly eligible
/// demand has to wait for a successor.
///
/// # Panics
///
/// Panics when the release failure is swallowed, readiness survives it, the
/// stopping worker claims newly eligible work, or the respawned worker does not
/// recover.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn release_failure_clears_readiness() {
    let metrics = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("process-local Prometheus recorder");
    let observer = vala_bifrost_redux::forge::ForgeWorkerCompletionObserver::new();
    let server = WyrdTestServer::builder()
        .with_forge_completion_observer_for_test(observer.clone())
        .start_in_process()
        .await
        .expect("the test server starts");
    let readiness = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .worker_readiness();
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool");

    let stop = server.state().shutdown_token.child_token();
    let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone())
        .expect("the worker composes");
    let mut handle = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(worker));
    // The fault is a one-shot, so it is armed only after the recovery drain has
    // published ready. Arming it before startup would spend it on a recovery
    // release and leave the claim under test running unfaulted.
    await_forge_role(
        &server,
        true,
        wyrd_server::state::Forge::worker_readiness,
        "worker before its release fault is armed",
    )
    .await;
    observer.hold_before_fatal_observation_for_test();
    observer.fail_next_lease_release();

    // A well-formed payload on a table this catalog does not carry reaches the
    // table fence, settles its attempt durably, and then releases: exactly the
    // ordering this fault needs.
    let task_id = seed_ready_forge_task(
        &pool,
        uuid::Uuid::from(server.data_tenant_id()),
        "release_only",
        "small_files",
        LIVE_REWRITE_PLAN,
    )
    .await;

    if tokio::time::timeout(
        FORGE_READINESS_CEILING,
        observer.wait_for_fatal_observation_for_test(),
    )
    .await
    .is_err()
    {
        stop.cancel();
        handle.abort();
        panic!(
            "release attempt did not reach its barrier: ready={}, attempts={}",
            readiness.is_ready(),
            observer.attempts()
        );
    }
    assert!(
        !observer.lease_release_failure_armed(),
        "the first task consumed the release fault"
    );
    assert!(
        !readiness.is_ready(),
        "known fatal release must close readiness before telemetry"
    );

    // Demand that becomes eligible while the worker is stopping belongs to a
    // successor, not to the owner that is on its way out.
    let successor_work = seed_ready_forge_task(
        &pool,
        uuid::Uuid::from(server.data_tenant_id()),
        "release_successor_work",
        "small_files",
        LIVE_REWRITE_PLAN,
    )
    .await;
    let before: (String, i32, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT state,attempt_count,attempt_id FROM vala.forge_tasks WHERE task_id=$1",
    )
    .bind(successor_work)
    .fetch_one(&pool)
    .await
    .expect("newly eligible row");
    assert_eq!(before.0, "ready");
    assert_eq!(before.1, 0, "the new task has not executed an attempt");
    assert!(before.2.is_none());

    observer.release_fatal_observation_for_test();
    let joined = tokio::time::timeout(FORGE_READINESS_CEILING, &mut handle).await;
    stop.cancel();
    let error = if let Ok(outcome) = joined {
        outcome
            .expect("worker joins")
            .expect_err("the injected release failure is returned")
    } else {
        handle.abort();
        panic!(
            "faulted worker did not return; new demand={before:?}, ready={}, attempts={}",
            readiness.is_ready(),
            observer.attempts()
        )
    };
    assert!(
        format!("{error}").contains("injected Forge table lease release failure"),
        "the worker returned an unrelated error: {error}"
    );
    assert!(
        !readiness.is_ready(),
        "a worker still holding an unreleased table fence advertised ready"
    );
    let rendered = metrics.render();
    let sample = rendered
        .lines()
        .find(|line| {
            line.starts_with("bifrost_forge_task_attempts_total")
                && line.contains("task_type=\"small_files\"")
                && line.contains("result=\"retry\"")
        })
        .expect("the durable Retryable ownership episode is observed");
    assert_eq!(
        sample.rsplit_once(' ').expect("Prometheus sample").1,
        "1",
        "{sample}"
    );
    let active = rendered
        .lines()
        .find(|line| {
            line.starts_with("bifrost_forge_active_tasks{")
                && line.contains("task_type=\"small_files\"")
        })
        .expect("active ownership series");
    assert_eq!(
        active.rsplit_once(' ').expect("active sample").1,
        "0",
        "{active}"
    );
    let state: String = sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
        .bind(task_id)
        .fetch_one(&pool)
        .await
        .expect("the settled task row is readable");
    assert_ne!(
        state, "ready",
        "the attempt settled before its release failed"
    );
    let after: (String, i32, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT state,attempt_count,attempt_id FROM vala.forge_tasks WHERE task_id=$1",
    )
    .bind(successor_work)
    .fetch_one(&pool)
    .await
    .expect("newly eligible row after the worker joins");
    assert_eq!(
        after, before,
        "the stopping worker must not execute or claim newly eligible work"
    );
    let successor_claims = observer.lifecycle_events().iter().filter(|event| matches!(event,
        vala_bifrost_redux::forge::ForgeLifecycleEvent::Claimed { task_id, .. } if *task_id == successor_work
    )).count();
    assert_eq!(
        successor_claims, 0,
        "newly eligible work must receive no claim from the stopping worker"
    );
    eprintln!("after worker join: new demand={after:?}; no later claim or attempt");
    await_forge_role_while_running(&server, "worker after its release recovers").await;
    server.shutdown().await.expect("test server shuts down");
}

/// An unavailable promotion-debt read clears coordinator readiness until the
/// read returns.
///
/// Scribe's `file_list` is the one durable read the leader makes on every
/// heartbeat, so a leader that cannot read it cannot recover stranded
/// promotions and must not advertise that it coordinates. Restoring the read
/// lets the next pass restore readiness.
///
/// # Panics
/// Panics if the pass without the debt read reports ready or the restored pass
/// does not.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coordinator_promotion_debt_read_failure_clears_readiness() {
    let server = WyrdTestServer::start_in_process().await.expect("server");
    let readiness = server
        .state()
        .forge()
        .expect("Forge")
        .coordinator_readiness();
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool");
    sqlx::query("ALTER TABLE vala.file_list RENAME TO unavailable_promotion_debt")
        .execute(&pool)
        .await
        .expect("fail promotion-debt read");
    let stop = server.state().shutdown_token.child_token();
    let handle = spawn_coordinator(&server, &stop);
    await_boot_scheduler_pass(&server, "boot pass with unavailable debt read").await;
    drive_scheduler_pass(&server, "pass with unavailable debt read").await;
    assert!(
        !readiness.is_ready(),
        "a leader that cannot read promotion debt advertised ready"
    );
    sqlx::query("ALTER TABLE vala.unavailable_promotion_debt RENAME TO file_list")
        .execute(&pool)
        .await
        .expect("restore promotion-debt read");
    drive_scheduler_pass(&server, "restored promotion-debt read").await;
    assert!(
        readiness.is_ready(),
        "the pass after restoring the debt read stayed unready"
    );
    stop.cancel();
    join_forge_loop(&server, handle)
        .await
        .expect("coordinator stops");
    server.shutdown().await.expect("server shuts down");
}

/// A real boot composes Forge only for a Forge process.
///
/// Composition runs against the same Postgres-backed graph the worker later
/// admits plans on, so a booted server is where it is observed: the default
/// target composes a Forge coordinator, and a process without the Forge role
/// composes no Forge at all.
///
/// # Panics
///
/// Panics when either server fails to boot, when the default target composes
/// no Forge, or when a Forge-absent target still composes one.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forge_runtime_composes_only_for_forge_targets() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    assert!(
        server
            .state()
            .forge()
            .and_then(|forge| forge.coordinator())
            .is_some(),
        "the default target selects Forge"
    );

    server.shutdown().await.expect("server shuts down");

    let scribe = WyrdTestServer::builder()
        .with_bifrost_target_for_test(wyrd_server::config::BifrostTarget::Scribe)
        .start_in_process()
        .await
        .expect("a Scribe-only server starts");
    assert!(
        scribe.state().forge().is_none(),
        "a process without the Forge role composes no admitting Forge"
    );
    scribe.shutdown().await.expect("server shuts down");
}
