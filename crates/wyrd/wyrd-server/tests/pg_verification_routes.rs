//! Verification control-plane routes through the authenticated HTTP surface.
//!
//! Drives `GET /v1/verification/bindings/{id}`, `POST /v1/verification/runs`,
//! and `GET /v1/verification/runs/{id}` against the in-process server and the
//! repository-managed Postgres: durable enqueue with requester identity,
//! Idempotency-Key replay and conflict, every refusal before enqueue, the
//! audited allow and deny decisions, and tenant isolation. No runtime executes
//! the runs, so every enqueued run stays `pending`. It also proves the
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
        .await
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
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let rows = sqlx::query_as(
        "SELECT permission, outcome FROM vala.audit_staging \
          WHERE operation = 'verification.run.start' AND principal_id = $1 \
          ORDER BY outcome",
    )
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
        .await
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
