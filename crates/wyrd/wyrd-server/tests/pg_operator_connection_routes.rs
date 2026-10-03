//! Operator connection management through the authenticated HTTP surface.
//!
//! Drives `/v1/operator-connections` against the in-process server and the
//! repository-managed Postgres: create for every provider, redacted list and
//! read, PATCH preserve/replace semantics, disable and re-enable, conflicts,
//! `operators:read` versus `operators:write` separation with audited
//! decisions, tenant isolation, malformed typed IDs, and a missing key
//! refusing only credential writes. Row inspection proves only ciphertext and
//! key-version metadata reach Postgres.

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_server::components::operators::keys::OperatorKeys;
use wyrd_server::config::{
    DeploymentProfile, OperatorKeySource, OperatorKeysConfig, VaultKeysConfig, WyrdServerConfig,
};
use wyrd_spec::ids::DataTenantId;
use wyrd_testing::logs::LogCapture;
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// Plaintext secrets the journey sends; none may appear in a response or row.
const SECRETS: [&str; 4] = [
    "xoxb-slack-secret-token",
    "pd-integration-secret-key",
    "http-header-secret-value",
    "rotated-slack-secret-token",
];

/// Send `method uri` with an optional JSON body as `jwt`.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn call(
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

/// Split a response into its status and JSON body, asserting no plaintext
/// secret appears in it.
///
/// # Panics
/// Panics when the body cannot be read, is not JSON, or contains a secret.
async fn decoded(response: Response<Body>) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("response body reads");
    let text = String::from_utf8_lossy(&bytes);
    for secret in SECRETS {
        assert!(!text.contains(secret), "response leaked a secret: {text}");
    }
    (
        status,
        serde_json::from_slice(&bytes).expect("response body is JSON"),
    )
}

/// Bootstrap a user holding `roles` and return its principal ID and token.
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

/// Create a custom tenant role named `name` holding exactly `permissions`.
///
/// # Panics
/// Panics when the role cannot be inserted.
async fn custom_role(server: &WyrdTestServer, name: &str, permissions: Value) {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    sqlx::query(
        "INSERT INTO wyrd.auth_roles (id, data_tenant_id, name, permissions) \
         VALUES ($1, wyrd.current_tenant(), $2, $3)",
    )
    .bind(Uuid::now_v7())
    .bind(name)
    .bind(permissions)
    .execute(&mut **conn.transaction())
    .await
    .expect("role inserts");
    conn.commit().await.expect("role commits");
}

/// Read one connection's stored secret columns as text for inspection.
///
/// # Panics
/// Panics when the row cannot be read.
async fn stored_row(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    connection_id: &str,
) -> (String, Vec<u8>, Vec<u8>, i32, i32) {
    let mut conn = server
        .tenant_conn_for(tenant)
        .await
        .expect("tenant connection opens");
    let row = sqlx::query_as(
        "SELECT config::text, secret_ciphertext, wrapped_dek, key_version, secret_version \
           FROM wyrd.operator_connections WHERE connection_id = $1::uuid",
    )
    .bind(connection_id)
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("row reads");
    conn.commit().await.expect("read commits");
    row
}

/// `principal`'s decisions for `operation` as (permission, outcome).
///
/// # Panics
/// Panics when the audit rows cannot be read.
async fn decisions(
    server: &WyrdTestServer,
    principal: Uuid,
    operation: &str,
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
          WHERE operation = $1 AND principal_id = $2 ORDER BY outcome",
    )
    .bind(operation)
    .bind(principal)
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("audit rows read");
    conn.commit().await.expect("audit read commits");
    rows
}

/// The three create bodies.
fn creates() -> [Value; 3] {
    [
        json!({ "provider": "slack", "name": "ops-slack", "workspace_id": "T0001",
                "bot_token": SECRETS[0] }),
        json!({ "provider": "pager_duty", "name": "ops-pagerduty",
                "integration_key": SECRETS[1] }),
        json!({ "provider": "http", "name": "ops-hooks", "origin": "https://Hooks.Example.com:443",
                "auth": { "scheme": "header", "name": "X-Api-Key", "value": SECRETS[2] } }),
    ]
}

/// A tenant administrator creates, lists, reads, updates, rotates, disables,
/// and re-enables connections of every provider; only redacted metadata is
/// returned, only ciphertext is stored, no route or query reveals a stored
/// secret, and no plaintext secret reaches a log line.
///
/// # Panics
/// Panics when any status, shape, redaction, storage, or log expectation
/// fails.
#[tokio::test(flavor = "current_thread")]
async fn admin_manages_redacted_encrypted_connections() {
    let logs = LogCapture::install();
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let tenant = server.data_tenant_id();
    let (admin, jwt) = user(&server, "oc-admin", &["admin"]).await;

    let mut ids = Vec::new();
    for body in creates() {
        let (status, view) = call(
            &server,
            &jwt,
            Method::POST,
            "/v1/operator-connections",
            Some(&body),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{view}");
        assert_eq!(view["status"], "active");
        let id = view["connection_id"].as_str().expect("id").to_owned();
        assert_eq!(Uuid::parse_str(&id).expect("uuid").get_version_num(), 7);
        let (config, ciphertext, wrapped, key_version, secret_version) =
            stored_row(&server, tenant, &id).await;
        for secret in SECRETS {
            assert!(!config.contains(secret));
            assert!(!String::from_utf8_lossy(&ciphertext).contains(secret));
        }
        assert!(!ciphertext.is_empty() && !wrapped.is_empty());
        assert_eq!((key_version, secret_version), (1, 1));
        ids.push(id);
    }
    let (_, http) = call(
        &server,
        &jwt,
        Method::GET,
        &format!("/v1/operator-connections/{}", ids[2]),
        None,
    )
    .await;
    assert_eq!(http["origin"], "https://hooks.example.com");
    assert_eq!(
        http["auth"],
        json!({ "scheme": "header", "name": "X-Api-Key" })
    );
    assert_eq!(
        decisions(&server, admin, "operator_connection.create")
            .await
            .len(),
        3
    );

    let (status, problem) = call(
        &server,
        &jwt,
        Method::POST,
        "/v1/operator-connections",
        Some(&creates()[0]),
    )
    .await;
    assert_eq!(
        (status, problem["code"].as_str()),
        (
            StatusCode::CONFLICT,
            Some("WYRD_OPERATOR_409_CONNECTION_CONFLICT")
        )
    );

    let (status, listed) = call(&server, &jwt, Method::GET, "/v1/operator-connections", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().expect("array").len(), 3);

    let slack = format!("/v1/operator-connections/{}", ids[0]);
    let before = stored_row(&server, tenant, &ids[0]).await;
    let (status, view) = call(
        &server,
        &jwt,
        Method::PATCH,
        &slack,
        Some(&json!({ "provider": "slack", "workspace_id": "T0002" })),
    )
    .await;
    assert_eq!(
        (status, view["workspace_id"].as_str()),
        (StatusCode::OK, Some("T0002"))
    );
    let metadata_only = stored_row(&server, tenant, &ids[0]).await;
    assert_eq!(
        (&metadata_only.1, metadata_only.4),
        (&before.1, 1),
        "omitted secret is preserved"
    );

    let (status, view) = call(
        &server,
        &jwt,
        Method::PATCH,
        &slack,
        Some(&json!({ "provider": "slack", "bot_token": SECRETS[3] })),
    )
    .await;
    assert_eq!(
        (status, view["connection_id"].as_str()),
        (StatusCode::OK, Some(ids[0].as_str()))
    );
    let rotated = stored_row(&server, tenant, &ids[0]).await;
    assert_ne!(
        rotated.1, before.1,
        "supplied secret replaces the ciphertext"
    );
    assert_eq!(
        rotated.4, 2,
        "rotation bumps the secret version on the same identity"
    );

    let (status, problem) = call(
        &server,
        &jwt,
        Method::PATCH,
        &slack,
        Some(&json!({ "provider": "pager_duty", "integration_key": "x" })),
    )
    .await;
    assert_eq!(
        (status, problem["code"].as_str()),
        (
            StatusCode::BAD_REQUEST,
            Some("WYRD_OPERATOR_400_INVALID_CONNECTION")
        )
    );
    let (status, problem) = call(
        &server,
        &jwt,
        Method::PATCH,
        &slack,
        Some(&json!({ "provider": "slack", "name": "renamed", "bot_token": 123_456 })),
    )
    .await;
    assert_eq!(
        (status, problem["code"].as_str()),
        (
            StatusCode::BAD_REQUEST,
            Some("WYRD_OPERATOR_400_INVALID_CONNECTION")
        )
    );
    assert!(
        !problem.to_string().contains("123456"),
        "a rejected value is not echoed"
    );

    let (status, view) = call(&server, &jwt, Method::DELETE, &slack, None).await;
    assert_eq!(
        (status, view["status"].as_str()),
        (StatusCode::OK, Some("disabled"))
    );
    let (_, view) = call(&server, &jwt, Method::GET, &slack, None).await;
    assert_eq!(view["status"], "disabled", "DELETE keeps the row");
    let (status, view) = call(
        &server,
        &jwt,
        Method::PATCH,
        &slack,
        Some(&json!({ "provider": "slack", "status": "active" })),
    )
    .await;
    assert_eq!(
        (status, view["status"].as_str()),
        (StatusCode::OK, Some("active"))
    );

    let (status, view) = call(
        &server,
        &jwt,
        Method::GET,
        &format!("{slack}?include_secret=true&reveal=true"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        view.get("bot_token").is_none(),
        "a reveal query still returns the redacted view: {view}"
    );
    for suffix in ["secret", "reveal", "credentials"] {
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("{slack}/{suffix}"))
            .body(Body::empty())
            .expect("request builds");
        let response = server
            .oneshot_authenticated(&jwt, request)
            .await
            .expect("request responds");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("response body reads");
        assert!(
            matches!(
                status,
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            ),
            "no secret read route exists: {suffix} -> {status}"
        );
        let text = String::from_utf8_lossy(&bytes);
        for secret in SECRETS {
            assert!(!text.contains(secret), "{suffix} leaked a secret");
        }
    }

    let (status, problem) = call(
        &server,
        &jwt,
        Method::GET,
        "/v1/operator-connections/not-a-uuid",
        None,
    )
    .await;
    assert_eq!(
        (status, problem["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("WYRD_SPEC_400_VALIDATION"))
    );
    let (status, problem) = call(
        &server,
        &jwt,
        Method::GET,
        &format!("/v1/operator-connections/{}", Uuid::now_v7()),
        None,
    )
    .await;
    assert_eq!(
        (status, problem["code"].as_str()),
        (
            StatusCode::NOT_FOUND,
            Some("WYRD_OPERATOR_404_CONNECTION_NOT_FOUND")
        )
    );

    let logged = logs.text();
    assert!(
        logged.contains("operator-connections"),
        "the capture observed the connection requests"
    );
    for secret in SECRETS {
        assert!(!logged.contains(secret), "a plaintext secret was logged");
    }
}

/// `operators:read` reads but cannot write, a role without either is denied
/// both, every decision is audited, and another tenant's administrator sees
/// none of the connections.
///
/// # Panics
/// Panics when a permission, audit, or isolation expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn read_write_separation_and_tenant_isolation() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let (_, admin) = user(&server, "oc-iso-admin", &["admin"]).await;
    let (_, created) = call(
        &server,
        &admin,
        Method::POST,
        "/v1/operator-connections",
        Some(&creates()[1]),
    )
    .await;
    let path = format!(
        "/v1/operator-connections/{}",
        created["connection_id"].as_str().expect("id")
    );

    custom_role(
        &server,
        "operators_reader",
        json!([{ "resource": "operators", "action": "read", "scope": "all" }]),
    )
    .await;
    let (reader_id, reader) = user(&server, "oc-reader", &["operators_reader"]).await;
    assert_eq!(
        call(&server, &reader, Method::GET, &path, None).await.0,
        StatusCode::OK
    );
    let (status, problem) = call(&server, &reader, Method::DELETE, &path, None).await;
    assert_eq!(
        (status, problem["code"].as_str()),
        (
            StatusCode::FORBIDDEN,
            Some("WYRD_PERMISSION_403_DENIED_RBAC")
        )
    );
    assert_eq!(
        decisions(&server, reader_id, "operator_connection.disable").await,
        vec![("operators:write".to_owned(), "denied".to_owned())]
    );
    assert_eq!(
        decisions(&server, reader_id, "operator_connection.read").await,
        vec![("operators:read".to_owned(), "allowed".to_owned())]
    );

    let (_, writer) = user(&server, "oc-writer", &["writer"]).await;
    assert_eq!(
        call(
            &server,
            &writer,
            Method::GET,
            "/v1/operator-connections",
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &server,
            &writer,
            Method::POST,
            "/v1/operator-connections",
            Some(&creates()[0])
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    let other = server.seed_tenant("oc-other").await.expect("tenant seeds");
    let foreign = server
        .bootstrap_service_in_tenant(other, "oc-other-admin", &["admin"])
        .await
        .expect("foreign admin bootstraps");
    let foreign = server
        .exchange_api_key(foreign.api_key().expect("api key"))
        .await
        .expect("api key exchanges");
    let (status, listed) = call(
        &server,
        &foreign,
        Method::GET,
        "/v1/operator-connections",
        None,
    )
    .await;
    assert_eq!((status, listed), (StatusCode::OK, json!([])));
    for method in [Method::GET, Method::DELETE] {
        let (status, problem) = call(&server, &foreign, method, &path, None).await;
        assert_eq!(
            (status, problem["code"].as_str()),
            (
                StatusCode::NOT_FOUND,
                Some("WYRD_OPERATOR_404_CONNECTION_NOT_FOUND")
            )
        );
    }
    let (status, _) = call(
        &server,
        &foreign,
        Method::POST,
        "/v1/operator-connections",
        Some(&creates()[1]),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "names are unique per tenant only"
    );
}

/// Without a readable active key, credential writes refuse with the stable
/// retryable key error and a constant detail that names no key location,
/// while reads keep working.
///
/// # Panics
/// Panics when the refusal or the unaffected read differs.
#[tokio::test(flavor = "current_thread")]
async fn missing_key_refuses_only_credential_writes() {
    let empty = tempfile::tempdir().expect("tempdir");
    let server = WyrdTestServer::builder()
        .with_operator_keys_for_test(OperatorKeysConfig {
            source: OperatorKeySource::File,
            dir: Some(empty.path().to_path_buf()),
            ..OperatorKeysConfig::default()
        })
        .start_in_process()
        .await
        .expect("test server starts");
    let (_, jwt) = user(&server, "oc-nokey", &["admin"]).await;
    let (status, problem) = call(
        &server,
        &jwt,
        Method::POST,
        "/v1/operator-connections",
        Some(&creates()[0]),
    )
    .await;
    assert_eq!(
        (status, problem["code"].as_str()),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Some("WYRD_OPERATOR_503_KEY_UNAVAILABLE")
        )
    );
    let empty_dir = empty.path().display().to_string();
    assert!(
        !problem.to_string().contains(&empty_dir),
        "the key directory stays internal: {problem}"
    );
    assert_eq!(
        problem["detail"].as_str(),
        Some("the Operator key provider could not supply a usable key; retry once it is restored")
    );
    assert_eq!(
        call(&server, &jwt, Method::GET, "/v1/operator-connections", None)
            .await
            .0,
        StatusCode::OK
    );
}

/// Register one Operator Card named `name` with `spec` as `jwt`.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn register_operator(
    server: &WyrdTestServer,
    jwt: &str,
    name: &str,
    spec: Value,
) -> (StatusCode, Value) {
    let body = json!({ "submissions": [{
        "apiVersion": "wyrd/v1", "kind": "Operator",
        "metadata": { "name": name, "version": "1.0.0", "space": "default" },
        "spec": spec, "artifacts": []
    }] });
    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/cards")
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", format!("oc-register-{name}"))
        .body(Body::from(body.to_string()))
        .expect("registration request builds");
    decoded(
        server
            .oneshot_authenticated(jwt, request)
            .await
            .expect("registration responds"),
    )
    .await
}

/// Registration binds an Operator to an active connection of its provider
/// whose HTTP origin and auth scheme match, and refuses a missing name, a
/// mismatched origin or scheme, and a disabled connection with one code.
///
/// # Panics
/// Panics when an accepted registration is refused or a mismatch is accepted.
#[tokio::test(flavor = "current_thread")]
async fn registration_binds_exact_connection_authority() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let (_, jwt) = user(&server, "oc-registrar", &["admin"]).await;
    let mut slack_id = String::new();
    for body in creates() {
        let (status, view) = call(
            &server,
            &jwt,
            Method::POST,
            "/v1/operator-connections",
            Some(&body),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{view}");
        if view["provider"] == "slack" {
            slack_id = view["connection_id"].as_str().expect("id").to_owned();
        }
    }
    let slack = |connection: &str| {
        json!({ "kind": "notify", "channel": { "kind": "slack", "connection": connection,
                "channel_id": "C0123456789", "text": "verification failed" } })
    };
    let http = |url: &str, auth: Value| json!({ "kind": "http", "method": "post", "url": url, "auth": auth });
    let header_scheme =
        json!({ "scheme": "header", "name": "x-api-key", "connection": "ops-hooks" });
    let accepted = [
        ("oc-slack", slack("ops-slack")),
        (
            "oc-pager",
            json!({ "kind": "notify", "channel": { "kind": "pager_duty",
                    "connection": "ops-pagerduty", "route": "retention",
                    "severity": "warning", "summary": "verification failed" } }),
        ),
        (
            "oc-http",
            http(
                "https://hooks.example.com/v1/{{subject_uid}}",
                header_scheme.clone(),
            ),
        ),
    ];
    for (name, spec) in accepted {
        let (status, body) = register_operator(&server, &jwt, name, spec).await;
        assert_eq!(status, StatusCode::CREATED, "{name}: {body}");
    }
    let refused = [
        ("oc-missing", slack("no-such-connection")),
        ("oc-wrong-provider", slack("ops-pagerduty")),
        (
            "oc-wrong-origin",
            http("https://hooks.example.com:8443/v1", header_scheme),
        ),
        (
            "oc-wrong-scheme",
            http(
                "https://hooks.example.com/v1",
                json!({ "scheme": "bearer", "connection": "ops-hooks" }),
            ),
        ),
    ];
    for (name, spec) in refused {
        let (status, problem) = register_operator(&server, &jwt, name, spec).await;
        assert_eq!(
            (status, problem["code"].as_str()),
            (
                StatusCode::BAD_REQUEST,
                Some("WYRD_SPEC_400_OPERATOR_CONNECTION_UNAVAILABLE")
            ),
            "{name}: {problem}"
        );
    }
    let (status, _) = call(
        &server,
        &jwt,
        Method::DELETE,
        &format!("/v1/operator-connections/{slack_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, problem) =
        register_operator(&server, &jwt, "oc-disabled", slack("ops-slack")).await;
    assert_eq!(
        (status, problem["code"].as_str()),
        (
            StatusCode::BAD_REQUEST,
            Some("WYRD_SPEC_400_OPERATOR_CONNECTION_UNAVAILABLE")
        ),
        "{problem}"
    );
}

/// A Vault-sourced key owner at `addr` using an inline token.
///
/// # Panics
/// Panics when the owner cannot be built.
fn vault_keys(addr: &str) -> OperatorKeys {
    OperatorKeys::new(OperatorKeysConfig {
        source: OperatorKeySource::Vault,
        vault: Some(VaultKeysConfig {
            addr: addr.to_owned(),
            mount: "secret".to_owned(),
            prefix: "wyrd/operator-keys".to_owned(),
            token_file: None,
            token: Some(secrecy::SecretString::from("boot-vault-token")),
        }),
        ..OperatorKeysConfig::default()
    })
    .expect("vault keys build")
}

/// A local Vault answering every tenant's version-1 key with `key`.
async fn vault_serving(key: Option<String>) -> MockServer {
    use wiremock::matchers::{method, path_regex};
    let vault = MockServer::start().await;
    let response = match key {
        Some(key) => {
            ResponseTemplate::new(200).set_body_json(json!({ "data": { "data": { "key": key } } }))
        }
        None => ResponseTemplate::new(404),
    };
    Mock::given(method("GET"))
        .and(path_regex(
            r"^/v1/secret/data/wyrd/operator-keys/[0-9a-f-]+/1$",
        ))
        .respond_with(response)
        .mount(&vault)
        .await;
    vault
}

/// Multi-tenant production boot refuses readiness unless every active
/// provisioned tenant's active key reads and decodes to 32 bytes, naming no
/// key location; development and explicitly single-tenant deployments keep
/// the deferred failure.
///
/// # Panics
/// Panics when a broken key source is accepted, a readable one refused, or a
/// refusal discloses a selector.
#[tokio::test(flavor = "current_thread")]
async fn production_boot_requires_every_active_tenant_key() {
    use base64::Engine as _;
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let production = WyrdServerConfig {
        deployment_profile: DeploymentProfile::Production,
        ..WyrdServerConfig::default()
    };
    let engine = base64::engine::general_purpose::STANDARD;
    let mut state = server.state().clone();

    let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("port reserves");
    let unreachable = format!("http://{}", closed.local_addr().expect("addr"));
    drop(closed);
    let missing = vault_serving(None).await;
    let malformed = vault_serving(Some("not base64!".to_owned())).await;
    let short = vault_serving(Some(engine.encode([4_u8; 16]))).await;
    for (case, addr) in [
        ("unavailable provider", unreachable.clone()),
        ("missing tenant key", missing.uri()),
        ("malformed key", malformed.uri()),
        ("wrong-length key", short.uri()),
    ] {
        state.operator_keys = std::sync::Arc::new(vault_keys(&addr));
        let refused = wyrd_server::boot::verify_operator_keys(&state, &production)
            .await
            .expect_err(case)
            .to_string();
        for selector in [addr.as_str(), "wyrd/operator-keys", "boot-vault-token"] {
            assert!(
                !refused.contains(selector),
                "{case} leaked {selector}: {refused}"
            );
        }
    }

    let readable = vault_serving(Some(engine.encode([4_u8; 32]))).await;
    state.operator_keys = std::sync::Arc::new(vault_keys(&readable.uri()));
    wyrd_server::boot::verify_operator_keys(&state, &production)
        .await
        .expect("readable 32-byte keys boot");
    let reads = readable.received_requests().await.expect("recorded").len();
    assert!(
        reads >= 2,
        "every active tenant, including the fixture, is checked: {reads}"
    );

    state.operator_keys = std::sync::Arc::new(vault_keys(&unreachable));
    wyrd_server::boot::verify_operator_keys(&state, &WyrdServerConfig::default())
        .await
        .expect("development defers key failures");
    let mut single_tenant = production.clone();
    single_tenant.auth.tenant_slug = Some(wyrd_spec::TenantSlug::new("acme").expect("slug"));
    wyrd_server::boot::verify_operator_keys(&state, &single_tenant)
        .await
        .expect("an explicitly single-tenant deployment defers key failures");
}
