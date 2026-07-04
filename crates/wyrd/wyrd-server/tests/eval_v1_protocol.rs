//! Integration coverage for the native `/v1/eval/*` pull-protocol surface.
//!
//! Acceptance gates (commit 02):
//! - (a) full run lifecycle under one JWT
//! - (b) 401 on all four routes with no token
//! - (c) cross-tenant `eval_ref` with space-name collision → 404 (proves RLS)
//! - (d) cross-tenant `dataset` → denied before any bytes are read
//! - (e) cross-tenant judge/prompt `CardRef` → denied by the tenant-bound
//!   resolver, no provider call
//! - (f) foreign `run_id` → 404, not 403 (existence-oracle guard)
//! - (g) per-tenant concurrency cap counting (unit-tested in `eval::state`)
//! - (h) 5xx responses carry a generic message + empty details
//! - (i) run open/complete emits an audit event
//! - RBAC gate: in-tenant principal lacking `evals:run` → denied at open;
//!   principal holding `evals:run` opens successfully (tightened past card_write)

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use chrono::Duration;
use serde_json::{Value, json};
use sqlx::PgPool;
use tempfile::TempDir;
use tower::ServiceExt;
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_verify::{
    Kid, PrincipalKindWire, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings,
    public_key_from_pem,
};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::PrincipalId;
use wyrd_runtime::RoleRef;
use wyrd_semver::VersionBlock;
use wyrd_server::auth::ServerAuth;
use wyrd_server::auth::permission_resolver::SqlPermissionResolver;
use wyrd_server::auth::seed::seed_builtin_roles_for_tenant;
use wyrd_server::eval::resolver::{resolve_card_for_tenant, scenario_object_path};
use wyrd_server::eval::{EvalAuditEvent, EvalAuditKind, EvalAuditWriter};
use wyrd_server::postgres::ServerPostgres;
use wyrd_server::{AppState, build_router};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::data::{
    DataInterface, DataSchema, DataSpec, DataStats, PandasMeta, ParquetCompression,
};
use wyrd_spec::card::eval::EvalSpec;
use wyrd_spec::card::field::FieldSpec;
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::ids::{CardName, CardUid, ColumnName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::spec::DatasetRef;
use wyrd_spec::vala::eval::{EvalScenario, EvalScenarioCollection, ScenarioId, ScenarioTask};
use wyrd_sql::TenantConn;
use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle, tenant_path};

const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

// --------------------------------------------------------------------------
// Harness
// --------------------------------------------------------------------------

fn build_state(fixture: &PgFixture) -> (AppState, TempDir) {
    let postgres = Arc::new(ServerPostgres::from_parts(
        fixture.wyrd_postgres().clone(),
        fixture.vala_postgres().clone(),
    ));
    let root = tempfile::tempdir().expect("temp dir");
    let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
    let issuing_key = Arc::new(
        IssuingKey::from_ed_pem(
            secrecy::SecretString::from(PRIVATE_KEY_PEM),
            Kid::new("k1").expect("kid is valid"),
            "wyrd",
        )
        .expect("test issuing key loads"),
    );
    let mut keys = std::collections::HashMap::new();
    keys.insert(
        Kid::new("k1").expect("kid is valid"),
        Arc::new(public_key_from_pem(PUBLIC_KEY_PEM).expect("public key loads")),
    );
    let verifier = Arc::new(TokenVerifier::new(
        keys,
        "wyrd",
        Arc::new(SqlPermissionResolver::new(Arc::new(
            fixture.app_pool().clone(),
        ))),
        WyrdAuthVerifySettings::default(),
    ));
    let state = AppState::new(
        postgres,
        Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
    )
    .with_auth(ServerAuth {
        issuing_key: Some(issuing_key),
        token_verifier: Some(verifier),
        ..ServerAuth::default()
    });
    (state, root)
}

fn mint_jwt(state: &AppState, tenant: DataTenantId, roles: &[&str]) -> String {
    let role_refs = roles
        .iter()
        .map(|role| RoleRef::new(role).expect("valid role"))
        .collect::<Vec<RoleRef>>();
    state
        .auth
        .issuing_key
        .as_ref()
        .expect("issuing key")
        .issue_user_access_token(
            TokenPrincipalRef {
                id: PrincipalId::new(uuid::Uuid::now_v7()),
                kind: PrincipalKindWire::User,
                tenant_id: tenant,
                card_ref: None,
                card_ref_scope: Default::default(),
            },
            role_refs,
            Duration::minutes(15),
        )
        .expect("jwt mints")
}

async fn seed_roles(pool: &PgPool, tenant: DataTenantId) {
    let mut conn = TenantConn::acquire(pool, tenant)
        .await
        .expect("tenant conn");
    seed_builtin_roles_for_tenant(&mut conn, tenant)
        .await
        .expect("seed builtin roles");
    conn.commit().await.expect("commit roles");
}

async fn seed_tenant(admin_pool: &PgPool, tenant: DataTenantId, slug: &str) {
    sqlx::query(
        "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status) \
         VALUES ($1, $2, $3, 'active')",
    )
    .bind(tenant.as_uuid())
    .bind(slug)
    .bind(slug)
    .execute(admin_pool)
    .await
    .expect("seed tenant");
}

#[allow(clippy::too_many_arguments)]
async fn insert_card(
    pool: &PgPool,
    tenant: DataTenantId,
    uid: CardUid,
    kind: CardKind,
    space: &str,
    name: &str,
    version: &str,
    spec: &Spec,
) {
    let mut conn = TenantConn::acquire(pool, tenant)
        .await
        .expect("tenant conn");
    let spec_json = serde_json::to_value(spec).expect("spec serializes");
    sqlx::query(
        "INSERT INTO wyrd.cards \
         (card_uid, data_tenant_id, kind, space, name, version, spec, spec_hash, status) \
         VALUES ($1, wyrd.current_tenant(), $2, $3, $4, $5, $6, $7, 'active')",
    )
    .bind(uid.as_uuid())
    .bind(kind.wire_name())
    .bind(space)
    .bind(name)
    .bind(version)
    .bind(spec_json)
    .bind("0".repeat(64))
    .execute(&mut **conn.transaction())
    .await
    .expect("insert card");
    conn.commit().await.expect("commit card");
}

async fn insert_custom_role(pool: &PgPool, tenant: DataTenantId, name: &str, permissions: Value) {
    let mut conn = TenantConn::acquire(pool, tenant)
        .await
        .expect("tenant conn");
    sqlx::query(
        "INSERT INTO wyrd.auth_roles (id, data_tenant_id, name, permissions, builtin) \
         VALUES ($1, $2, $3, $4, FALSE)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(tenant.as_uuid())
    .bind(name)
    .bind(permissions)
    .execute(&mut **conn.transaction())
    .await
    .expect("insert custom role");
    conn.commit().await.expect("commit custom role");
}

fn card_ref(kind: CardKind, space: &str, name: &str) -> CardRef {
    CardRef {
        kind,
        name: CardName::new(name).expect("card name"),
        version: VersionBlock::parse("1.0.0").expect("version"),
        space: SpaceName::new(space).expect("space"),
        uid: None,
    }
}

fn eval_spec_with_dataset(data_ref: CardRef) -> Spec {
    let mut spec = EvalSpec::new(BTreeMap::new()).expect("eval spec");
    spec.dataset = Some(DatasetRef::new(data_ref).expect("dataset ref"));
    Spec::Eval(spec)
}

fn minimal_data_spec() -> Spec {
    Spec::Data(DataSpec {
        interface: DataInterface::Pandas(PandasMeta {
            framework_version: "2.2.2".to_owned(),
            compression: ParquetCompression::Snappy,
        }),
        schema: DataSchema::new(vec![FieldSpec::new(
            ColumnName::new("x").expect("column"),
            "int64",
        )]),
        card_refs: vec![],
        splits: Default::default(),
        target_columns: vec![],
        sql: None,
        stats: DataStats {
            row_count: Some(1),
            col_count: Some(1),
            byte_count: 1,
            sha256: "a".repeat(64),
        },
    })
}

fn scenario() -> EvalScenario {
    EvalScenario {
        id: ScenarioId::new("happy_path").expect("scenario id"),
        initial_query: "Start".to_owned(),
        expected_outcome: Some("Agent says DONE.".to_owned()),
        predefined_turns: vec!["Continue".to_owned()],
        simulated_user_persona: None,
        termination_signal: Some("DONE".to_owned()),
        max_turns: 2,
        tasks: Vec::<ScenarioTask>::new(),
    }
}

async fn upload_scenarios(storage: &StorageHandle, tenant: DataTenantId, data_uid: &CardUid) {
    let collection = EvalScenarioCollection {
        collection_id: "collection-1".to_owned(),
        scenarios: vec![scenario()],
    };
    let path = scenario_object_path(tenant, &data_uid.as_uuid().to_string());
    let validated = tenant_path::validate(&path, tenant).expect("scenario path valid");
    storage
        .put_object(
            &validated,
            serde_json::to_vec(&collection).expect("collection bytes"),
        )
        .await
        .expect("upload scenarios");
}

/// Seed a complete, self-consistent eval (Eval card + Data card + scenario
/// object) in `tenant`, returning the `eval_ref` a client would open against.
async fn seed_eval(state: &AppState, tenant: DataTenantId, space: &str, name: &str) -> CardRef {
    let data_uid = CardUid::new(uuid::Uuid::now_v7()).expect("data uid");
    let data_ref = card_ref(CardKind::Data, space, &format!("{name}-data"));
    insert_card(
        state.postgres.app_pool(),
        tenant,
        data_uid.clone(),
        CardKind::Data,
        space,
        &format!("{name}-data"),
        "1.0.0",
        &minimal_data_spec(),
    )
    .await;
    upload_scenarios(&state.storage, tenant, &data_uid).await;

    let eval_uid = CardUid::new(uuid::Uuid::now_v7()).expect("eval uid");
    insert_card(
        state.postgres.app_pool(),
        tenant,
        eval_uid,
        CardKind::Eval,
        space,
        name,
        "1.0.0",
        &eval_spec_with_dataset(data_ref),
    )
    .await;
    card_ref(CardKind::Eval, space, name)
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    lease: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header("x-wyrd-access-token", format!("Bearer {token}"));
    }
    if let Some(lease) = lease {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {lease}"));
    }
    let response = app
        .clone()
        .oneshot(
            builder
                .body(Body::from(body.to_string()))
                .expect("request builds"),
        )
        .await
        .expect("router responds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    (status, value)
}

async fn open_body(eval_ref: &CardRef) -> Value {
    json!({ "eval_ref": eval_ref, "simulated_user": "client" })
}

/// Issue a request with a valid JWT but a caller-controlled raw `Authorization`
/// header, so the lease scheme (non-`Bearer`) can be exercised — the standard
/// `call` helper always prefixes `Bearer`.
async fn call_with_raw_lease(
    app: &Router,
    uri: &str,
    token: &str,
    authorization: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-wyrd-access-token", format!("Bearer {token}"))
                .header(header::AUTHORIZATION, authorization)
                .body(Body::from(body.to_string()))
                .expect("request builds"),
        )
        .await
        .expect("router responds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    (status, value)
}

// --------------------------------------------------------------------------
// (b) Auth required on all four routes
// --------------------------------------------------------------------------

#[tokio::test]
async fn unauthenticated_request_to_each_route_returns_401() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let (state, _root) = build_state(&fixture);
    let app = build_router(state);
    let run = uuid::Uuid::now_v7();

    let routes = [
        ("POST", "/v1/eval/runs".to_owned()),
        ("POST", format!("/v1/eval/runs/{run}/next")),
        ("POST", format!("/v1/eval/runs/{run}/agent-turn")),
        ("POST", format!("/v1/eval/runs/{run}/user-turn")),
    ];
    for (method, uri) in routes {
        let (status, body) = call(&app, method, &uri, None, None, json!({})).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "route {uri} must require auth"
        );
        assert_eq!(body["code"], "WYRD_AUTH_401_UNAUTHENTICATED", "route {uri}");
    }
}

// --------------------------------------------------------------------------
// (a) Full lifecycle + (i) audit
// --------------------------------------------------------------------------

#[derive(Default)]
struct RecordingAudit {
    events: Mutex<Vec<EvalAuditEvent>>,
}

impl EvalAuditWriter for RecordingAudit {
    fn record(&self, event: &EvalAuditEvent) {
        self.events.lock().expect("audit lock").push(event.clone());
    }
}

#[tokio::test]
async fn full_lifecycle_and_audit_under_one_jwt() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    seed_roles(fixture.app_pool(), tenant).await;
    let (state, _root) = build_state(&fixture);
    let audit = Arc::new(RecordingAudit::default());
    let state = state.with_eval_audit(audit.clone());

    let eval_ref = seed_eval(&state, tenant, "prod", "rubric").await;
    let jwt = mint_jwt(&state, tenant, &["writer"]);
    let app = build_router(state);

    // open
    let (status, body) = call(
        &app,
        "POST",
        "/v1/eval/runs",
        Some(&jwt),
        None,
        open_body(&eval_ref).await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "open: {body}");
    let run_id = body["run_id"].as_str().expect("run_id").to_owned();
    let lease = body["lease_token"].as_str().expect("lease").to_owned();
    let base = format!("/v1/eval/runs/{run_id}");

    // Two agent turns ("ack" then "DONE"), then scenario + run complete.
    for response in ["ack", "DONE"] {
        let (status, directive) = call(
            &app,
            "POST",
            &format!("{base}/next"),
            Some(&jwt),
            Some(&lease),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "next: {directive}");
        assert_eq!(
            directive["kind"], "agent_turn",
            "expected agent_turn: {directive}"
        );
        let submission = json!({
            "scenario_id": directive["scenario_id"],
            "turn": directive["turn"],
            "response": response,
            "records": [],
        });
        let (status, _) = call(
            &app,
            "POST",
            &format!("{base}/agent-turn"),
            Some(&jwt),
            Some(&lease),
            submission,
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
    }

    let (status, directive) = call(
        &app,
        "POST",
        &format!("{base}/next"),
        Some(&jwt),
        Some(&lease),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        directive["kind"], "scenario_complete",
        "expected ScenarioComplete: {directive}"
    );

    let (status, directive) = call(
        &app,
        "POST",
        &format!("{base}/next"),
        Some(&jwt),
        Some(&lease),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        directive["kind"], "run_complete",
        "expected RunComplete: {directive}"
    );

    // (i) audit: open + complete recorded with the eval_ref and run_id.
    let events = audit.events.lock().expect("audit lock");
    assert_eq!(events.len(), 2, "open + complete audited");
    assert_eq!(events[0].kind, EvalAuditKind::RunOpen);
    assert_eq!(events[1].kind, EvalAuditKind::RunComplete);
    assert_eq!(events[0].tenant, tenant);
    assert_eq!(events[0].run_id.as_str(), run_id);
    assert_eq!(events[0].eval_ref.name.as_str(), "rubric");
}

// --------------------------------------------------------------------------
// (c) Cross-tenant eval_ref with space-name collision → 404
// --------------------------------------------------------------------------

#[tokio::test]
async fn cross_tenant_eval_ref_space_collision_returns_404() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let tenant_a = fixture.data_tenant_id();
    seed_roles(fixture.app_pool(), tenant_a).await;
    let (state, _root) = build_state(&fixture);

    let tenant_b = DataTenantId::new_v7();
    seed_tenant(fixture.platform_admin_pool(), tenant_b, "tenant-b").await;
    // The eval only exists in tenant B, at a colliding space/name.
    let eval_ref = seed_eval(&state, tenant_b, "shared", "collide").await;

    let jwt = mint_jwt(&state, tenant_a, &["writer"]);
    let app = build_router(state);
    let (status, body) = call(
        &app,
        "POST",
        "/v1/eval/runs",
        Some(&jwt),
        None,
        open_body(&eval_ref).await,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "RLS must hide tenant B's card: {body}"
    );
    assert_eq!(body["code"], "WYRD_EVAL_404_RUN_NOT_FOUND");
}

// --------------------------------------------------------------------------
// (d) Cross-tenant dataset → denied before bytes are read
// --------------------------------------------------------------------------

#[tokio::test]
async fn cross_tenant_dataset_denied_before_bytes() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let tenant_a = fixture.data_tenant_id();
    seed_roles(fixture.app_pool(), tenant_a).await;
    let (state, _root) = build_state(&fixture);

    let tenant_b = DataTenantId::new_v7();
    seed_tenant(fixture.platform_admin_pool(), tenant_b, "tenant-b").await;

    // Data card lives only in tenant B (colliding identity).
    let data_uid = CardUid::new(uuid::Uuid::now_v7()).expect("uid");
    insert_card(
        state.postgres.app_pool(),
        tenant_b,
        data_uid,
        CardKind::Data,
        "shared",
        "dataset",
        "1.0.0",
        &minimal_data_spec(),
    )
    .await;
    // Eval card in tenant A points its dataset at the tenant-B Data card.
    let eval_uid = CardUid::new(uuid::Uuid::now_v7()).expect("uid");
    let dataset_ref = card_ref(CardKind::Data, "shared", "dataset");
    insert_card(
        state.postgres.app_pool(),
        tenant_a,
        eval_uid,
        CardKind::Eval,
        "prod",
        "rubric",
        "1.0.0",
        &eval_spec_with_dataset(dataset_ref),
    )
    .await;

    let jwt = mint_jwt(&state, tenant_a, &["writer"]);
    let app = build_router(state);
    let (status, body) = call(
        &app,
        "POST",
        "/v1/eval/runs",
        Some(&jwt),
        None,
        open_body(&card_ref(CardKind::Eval, "prod", "rubric")).await,
    )
    .await;

    // 404 (dataset resolution), not 500 — proves denial happened at the Data
    // card RLS hop, before any object bytes were read.
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "dataset must be denied: {body}"
    );
    assert_eq!(body["code"], "WYRD_EVAL_404_RUN_NOT_FOUND");
}

// --------------------------------------------------------------------------
// (e) Cross-tenant judge/prompt CardRef → denied by the tenant-bound resolver
// --------------------------------------------------------------------------

#[tokio::test]
async fn cross_tenant_judge_ref_denied_no_provider_call() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let tenant_a = fixture.data_tenant_id();
    let (state, _root) = build_state(&fixture);

    let tenant_b = DataTenantId::new_v7();
    seed_tenant(fixture.platform_admin_pool(), tenant_b, "tenant-b").await;
    // A Prompt (judge) card exists only in tenant B.
    let prompt_uid = CardUid::new(uuid::Uuid::now_v7()).expect("uid");
    let prompt_spec = Spec::from_kind_and_value(
        &CardKind::Prompt,
        json!({ "provider": "openai", "model": "gpt-4o-mini", "messages": "judge" }),
    )
    .expect("prompt spec");
    insert_card(
        state.postgres.app_pool(),
        tenant_b,
        prompt_uid,
        CardKind::Prompt,
        "shared",
        "judge",
        "1.0.0",
        &prompt_spec,
    )
    .await;

    let judge_ref = card_ref(CardKind::Prompt, "shared", "judge");

    // The judge path (whenever server-side scoring wires) resolves through the
    // same tenant-bound resolver. Under tenant A the tenant-B judge is invisible;
    // no provider is ever constructed, so no provider call is possible.
    let denied = resolve_card_for_tenant(
        state.postgres.app_pool(),
        tenant_a,
        CardKind::Prompt,
        &judge_ref,
    )
    .await;
    assert!(
        denied.is_err(),
        "tenant A must not resolve tenant B's judge"
    );
    assert_eq!(denied.unwrap_err().status(), 404);

    // Sanity: the same ref resolves under its own tenant.
    let allowed = resolve_card_for_tenant(
        state.postgres.app_pool(),
        tenant_b,
        CardKind::Prompt,
        &judge_ref,
    )
    .await;
    assert!(allowed.is_ok(), "tenant B owns the judge card");
}

// --------------------------------------------------------------------------
// (f) Foreign / cross-owner run_id → 404, not 403
// --------------------------------------------------------------------------

#[tokio::test]
async fn foreign_run_id_returns_404_not_403() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    seed_roles(fixture.app_pool(), tenant).await;
    let (state, _root) = build_state(&fixture);
    let eval_ref = seed_eval(&state, tenant, "prod", "rubric").await;

    // Two distinct principals in the same tenant, minted before the router
    // consumes state.
    let opener = mint_jwt(&state, tenant, &["writer"]);
    let other = mint_jwt(&state, tenant, &["writer"]);
    let app = build_router(state);

    // Open a real run.
    let (status, body) = call(
        &app,
        "POST",
        "/v1/eval/runs",
        Some(&opener),
        None,
        open_body(&eval_ref).await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "open: {body}");
    let real_run_id = body["run_id"].as_str().expect("run_id").to_owned();
    let real_lease = body["lease_token"].as_str().expect("lease").to_owned();

    // A nonexistent run_id (valid JWT + a lease) → 404, never a 403 oracle.
    let stranger = uuid::Uuid::now_v7();
    let (status, body) = call(
        &app,
        "POST",
        &format!("/v1/eval/runs/{stranger}/next"),
        Some(&opener),
        Some(&real_lease),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "WYRD_EVAL_404_RUN_NOT_FOUND");

    // A second principal in the same tenant probing the real run → 404, not 403:
    // owner-mismatch must not leak run existence.
    let (status, body) = call(
        &app,
        "POST",
        &format!("/v1/eval/runs/{real_run_id}/next"),
        Some(&other),
        Some(&real_lease),
        json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "owner mismatch must be 404: {body}"
    );
    assert_eq!(body["code"], "WYRD_EVAL_404_RUN_NOT_FOUND");
}

// --------------------------------------------------------------------------
// (h) 5xx responses carry a generic message + empty details
// --------------------------------------------------------------------------

#[tokio::test]
async fn internal_failure_scrubs_5xx_body() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    seed_roles(fixture.app_pool(), tenant).await;
    let (state, _root) = build_state(&fixture);

    // Eval + Data cards exist, but no scenario object was uploaded, so the
    // object fetch fails after the (successful) RLS hops.
    let data_uid = CardUid::new(uuid::Uuid::now_v7()).expect("uid");
    let data_ref = card_ref(CardKind::Data, "prod", "rubric-data");
    insert_card(
        state.postgres.app_pool(),
        tenant,
        data_uid,
        CardKind::Data,
        "prod",
        "rubric-data",
        "1.0.0",
        &minimal_data_spec(),
    )
    .await;
    let eval_uid = CardUid::new(uuid::Uuid::now_v7()).expect("uid");
    insert_card(
        state.postgres.app_pool(),
        tenant,
        eval_uid,
        CardKind::Eval,
        "prod",
        "rubric",
        "1.0.0",
        &eval_spec_with_dataset(data_ref),
    )
    .await;

    let jwt = mint_jwt(&state, tenant, &["writer"]);
    let app = build_router(state);
    let (status, body) = call(
        &app,
        "POST",
        "/v1/eval/runs",
        Some(&jwt),
        None,
        open_body(&card_ref(CardKind::Eval, "prod", "rubric")).await,
    )
    .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(body["code"], "WYRD_EVAL_500_RUN_FAILED");
    assert_eq!(body["detail"], "eval run failed", "message must be generic");
    assert_eq!(body["details"], json!({}), "details must be empty");
}

// --------------------------------------------------------------------------
// RBAC gate: eval run creation requires evals:run (tightened past card_write)
// --------------------------------------------------------------------------

#[tokio::test]
async fn open_requires_eval_run_permission() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    seed_roles(fixture.app_pool(), tenant).await;
    let (state, _root) = build_state(&fixture);
    let eval_ref = seed_eval(&state, tenant, "prod", "rubric").await;

    // In-tenant custom role holding cards:write but NOT evals:run. It would have
    // passed the interim card_write gate, so denying it proves the gate tightened.
    insert_custom_role(
        fixture.app_pool(),
        tenant,
        "card_only",
        json!([{ "resource": "cards", "action": "write" }]),
    )
    .await;

    // Both principals resolve the Eval card (RLS is tenant-scoped); only the one
    // holding evals:run may open a run.
    let card_only_jwt = mint_jwt(&state, tenant, &["card_only"]);
    let writer_jwt = mint_jwt(&state, tenant, &["writer"]);
    let app = build_router(state);

    let (denied_status, denied_body) = call(
        &app,
        "POST",
        "/v1/eval/runs",
        Some(&card_only_jwt),
        None,
        open_body(&eval_ref).await,
    )
    .await;
    assert_eq!(denied_status, StatusCode::FORBIDDEN, "{denied_body}");
    assert_eq!(denied_body["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    assert_eq!(denied_body["details"]["required"], "evals:run");

    let (allowed_status, allowed_body) = call(
        &app,
        "POST",
        "/v1/eval/runs",
        Some(&writer_jwt),
        None,
        open_body(&eval_ref).await,
    )
    .await;
    assert_eq!(allowed_status, StatusCode::OK, "{allowed_body}");
    assert!(allowed_body["run_id"].is_string(), "{allowed_body}");
}

// --------------------------------------------------------------------------
// Lease enforcement on an owned run: the security control `check_lease` guards
// every protected route once the owner check passes. Missing/malformed →
// 401 missing-lease; wrong value → 403 invalid-lease. Uses a valid JWT and the
// caller's own run so the owner check passes and the lease path is reached.
// --------------------------------------------------------------------------

#[tokio::test]
async fn lease_failures_on_owned_run_are_rejected() {
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
        .await
        .expect("fixture starts");
    let tenant = fixture.data_tenant_id();
    seed_roles(fixture.app_pool(), tenant).await;
    let (state, _root) = build_state(&fixture);
    let eval_ref = seed_eval(&state, tenant, "prod", "rubric").await;

    let jwt = mint_jwt(&state, tenant, &["writer"]);
    let app = build_router(state);

    // Open a real run the caller owns.
    let (status, body) = call(
        &app,
        "POST",
        "/v1/eval/runs",
        Some(&jwt),
        None,
        open_body(&eval_ref).await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "open: {body}");
    let run_id = body["run_id"].as_str().expect("run_id").to_owned();
    let real_lease = body["lease_token"].as_str().expect("lease").to_owned();
    let next_uri = format!("/v1/eval/runs/{run_id}/next");

    // (1) No Authorization lease header at all → 401 missing-lease.
    let (status, body) = call(&app, "POST", &next_uri, Some(&jwt), None, json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "missing lease: {body}");
    assert_eq!(body["code"], "WYRD_EVAL_401_MISSING_LEASE");

    // (2) Wrong lease value (correct Bearer scheme) → 403 invalid-lease.
    let (status, body) = call(
        &app,
        "POST",
        &next_uri,
        Some(&jwt),
        Some("not-the-real-lease"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "invalid lease: {body}");
    assert_eq!(body["code"], "WYRD_EVAL_403_INVALID_LEASE");

    // (3) Non-Bearer scheme carrying the real lease → 401 missing-lease.
    let (status, body) = call_with_raw_lease(
        &app,
        &next_uri,
        &jwt,
        &format!("Basic {real_lease}"),
        json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "non-bearer scheme: {body}"
    );
    assert_eq!(body["code"], "WYRD_EVAL_401_MISSING_LEASE");
}
