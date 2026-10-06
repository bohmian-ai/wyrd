//! Verification control-plane routes through the authenticated HTTP surface.
//!
//! Drives `GET /v1/verification/bindings/{id}`, `POST /v1/verification/runs`,
//! `GET /v1/verification/runs/{id}`, and `POST /v1/verification/execute` against the in-process server and the
//! repository-managed Postgres: durable enqueue with requester identity,
//! Idempotency-Key replay and conflict, every refusal before enqueue, the
//! audited allow and deny decisions, and tenant isolation. No runtime executes
//! the runs, so every enqueued run stays `pending`; direct executions judge
//! inline and never enqueue. It also proves the
//! tenant's internal SYSTEM result writer is unreachable through the public
//! principal and credential routes.

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use serde_json::{Value, json};
use uuid::Uuid;
use wyrd_spec::card::verifier::OWNER_OCCURRENCE_KEY;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{BindingId, CardUid, DataTenantId};
use wyrd_sql::queries::verification::{
    BindingActivation, FrozenTarget, NewBinding, project_bindings,
};
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// A ready Custom Drift Verifier implementation.
fn custom_drift() -> Value {
    json!({ "implementation": { "kind": "drift", "spec": { "method": "Custom" } } })
}

/// A PSI Drift Verifier implementation, never ready until a baseline exists.
fn psi_drift() -> Value {
    json!({ "implementation": { "kind": "drift", "spec": { "method": "Psi" } } })
}

/// A ready Eval Verifier implementation, which cannot analyze a Drift window.
fn eval() -> Value {
    json!({ "implementation": { "kind": "eval", "spec": { "tasks": {} } } })
}

/// Insert one active Card row directly and return its UID.
///
/// # Panics
/// Panics when the fixture superuser pool cannot open or the row fails to insert.
async fn seed_card(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    kind: &str,
    name: &str,
    spec: Value,
) -> CardUid {
    let uid = CardUid::from_uuid(Uuid::now_v7()).expect("fixture UID is valid");
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    sqlx::query(
        "INSERT INTO wyrd.cards \
            (card_uid, data_tenant_id, kind, space, name, version, spec, spec_hash, status) \
         VALUES ($1, $2, $3, 'default', $4, '1.0.0', $5, $4, 'active')",
    )
    .bind(uid.as_uuid())
    .bind(tenant.as_uuid())
    .bind(kind)
    .bind(name)
    .bind(spec)
    .execute(&pool)
    .await
    .expect("fixture card inserts");
    uid
}

/// Project one owner-level scheduled binding of `verifier` on the Service `owner`.
///
/// # Panics
/// Panics when the tenant connection, projection, or commit fails.
async fn bind(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    owner: &CardUid,
    verifier: &CardUid,
) -> BindingId {
    let mut conn = server
        .tenant_conn_for(tenant)
        .await
        .expect("tenant connection opens");
    let binding = project_bindings(
        &mut conn,
        owner,
        &CardKind::Service,
        &[NewBinding {
            subject_occurrence_key: OWNER_OCCURRENCE_KEY.to_owned(),
            subject_card_uid: owner.clone(),
            verifier_uid: verifier.clone(),
            trigger: FrozenTarget::Digest("sha256:trigger".to_owned()),
            operators: Vec::new(),
            activation: BindingActivation::Schedule {
                cron: "0 2 * * *".to_owned(),
                tz: None,
            },
        }],
    )
    .await
    .expect("binding projects")[0];
    conn.commit().await.expect("binding commits");
    binding
}

/// The tenant fixture: one Service with a ready and an unready binding, plus
/// the Custom and Eval Verifiers for direct targets.
struct Fixture {
    /// The Service owning both bindings and the subject of every run.
    service: CardUid,
    /// Ready Custom Drift Verifier.
    custom: CardUid,
    /// Ready Eval Verifier.
    eval: CardUid,
    /// Binding of the Custom Verifier.
    ready_binding: BindingId,
    /// Binding of a PSI Verifier with no fitted baseline.
    unready_binding: BindingId,
}

impl Fixture {
    /// Seed the fixture Cards and bindings in `tenant`.
    ///
    /// # Panics
    /// Panics when any fixture write fails.
    async fn seed(server: &WyrdTestServer, tenant: DataTenantId) -> Self {
        let service = seed_card(server, tenant, "Service", "vr-service", json!({})).await;
        let custom = seed_card(server, tenant, "Verifier", "vr-custom", custom_drift()).await;
        let psi = seed_card(server, tenant, "Verifier", "vr-psi", psi_drift()).await;
        let eval = seed_card(server, tenant, "Verifier", "vr-eval", eval()).await;
        let ready_binding = bind(server, tenant, &service, &custom).await;
        let unready_binding = bind(server, tenant, &service, &psi).await;
        Self {
            service,
            custom,
            eval,
            ready_binding,
            unready_binding,
        }
    }
}

/// A one-hour manual Drift window.
fn window() -> Value {
    json!({ "kind": "drift_window", "start": "2026-09-17T00:00:00Z", "end": "2026-09-17T01:00:00Z" })
}

/// A manual run body for `target` over [`window`].
fn run_body(target: Value) -> Value {
    json!({ "target": target, "input": window() })
}

/// A binding target.
fn binding_target(binding: BindingId) -> Value {
    json!({ "kind": "binding", "binding_id": binding })
}

/// POST one manual run request as `jwt`, optionally keyed.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn post_run(
    server: &WyrdTestServer,
    jwt: &str,
    body: &Value,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri("/v1/verification/runs")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(key) = key {
        request = request.header("Idempotency-Key", key);
    }
    let response = server
        .oneshot_authenticated(
            jwt,
            request
                .body(Body::from(body.to_string()))
                .expect("run request builds"),
        )
        .await
        .expect("run request responds");
    decoded(response).await
}

/// GET one path as `jwt`.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn get(server: &WyrdTestServer, jwt: &str, uri: &str) -> (StatusCode, Value) {
    let response = server
        .oneshot_authenticated(
            jwt,
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .body(Body::empty())
                .expect("read request builds"),
        )
        .await
        .expect("read request responds");
    decoded(response).await
}

/// Split a response into its status and JSON body.
///
/// # Panics
/// Panics when the body cannot be read or is not JSON.
async fn decoded(response: Response<Body>) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("response body reads");
    (
        status,
        serde_json::from_slice(&bytes).expect("response body is JSON"),
    )
}

/// Bootstrap a user holding `roles` and return its principal ID and access token.
///
/// # Panics
/// Panics when bootstrapping fails or returns a machine principal.
async fn user(server: &WyrdTestServer, name: &str, roles: &[&str]) -> (Uuid, String) {
    match server
        .bootstrap_user(name, roles)
        .await
        .expect("user bootstraps")
    {
        Bootstrap::User { id, jwt } => (id.as_uuid(), jwt),
        Bootstrap::Machine { .. } => panic!("user bootstrap returned a machine principal"),
    }
}

/// Exchange a bootstrapped machine principal's API key for an access token.
///
/// # Panics
/// Panics when the principal has no API key or the exchange fails.
async fn machine_jwt(server: &WyrdTestServer, machine: &Bootstrap) -> String {
    server
        .exchange_api_key(machine.api_key().expect("machine has an API key"))
        .await
        .expect("API key exchanges")
}

/// Count the tenant's runs.
///
/// # Panics
/// Panics when the count cannot be read.
async fn run_count(server: &WyrdTestServer, tenant: DataTenantId) -> i64 {
    let mut conn = server
        .tenant_conn_for(tenant)
        .await
        .expect("tenant connection opens");
    let count = sqlx::query_scalar("SELECT count(*) FROM wyrd.verifier_runs")
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("runs count");
    conn.commit().await.expect("count commits");
    count
}

/// Read one run's frozen (origin, owner, binding, requester) identity columns.
///
/// # Panics
/// Panics when the run cannot be read.
async fn run_identity(
    server: &WyrdTestServer,
    run_id: &str,
) -> (String, Option<Uuid>, Option<Uuid>, Option<Uuid>) {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let row = sqlx::query_as(
        "SELECT origin, owner_card_uid, binding_id, requested_by_principal_id \
           FROM wyrd.verifier_runs WHERE run_id = $1::uuid",
    )
    .bind(run_id)
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("run identity reads");
    conn.commit().await.expect("identity read commits");
    row
}

/// List `principal`'s `verification.run.start` decisions as (permission, outcome).
///
/// # Panics
/// Panics when the audit rows cannot be read.
async fn start_decisions(server: &WyrdTestServer, principal: Uuid) -> Vec<(String, String)> {
    decisions(server, "verification.run.start", principal).await
}

/// List `principal`'s `verification.execute` decisions as (permission,
/// outcome) once the non-blocking audit outbox has committed every staged
/// decision.
///
/// # Panics
/// Panics when the outbox does not drain within ten seconds or the audit rows
/// cannot be read.
async fn execute_decisions(server: &WyrdTestServer, principal: Uuid) -> Vec<(String, String)> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while server.state().audit_outbox.pending() != 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "audit outbox drains"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    decisions(server, "verification.execute", principal).await
}

/// List `principal`'s staged `operation` decisions as (permission, outcome).
///
/// # Panics
/// Panics when the audit rows cannot be read.
async fn decisions(
    server: &WyrdTestServer,
    operation: &str,
    principal: Uuid,
) -> Vec<(String, String)> {
    server
        .wait_oracle_audit_staged(std::time::Duration::from_secs(30))
        .await
        .expect("audit outbox settles");
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let rows = sqlx::query_as(
        "SELECT permission, outcome FROM vala.audit_staging \
          WHERE operation = $1 AND principal_id = $2 \
          ORDER BY outcome",
    )
    .bind(operation)
    .bind(principal)
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("audit rows read");
    conn.commit().await.expect("audit read commits");
    rows
}

/// Binding and direct manual runs enqueue with the caller as requester, never
/// as owner; a keyed retry replays its run and a key reused for a different
/// body is refused; binding and run status read back the frozen identities.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, identity, replay, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn manual_runs_enqueue_replay_and_read_back() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let tenant = server.data_tenant_id();
    let fixture = Fixture::seed(&server, tenant).await;
    let (requester, jwt) = user(&server, "vr-writer", &["writer"]).await;

    let binding_body = run_body(binding_target(fixture.ready_binding));
    let (status, bound) = post_run(&server, &jwt, &binding_body, Some("vr-retry-0001")).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{bound}");
    let bound_run = bound["run_id"]
        .as_str()
        .expect("run_id is a string")
        .to_owned();
    let (status, replay) = post_run(&server, &jwt, &binding_body, Some("vr-retry-0001")).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{replay}");
    assert_eq!(
        replay["run_id"], bound["run_id"],
        "same key and body replays"
    );
    let other_window = json!({
        "target": binding_target(fixture.ready_binding),
        "input": { "kind": "drift_window", "start": "2026-09-16T00:00:00Z", "end": "2026-09-16T01:00:00Z" }
    });
    let (status, conflict) = post_run(&server, &jwt, &other_window, Some("vr-retry-0001")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["code"], "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT");

    let direct_body = run_body(json!({
        "kind": "verifier", "verifier_uid": fixture.custom, "subject_card_uid": fixture.service
    }));
    let (status, direct) = post_run(&server, &jwt, &direct_body, None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{direct}");
    let direct_run = direct["run_id"]
        .as_str()
        .expect("run_id is a string")
        .to_owned();
    let (status, unkeyed) = post_run(&server, &jwt, &direct_body, None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_ne!(
        unkeyed["run_id"], direct["run_id"],
        "no key, no deduplication"
    );
    assert_eq!(run_count(&server, tenant).await, 3);

    assert_eq!(
        run_identity(&server, &bound_run).await,
        (
            "manual".to_owned(),
            Some(fixture.service.as_uuid()),
            Some(fixture.ready_binding.as_uuid()),
            Some(requester)
        )
    );
    assert_eq!(
        run_identity(&server, &direct_run).await,
        ("manual".to_owned(), None, None, Some(requester))
    );

    let (status, run) = get(&server, &jwt, &format!("/v1/verification/runs/{bound_run}")).await;
    assert_eq!(status, StatusCode::OK, "{run}");
    assert_eq!(run["status"], "pending");
    assert_eq!(run["requested_by_principal_id"], requester.to_string());
    assert_eq!(run["result_id"], Value::Null);
    assert_eq!(run["dispatches"], json!([]));
    let (status, binding) = get(
        &server,
        &jwt,
        &format!("/v1/verification/bindings/{}", fixture.ready_binding),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{binding}");
    assert_eq!(binding["owner_card_uid"], fixture.service.to_string());
    assert_eq!(binding["subject_card_uid"], fixture.service.to_string());
    assert_eq!(binding["verifier_uid"], fixture.custom.to_string());
    assert_eq!(binding["readiness"], "ready");
    assert_eq!(binding["active"], false);
    assert!(binding["last_run_id"].is_string());

    let decisions = start_decisions(&server, requester).await;
    assert_eq!(decisions.len(), 5, "one decision per request");
    assert!(
        decisions
            .iter()
            .all(|decision| decision == &("evals:run".to_owned(), "allowed".to_owned()))
    );
    server.shutdown().await.expect("test server shuts down");
}

/// Invalid bodies, windows, and targets, unknown and unready bindings, a
/// caller without `evals:run`, and a Card-bound caller without scope over the
/// subject are all refused with stable codes before any run exists; the two
/// authorization refusals each audit exactly one denial.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, code, run count, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn manual_run_refusals_fail_before_enqueue() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let tenant = server.data_tenant_id();
    let fixture = Fixture::seed(&server, tenant).await;
    let (_, jwt) = user(&server, "vr-refused-writer", &["writer"]).await;

    let cases = [
        (
            json!({ "target": binding_target(fixture.ready_binding),
                    "input": { "kind": "drift_window",
                               "start": "2026-09-17T01:00:00Z", "end": "2026-09-17T00:00:00Z" } }),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INVALID_WINDOW",
        ),
        (
            json!({ "target": binding_target(fixture.ready_binding),
                    "input": { "kind": "drift_window",
                               "start": "2026-01-01T00:00:00Z", "end": "2026-09-17T00:00:00Z" } }),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INVALID_WINDOW",
        ),
        (
            run_body(json!({ "kind": "binding", "binding_id": "not-a-uuid" })),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INVALID_TARGET",
        ),
        (
            json!({ "target": binding_target(fixture.ready_binding), "input": window(), "x": 1 }),
            StatusCode::BAD_REQUEST,
            "WYRD_SPEC_400_VALIDATION",
        ),
        (
            run_body(binding_target(BindingId::new_v7())),
            StatusCode::NOT_FOUND,
            "WYRD_VERIFICATION_404_BINDING_NOT_FOUND",
        ),
        (
            run_body(binding_target(fixture.unready_binding)),
            StatusCode::CONFLICT,
            "WYRD_VERIFICATION_409_VERIFIER_NOT_READY",
        ),
        (
            run_body(json!({ "kind": "verifier", "verifier_uid": fixture.eval,
                             "subject_card_uid": fixture.service })),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INVALID_TARGET",
        ),
        (
            run_body(json!({ "kind": "verifier", "verifier_uid": fixture.service,
                             "subject_card_uid": fixture.service })),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INVALID_TARGET",
        ),
        (
            run_body(json!({ "kind": "verifier", "verifier_uid": fixture.custom,
                             "subject_card_uid": Uuid::now_v7() })),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INVALID_TARGET",
        ),
    ];
    for (body, status, code) in cases {
        let (actual, problem) = post_run(&server, &jwt, &body, None).await;
        assert_eq!(
            (actual, problem["code"].as_str()),
            (status, Some(code)),
            "{body}"
        );
    }
    let (status, problem) = post_run(
        &server,
        &jwt,
        &run_body(binding_target(fixture.ready_binding)),
        Some("bad key"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(problem["code"], "WYRD_SPEC_400_VALIDATION");

    let (reader, reader_jwt) = user(&server, "vr-reader", &["reader"]).await;
    let bound = run_body(binding_target(fixture.ready_binding));
    let (status, problem) = post_run(&server, &reader_jwt, &bound, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(
        start_decisions(&server, reader).await,
        vec![("evals:run".to_owned(), "denied".to_owned())]
    );

    let machine = server
        .bootstrap_service("vr-foreign-service", &["writer"])
        .await
        .expect("card-bound service bootstraps");
    let machine_token = machine_jwt(&server, &machine).await;
    let (status, problem) = post_run(&server, &machine_token, &bound, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(
        start_decisions(&server, machine.id().as_uuid()).await,
        vec![("evals:run".to_owned(), "denied".to_owned())]
    );

    let (status, problem) = get(
        &server,
        &jwt,
        &format!("/v1/verification/runs/{}", Uuid::now_v7()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(problem["code"], "WYRD_VERIFICATION_404_RUN_NOT_FOUND");
    let (status, problem) = get(&server, &jwt, "/v1/verification/runs/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");
    assert_eq!(run_count(&server, tenant).await, 0);
    server.shutdown().await.expect("test server shuts down");
}

/// Binding and run status reads each authorize and audit `cards:read`: a
/// reader is allowed and stages one allowed decision per read, while a
/// principal whose only role lacks `cards:read` is refused with the stable
/// RBAC code and stages one denied decision per read, before any lookup.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, code, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn status_reads_audit_cards_read_decisions() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let fixture = Fixture::seed(&server, server.data_tenant_id()).await;
    let (_, writer) = user(&server, "vr-status-writer", &["writer"]).await;
    let (status, run) = post_run(
        &server,
        &writer,
        &run_body(binding_target(fixture.ready_binding)),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{run}");
    let binding_uri = format!("/v1/verification/bindings/{}", fixture.ready_binding);
    let run_uri = format!(
        "/v1/verification/runs/{}",
        run["run_id"].as_str().expect("run_id is a string")
    );

    let (reader, reader_jwt) = user(&server, "vr-status-reader", &["reader"]).await;
    for uri in [&binding_uri, &run_uri] {
        let (status, body) = get(&server, &reader_jwt, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
    let allowed = vec![("cards:read".to_owned(), "allowed".to_owned())];
    assert_eq!(
        decisions(&server, "verification.binding.read", reader).await,
        allowed
    );
    assert_eq!(
        decisions(&server, "verification.run.read", reader).await,
        allowed
    );

    let (outsider, outsider_jwt) = user(&server, "vr-status-outsider", &["runtime_admin"]).await;
    for uri in [&binding_uri, &run_uri] {
        let (status, problem) = get(&server, &outsider_jwt, uri).await;
        assert_eq!(
            (status, problem["code"].as_str()),
            (
                StatusCode::FORBIDDEN,
                Some("WYRD_PERMISSION_403_DENIED_RBAC")
            ),
            "{uri}"
        );
    }
    let denied = vec![("cards:read".to_owned(), "denied".to_owned())];
    assert_eq!(
        decisions(&server, "verification.binding.read", outsider).await,
        denied
    );
    assert_eq!(
        decisions(&server, "verification.run.read", outsider).await,
        denied
    );
    server.shutdown().await.expect("test server shuts down");
}

/// Another tenant's administrator cannot read or run this tenant's binding
/// or read its run: each is indistinguishable from an absent one.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, code, or run count expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn verification_state_is_tenant_isolated() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let tenant = server.data_tenant_id();
    let fixture = Fixture::seed(&server, tenant).await;
    let (_, jwt) = user(&server, "vr-home-writer", &["writer"]).await;
    let (status, run) = post_run(
        &server,
        &jwt,
        &run_body(binding_target(fixture.ready_binding)),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let other = server.seed_tenant("vr-other").await.expect("tenant seeds");
    let foreign = server
        .bootstrap_service_in_tenant(other, "vr-other-admin", &["admin"])
        .await
        .expect("foreign admin bootstraps");
    let foreign_jwt = machine_jwt(&server, &foreign).await;
    let (status, problem) = get(
        &server,
        &foreign_jwt,
        &format!("/v1/verification/bindings/{}", fixture.ready_binding),
    )
    .await;
    assert_eq!(
        (status, problem["code"].as_str()),
        (
            StatusCode::NOT_FOUND,
            Some("WYRD_VERIFICATION_404_BINDING_NOT_FOUND")
        )
    );
    let run_id = run["run_id"].as_str().expect("run_id is a string");
    let (status, problem) = get(
        &server,
        &foreign_jwt,
        &format!("/v1/verification/runs/{run_id}"),
    )
    .await;
    assert_eq!(
        (status, problem["code"].as_str()),
        (
            StatusCode::NOT_FOUND,
            Some("WYRD_VERIFICATION_404_RUN_NOT_FOUND")
        )
    );
    let (status, problem) = post_run(
        &server,
        &foreign_jwt,
        &run_body(binding_target(fixture.ready_binding)),
        None,
    )
    .await;
    assert_eq!(
        (status, problem["code"].as_str()),
        (
            StatusCode::NOT_FOUND,
            Some("WYRD_VERIFICATION_404_BINDING_NOT_FOUND")
        )
    );
    assert_eq!(run_count(&server, tenant).await, 1);
    assert_eq!(run_count(&server, other).await, 0);
    server.shutdown().await.expect("test server shuts down");
}

/// Send one request with an optional JSON body as `jwt`.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn send(
    server: &WyrdTestServer,
    jwt: &str,
    method: Method,
    uri: &str,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
        .expect("request builds");
    let response = server
        .oneshot_authenticated(jwt, request)
        .await
        .expect("request responds");
    decoded(response).await
}

/// Read the tenant's provisioned SYSTEM writer as (id, name, status, API-key count).
///
/// Reads through the superuser pool because every public tenant query omits
/// the SYSTEM row.
///
/// # Panics
/// Panics when the superuser pool cannot open or the tenant has no single
/// SYSTEM row.
async fn system_writer(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> (Uuid, String, String, i64) {
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    sqlx::query_as(
        "SELECT a.id, a.name, a.status, \
                (SELECT count(*) FROM wyrd.auth_api_keys k WHERE k.principal_id = a.id) \
           FROM wyrd.auth_service_accounts a \
          WHERE a.data_tenant_id = $1 AND a.principal_kind = 'system'",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("the tenant has exactly one SYSTEM writer")
}

/// The provisioned SYSTEM writer is unreachable through every public principal
/// route: a tenant administrator's credential list, credential (API-key)
/// issuance, credential revocation, and principal revocation for its id all
/// answer the non-enumerating principal-not-found refusal, and the row stays
/// active and credentialless. The public surface has no principal list, get,
/// or update route; list omission is pinned by the SQL principal listing.
///
/// # Panics
/// Panics when the server fails to start, a route fails to respond, any route
/// reaches the SYSTEM row, or the row changes.
#[tokio::test(flavor = "current_thread")]
async fn system_writer_is_unreachable_through_public_principal_routes() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let tenant = server.data_tenant_id();
    // The fixture tenant is created after migration backfill, so provision it
    // through the one idempotent owner tenant provisioning calls.
    let mut conn = server
        .tenant_conn_for(tenant)
        .await
        .expect("tenant connection opens");
    let provisioned = wyrd_sql::queries::auth::provision_system_principal(&mut conn)
        .await
        .expect("SYSTEM writer provisions");
    conn.commit().await.expect("provisioning commits");
    let (system, name, status, keys) = system_writer(&server, tenant).await;
    assert_eq!(system, provisioned);
    assert_eq!(
        (name.as_str(), status.as_str(), keys),
        ("verification-results-writer", "active", 0)
    );
    assert_eq!(system.get_version_num(), 7, "the SYSTEM id is UUIDv7");
    let (_, jwt) = user(&server, "vr-principal-admin", &["admin"]).await;
    let principal_missing = "WYRD_AUTH_404_PRINCIPAL_NOT_FOUND";
    let credentials = format!("/v1/principals/{system}/credentials");
    let revoke = format!("/v1/principals/{system}/revoke");
    let attempts = [
        (Method::GET, credentials.clone(), None, principal_missing),
        (Method::POST, credentials.clone(), None, principal_missing),
        (
            Method::DELETE,
            format!("{credentials}/{}", Uuid::now_v7()),
            None,
            "WYRD_SPEC_404_NOT_FOUND",
        ),
        (
            Method::POST,
            revoke.clone(),
            Some(json!({ "principal_kind": "system", "reason": "probe" })),
            principal_missing,
        ),
        (
            Method::POST,
            revoke,
            Some(json!({ "principal_kind": "service", "reason": "probe" })),
            principal_missing,
        ),
    ];
    for (method, uri, body, code) in attempts {
        let (status, problem) = send(&server, &jwt, method.clone(), &uri, body.as_ref()).await;
        assert_eq!(
            (status, problem["code"].as_str()),
            (StatusCode::NOT_FOUND, Some(code)),
            "{method} {uri} must not reach SYSTEM: {problem}"
        );
    }
    assert_eq!(
        system_writer(&server, tenant).await,
        (system, name, status, 0),
        "no public route changed or credentialed the SYSTEM writer"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// A PSI Drift Verifier spec over the numeric feature `score`.
fn psi_spec() -> Value {
    json!({ "implementation": { "kind": "drift", "spec": {
        "method": "Psi",
        "signal": { "kind": "Distribution",
                    "baseline_ref": { "kind": "Data", "name": "vx-psi-baseline", "version": "1.0.0" },
                    "features": ["score"] },
        "condition": { "kind": "Statistical" },
        "profile": { "kind": "Psi",
                     "binning_strategy": { "kind": "EqualWidth", "n_bins": 10 },
                     "threshold": { "kind": "Fixed", "value": 0.25 } }
    } } })
}

/// The serialized PSI fit of `score` uniformly spread over `0..1000`.
///
/// # Panics
/// Panics when the spec does not decode or the fit fails.
fn psi_fitted() -> Value {
    let spec: wyrd_spec::card::drift::DriftSpec =
        serde_json::from_value(psi_spec()["implementation"]["spec"].clone())
            .expect("PSI spec decodes");
    let values: Vec<f64> = (0..1000).map(f64::from).collect();
    let batch = arrow::record_batch::RecordBatch::try_from_iter([(
        "score",
        std::sync::Arc::new(arrow::array::Float64Array::from(values)) as arrow::array::ArrayRef,
    )])
    .expect("fit batch builds");
    serde_json::to_value(vala_drift::fit_baseline(&batch, &spec).expect("PSI fits"))
        .expect("fit serializes")
}

/// An Eval Verifier spec with one gated `is_not_null` assertion on `$.x`.
fn eval_assertion_spec() -> Value {
    json!({ "implementation": { "kind": "eval", "spec": {
        "pass_gate": { "kind": "all_pass" },
        "tasks": { "a": { "kind": "assertion", "id": "a", "context_path": "$.x",
                          "operator": "is_not_null", "expected": null } }
    } } })
}

/// A direct execution body judging `subject` with `verifier` over `input`.
fn execute_body(verifier: &CardUid, subject: &CardUid, input: Value) -> Value {
    json!({ "verifier_uid": verifier, "subject_card_uid": subject, "input": input })
}

/// `drift_samples` input of one numeric column.
fn samples(column: &str, values: &[f64]) -> Value {
    json!({ "kind": "drift_samples", "columns": { column: values } })
}

/// POST one raw direct execution body as `jwt`.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn post_execute(server: &WyrdTestServer, jwt: &str, body: String) -> (StatusCode, Value) {
    let response = server
        .oneshot_authenticated(
            jwt,
            Request::builder()
                .method(Method::POST)
                .uri("/v1/verification/execute")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("execute request builds"),
        )
        .await
        .expect("execute request responds");
    decoded(response).await
}

/// The direct execution targets of one tenant.
struct Direct {
    /// Subject Service every execution judges.
    service: CardUid,
    /// Custom Drift Verifier on `latency` with baseline 100 and threshold 10.
    custom: CardUid,
    /// PSI Verifier with a ready current-format baseline.
    psi: CardUid,
    /// PSI Verifier whose baseline was fitted before the format field.
    legacy: CardUid,
    /// PSI Verifier with no fitted baseline.
    unfitted: CardUid,
    /// Eval Verifier with one context assertion.
    eval: CardUid,
    /// Eval Verifier with a trace assertion direct execution cannot run.
    traced: CardUid,
}

impl Direct {
    /// Seed every direct target in the server's tenant.
    ///
    /// # Panics
    /// Panics when any fixture write fails.
    async fn seed(server: &WyrdTestServer) -> Self {
        let tenant = server.data_tenant_id();
        let fixture = server
            .verification_fixture()
            .await
            .expect("verification fixture provisions");
        let mut legacy_fit = psi_fitted();
        legacy_fit["Psi"]
            .as_object_mut()
            .expect("PSI fit is an object")
            .remove("format");
        Self {
            service: seed_card(server, tenant, "Service", "vx-service", json!({})).await,
            custom: fixture
                .custom_drift_verifier("vx-custom", "latency", 100.0, 10.0)
                .await
                .expect("Custom Verifier registers"),
            psi: fixture
                .fitted_verifier("vx-psi", &psi_spec(), &psi_fitted())
                .await
                .expect("fitted PSI Verifier registers"),
            legacy: fixture
                .fitted_verifier("vx-legacy", &psi_spec(), &legacy_fit)
                .await
                .expect("legacy PSI Verifier registers"),
            unfitted: fixture
                .verifier("vx-unfitted", &psi_spec())
                .await
                .expect("unfitted PSI Verifier registers"),
            eval: fixture
                .verifier("vx-eval", &eval_assertion_spec())
                .await
                .expect("Eval Verifier registers"),
            traced: fixture
                .verifier(
                    "vx-traced",
                    &json!({ "implementation": { "kind": "eval", "spec": { "tasks": {
                        "t": { "kind": "trace_assertion", "id": "t", "span_selector": "$.spans[0].name",
                               "operator": "equals", "expected": "x" } } } } }),
                )
                .await
                .expect("traced Eval Verifier registers"),
        }
    }
}

/// Direct execution judges Custom and PSI Drift samples and an Eval record
/// inline, returning the exact identities, verdict, counts, and detail;
/// unscoreable (null) samples are `inconclusive`. Nothing is enqueued and each
/// request audits exactly one allowed `evals:run` decision.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any verdict, identity, run count, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn direct_execution_judges_inline_without_runs() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let direct = Direct::seed(&server).await;
    let (caller, jwt) = user(&server, "vx-writer", &["writer"]).await;

    let spread: Vec<f64> = (0..1000).map(f64::from).collect();
    let skewed = vec![990.0; 500];
    let cases = [
        (
            direct.custom.clone(),
            samples("latency", &[101.0, 99.0]),
            "passed",
            "drift",
        ),
        (
            direct.custom.clone(),
            samples("latency", &[150.0]),
            "failed",
            "drift",
        ),
        (
            direct.psi.clone(),
            samples("score", &spread),
            "passed",
            "drift",
        ),
        (
            direct.psi.clone(),
            samples("score", &skewed),
            "failed",
            "drift",
        ),
        (
            direct.psi.clone(),
            json!({ "kind": "drift_samples", "columns": { "score": [1.0, null] } }),
            "inconclusive",
            "drift",
        ),
        (
            direct.eval.clone(),
            json!({ "kind": "eval_record", "context": { "x": 1 } }),
            "passed",
            "eval",
        ),
        (
            direct.eval.clone(),
            json!({ "kind": "eval_record", "context": { "x": null } }),
            "failed",
            "eval",
        ),
    ];
    let requests = cases.len();
    for (verifier, input, verdict, kind) in cases {
        let body = execute_body(&verifier, &direct.service, input);
        let (status, response) = post_execute(&server, &jwt, body.to_string()).await;
        assert_eq!(status, StatusCode::OK, "{body} -> {response}");
        assert_eq!(response["verdict"], verdict, "{body} -> {response}");
        assert_eq!(response["verifier"]["uid"], verifier.to_string());
        assert_eq!(response["subject"]["uid"], direct.service.to_string());
        assert_eq!(
            response["detail"].as_object().map(|d| d.contains_key(kind)),
            Some(true)
        );
        assert!(response["execution_id"].is_string());
        assert!(response["summary"].is_string());
    }
    assert_eq!(run_count(&server, server.data_tenant_id()).await, 0);
    let decisions = execute_decisions(&server, caller).await;
    assert_eq!(decisions.len(), requests, "one decision per request");
    assert!(
        decisions
            .iter()
            .all(|decision| decision == &("evals:run".to_owned(), "allowed".to_owned()))
    );
    server.shutdown().await.expect("test server shuts down");
}

/// Malformed, oversized, unknown, unready, legacy, incompatible, and
/// unsupported executions are refused with stable codes, and a caller without
/// `evals:run` or without scope over the subject is refused with one audited
/// denial; nothing is ever enqueued.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, a route fails
/// to respond, or any status, code, run count, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn direct_execution_refusals_are_stable() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let direct = Direct::seed(&server).await;
    let (_, jwt) = user(&server, "vx-refused-writer", &["writer"]).await;
    let service = &direct.service;
    let record = json!({ "kind": "eval_record", "context": { "x": 1 } });
    let wide: serde_json::Map<String, Value> =
        (0..65).map(|i| (format!("c{i}"), json!([1.0]))).collect();

    let cases = [
        (
            "{".to_owned(),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            json!({ "verifier_uid": direct.custom, "subject_card_uid": service,
                    "input": samples("latency", &[1.0]), "x": 1 })
            .to_string(),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            execute_body(
                &direct.custom,
                service,
                json!({ "kind": "drift_samples", "columns": { "latency": [1.0, "a"] } }),
            )
            .to_string(),
            StatusCode::BAD_REQUEST,
            "WYRD_VERIFICATION_400_INPUT_INVALID",
        ),
        (
            " ".repeat(1024 * 1024 + 1),
            StatusCode::PAYLOAD_TOO_LARGE,
            "WYRD_VERIFICATION_413_INPUT_TOO_LARGE",
        ),
        (
            execute_body(
                &direct.eval,
                service,
                json!({ "kind": "eval_record",
                "context": { "x": "a".repeat(256 * 1024) } }),
            )
            .to_string(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "WYRD_VERIFICATION_413_INPUT_TOO_LARGE",
        ),
        (
            execute_body(
                &direct.custom,
                service,
                json!({ "kind": "drift_samples", "columns": wide }),
            )
            .to_string(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "WYRD_VERIFICATION_413_INPUT_TOO_LARGE",
        ),
        (
            execute_body(
                &CardUid::from_uuid(Uuid::now_v7()).expect("UID is valid"),
                service,
                record.clone(),
            )
            .to_string(),
            StatusCode::NOT_FOUND,
            "WYRD_VERIFICATION_404_TARGET_NOT_FOUND",
        ),
        (
            execute_body(service, service, record.clone()).to_string(),
            StatusCode::NOT_FOUND,
            "WYRD_VERIFICATION_404_TARGET_NOT_FOUND",
        ),
        (
            execute_body(&direct.unfitted, service, samples("score", &[1.0])).to_string(),
            StatusCode::CONFLICT,
            "WYRD_VERIFICATION_409_BASELINE_NOT_READY",
        ),
        (
            execute_body(&direct.legacy, service, samples("score", &[1.0])).to_string(),
            StatusCode::CONFLICT,
            "WYRD_VERIFICATION_409_BASELINE_LEGACY",
        ),
        (
            execute_body(&direct.custom, service, record.clone()).to_string(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "WYRD_VERIFICATION_422_INPUT_INCOMPATIBLE",
        ),
        (
            execute_body(&direct.psi, service, samples("other", &[1.0])).to_string(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "WYRD_VERIFICATION_422_INPUT_INCOMPATIBLE",
        ),
        (
            execute_body(
                &direct.eval,
                service,
                json!({ "kind": "eval_record", "context": { "y": 1 } }),
            )
            .to_string(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "WYRD_VERIFICATION_422_INPUT_INCOMPATIBLE",
        ),
        (
            execute_body(&direct.traced, service, record.clone()).to_string(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "WYRD_VERIFICATION_422_INPUT_UNSUPPORTED",
        ),
    ];
    for (body, status, code) in cases {
        let (actual, problem) = post_execute(&server, &jwt, body.clone()).await;
        assert_eq!(
            (actual, problem["code"].as_str()),
            (status, Some(code)),
            "{} -> {problem}",
            &body[..body.len().min(200)]
        );
    }

    let allowed = execute_body(&direct.eval, service, record).to_string();
    let (reader, reader_jwt) = user(&server, "vx-reader", &["reader"]).await;
    let (status, problem) = post_execute(&server, &reader_jwt, allowed.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(
        execute_decisions(&server, reader).await,
        vec![("evals:run".to_owned(), "denied".to_owned())]
    );
    let machine = server
        .bootstrap_service("vx-foreign-service", &["writer"])
        .await
        .expect("card-bound service bootstraps");
    let machine_token = machine_jwt(&server, &machine).await;
    let (status, problem) = post_execute(&server, &machine_token, allowed).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(
        execute_decisions(&server, machine.id().as_uuid()).await,
        vec![("evals:run".to_owned(), "denied".to_owned())]
    );
    assert_eq!(run_count(&server, server.data_tenant_id()).await, 0);
    server.shutdown().await.expect("test server shuts down");
}

/// A direct execution whose decision cannot be audited still executes: audit
/// is non-blocking, so the allowed caller gets its judgment and the Card-bound
/// caller without subject scope gets its permission refusal. Neither decision
/// reaches staging while audit fails; both are retried and commit exactly once
/// when it recovers.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, the failure
/// trigger cannot be installed or dropped, a route fails to respond, or any
/// status, code, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn direct_execution_does_not_wait_on_audit() {
    let failures = wyrd_testing::AuditCommitFailures::install().expect("metrics recorder installs");
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let direct = Direct::seed(&server).await;
    let (caller, jwt) = user(&server, "vx-audit-writer", &["writer"]).await;
    let machine = server
        .bootstrap_service("vx-audit-foreign-service", &["writer"])
        .await
        .expect("card-bound service bootstraps");
    let machine_token = machine_jwt(&server, &machine).await;
    let superuser = server
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    for statement in [
        r"CREATE OR REPLACE FUNCTION vala.test_fail_execute_audit()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             IF NEW.operation = 'verification.execute' THEN
               RAISE EXCEPTION 'injected execute audit failure';
             END IF;
             RETURN NEW;
           END;
           $$;",
        r"CREATE TRIGGER test_fail_execute_audit
           BEFORE INSERT ON vala.audit_staging
           FOR EACH ROW EXECUTE FUNCTION vala.test_fail_execute_audit()",
    ] {
        sqlx::query(statement)
            .execute(&superuser)
            .await
            .expect("failure trigger installs");
    }

    let body = execute_body(
        &direct.eval,
        &direct.service,
        json!({ "kind": "eval_record", "context": { "x": 1 } }),
    )
    .to_string();
    let (status, judged) = post_execute(&server, &jwt, body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{judged}");
    assert_eq!(judged["verdict"], "passed", "{judged}");
    let (status, problem) = post_execute(&server, &machine_token, body).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    failures
        .await_failure(std::time::Duration::from_secs(30))
        .await
        .expect("the failed audit write is counted");
    let staged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging WHERE operation = 'verification.execute'",
    )
    .fetch_one(&superuser)
    .await
    .expect("staging reads");
    assert_eq!(staged, 0, "no decision is staged while audit fails");

    sqlx::query("DROP TRIGGER test_fail_execute_audit ON vala.audit_staging")
        .execute(&superuser)
        .await
        .expect("failure trigger drops");
    assert_eq!(
        execute_decisions(&server, caller).await,
        vec![("evals:run".to_owned(), "allowed".to_owned())],
        "the allowance commits exactly once after recovery"
    );
    assert_eq!(
        execute_decisions(&server, machine.id().as_uuid()).await,
        vec![("evals:run".to_owned(), "denied".to_owned())],
        "the denial commits exactly once after recovery"
    );
    server.shutdown().await.expect("test server shuts down");
}

/// An Eval Verifier whose one gated task asks an inline OpenAI judge to grade
/// `$.answer`, with no task-level retries.
///
/// # Panics
/// Panics when the judge prompt cannot be built.
fn judge_spec() -> Value {
    let prompt = skald_prompt::openai_chat(
        "gpt-test",
        skald_prompt::OpenAiChatOptions {
            messages: vec!["Grade the answer ${answer}.".to_owned()],
            variables: vec!["answer".to_owned()],
            output: Some(
                skald_prompt::ResponseFormat::json_schema(
                    "judge_result",
                    json!({
                        "type": "object",
                        "properties": { "passed": { "type": "boolean" } },
                        "required": ["passed"],
                        "additionalProperties": false
                    }),
                )
                .expect("the judge response format builds"),
            ),
            ..skald_prompt::OpenAiChatOptions::default()
        },
    )
    .expect("the judge prompt builds");
    json!({ "implementation": { "kind": "eval", "spec": {
        "pass_gate": { "kind": "all_pass" },
        "tasks": { "judge": {
            "kind": "llm_judge", "id": "judge",
            "judge_ref": { "prompt": prompt.into_native(), "tool_names": [],
                           "run_config": { "max_iterations": 1 } },
            "context_path": "$.answer", "operator": "equals",
            "expected": { "passed": true }, "max_retries": 0
        } }
    } } })
}

/// A direct Eval execution calls the configured judge provider and returns
/// its graded verdict; when the provider fails, the execution is refused with
/// the stable dependency code and no provider detail.
///
/// # Panics
/// Panics when the mock provider or server fails to start, a fixture write
/// fails, a route fails to respond, or any status, verdict, or code
/// expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn direct_execution_calls_the_judge_provider() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let provider = MockServer::start().await;
    let server = WyrdTestServer::builder()
        .with_gateway_provider_root_for_test(
            url::Url::parse(&provider.uri()).expect("mock URL parses"),
        )
        .start_in_process()
        .await
        .expect("test server starts");
    let service = seed_card(
        &server,
        server.data_tenant_id(),
        "Service",
        "vx-judged",
        json!({}),
    )
    .await;
    let judge = server
        .verification_fixture()
        .await
        .expect("verification fixture provisions")
        .verifier("vx-judge", &judge_spec())
        .await
        .expect("judge Verifier registers");
    let (_, jwt) = user(&server, "vx-judge-writer", &["writer"]).await;
    let body = execute_body(
        &judge,
        &service,
        json!({ "kind": "eval_record", "context": { "answer": "yes" } }),
    )
    .to_string();

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_direct", "object": "chat.completion", "created": 1_700_000_000,
            "model": "gpt-test",
            "choices": [{ "index": 0, "finish_reason": "stop",
                "message": { "role": "assistant", "content": "{\"passed\":true}" } }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8 }
        })))
        .expect(1)
        .mount(&provider)
        .await;
    let (status, response) = post_execute(&server, &jwt, body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["verdict"], "passed", "{response}");
    provider.verify().await;

    provider.reset().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "error": { "message": "provider-secret-detail", "type": "server_error" }
        })))
        .mount(&provider)
        .await;
    let (status, problem) = post_execute(&server, &jwt, body).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{problem}");
    assert_eq!(problem["code"], "WYRD_VERIFICATION_502_DEPENDENCY_FAILED");
    assert!(!problem.to_string().contains("provider-secret-detail"));
    server.shutdown().await.expect("test server shuts down");
}
