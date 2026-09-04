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
        forge_coordinator: None,
        forge_worker: None,
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
/// and one complete audit sequence. A renewal already stalled inside Postgres
/// is the third case, because it is the one state in which the supervisor has
/// no local reason to look at the clock at all.
///
/// # Panics
///
/// Panics when readiness, liveness, durable advertisement, the audit
/// sequence, or the retired epoch row differs from that contract.
#[cfg(feature = "test-support")]
#[tokio::test]
async fn oracle_epoch_cutoff_removes_readiness_and_retirement_joins_loss_owner() {
    supervisor_first_loss_closes_readiness_before_its_audit().await;
    blocked_renewal_cannot_suppress_cutoff_or_bounded_settlement().await;
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

/// Reports how many backends are queued behind an Oracle epoch row lock.
///
/// A writer that has to wait for a row first takes a `tuple` lock on the
/// relation and only then blocks on the holder's transaction, so the presence
/// of that lock is the evidence — visible without any elevated statistics
/// privilege — that a statement reached Postgres and cannot return. Renewal is
/// the only statement a live epoch issues against this table.
///
/// # Panics
///
/// Panics when `pg_locks` cannot be read.
#[cfg(feature = "test-support")]
async fn epoch_row_lock_waiters(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM pg_locks \
          WHERE locktype = 'tuple' \
            AND relation = 'vala.oracle_reader_epochs'::regclass",
    )
    .fetch_one(pool)
    .await
    .expect("epoch row lock waiters counted")
}

/// Proves a renewal stuck in Postgres cannot hold admission open past cutoff.
///
/// The supervisor used to await its renewal without any competing deadline, so
/// a renewal that reached SQL and never returned kept admission, readiness, and
/// the durable role advertisement open for as long as Postgres stalled it —
/// well past the instant the confirmed lease stopped authorizing source IO.
/// Holding the epoch row reproduces exactly that stall, and the absolute
/// cutoff derived from the last confirmed lease must still close this Oracle.
///
/// Nothing here holds a query guard: this server carries the production
/// aborting terminator, so a descendant that outlived the cutoff would end the
/// test process rather than fail it. Retained protection under an exhausted
/// deadline is proved in `vala-bifrost-redux`'s Tier 2 authority target, which
/// injects a recording terminator.
///
/// # Panics
///
/// Panics when the blocked renewal suppresses the cutoff, when readiness or
/// the durable advertisement survives it, when the loss edge is audited while
/// settlement is still blocked, or when the released epoch does not retire
/// through its exact audit sequence.
#[cfg(feature = "test-support")]
async fn blocked_renewal_cannot_suppress_cutoff_or_bounded_settlement() {
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

    // Hold the epoch row itself. Renewal is the only statement the live epoch
    // issues against it, so this stalls the renewal inside Postgres without
    // touching the deadlines the supervisor already derived.
    let mut gate = pool.begin().await.expect("epoch gate transaction begins");
    sqlx::query(
        "SELECT state_revision FROM vala.oracle_reader_epochs \
          WHERE node_id = $1 AND fencing_token = $2 FOR UPDATE",
    )
    .bind(node_id)
    .bind(fencing_token)
    .fetch_one(&mut *gate)
    .await
    .expect("the epoch row is held");

    // The renewal cadence is short relative to the lease, so the stall is
    // observable long before the cutoff the test is waiting for.
    let blocked_by = std::time::Instant::now() + std::time::Duration::from_mins(2);
    while epoch_row_lock_waiters(&pool).await == 0 {
        assert!(
            std::time::Instant::now() < blocked_by,
            "the cadence renewal never reached the held epoch row"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // No collapse: the absolute cutoff of the last confirmed lease must arrive
    // on its own while the renewal is still stuck.
    let cutoff_by = std::time::Instant::now() + std::time::Duration::from_mins(2);
    while authority.admits() {
        assert!(
            std::time::Instant::now() < cutoff_by,
            "a blocked renewal held admission open past the epoch's cutoff"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Settlement is still blocked behind the same held row, and everything the
    // process publishes about its authority must already be closed.
    assert!(
        !server
            .state()
            .bifrost_query()
            .expect("the Oracle runtime is retained")
            .engine()
            .is_ready(),
        "an epoch past its admission cutoff is not a ready Oracle"
    );
    assert_eq!(
        epoch_audit_operations(&pool, node_id, fencing_token).await,
        vec![
            "oracle.reader_epoch.acquired".to_owned(),
            "oracle.reader_epoch.activated".to_owned(),
        ],
        "the loss edge is not audited while its transaction is still blocked"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM vala.oracle_table_protections WHERE node_id = $1"
        )
        .bind(node_id)
        .fetch_one(&pool)
        .await
        .expect("protection rows counted"),
        0,
        "nothing is released while settlement is blocked"
    );

    // Release before the bounded settlement window expires, so the audited
    // loss edge is the one the supervisor already selected.
    gate.rollback().await.expect("the epoch gate releases");

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

    let advertising = std::time::Instant::now() + std::time::Duration::from_mins(2);
    while role_advertises_ready(&pool, node_id).await {
        assert!(
            std::time::Instant::now() < advertising,
            "the durable Oracle role kept advertising ready after a blocked renewal lost its lease"
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
        "the supervisor that lost the lease owns the one loss edge, and retirement joins it"
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
    let before = server.completed_forge_scheduler_passes_for_test();
    server.request_forge_scheduler_pass_for_test();
    tokio::time::timeout(
        FORGE_READINESS_CEILING,
        server.wait_for_forge_scheduler_passes_for_test(before + 1),
    )
    .await
    .unwrap_or_else(|_| {
        panic!("{label}: no scheduler pass completed within {FORGE_READINESS_CEILING:?}")
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
    let deadline = std::time::Instant::now() + FORGE_READINESS_CEILING;
    loop {
        let forge = server
            .state()
            .forge()
            .expect("this target selected a Forge role");
        if select(forge).is_ready() == expected {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{label} readiness never reached {expected} within {FORGE_READINESS_CEILING:?} \
             (completed passes: {})",
            server.completed_forge_scheduler_passes_for_test()
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
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
    let stop = tokio_util::sync::CancellationToken::new();
    let loop_handle = tokio::spawn(wyrd_server::components::health::readiness_loop(
        server.state().clone(),
        std::time::Duration::from_millis(50),
        std::time::Duration::from_secs(5),
        stop.clone(),
    ));
    let deadline = std::time::Instant::now() + SNAPSHOT_PUBLICATION_CEILING;
    let snapshot = loop {
        let snapshot = server.state().readiness.load_full();
        if snapshot.postgres.reason != ProbeReason::Warmup {
            break snapshot;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the readiness loop never published a computed snapshot"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    stop.cancel();
    let _ = loop_handle.await;
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

    let stop = tokio_util::sync::CancellationToken::new();
    let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone(), 1, 1)
        .expect("the worker composes");
    let handle = tokio::spawn(worker);

    await_forge_role(
        &server,
        true,
        wyrd_server::state::Forge::worker_readiness,
        "worker",
    )
    .await;

    stop.cancel();
    handle.await.expect("worker joins").expect("worker loop ok");
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

    let stop = tokio_util::sync::CancellationToken::new();
    let scheduler = wyrd_server::boot::spawn_maintenance_scheduler(server.state(), stop.clone())
        .expect("the scheduler composes")
        .expect("the default target selects a coordinator");
    let handle = tokio::spawn(scheduler);

    drive_scheduler_pass(&server, "first coordinator pass").await;
    assert!(
        forge.is_ready(),
        "a coordinator that completed one fenced pass is ready"
    );

    stop.cancel();
    handle
        .await
        .expect("scheduler joins")
        .expect("pass loop ok");
    assert!(
        !forge.is_ready(),
        "a stopped coordinator does not advertise ready"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// A standby pass leaves the coordinator unready, and the next complete fenced
/// pass under a reclaimed fence restores it.
///
/// Readiness answers whether this replica currently holds the authority the
/// role promises. A standby replica lost the fence to a live peer, so it plans
/// nothing. Routing maintenance to it is routing it to a coordinator that will
/// not do the work.
///
/// # Panics
///
/// Panics when a standby or completed pass publishes the wrong bit.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coordinator_standby_pass_is_not_ready() {
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
        .await
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

    let stop = tokio_util::sync::CancellationToken::new();
    let scheduler = wyrd_server::boot::spawn_maintenance_scheduler(server.state(), stop.clone())
        .expect("the scheduler composes")
        .expect("the default target selects a coordinator");
    let handle = tokio::spawn(scheduler);

    drive_scheduler_pass(&server, "standby pass").await;
    assert!(
        !readiness.is_ready(),
        "a standby replica holds no fence and is not a ready coordinator"
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
    handle
        .await
        .expect("scheduler joins")
        .expect("pass loop ok");
    server.shutdown().await.expect("test server shuts down");
}

/// An overflowed partial planning pass leaves the coordinator unready until the
/// next pass drains the remaining demand.
///
/// The per-wake hint budget bounds one pass, not the queue. A replica that
/// stopped at its budget has left demand unacknowledged, so it has not yet
/// proved it can plan what it was asked to plan.
///
/// # Panics
///
/// Panics when a partial or completed pass publishes the wrong bit.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coordinator_partial_pass_is_not_ready() {
    let server = WyrdTestServer::builder()
        .with_forge_config_for_test(vala_bifrost_redux::forge::ForgeConfig {
            max_hints_per_wake: 1,
            ..vala_bifrost_redux::forge::ForgeConfig::default()
        })
        .start_in_process()
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
        .await
        .expect("superuser pool");

    // Two outstanding demands are one more than this coordinator's per-wake
    // budget can page, so every pass it runs overflows and stays incomplete.
    for table in ["partial_one", "partial_two"] {
        sqlx::query(
            "INSERT INTO vala.forge_planning_demands \
             (data_tenant_id,catalog_name,namespace_name,table_name,last_source) \
             VALUES ($1,'wyrd-redux','vala.bifrost',$2,'periodic')",
        )
        .bind(uuid::Uuid::from(server.data_tenant_id()))
        .bind(table)
        .execute(&pool)
        .await
        .expect("seed planning demand");
    }

    let stop = tokio_util::sync::CancellationToken::new();
    let scheduler = wyrd_server::boot::spawn_maintenance_scheduler(server.state(), stop.clone())
        .expect("the scheduler composes")
        .expect("the default target selects a coordinator");
    let handle = tokio::spawn(scheduler);

    drive_scheduler_pass(&server, "overflowed partial pass").await;
    assert!(
        !readiness.is_ready(),
        "a pass that stopped at its hint budget left demand unplanned"
    );

    sqlx::query("DELETE FROM vala.forge_planning_demands WHERE data_tenant_id=$1")
        .bind(uuid::Uuid::from(server.data_tenant_id()))
        .execute(&pool)
        .await
        .expect("drain the remaining demand");
    drive_scheduler_pass(&server, "pass after the remaining demand drains").await;
    assert!(
        readiness.is_ready(),
        "a pass with no demand left to page completed and is ready"
    );

    stop.cancel();
    handle
        .await
        .expect("scheduler joins")
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
    let stop = tokio_util::sync::CancellationToken::new();
    let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone(), 1, 1)
        .expect("the worker composes");
    let outcome = tokio::time::timeout(FORGE_READINESS_CEILING, tokio::spawn(worker))
        .await
        .expect("the worker returns rather than looping for more work")
        .expect("worker joins");
    stop.cancel();
    outcome
}

/// A worker whose scratch volume is not a writable directory quarantines
/// itself, never publishes ready, and recovers once the volume is restored.
///
/// The scratch probe is the first thing a worker does, before it registers or
/// drains anything, so a pod whose spill volume failed to mount must not be
/// routed work. Quarantine is durable, so an operator can see which pod refused
/// itself rather than inferring it from an unready replica.
///
/// # Panics
///
/// Panics when the quarantined worker publishes ready or the restored worker
/// does not.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_scratch_failure_quarantines_without_readiness() {
    let (server, _observer) = server_with_forge_observer().await;
    let scratch = server
        .scribe_wal_root_for_test()
        .expect("the in-process server owns a WAL root")
        .join("forge-spill");
    std::fs::remove_dir_all(&scratch).expect("the composed scratch root is removable");
    std::fs::write(&scratch, b"not a directory").expect("a regular file replaces the scratch root");

    let readiness = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .worker_readiness();
    run_forge_worker_once(&server)
        .await
        .expect("a quarantined worker returns rather than crashing the pod");
    assert!(
        !readiness.is_ready(),
        "a worker quarantined by its scratch probe advertised ready"
    );

    std::fs::remove_file(&scratch).expect("the placeholder file is removable");
    std::fs::create_dir_all(&scratch).expect("the scratch root is restorable");
    await_forge_role_while_running(&server, "worker after its scratch volume is restored").await;
    server.shutdown().await.expect("test server shuts down");
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
    let (server, observer) = server_with_forge_observer().await;
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
    let (server, observer) = server_with_forge_observer().await;
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
    let stop = tokio_util::sync::CancellationToken::new();
    let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone(), 1, 1)
        .expect("the worker composes");
    let handle = tokio::spawn(worker);
    await_forge_role(
        server,
        true,
        wyrd_server::state::Forge::worker_readiness,
        label,
    )
    .await;
    stop.cancel();
    handle.await.expect("worker joins").expect("worker loop ok");
}

/// Installs the shared planning-failure function and one trigger that raises on
/// the named durable step.
///
/// The function is the same one the SQL-tier planning tests use, so a router
/// test fails exactly the step a real coordinator commits rather than a stub.
///
/// # Panics
///
/// Panics when the fault DDL cannot be installed.
#[cfg(feature = "test-support")]
async fn install_planning_failure(pool: &sqlx::PgPool, create_trigger: &'static str) {
    sqlx::query(
        "CREATE FUNCTION vala.fail_forge_planning_step() RETURNS trigger LANGUAGE plpgsql \
         AS $$ BEGIN RAISE EXCEPTION 'injected Forge planning failure'; END $$",
    )
    .execute(pool)
    .await
    .expect("the planning failure function installs");
    sqlx::query(create_trigger)
        .execute(pool)
        .await
        .expect("the planning failure trigger installs");
}

/// Removes the planning-failure trigger from `table` and its shared function.
///
/// # Panics
///
/// Panics when the fault DDL cannot be removed, which would leak into the next
/// boundary this test drives.
#[cfg(feature = "test-support")]
async fn remove_planning_failure(pool: &sqlx::PgPool, table: &str) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER forge_fail_step ON vala.{table}"
    )))
    .execute(pool)
    .await
    .expect("the planning failure trigger is removable");
    sqlx::query("DROP FUNCTION vala.fail_forge_planning_step()")
        .execute(pool)
        .await
        .expect("the planning failure function is removable");
}

/// Counts one registered table's outstanding planning demand.
///
/// The count is table-scoped because roster repair re-upserts periodic demand
/// for every registered table on every pass, so a tenant-wide count cannot tell
/// a retained rollback apart from an unrelated table's fresh demand.
///
/// # Panics
///
/// Panics when the demand table cannot be read.
#[cfg(feature = "test-support")]
async fn outstanding_demands(pool: &sqlx::PgPool, tenant: uuid::Uuid, table: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_planning_demands \
         WHERE data_tenant_id=$1 AND table_name=$2",
    )
    .bind(tenant)
    .bind(table)
    .fetch_one(pool)
    .await
    .expect("outstanding planning demand is readable")
}

/// Counts the Forge tasks one registered table currently owns.
///
/// # Panics
///
/// Panics when the task table cannot be read.
#[cfg(feature = "test-support")]
async fn table_task_count(pool: &sqlx::PgPool, tenant: uuid::Uuid, table: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_tasks WHERE data_tenant_id=$1 AND table_name=$2",
    )
    .bind(tenant)
    .bind(table)
    .fetch_one(pool)
    .await
    .expect("the Forge task table is readable")
}

/// Registers one real Bifrost table through the retained production catalog.
///
/// Planning discovers a table's snapshot before it reaches any durable write,
/// so a demand naming a table the catalog does not hold fails in discovery and
/// never exercises the enqueue transaction. Every coordinator fault this matrix
/// injects therefore has to be raised against a table that really exists.
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
            audit: None,
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
) -> tokio::task::JoinHandle<Result<(), vala_bifrost_redux::forge::ForgeError>> {
    let scheduler = wyrd_server::boot::spawn_maintenance_scheduler(server.state(), stop.clone())
        .expect("the scheduler composes")
        .expect("the default target selects a coordinator");
    tokio::spawn(scheduler)
}

/// A coordinator whose durable task insert fails advances its pass counter,
/// clears readiness, keeps the demand, and recovers on the next complete pass.
///
/// The insert commits inside the planning transaction, so a failure must roll
/// the whole pass back rather than acknowledge demand it never planned.
/// Readiness is the externally visible consequence: a replica whose planning
/// writes are failing must not be routed maintenance.
///
/// The faulted table is registered only after the unfaulted pass, so the failing
/// pass is the first one to plan it. A table already carrying its planned task
/// would insert nothing on a replan and never reach the fault.
///
/// # Panics
///
/// Panics when a failed pass publishes ready, silently drops demand, or the
/// healthy pass that follows does not restore readiness.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coordinator_task_insert_failure_clears_readiness() {
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
        .await
        .expect("superuser pool");
    let tenant = uuid::Uuid::from(server.data_tenant_id());

    let stop = tokio_util::sync::CancellationToken::new();
    let handle = spawn_coordinator(&server, &stop);
    drive_scheduler_pass(&server, "healthy pass before fault injection").await;
    assert!(
        readiness.is_ready(),
        "the unfaulted pass this case builds on did not complete"
    );

    // An idle registered table still projects one orphan-cleanup task, so a
    // real table is all the demand needed to reach the durable insert.
    register_forge_table(&server, "coordinator_sql").await;
    install_planning_failure(
        &pool,
        "CREATE TRIGGER forge_fail_step BEFORE INSERT ON vala.forge_tasks \
         FOR EACH ROW EXECUTE FUNCTION vala.fail_forge_planning_step()",
    )
    .await;

    drive_scheduler_pass(&server, "faulted planning pass").await;
    assert!(
        !readiness.is_ready(),
        "a coordinator whose forge_tasks write failed advertised ready"
    );
    assert_eq!(
        table_task_count(&pool, tenant, "coordinator_sql").await,
        0,
        "a rolled-back pass left a task behind"
    );
    assert_eq!(
        outstanding_demands(&pool, tenant, "coordinator_sql").await,
        1,
        "a rolled-back pass acknowledged demand it never planned"
    );

    remove_planning_failure(&pool, "forge_tasks").await;
    drive_scheduler_pass(&server, "restoration pass").await;
    assert!(
        readiness.is_ready(),
        "the pass after the forge_tasks fault was removed stayed unready"
    );
    assert_eq!(
        table_task_count(&pool, tenant, "coordinator_sql").await,
        1,
        "the recovered pass did not enqueue the task the failed pass rolled back"
    );
    assert_eq!(
        outstanding_demands(&pool, tenant, "coordinator_sql").await,
        0,
        "the recovered pass left demand unacknowledged"
    );

    stop.cancel();
    handle
        .await
        .expect("scheduler joins")
        .expect("pass loop ok");
    server.shutdown().await.expect("test server shuts down");
}

/// A coordinator whose unschedulable-task audit append fails rolls the task,
/// the audit, and the demand acknowledgement back together, and recovers on the
/// next complete pass.
///
/// The audit append only happens for a task planning classified unschedulable,
/// so the fixture drives a real promotion candidate against a byte ceiling it
/// cannot fit. The unfaulted table proves the append is genuinely reached before
/// a second, freshly registered table is failed at it.
///
/// # Panics
///
/// Panics when the unfaulted pass never appends, a failed pass publishes ready
/// or leaks a task, or the restored pass does not recover.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coordinator_audit_append_failure_clears_readiness() {
    // One byte is below every executable working set, so any real promotion
    // candidate this server plans is classified unschedulable and carries the
    // audit append the fault targets.
    let server = WyrdTestServer::builder()
        .with_forge_config_for_test(vala_bifrost_redux::forge::ForgeConfig {
            max_large_task_bytes: 1,
            ..vala_bifrost_redux::forge::ForgeConfig::default()
        })
        .start_bound()
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
        .await
        .expect("superuser pool");
    let tenant = uuid::Uuid::from(server.data_tenant_id());

    // A bound server already runs the production maintenance scheduler, so this
    // case drives that loop rather than composing a second one.

    // The unfaulted table proves the audit append is on the path at all.
    publish_unschedulable_demand(&server, "coordinator_audit_reached").await;
    drive_scheduler_pass(&server, "unfaulted unschedulable pass").await;
    assert!(
        readiness.is_ready(),
        "the unfaulted unschedulable pass did not complete"
    );
    assert_eq!(
        unschedulable_audits(&pool, tenant).await,
        1,
        "an unschedulable planning result did not append its audit"
    );

    publish_unschedulable_demand(&server, "coordinator_audit_faulted").await;
    install_planning_failure(
        &pool,
        "CREATE TRIGGER forge_fail_step BEFORE INSERT ON vala.audit_outbox \
         FOR EACH ROW EXECUTE FUNCTION vala.fail_forge_planning_step()",
    )
    .await;

    drive_scheduler_pass(&server, "faulted audit pass").await;
    assert!(
        !readiness.is_ready(),
        "a coordinator whose audit_outbox write failed advertised ready"
    );
    assert_eq!(
        table_task_count(&pool, tenant, "coordinator_audit_faulted").await,
        0,
        "the task rolled back with its audit was left behind"
    );
    assert_eq!(
        outstanding_demands(&pool, tenant, "coordinator_audit_faulted").await,
        1,
        "a rolled-back pass acknowledged demand whose audit never committed"
    );

    remove_planning_failure(&pool, "audit_outbox").await;
    drive_scheduler_pass(&server, "restoration pass").await;
    assert!(
        readiness.is_ready(),
        "the pass after the audit_outbox fault was removed stayed unready"
    );
    assert_eq!(
        table_task_count(&pool, tenant, "coordinator_audit_faulted").await,
        1,
        "the recovered pass did not enqueue the task the failed pass rolled back"
    );
    assert_eq!(
        outstanding_demands(&pool, tenant, "coordinator_audit_faulted").await,
        0,
        "the recovered pass left demand unacknowledged"
    );

    server.shutdown().await.expect("test server shuts down");
}

/// Registers one table and publishes rows through it so the next planning pass
/// derives a real Scribe-promotion candidate for it.
///
/// # Panics
///
/// Panics when registration, ingest, or the durable flush fails.
#[cfg(feature = "test-support")]
async fn publish_unschedulable_demand(server: &WyrdTestServer, table: &str) {
    register_forge_table(server, table).await;
    server
        .seed_bifrost_rows(&format!("vala.bifrost.{table}"), &[1, 2, 3])
        .await
        .expect("publish rows the coordinator can plan against");
}

/// Counts the tenant's committed unschedulable-task audit records.
///
/// This is the exact durable row the coordinator's planning transaction appends
/// beside an unschedulable task, so it distinguishes a pass that reached the
/// audit step from one that never planned a terminal task at all.
///
/// # Panics
///
/// Panics when the audit outbox cannot be read.
#[cfg(feature = "test-support")]
async fn unschedulable_audits(pool: &sqlx::PgPool, tenant: uuid::Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_outbox \
         WHERE data_tenant_id=$1 AND operation='forge.task.unschedulable'",
    )
    .bind(tenant)
    .fetch_one(pool)
    .await
    .expect("the audit outbox is readable")
}

/// Seeds one claimable Forge task row directly.
///
/// Both durable non-success outcomes under test are reachable only from a row
/// the planner would not produce today — a pre-envelope legacy row and a row
/// whose plan parameters do not match its strategy — so the row is written
/// directly rather than through an enqueue that would reject it. `envelope`
/// controls whether the version-two resource columns are present, and
/// `parameters` is the exact plan parameter object: the worker's pre-effect gate
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
    envelope: bool,
    parameters: &str,
) -> uuid::Uuid {
    let task_id = uuid::Uuid::now_v7();
    let statement = if envelope {
        "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,\
         table_name,strategy,lane,base_snapshot_id,plan,plan_hash,estimated_files,\
         estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,\
         large_task_ceiling_bytes,envelope_version,decoded_batch_bytes,decoded_input_bytes,\
         sort_working_bytes,sort_merge_reservation_bytes,encoder_buffer_bytes,\
         upload_chunk_bytes,footer_encoded_bytes,footer_decode_workspace_bytes,\
         sort_spill_bytes,state,ready_at,next_eligible_at,updated_at) \
         VALUES ($1,$2,'wyrd-redux','vala.bifrost',$3,$4,'ordinary',4242,\
         $5::jsonb,\
         decode(repeat('33',32),'hex'),1,100,1,41943040,50,67108864,2,10,10,30,10,40,20,\
         8388608,33554432,50,'ready',statement_timestamp()-interval '1 hour',\
         statement_timestamp()-interval '1 hour',statement_timestamp())"
    } else {
        "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,\
         table_name,strategy,lane,base_snapshot_id,plan,plan_hash,estimated_files,\
         estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,\
         large_task_ceiling_bytes,state,ready_at,next_eligible_at,updated_at) \
         VALUES ($1,$2,'wyrd-redux','vala.bifrost',$3,$4,'ordinary',4242,\
         $5::jsonb,\
         decode(repeat('33',32),'hex'),1,100,1,41943040,50,67108864,\
         'ready',statement_timestamp()-interval '1 hour',\
         statement_timestamp()-interval '1 hour',statement_timestamp())"
    };
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
async fn await_settled_task_state(pool: &sqlx::PgPool, task_id: uuid::Uuid) -> String {
    for _ in 0..600 {
        let state: String =
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(pool)
                .await
                .expect("the seeded task row is readable");
        if !matches!(
            state.as_str(),
            "ready" | "retryable" | "claimed" | "running"
        ) {
            return state;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the seeded Forge task never settled");
}

/// A claim that settles durably without producing an effect keeps the worker
/// ready and reports no completion.
///
/// Both a pre-envelope legacy row and a row whose payload does not match its
/// strategy are healthy worker outcomes: the worker terminalized durable state
/// and produced nothing. Readiness answers whether this replica can take the
/// next claim, so neither outcome may clear it.
///
/// # Panics
///
/// Panics when either row is reported as a completion or clears readiness.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn durably_settled_claims_keep_readiness() {
    for (table_name, strategy, envelope, expected) in [
        ("legacy_supersession", "small_files", false, "cancelled"),
        // Expired cleanup's exact work is its parameters, so a copied input set
        // is exactly what its payload contract forbids.
        ("malformed_payload", "expired_cleanup", true, "failed"),
    ] {
        let (server, observer) = server_with_forge_observer().await;
        let readiness = server
            .state()
            .forge()
            .expect("the default target selects Forge")
            .worker_readiness();
        let pool = server
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("superuser pool");
        let task_id = seed_ready_forge_task(
            &pool,
            uuid::Uuid::from(server.data_tenant_id()),
            table_name,
            strategy,
            envelope,
            "{\"version\":1,\"inputs\":[\"a.parquet\"],\"parameters\":{}}",
        )
        .await;

        let stop = tokio_util::sync::CancellationToken::new();
        let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone(), 1, 1)
            .expect("the worker composes");
        let handle = tokio::spawn(worker);
        await_forge_role(
            &server,
            true,
            wyrd_server::state::Forge::worker_readiness,
            table_name,
        )
        .await;

        let settled = await_settled_task_state(&pool, task_id).await;
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
        handle.await.expect("worker joins").expect("worker loop ok");
        server.shutdown().await.expect("test server shuts down");
    }
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
    let (server, _observer) = server_with_forge_observer().await;
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool");
    let task_id = seed_ready_forge_task(
        &pool,
        uuid::Uuid::from(server.data_tenant_id()),
        "foreign_claim",
        "small_files",
        true,
        "{\"version\":1,\"inputs\":[\"a.parquet\"],\"parameters\":{}}",
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
/// it. Readiness must clear before any sibling can claim behind that fence.
///
/// # Panics
///
/// Panics when the release failure is swallowed, readiness survives it, or the
/// respawned worker does not recover.
#[cfg(feature = "test-support")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn release_failure_clears_readiness() {
    let (server, observer) = server_with_forge_observer().await;
    let readiness = server
        .state()
        .forge()
        .expect("the default target selects Forge")
        .worker_readiness();
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool");

    let stop = tokio_util::sync::CancellationToken::new();
    let worker = wyrd_server::boot::spawn_forge_worker(server.state(), stop.clone(), 1, 1)
        .expect("the worker composes");
    let handle = tokio::spawn(worker);
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
    observer.fail_next_lease_release();

    // A well-formed payload on a table this catalog does not carry reaches the
    // table fence, settles its attempt durably, and then releases: exactly the
    // ordering this fault needs.
    let task_id = seed_ready_forge_task(
        &pool,
        uuid::Uuid::from(server.data_tenant_id()),
        "release_only",
        "small_files",
        true,
        // The live-rewrite parameter object is the exact contract the worker's
        // pre-effect gate requires, so this claim reaches the table fence
        // instead of being terminalized as malformed before one is acquired.
        "{\"version\":1,\"inputs\":[\"a.parquet\"],\"parameters\":{\"kind\":\"live_rewrite\"}}",
    )
    .await;

    let joined = tokio::time::timeout(FORGE_READINESS_CEILING, handle).await;
    stop.cancel();
    let error = match joined {
        Ok(outcome) => outcome
            .expect("worker joins")
            .expect_err("the injected release failure is returned"),
        Err(_) => {
            let state: String =
                sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                    .bind(task_id)
                    .fetch_one(&pool)
                    .await
                    .expect("the seeded task row is readable");
            panic!(
                "the faulted worker kept claiming instead of returning its release failure \
                 (task state: {state}, ready: {})",
                readiness.is_ready()
            )
        }
    };
    assert!(
        format!("{error}").contains("injected Forge table lease release failure"),
        "the worker returned an unrelated error: {error}"
    );
    assert!(
        !readiness.is_ready(),
        "a worker still holding an unreleased table fence advertised ready"
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
    // No sibling may claim behind a fence this owner never released.
    let claimed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.forge_tasks WHERE task_id=$1 AND state IN ('claimed','running')",
    )
    .bind(task_id)
    .fetch_one(&pool)
    .await
    .expect("the claim inventory is readable");
    assert_eq!(claimed, 0, "a sibling claimed behind the unreleased fence");

    await_forge_role_while_running(&server, "worker after its release recovers").await;
    server.shutdown().await.expect("test server shuts down");
}
