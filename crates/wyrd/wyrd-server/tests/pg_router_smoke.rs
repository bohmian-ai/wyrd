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
        oracle: ok_probe,
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

/// Polls the Oracle readiness input until it reports `expected`.
///
/// `/readyz` reads this value through its published snapshot; the snapshot is
/// republished by a background loop that a composed-in-process server does not
/// run, so the readiness contract is observed at the same input the probe
/// reads rather than through a snapshot that never ticks.
///
/// # Panics
///
/// Panics when Oracle readiness does not reach `expected` within the budget.
#[cfg(feature = "test-support")]
async fn await_oracle_ready(server: &WyrdTestServer, expected: bool) {
    // Generous because every wait here is condition-polled, and this lane runs
    // many servers against one Postgres: a budget tuned to an idle machine
    // turns load into a false failure.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    loop {
        let ready = server
            .state()
            .bifrost_query()
            .expect("this server hosts an Oracle role")
            .engine()
            .is_ready();
        if ready == expected {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Oracle readiness never reached {expected}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Lists this node's Oracle reader-epoch lifecycle audit operations, in order.
///
/// # Panics
///
/// Panics when the audit rows cannot be read.
#[cfg(feature = "test-support")]
async fn epoch_audit_operations(
    pool: &sqlx::PgPool,
    node_id: uuid::Uuid,
    fencing_token: i64,
) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT operation FROM vala.audit_outbox \
          WHERE data_tenant_id = $1 AND resource = $2 ORDER BY seq",
    )
    .bind(uuid::Uuid::from(wyrd_spec::DataTenantId::SYSTEM_OWNER))
    .bind(format!("oracle/reader_epoch/{node_id}/{fencing_token}"))
    .fetch_all(pool)
    .await
    .expect("epoch audit rows read")
}

/// Reports whether this node's durable Oracle role row still advertises ready.
///
/// # Panics
///
/// Panics when the role row cannot be read.
#[cfg(feature = "test-support")]
async fn role_advertises_ready(pool: &sqlx::PgPool, node_id: uuid::Uuid) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT ready FROM vala.cluster_nodes WHERE node_id = $1 AND role = 'oracle'",
    )
    .bind(node_id)
    .fetch_one(pool)
    .await
    .expect("oracle role row read")
}

/// Proves epoch loss closes readiness before its audit, and that retirement
/// joins whichever owner selected that loss.
///
/// Two properties are inseparable here and are therefore proved together.
/// Reaching the admission cutoff must remove readiness — the authority's own
/// admission, the engine, `/readyz`, and the durable role advertisement — at
/// the instant loss is selected, which is strictly before the audited loss
/// edge commits, while liveness stays true because the process is healthy and
/// merely no longer authorized. And retirement must resolve who owns that
/// loss before it releases anything: whether the lease supervisor selected it
/// first or retirement did, the epoch ends with exactly one durable loss edge
/// and one complete audit sequence.
///
/// # Panics
///
/// Panics when readiness, liveness, durable advertisement, the audit
/// sequence, or the retired epoch row differs from that contract.
#[cfg(feature = "test-support")]
#[tokio::test]
async fn oracle_epoch_cutoff_removes_readiness_and_retirement_joins_loss_owner() {
    supervisor_first_loss_closes_readiness_before_its_audit().await;
    retirement_first_loss_commits_its_own_edge().await;
}

/// Drives the race in which the lease supervisor selects loss first.
///
/// # Panics
///
/// Panics when readiness, liveness, advertisement, or the audit sequence
/// differs from the contract.
#[cfg(feature = "test-support")]
async fn supervisor_first_loss_closes_readiness_before_its_audit() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let authority = std::sync::Arc::clone(
        server
            .state()
            .bifrost_query()
            .expect("this server hosts an Oracle role")
            .engine()
            .reader_authority(),
    );
    let node_id = uuid::Uuid::from(server.node_id());
    let fencing_token = authority.fencing_token();
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool");

    await_oracle_ready(&server, true).await;
    assert!(role_advertises_ready(&pool, node_id).await);

    // Hold every audit append. Renewal deliberately writes no audit row, so
    // this gates exactly the loss edge's transaction and nothing the epoch
    // needs in order to reach its cutoff.
    let mut gate = pool.begin().await.expect("audit gate transaction begins");
    sqlx::query("LOCK TABLE vala.audit_outbox IN EXCLUSIVE MODE")
        .execute(&mut *gate)
        .await
        .expect("the audit outbox is held");

    // Reach the cutoff through the production supervisor. A renewal that lands
    // first re-derives the deadlines from its fresh lease, so the collapse is
    // reapplied until admission actually closes.
    let deadline = std::time::Instant::now() + std::time::Duration::from_mins(2);
    while authority.admits() {
        assert!(
            std::time::Instant::now() < deadline,
            "the epoch never reached its admission cutoff"
        );
        authority.collapse_lease_for_test();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Loss is selected and the audited edge is still blocked behind the gate.
    // Readiness must already be gone everywhere it is published.
    assert!(
        !server
            .state()
            .bifrost_query()
            .expect("the Oracle runtime is retained")
            .engine()
            .is_ready(),
        "an epoch past its admission cutoff is not a ready Oracle"
    );
    let liveness = server
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");
    assert_eq!(
        liveness.status(),
        StatusCode::OK,
        "a fenced epoch is unready, not unhealthy"
    );
    assert_eq!(
        epoch_audit_operations(&pool, node_id, fencing_token).await,
        vec![
            "oracle.reader_epoch.acquired".to_owned(),
            "oracle.reader_epoch.activated".to_owned(),
        ],
        "readiness closes before the loss edge is audited, not after"
    );

    // Releasing the gate lets the supervisor finish its audited loss edge, and
    // lets the continuity monitor's own audited deactivation land.
    gate.rollback().await.expect("the audit gate releases");
    let advertising = std::time::Instant::now() + std::time::Duration::from_mins(2);
    while role_advertises_ready(&pool, node_id).await {
        assert!(
            std::time::Instant::now() < advertising,
            "the durable Oracle role kept advertising ready after epoch loss"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    // Drive the production Oracle drain in place: `shutdown` consumes the
    // harness, and with it the Postgres fixture whose rows this asserts on.
    server
        .state()
        .bifrost_query()
        .expect("the Oracle runtime is retained")
        .engine()
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(10))
        .await;

    assert_eq!(
        epoch_audit_operations(&pool, node_id, fencing_token).await,
        vec![
            "oracle.reader_epoch.acquired".to_owned(),
            "oracle.reader_epoch.activated".to_owned(),
            "oracle.reader_epoch.draining".to_owned(),
            "oracle.reader_epoch.invalidated".to_owned(),
            "oracle.reader_epoch.retired".to_owned(),
        ],
        "retirement joins the loss owner and adds no second loss edge"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM vala.oracle_reader_epochs WHERE node_id = $1"
        )
        .bind(node_id)
        .fetch_one(&pool)
        .await
        .expect("epoch rows counted"),
        0,
        "a retired epoch leaves no row"
    );

    server.shutdown().await.expect("server shuts down");
}

/// Drives the race in which retirement selects loss first.
///
/// # Panics
///
/// Panics when the audit sequence or the retired epoch row differs from the
/// contract.
#[cfg(feature = "test-support")]
async fn retirement_first_loss_commits_its_own_edge() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let node_id = uuid::Uuid::from(server.node_id());
    let fencing_token = server
        .state()
        .bifrost_query()
        .expect("this server hosts an Oracle role")
        .engine()
        .reader_authority()
        .fencing_token();
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool");

    await_oracle_ready(&server, true).await;
    // Drive the production Oracle drain in place: `shutdown` consumes the
    // harness, and with it the Postgres fixture whose rows this asserts on.
    server
        .state()
        .bifrost_query()
        .expect("the Oracle runtime is retained")
        .engine()
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(10))
        .await;

    assert_eq!(
        epoch_audit_operations(&pool, node_id, fencing_token).await,
        vec![
            "oracle.reader_epoch.acquired".to_owned(),
            "oracle.reader_epoch.activated".to_owned(),
            "oracle.reader_epoch.draining".to_owned(),
            "oracle.reader_epoch.invalidated".to_owned(),
            "oracle.reader_epoch.retired".to_owned(),
        ],
        "retirement that selects loss commits exactly one loss edge itself"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM vala.oracle_reader_epochs WHERE node_id = $1"
        )
        .bind(node_id)
        .fetch_one(&pool)
        .await
        .expect("epoch rows counted"),
        0,
        "a retired epoch leaves no row"
    );

    server.shutdown().await.expect("server shuts down");
}
