//! Public card-registration journey through the authenticated HTTP surface.

use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use base64::Engine;
use chrono::{DateTime, Utc};
use secrecy::SecretString;
use serde_json::{Value, json};
use sha2::Digest;
use tokio::time::sleep;
use url::Url;
use uuid::Uuid;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::cards::{CardSelector, Cards, ListCardsRequest};
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_loader::{build_registration_input, load};
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::ids::{CardName, CardUid, DataTenantId, SpaceName};
use wyrd_spec::reference::{CardRef, InlineableRef};
use wyrd_spec::registry::{CardLifecycleStatus, RegistrationOutcomeKind};
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_sql::queries::verification::{InactivityTimeout, binding_activity};
use wyrd_storage::settings::{BackendConfig, StorageSettings};
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// Seed a dependency row directly so the journey can exercise non-Active states.
///
/// # Panics
/// Panics when the fixture superuser pool cannot open or the dependency row
/// fails to insert.
async fn seed_dependency(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    name: &str,
    status: &str,
) -> CardUid {
    seed_card(
        server,
        tenant,
        "Prompt",
        name,
        json!({ "provider": "openai", "model": "gpt-4o", "messages": ["hello"] }),
        status,
    )
    .await
}

/// Seed one already-registered Card of any kind for a reference to resolve to.
///
/// Registration reads a referenced binding target's effective spec out of the
/// registry, so the server-only refusals need real rows rather than siblings in
/// the same request.
///
/// # Panics
/// Panics when the generated UID is invalid, the fixture superuser pool
/// cannot open, or the Card row fails to insert.
async fn seed_card(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    kind: &str,
    name: &str,
    spec: Value,
    status: &str,
) -> CardUid {
    let uid = CardUid::from_uuid(Uuid::now_v7()).expect("test dependency UID is valid");
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    let spec_hash = blake3::hash(name.as_bytes()).to_hex().to_string();
    sqlx::query(
        "INSERT INTO wyrd.cards \
            (card_uid, data_tenant_id, kind, space, name, version, spec, spec_hash, status) \
         VALUES ($1, $2, $3, 'default', $4, '1.0.0', $5, $6, $7)",
    )
    .bind(uid.as_uuid())
    .bind(tenant.as_uuid())
    .bind(kind)
    .bind(name)
    .bind(spec)
    .bind(spec_hash)
    .bind(status)
    .execute(&pool)
    .await
    .expect("dependency row inserts");
    uid
}

/// Assemble the production HTTP client used by the native registry saga.
fn registry_client(base_url: &str, jwt: &str) -> WyrdClient {
    let config = ClientConfig {
        http: HttpConfig {
            base_url: base_url.to_owned(),
            ..HttpConfig::default()
        },
        ..ClientConfig::default()
    };
    let auth = AuthMiddleware::new(
        &config,
        ResolvedCredential::BearerToken(SecretString::from(jwt.to_owned())),
    )
    .expect("client auth builds");
    let transport = HttpTransport::new(&config.http, Arc::clone(&auth)).expect("transport builds");
    WyrdClient::from_parts(auth, transport, config.grpc)
}

/// Decode an HTTP response body as JSON.
async fn response_json(response: axum::http::Response<Body>) -> Value {
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("response body reads");
    serde_json::from_slice(&bytes).expect("response body is JSON")
}

/// Read the durable reconciliation status for one Card.
async fn reconciliation_state(server: &WyrdTestServer, card_uid: &CardUid) -> (String, i32) {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let state = sqlx::query_as::<_, (String, i32)>(
        "SELECT reconcile_status, reconcile_attempts FROM wyrd.cards WHERE card_uid = $1",
    )
    .bind(card_uid.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("reconciliation state reads");
    conn.commit().await.expect("reconciliation read commits");
    state
}

/// Wait for one worker attempt to finish with the expected durable status.
async fn wait_for_reconciliation_state(
    server: &WyrdTestServer,
    card_uid: &CardUid,
    expected_attempts: i32,
    expected_status: &str,
) {
    for _ in 0..100 {
        let (status, attempts) = reconciliation_state(server, card_uid).await;
        if status == expected_status && attempts == expected_attempts {
            return;
        }
        sleep(Duration::from_millis(100)).await;
    }
    let (status, attempts) = reconciliation_state(server, card_uid).await;
    panic!(
        "reconciliation did not reach status={expected_status:?}, attempts={expected_attempts}; \
         got status={status:?}, attempts={attempts}"
    );
}

/// Make a pending reconciliation claim immediately eligible for the next pass.
async fn make_reconciliation_due(server: &WyrdTestServer, card_uid: &CardUid) {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    sqlx::query(
        "UPDATE wyrd.cards SET reconcile_next_attempt_at = now() - interval '1 second' \
         WHERE card_uid = $1 AND reconcile_status = 'pending'",
    )
    .bind(card_uid.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("reconciliation becomes due");
    conn.commit().await.expect("reconciliation update commits");
}

/// Build an authenticated registration request from a JSON payload and key.
fn request_with_body(idempotency_key: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/v1/cards")
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", idempotency_key)
        .body(Body::from(body.to_string()))
        .expect("registration request builds")
}

/// Build the fixed request used to prove exact idempotent replay.
fn registration_request() -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/v1/cards")
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", "route-journey-001")
        .body(Body::from(
            json!({
                "submissions": [{
                    "apiVersion": "wyrd/v1",
                    "kind": "Prompt",
                    "metadata": {
                        "name": "route-journey",
                        "version": "1.0.0",
                        "space": "default"
                    },
                    "spec": {
                        "provider": "openai",
                        "model": "gpt-4o",
                        "messages": ["hello"]
                    },
                    "artifacts": []
                }]
            })
            .to_string(),
        ))
        .expect("registration request builds")
}

/// Build a metadata-only Prompt registration request.
fn prompt_registration_request(name: &str, idempotency_key: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/v1/cards")
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", idempotency_key)
        .body(Body::from(
            json!({
                "submissions": [{
                    "apiVersion": "wyrd/v1",
                    "kind": "Prompt",
                    "metadata": {
                        "name": name,
                        "version": "1.0.0",
                        "space": "default"
                    },
                    "spec": {
                        "provider": "openai",
                        "model": "gpt-4o",
                        "messages": ["hello"]
                    },
                    "artifacts": []
                }]
            })
            .to_string(),
        ))
        .expect("prompt registration request builds")
}

/// Build an Agent registration request referencing an existing Prompt.
fn agent_registration_request(
    name: &str,
    child_name: &str,
    idempotency_key: &str,
) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/v1/cards")
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", idempotency_key)
        .body(Body::from(
            json!({
                "submissions": [{
                    "apiVersion": "wyrd/v1",
                    "kind": "Agent",
                    "metadata": {
                        "name": name,
                        "version": "1.0.0",
                        "space": "default"
                    },
                    "spec": {
                        "prompt": {
                            "kind": "Prompt",
                            "name": child_name,
                            "version": "1.0.0",
                            "space": "default"
                        }
                    },
                    "artifacts": []
                }]
            })
            .to_string(),
        ))
        .expect("agent registration request builds")
}

/// Build a leaf-to-root Prompt, Agent, and Service composite request.
fn three_card_composite_request(idempotency_key: &str) -> Request<Body> {
    request_with_body(
        idempotency_key,
        json!({ "submissions": [
            {
                "apiVersion": "wyrd/v1", "kind": "Service",
                "metadata": { "name": "composite-service", "version": "1.0.0", "space": "default" },
                "spec": { "components": [{
                    "alias": "agent",
                    "ref": { "sibling": { "kind": "Agent", "name": "composite-agent", "version": "1.0.0", "space": "default" } }
                }] },
                "artifacts": []
            },
            {
                "apiVersion": "wyrd/v1", "kind": "Agent",
                "metadata": { "name": "composite-agent", "version": "1.0.0", "space": "default" },
                "spec": { "prompt": {
                    "sibling": { "kind": "Prompt", "name": "composite-prompt", "version": "1.0.0", "space": "default" }
                } },
                "artifacts": []
            },
            {
                "apiVersion": "wyrd/v1", "kind": "Prompt",
                "metadata": { "name": "composite-prompt", "version": "1.0.0", "space": "default" },
                "spec": { "provider": "openai", "model": "gpt-4o", "messages": ["hello"] },
                "artifacts": []
            }
        ] }),
    )
}

/// Build the same three-card graph in a different authored wire order.
fn permuted_three_card_composite_request(idempotency_key: &str) -> Request<Body> {
    request_with_body(
        idempotency_key,
        json!({ "submissions": [
            {
                "apiVersion": "wyrd/v1", "kind": "Prompt",
                "metadata": { "name": "composite-prompt", "version": "1.0.0", "space": "default" },
                "spec": { "provider": "openai", "model": "gpt-4o", "messages": ["hello"] },
                "artifacts": []
            },
            {
                "apiVersion": "wyrd/v1", "kind": "Service",
                "metadata": { "name": "composite-service", "version": "1.0.0", "space": "default" },
                "spec": { "components": [{
                    "alias": "agent",
                    "ref": { "sibling": { "kind": "Agent", "name": "composite-agent", "version": "1.0.0", "space": "default" } }
                }] },
                "artifacts": []
            },
            {
                "apiVersion": "wyrd/v1", "kind": "Agent",
                "metadata": { "name": "composite-agent", "version": "1.0.0", "space": "default" },
                "spec": { "prompt": {
                    "sibling": { "kind": "Prompt", "name": "composite-prompt", "version": "1.0.0", "space": "default" }
                } },
                "artifacts": []
            }
        ] }),
    )
}

/// Build one artifact-bearing card registration request.
fn heavy_registration_request(idempotency_key: &str) -> Request<Body> {
    request_with_body(
        idempotency_key,
        json!({ "submissions": [{
            "apiVersion": "wyrd/v1", "kind": "Prompt",
            "metadata": { "name": "heavy-prompt", "version": "1.0.0", "space": "default" },
            "spec": { "provider": "openai", "model": "gpt-4o", "messages": ["hello"] },
            "artifacts": [{
                "relative_path": "prompt.txt",
                "sha256": "ypeBEsobvcr6wjGzmiPcTaeG7/gUfE5yuYB3ha/uSLs=",
                "size_bytes": 1,
                "content_type": "text/plain"
            }]
        }] }),
    )
}

#[tokio::test(flavor = "current_thread")]
/// The native registry saga returns only a final Active receipt.
async fn client_registration_saga_returns_active_receipt() {
    let temp = tempfile::tempdir().expect("loader workspace creates");
    let prompt_path = temp.path().join("client-prompt.yaml");
    let artifact = b"native registry artifact";
    std::fs::write(temp.path().join("prompt.txt"), artifact).expect("artifact writes");
    let digest = base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(artifact));
    std::fs::write(
        &prompt_path,
        format!(
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: client-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: [hello]\nartifacts:\n  - relative_path: prompt.txt\n    sha256: {digest}\n    size_bytes: {}\n    content_type: text/plain\n",
            artifact.len()
        ),
    )
    .expect("prompt card writes");
    let input = build_registration_input(load(&prompt_path).expect("loader tree builds"))
        .expect("registration input builds");

    let storage_root = tempfile::tempdir().expect("storage root creates");
    let server = WyrdTestServer::builder()
        .with_storage_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: storage_root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some(String::new()),
        })
        .start_bound()
        .await
        .expect("bound test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-native-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let cards = Cards::with_client(registry_client(
        server.base_url().expect("bound server has a base URL"),
        &jwt,
    ));

    let receipt = cards
        .register(&input)
        .await
        .expect("native registration saga succeeds");
    assert_eq!(receipt.outcomes.len(), 1);
    assert_eq!(receipt.outcomes[0].status, CardLifecycleStatus::Active);
    assert_eq!(
        receipt.outcomes[0].outcome,
        RegistrationOutcomeKind::Registered
    );
    assert!(receipt.outcomes[0].card_blob_uri.is_some());
    assert_eq!(receipt.root.uid, receipt.outcomes[0].card_ref.uid);

    let card_uid = receipt.outcomes[0]
        .card_ref
        .uid
        .clone()
        .expect("artifact receipt contains a Card UID");
    let selected = cards
        .get(CardSelector::uid(CardKind::Prompt, card_uid.clone()))
        .await
        .expect("public Cards get reads the registered card");
    assert_eq!(selected.metadata.uid, Some(card_uid.clone()));

    let listed = cards
        .list(ListCardsRequest {
            kind: Some(CardKind::Prompt),
            space: Some(SpaceName::new("default").expect("test space is valid")),
            name: Some(CardName::new("client-prompt").expect("test name is valid")),
            version_range: None,
            status: Some(CardLifecycleStatus::Active),
            filter: None,
            include_prerelease: false,
            limit: None,
            cursor: None,
        })
        .await
        .expect("public Cards list reads the registered card");
    assert_eq!(listed.items.len(), 1);
    assert_eq!(listed.items[0].card_uid, card_uid);

    let latest = cards
        .resolve_latest(
            CardKind::Prompt,
            SpaceName::new("default").expect("test space is valid"),
            CardName::new("client-prompt").expect("test name is valid"),
        )
        .await
        .expect("public Cards latest resolves the registered card");
    assert_eq!(latest.uid, Some(card_uid.clone()));

    let load_destination = tempfile::tempdir().expect("load destination creates");
    let loaded = cards
        .load(
            CardSelector::uid(CardKind::Prompt, card_uid.clone()),
            Some(load_destination.path()),
        )
        .await
        .expect("public Cards load downloads the registered artifact");
    assert_eq!(loaded.card.metadata.uid, Some(card_uid.clone()));
    assert_eq!(
        tokio::fs::read(load_destination.path().join("prompt.txt"))
            .await
            .expect("loaded artifact reads"),
        artifact
    );

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let stored = get_card_by_uid(&mut conn, &card_uid)
        .await
        .expect("artifact card loads");
    assert_eq!(stored.status, wyrd_sql::CardStatus::Active);
    assert!(stored.card_blob_uri.is_some());
    let manifest_status: String = sqlx::query_scalar(
        "SELECT upload_status FROM wyrd.card_artifact_manifest WHERE card_uid = $1",
    )
    .bind(card_uid.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("artifact manifest loads");
    assert_eq!(manifest_status, "verified");
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

#[tokio::test(flavor = "current_thread")]
/// Registering the same request twice returns the stored response without duplicate writes.
async fn registration_replays_through_public_authenticated_route() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { id, jwt } = server
        .bootstrap_user("registry-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let first = server
        .oneshot_authenticated(&jwt, registration_request())
        .await
        .expect("first registration responds");
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body = response_json(first).await;
    assert_eq!(first_body["outcomes"][0]["outcome"], "registered");

    let replay = server
        .oneshot_authenticated(&jwt, registration_request())
        .await
        .expect("replay responds");
    assert_eq!(replay.status(), StatusCode::CREATED);
    let replay_body = response_json(replay).await;
    assert_eq!(replay_body["outcomes"][0]["outcome"], "idempotent_noop");
    assert_eq!(
        replay_body["outcomes"][0]["card_ref"]["uid"],
        first_body["outcomes"][0]["card_ref"]["uid"]
    );
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging \
         WHERE operation = 'card.registration.create' AND principal_id = $1",
    )
    .bind(id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("audit count reads");
    assert_eq!(
        audit_count, 2,
        "each received registration request audits its own card:write decision"
    );
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

#[tokio::test(flavor = "current_thread")]
/// Registration resolves an external child reference and persists its UID.
async fn registration_resolves_child_card_refs_before_persisting() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-child-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let child = server
        .oneshot_authenticated(
            &jwt,
            prompt_registration_request("resolved-prompt", "child-resolution-001"),
        )
        .await
        .expect("child registration responds");
    assert_eq!(child.status(), StatusCode::CREATED);
    let child_body = response_json(child).await;

    let parent = server
        .oneshot_authenticated(
            &jwt,
            agent_registration_request("resolved-agent", "resolved-prompt", "child-resolution-002"),
        )
        .await
        .expect("parent registration responds");
    let parent_status = parent.status();
    let parent_body = response_json(parent).await;
    assert_eq!(parent_status, StatusCode::CREATED, "{parent_body}");
    let parent_uid = serde_json::from_value(parent_body["outcomes"][0]["card_ref"]["uid"].clone())
        .expect("parent response contains a card uid");

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let stored = get_card_by_uid(&mut conn, &parent_uid)
        .await
        .expect("persisted parent card loads");
    let Spec::Agent(agent) = stored.spec else {
        panic!("persisted card is not an Agent");
    };
    let InlineableRef::Ref(child_ref) = agent.prompt else {
        panic!("persisted agent prompt is not a card reference");
    };
    assert_eq!(child_ref.name.as_str(), "resolved-prompt");
    assert_eq!(
        child_ref.uid.as_ref().map(ToString::to_string),
        child_body["outcomes"][0]["card_ref"]["uid"]
            .as_str()
            .map(ToOwned::to_owned),
    );
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

#[tokio::test(flavor = "current_thread")]
/// The offline loader hands its wire projection to the composite registration route.
async fn registration_accepts_loader_projection_and_persists_sibling_binding() {
    let temp = tempfile::tempdir().expect("loader workspace creates");
    let prompt_path = temp.path().join("prompt.yaml");
    let agent_path = temp.path().join("agent.yaml");
    std::fs::write(
        &prompt_path,
        "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: loader-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: [hello]\n",
    )
    .expect("prompt card writes");
    std::fs::write(
        &agent_path,
        "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: loader-agent\n  version: 1.0.0\n  space: default\nspec:\n  prompt: prompt.yaml\n",
    )
    .expect("agent card writes");

    let input = build_registration_input(load(&agent_path).expect("loader tree builds"))
        .expect("registration input builds");
    let request = request_with_body(
        "loader-handoff-001",
        serde_json::to_value(input.to_create_card_request()).expect("request serializes"),
    );

    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("loader-registry-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let response = server
        .oneshot_authenticated(&jwt, request)
        .await
        .expect("loader registration responds");
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    let agent_uid: CardUid = serde_json::from_value(
        body["outcomes"]
            .as_array()
            .expect("outcomes are an array")
            .iter()
            .find(|outcome| outcome["card_ref"]["name"] == "loader-agent")
            .expect("agent outcome exists")["card_ref"]["uid"]
            .clone(),
    )
    .expect("agent outcome contains a UID");

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let stored = get_card_by_uid(&mut conn, &agent_uid)
        .await
        .expect("persisted loader agent loads");
    let Spec::Agent(agent) = stored.spec else {
        panic!("persisted card is not an Agent");
    };
    let InlineableRef::Ref(prompt_ref) = agent.prompt else {
        panic!("the server must bind the loader sibling before persistence");
    };
    assert_eq!(prompt_ref.name.as_str(), "loader-prompt");
    assert!(prompt_ref.uid.is_some());
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

/// Reusing an idempotency key for different content returns the stable conflict.
#[tokio::test(flavor = "current_thread")]
async fn registration_rejects_idempotency_key_reuse_for_different_content() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-conflict-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let first = server
        .oneshot_authenticated(
            &jwt,
            prompt_registration_request("conflict-first", "conflict-key-001"),
        )
        .await
        .expect("first registration responds");
    assert_eq!(first.status(), StatusCode::CREATED);
    let conflict = server
        .oneshot_authenticated(
            &jwt,
            prompt_registration_request("conflict-second", "conflict-key-001"),
        )
        .await
        .expect("conflicting registration responds");
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(conflict).await["code"],
        "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT"
    );

    server.shutdown().await.expect("test server shuts down");
}

/// Resolve failures return 422 before any registration operation is reserved.
#[tokio::test(flavor = "current_thread")]
async fn unresolved_dependency_leaves_no_registration_operation() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-unresolved-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let response = server
        .oneshot_authenticated(
            &jwt,
            agent_registration_request("missing-agent", "missing-prompt", "missing-ref-001"),
        )
        .await
        .expect("unresolved registration responds");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await["code"],
        "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY"
    );
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let operation_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.card_registration_operations WHERE idempotency_key = $1",
    )
    .bind("missing-ref-001")
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("operation count reads");
    assert_eq!(operation_count, 0);
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

/// Reject non-Active and cross-tenant targets before any registration write.
#[tokio::test(flavor = "current_thread")]
async fn non_active_and_cross_tenant_dependencies_leave_no_writes() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-dependency-state-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    for status in ["pending", "failed", "deleted"] {
        let child_name = format!("blocked-{status}");
        seed_dependency(&server, server.data_tenant_id(), &child_name, status).await;
        let operation_key = format!("blocked-{status}-operation");
        let response = server
            .oneshot_authenticated(
                &jwt,
                agent_registration_request("blocked-parent", &child_name, &operation_key),
            )
            .await
            .expect("blocked registration responds");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response_json(response).await["code"],
            "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY"
        );
        assert_no_registration_writes(&server, &operation_key, "blocked-parent").await;
    }

    let other_tenant = DataTenantId::new_v7();
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query(
        "INSERT INTO platform.tenants \
            (data_tenant_id, slug, display_name, status) VALUES ($1, $2, $3, 'active')",
    )
    .bind(other_tenant.as_uuid())
    .bind(format!("cross-{}", other_tenant.as_uuid()))
    .bind("Cross-tenant fixture")
    .execute(&pool)
    .await
    .expect("cross-tenant fixture inserts");
    let child_name = "cross-tenant-prompt";
    seed_dependency(&server, other_tenant, child_name, "active").await;
    let operation_key = "cross-tenant-operation";
    let response = server
        .oneshot_authenticated(
            &jwt,
            agent_registration_request("cross-tenant-parent", child_name, operation_key),
        )
        .await
        .expect("cross-tenant registration responds");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(response).await["code"],
        "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY"
    );
    assert_no_registration_writes(&server, operation_key, "cross-tenant-parent").await;

    server.shutdown().await.expect("test server shuts down");
}

/// Build an Agent submission whose `verified_by` binds already-registered peers.
fn bound_agent_request(
    name: &str,
    prompt_name: &str,
    verifier_name: &str,
    trigger_name: &str,
    on_failure: Value,
    idempotency_key: &str,
) -> Request<Body> {
    request_with_body(
        idempotency_key,
        json!({ "submissions": [{
            "apiVersion": "wyrd/v1",
            "kind": "Agent",
            "metadata": { "name": name, "version": "1.0.0", "space": "default" },
            "spec": {
                "prompt": {
                    "kind": "Prompt", "name": prompt_name,
                    "version": "1.0.0", "space": "default"
                },
                "verified_by": [{
                    "verifier": {
                        "kind": "Verifier", "name": verifier_name,
                        "version": "1.0.0", "space": "default"
                    },
                    "runs_on": {
                        "kind": "Trigger", "name": trigger_name,
                        "version": "1.0.0", "space": "default"
                    },
                    "on_failure": on_failure,
                }],
            },
            "artifacts": []
        }] }),
    )
}

/// Reject every binding refusal at the authenticated route before any write.
///
/// Four branches are only reachable with a real registry behind them: the
/// effective Operator and Trigger bodies come from previously registered
/// Cards, the cross-tenant case depends on RLS, and the under-privileged case
/// depends on the route's own scope check. The fifth, a binding nested under
/// an inline Agent, is decidable from the request alone, but it is proved here
/// so the seam from HTTP decoding through shared spec validation, stable error
/// mapping, and the no-write transaction boundary cannot regress silently.
///
/// # Panics
/// Panics when the test server fails to start or shut down, when either
/// bootstrap returns a non-user principal, when any fixture row fails to
/// insert, when the route fails to respond, or when any refusal does not
/// produce its exact status, stable error code, and absence of durable
/// registration state.
#[tokio::test(flavor = "current_thread")]
async fn referenced_binding_refusals_leave_no_writes() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-binding-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let Bootstrap::User {
        jwt: denied_jwt, ..
    } = server
        .bootstrap_user("registry-binding-denied", &[])
        .await
        .expect("denied user bootstraps")
    else {
        panic!("denied bootstrap returned a non-user principal");
    };

    let tenant = server.data_tenant_id();
    seed_dependency(&server, tenant, "binding-prompt", "active").await;
    seed_card(
        &server,
        tenant,
        "Verifier",
        "binding-eval-verifier",
        json!({ "implementation": { "kind": "eval", "spec": { "tasks": {} } } }),
        "active",
    )
    .await;
    seed_card(
        &server,
        tenant,
        "Trigger",
        "binding-observations-trigger",
        json!({ "kind": "observations_ready" }),
        "active",
    )
    .await;
    seed_card(
        &server,
        tenant,
        "Trigger",
        "binding-schedule-trigger",
        json!({ "kind": "schedule", "cron": "0 * * * *" }),
        "active",
    )
    .await;
    seed_card(
        &server,
        tenant,
        "Workflow",
        "binding-rollback-workflow",
        json!({ "steps": [] }),
        "active",
    )
    .await;
    seed_card(
        &server,
        tenant,
        "Operator",
        "binding-workflow-operator",
        json!({
            "kind": "workflow",
            "workflow_ref": {
                "kind": "Workflow", "name": "binding-rollback-workflow",
                "version": "1.0.0", "space": "default"
            }
        }),
        "active",
    )
    .await;

    let workflow_operator = server
        .oneshot_authenticated(
            &jwt,
            bound_agent_request(
                "binding-workflow-operator-agent",
                "binding-prompt",
                "binding-eval-verifier",
                "binding-observations-trigger",
                json!([{
                    "kind": "Operator", "name": "binding-workflow-operator",
                    "version": "1.0.0", "space": "default"
                }]),
                "binding-workflow-operator-operation",
            ),
        )
        .await
        .expect("workflow-Operator registration responds");
    assert_eq!(workflow_operator.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(workflow_operator).await["code"],
        "WYRD_SPEC_400_UNSUPPORTED_OPERATOR_ACTION"
    );
    assert_no_registration_writes(
        &server,
        "binding-workflow-operator-operation",
        "binding-workflow-operator-agent",
    )
    .await;

    let mismatch = server
        .oneshot_authenticated(
            &jwt,
            bound_agent_request(
                "binding-mismatch-agent",
                "binding-prompt",
                "binding-eval-verifier",
                "binding-schedule-trigger",
                json!([]),
                "binding-mismatch-operation",
            ),
        )
        .await
        .expect("activation-mismatch registration responds");
    assert_eq!(mismatch.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(mismatch).await["code"],
        "WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH"
    );
    assert_no_registration_writes(
        &server,
        "binding-mismatch-operation",
        "binding-mismatch-agent",
    )
    .await;

    let other_tenant = DataTenantId::new_v7();
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query(
        "INSERT INTO platform.tenants \
            (data_tenant_id, slug, display_name, status) VALUES ($1, $2, $3, 'active')",
    )
    .bind(other_tenant.as_uuid())
    .bind(format!("binding-{}", other_tenant.as_uuid()))
    .bind("Cross-tenant binding fixture")
    .execute(&pool)
    .await
    .expect("cross-tenant fixture inserts");
    seed_card(
        &server,
        other_tenant,
        "Verifier",
        "binding-foreign-verifier",
        json!({ "implementation": { "kind": "eval", "spec": { "tasks": {} } } }),
        "active",
    )
    .await;

    let cross_tenant = server
        .oneshot_authenticated(
            &jwt,
            bound_agent_request(
                "binding-cross-tenant-agent",
                "binding-prompt",
                "binding-foreign-verifier",
                "binding-observations-trigger",
                json!([]),
                "binding-cross-tenant-operation",
            ),
        )
        .await
        .expect("cross-tenant binding registration responds");
    assert_eq!(cross_tenant.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(cross_tenant).await["code"],
        "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY"
    );
    assert_no_registration_writes(
        &server,
        "binding-cross-tenant-operation",
        "binding-cross-tenant-agent",
    )
    .await;

    let denied = server
        .oneshot_authenticated(
            &denied_jwt,
            bound_agent_request(
                "binding-denied-agent",
                "binding-prompt",
                "binding-eval-verifier",
                "binding-observations-trigger",
                json!([]),
                "binding-denied-operation",
            ),
        )
        .await
        .expect("under-privileged binding registration responds");
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_json(denied).await["code"],
        "WYRD_PERMISSION_403_DENIED_RBAC"
    );
    assert_no_registration_writes(&server, "binding-denied-operation", "binding-denied-agent")
        .await;

    let nested = server
        .oneshot_authenticated(
            &jwt,
            request_with_body(
                "binding-nested-operation",
                json!({ "submissions": [{
                    "apiVersion": "wyrd/v1",
                    "kind": "Workflow",
                    "metadata": {
                        "name": "binding-nested-workflow",
                        "version": "1.0.0",
                        "space": "default"
                    },
                    "spec": {
                        "steps": [{
                            "id": "judge",
                            "action": {
                                "type": "agent",
                                "target": {
                                    "prompt": {
                                        "kind": "Prompt", "name": "binding-prompt",
                                        "version": "1.0.0", "space": "default"
                                    },
                                    "verified_by": [{
                                        "verifier": {
                                            "kind": "Verifier",
                                            "name": "binding-eval-verifier",
                                            "version": "1.0.0",
                                            "space": "default"
                                        },
                                        "runs_on": { "kind": "observations_ready" },
                                    }],
                                },
                            },
                        }],
                    },
                    "artifacts": []
                }] }),
            ),
        )
        .await
        .expect("nested inline-Agent binding registration responds");
    assert_eq!(nested.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(nested).await["code"],
        "WYRD_REGISTRY_400_INVALID_CARD_SPEC"
    );
    assert_no_registration_writes(
        &server,
        "binding-nested-operation",
        "binding-nested-workflow",
    )
    .await;

    server.shutdown().await.expect("test server shuts down");
}

/// Confirm a rejected dependency did not append any durable registration state.
///
/// Counts the registration operation under `operation_key` and, for Cards
/// named `name`, the Card rows, their relationships, Card-bound principals of
/// the same name, and verification bindings they own.
///
/// # Panics
/// Panics when a count cannot be read or any count is non-zero.
async fn assert_no_registration_writes(server: &WyrdTestServer, operation_key: &str, name: &str) {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let operation_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.card_registration_operations WHERE idempotency_key = $1",
    )
    .bind(operation_key)
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("operation count reads");
    let card_count: i64 = sqlx::query_scalar("SELECT count(*) FROM wyrd.cards WHERE name = $1")
        .bind(name)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("card count reads");
    let relationship_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.card_relationships WHERE card_uid IN \
         (SELECT card_uid FROM wyrd.cards WHERE name = $1)",
    )
    .bind(name)
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("relationship count reads");
    let principal_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM wyrd.auth_service_accounts WHERE name = $1")
            .bind(name)
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("principal count reads");
    let binding_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.verification_bindings WHERE owner_card_uid IN \
         (SELECT card_uid FROM wyrd.cards WHERE name = $1)",
    )
    .bind(name)
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("binding count reads");
    assert_eq!(operation_count, 0);
    assert_eq!(card_count, 0);
    assert_eq!(relationship_count, 0);
    assert_eq!(principal_count, 0);
    assert_eq!(binding_count, 0);
    conn.commit().await.expect("assertion transaction commits");
}

/// Reject an artifact-bearing submission when another card shares the request.
#[tokio::test(flavor = "current_thread")]
async fn composite_with_manifest_rejects_before_writes() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-heavy-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let response = server
        .oneshot_authenticated(
            &jwt,
            request_with_body(
                "heavy-composite-001",
                json!({ "submissions": [
                    {
                        "apiVersion": "wyrd/v1", "kind": "Prompt",
                        "metadata": { "name": "heavy", "version": "1.0.0", "space": "default" },
                        "spec": { "provider": "openai", "model": "gpt-4o", "messages": ["hello"] },
                        "artifacts": [{ "relative_path": "prompt.txt", "sha256": "YQ==", "size_bytes": 1 }]
                    },
                    {
                        "apiVersion": "wyrd/v1", "kind": "Prompt",
                        "metadata": { "name": "light", "version": "1.0.0", "space": "default" },
                        "spec": { "provider": "openai", "model": "gpt-4o", "messages": ["hello"] },
                        "artifacts": []
                    }
                ] }),
            ),
        )
        .await
        .expect("heavy composite responds");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(response).await["code"],
        "WYRD_REGISTRY_400_HEAVY_ARTIFACT_NOT_SOLE_SUBMISSION"
    );

    server.shutdown().await.expect("test server shuts down");
}

/// Concurrent identical registrations persist one operation and one card.
#[tokio::test(flavor = "current_thread")]
async fn concurrent_same_key_resolves_to_one_registration() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-concurrent-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let first = server.oneshot_authenticated(
        &jwt,
        prompt_registration_request("concurrent-prompt", "concurrent-key-001"),
    );
    let second = server.oneshot_authenticated(
        &jwt,
        prompt_registration_request("concurrent-prompt", "concurrent-key-001"),
    );
    let (first, second) = tokio::join!(first, second);
    assert_eq!(first.expect("first response").status(), StatusCode::CREATED);
    assert_eq!(
        second.expect("second response").status(),
        StatusCode::CREATED
    );

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let operation_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.card_registration_operations WHERE idempotency_key = $1",
    )
    .bind("concurrent-key-001")
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("operation count reads");
    let card_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.cards WHERE kind = 'Prompt' AND name = 'concurrent-prompt'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("card count reads");
    assert_eq!(operation_count, 1);
    assert_eq!(card_count, 1);
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

/// Persist a composite in leaf-first order and return the graph-selected root.
#[tokio::test(flavor = "current_thread")]
async fn composite_registration_returns_leaf_first_outcomes_and_root() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { id, jwt } = server
        .bootstrap_user("registry-composite-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let response = server
        .oneshot_authenticated(&jwt, three_card_composite_request("composite-key-001"))
        .await
        .expect("composite registration responds");
    let status = response.status();
    let body = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let names = body["outcomes"]
        .as_array()
        .expect("outcomes are an array")
        .iter()
        .map(|outcome| outcome["card_ref"]["name"].as_str().expect("name is text"))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec!["composite-prompt", "composite-agent", "composite-service"]
    );
    assert_eq!(body["root"]["name"], "composite-service");
    assert_eq!(body["outcomes"][0]["status"], "active");
    assert_eq!(body["outcomes"][1]["status"], "active");
    assert_eq!(body["outcomes"][2]["status"], "active");
    let service_uid: CardUid =
        serde_json::from_value(body["outcomes"][2]["card_ref"]["uid"].clone())
            .expect("service outcome contains a UID");
    let agent_uid: CardUid = serde_json::from_value(body["outcomes"][1]["card_ref"]["uid"].clone())
        .expect("agent outcome contains a UID");
    let prompt_uid: CardUid =
        serde_json::from_value(body["outcomes"][0]["card_ref"]["uid"].clone())
            .expect("prompt outcome contains a UID");
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging \
         WHERE operation = 'card.registration.create' AND principal_id = $1",
    )
    .bind(id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("audit count reads");
    assert_eq!(
        audit_count, 1,
        "one received request evaluates card:write once and audits it once"
    );
    let relationships: Vec<(Uuid, String, Uuid)> = sqlx::query_as(
        "SELECT card_uid, target_name, target_uid FROM wyrd.card_relationships \
         WHERE card_uid IN ($1, $2) ORDER BY target_name",
    )
    .bind(agent_uid.as_uuid())
    .bind(service_uid.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("relationship rows read");
    assert_eq!(relationships.len(), 2);
    assert_eq!(
        relationships,
        vec![
            (
                service_uid.as_uuid(),
                "composite-agent".to_owned(),
                agent_uid.as_uuid()
            ),
            (
                agent_uid.as_uuid(),
                "composite-prompt".to_owned(),
                prompt_uid.as_uuid()
            ),
        ]
    );
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

/// Initialize a sole heavy card after commit and advance its manifest row.
#[tokio::test(flavor = "current_thread")]
async fn heavy_registration_initializes_upload_after_commit() {
    let storage_root = tempfile::tempdir().expect("storage root creates");
    let server = WyrdTestServer::builder()
        .with_storage_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: storage_root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some(String::new()),
        })
        .start_bound()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-heavy-only-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let response = server
        .oneshot_authenticated(&jwt, heavy_registration_request("heavy-only-001"))
        .await
        .expect("heavy registration responds");
    let status = response.status();
    let body = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["outcomes"][0]["status"], "pending");
    assert_eq!(
        body["upload_plans"][0]["entries"][0]["relative_path"],
        "prompt.txt"
    );
    let card_uid: wyrd_spec::ids::CardUid =
        serde_json::from_value(body["outcomes"][0]["card_ref"]["uid"].clone())
            .expect("response contains card UID");

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let manifest: (String, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT upload_status, upload_id FROM wyrd.card_artifact_manifest \
         WHERE card_uid = $1 AND relative_path = 'prompt.txt'",
    )
    .bind(card_uid.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("manifest row reads");
    assert_eq!(manifest.0, "pending");
    assert!(manifest.1.is_some());
    conn.commit().await.expect("assertion transaction commits");

    let complete_request = || {
        Request::builder()
            .method(Method::POST)
            .uri(format!("/v1/cards/{card_uid}/complete"))
            .header("Idempotency-Key", "heavy-only-001")
            .body(Body::empty())
            .expect("card completion request builds")
    };
    let missing = server
        .oneshot_authenticated(&jwt, complete_request())
        .await
        .expect("missing-object completion responds");
    let missing_body = response_json(missing).await;
    assert_eq!(missing_body["code"], "WYRD_STORAGE_404_OBJECT_NOT_FOUND");

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let pending_after_missing: (String, Option<String>) =
        sqlx::query_as("SELECT status, card_blob_uri FROM wyrd.cards WHERE card_uid = $1")
            .bind(card_uid.as_uuid())
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("card state reads after missing object");
    assert_eq!(pending_after_missing.0, "pending");
    assert!(pending_after_missing.1.is_none());
    conn.commit().await.expect("assertion transaction commits");

    let upload_url = body["upload_plans"][0]["entries"][0]["plan"]["data"]["put_url"]
        .as_str()
        .expect("local upload plan contains a URL");
    let upload_path =
        Url::parse(upload_url).map_or_else(|_| upload_url.to_owned(), |url| url.path().to_owned());
    let upload = Request::builder()
        .method(Method::PUT)
        .uri(upload_path.clone())
        .body(Body::from("a"))
        .expect("local upload request builds");
    let upload_response = server
        .oneshot_authenticated(&jwt, upload)
        .await
        .expect("local upload responds");
    assert_eq!(upload_response.status(), StatusCode::OK);

    std::fs::write(
        storage_root
            .path()
            .join(server.data_tenant_id().to_string())
            .join("cards")
            .join(card_uid.to_string())
            .join("prompt.txt"),
        b"ab",
    )
    .expect("stored object is changed for size-mismatch coverage");

    let mismatch = server
        .oneshot_authenticated(&jwt, complete_request())
        .await
        .expect("size-mismatch completion responds");
    let mismatch_body = response_json(mismatch).await;
    assert_eq!(
        mismatch_body["code"],
        "WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED"
    );

    let upload = Request::builder()
        .method(Method::PUT)
        .uri(upload_path)
        .body(Body::from("a"))
        .expect("corrected local upload request builds");
    let upload_response = server
        .oneshot_authenticated(&jwt, upload)
        .await
        .expect("corrected local upload responds");
    assert_eq!(upload_response.status(), StatusCode::OK);

    let complete_response = server
        .oneshot_authenticated(&jwt, complete_request())
        .await
        .expect("card completion responds");
    assert_eq!(complete_response.status(), StatusCode::OK);
    let complete_body = response_json(complete_response).await;
    assert_eq!(complete_body["outcomes"][0]["status"], "active");
    assert_eq!(complete_body["outcomes"][0]["outcome"], "registered");

    let replay = server
        .oneshot_authenticated(&jwt, complete_request())
        .await
        .expect("completion replay responds");
    let replay_body = response_json(replay).await;
    assert_eq!(replay_body["outcomes"][0]["status"], "active");
    assert_eq!(replay_body["outcomes"][0]["outcome"], "idempotent_noop");

    server.shutdown().await.expect("test server shuts down");
}

/// The production reconciler retries a missing artifact and activates the Card
/// after the object becomes available.
#[tokio::test(flavor = "current_thread")]
async fn card_reconciler_recovers_after_storage_retry() {
    let storage_root = tempfile::tempdir().expect("storage root creates");
    let server = WyrdTestServer::builder()
        .with_storage_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: storage_root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some(String::new()),
        })
        .start_bound()
        .await
        .expect("bound test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-reconciler-recovery", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let response = server
        .oneshot_authenticated(&jwt, heavy_registration_request("reconcile-recovery-001"))
        .await
        .expect("heavy registration responds");
    let body = response_json(response).await;
    let card_uid: CardUid = serde_json::from_value(body["outcomes"][0]["card_ref"]["uid"].clone())
        .expect("response contains card UID");
    let upload_url = body["upload_plans"][0]["entries"][0]["plan"]["data"]["put_url"]
        .as_str()
        .expect("local upload plan contains a URL");

    wait_for_reconciliation_state(&server, &card_uid, 1, "pending").await;

    let upload_path =
        Url::parse(upload_url).map_or_else(|_| upload_url.to_owned(), |url| url.path().to_owned());
    let upload = Request::builder()
        .method(Method::PUT)
        .uri(upload_path)
        .body(Body::from("a"))
        .expect("local upload request builds");
    assert_eq!(
        server
            .oneshot_authenticated(&jwt, upload)
            .await
            .expect("local upload responds")
            .status(),
        StatusCode::OK
    );
    make_reconciliation_due(&server, &card_uid).await;

    for _ in 0..100 {
        let mut conn = server
            .tenant_conn_for(server.data_tenant_id())
            .await
            .expect("tenant connection opens");
        let state: (String, String, i32) = sqlx::query_as(
            "SELECT status, reconcile_status, reconcile_attempts \
               FROM wyrd.cards WHERE card_uid = $1",
        )
        .bind(card_uid.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("card state reads");
        conn.commit().await.expect("card state read commits");
        if state == ("active".to_owned(), "idle".to_owned(), 0) {
            server.shutdown().await.expect("test server shuts down");
            return;
        }
        sleep(Duration::from_millis(100)).await;
    }
    panic!("reconciler did not recover the Card after the object upload");
}

/// The production reconciler dead-letters exactly the third failed attempt and
/// records it as reconciliation lineage in the same durable state transition.
///
/// # Panics
/// Panics when the server cannot start, registration or reconciliation reads
/// fail, or the dead-letter state, error metadata, or attempt count differ.
#[tokio::test(flavor = "current_thread")]
async fn card_reconciler_dead_letters_after_three_failures() {
    let storage_root = tempfile::tempdir().expect("storage root creates");
    let server = WyrdTestServer::builder()
        .with_storage_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: storage_root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some(String::new()),
        })
        .start_bound()
        .await
        .expect("bound test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-reconciler-dead-letter", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let response = server
        .oneshot_authenticated(
            &jwt,
            heavy_registration_request("reconcile-dead-letter-001"),
        )
        .await
        .expect("heavy registration responds");
    let body = response_json(response).await;
    let card_uid: CardUid = serde_json::from_value(body["outcomes"][0]["card_ref"]["uid"].clone())
        .expect("response contains card UID");

    wait_for_reconciliation_state(&server, &card_uid, 1, "pending").await;
    make_reconciliation_due(&server, &card_uid).await;
    wait_for_reconciliation_state(&server, &card_uid, 2, "pending").await;
    make_reconciliation_due(&server, &card_uid).await;
    wait_for_reconciliation_state(&server, &card_uid, 3, "dead_lettered").await;

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let metadata: (String, String, Option<String>, Option<String>, i32) = sqlx::query_as(
        "SELECT reconcile_status, reconcile_kind, reconcile_last_error_code, \
                reconcile_last_error_message, reconcile_attempts \
           FROM wyrd.cards WHERE card_uid = $1",
    )
    .bind(card_uid.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("dead-letter metadata reads");
    assert_eq!(metadata.0, "dead_lettered");
    assert_eq!(metadata.1, "registration");
    assert!(
        metadata.2.is_some(),
        "dead letter stores the stable error code"
    );
    assert!(
        metadata.3.is_some(),
        "dead letter stores the inspectable error message"
    );
    assert_eq!(
        metadata.4, 3,
        "dead letter records its attempt count as reconciliation lineage"
    );
    conn.commit().await.expect("dead-letter read commits");

    sleep(Duration::from_millis(1_200)).await;
    let (status, attempts) = reconciliation_state(&server, &card_uid).await;
    assert_eq!(status, "dead_lettered");
    assert_eq!(attempts, 3, "dead-lettered Card receives no fourth attempt");
    server.shutdown().await.expect("test server shuts down");
}

/// Completion refuses and leaves the card Pending when its decision audit fails.
#[tokio::test(flavor = "current_thread")]
async fn completion_audit_failure_keeps_card_pending() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-completion-audit-failure", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let superuser = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION vala.test_fail_card_completion_audit()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             IF NEW.operation = 'card.registration.complete' THEN
               RAISE EXCEPTION 'injected completion audit failure';
             END IF;
             RETURN NEW;
           END;
           $$;"#,
    )
    .execute(&superuser)
    .await
    .expect("failure function installs");
    sqlx::query(
        r#"CREATE TRIGGER test_fail_card_completion_audit
           BEFORE INSERT ON vala.audit_staging
           FOR EACH ROW EXECUTE FUNCTION vala.test_fail_card_completion_audit()"#,
    )
    .execute(&superuser)
    .await
    .expect("failure trigger installs");

    let response = server
        .oneshot_authenticated(&jwt, heavy_registration_request("completion-audit-001"))
        .await
        .expect("heavy registration responds");
    let body = response_json(response).await;
    let card_uid: CardUid = serde_json::from_value(body["outcomes"][0]["card_ref"]["uid"].clone())
        .expect("response contains card UID");
    let upload_path = Url::parse(
        body["upload_plans"][0]["entries"][0]["plan"]["data"]["put_url"]
            .as_str()
            .expect("local upload plan contains a URL"),
    )
    .expect("upload URL parses")
    .path()
    .to_owned();
    let upload = Request::builder()
        .method(Method::PUT)
        .uri(upload_path)
        .body(Body::from("a"))
        .expect("local upload request builds");
    assert_eq!(
        server
            .oneshot_authenticated(&jwt, upload)
            .await
            .expect("local upload responds")
            .status(),
        StatusCode::OK
    );

    let complete = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cards/{card_uid}/complete"))
        .header("Idempotency-Key", "completion-audit-001")
        .body(Body::empty())
        .expect("card completion request builds");
    let completion = server
        .oneshot_authenticated(&jwt, complete)
        .await
        .expect("completion responds");
    let completion_status = completion.status();
    let completion_body = response_json(completion).await;
    assert_eq!(completion_status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(completion_body["code"], "WYRD_VALA_500_AUDIT_UNAVAILABLE");

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let card_state: (String, Option<String>) =
        sqlx::query_as("SELECT status, card_blob_uri FROM wyrd.cards WHERE card_uid = $1")
            .bind(card_uid.as_uuid())
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("card state reads");
    assert_eq!(card_state.0, "pending");
    assert!(card_state.1.is_none());
    let complete_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging WHERE operation = 'card.registration.complete'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("completion audit count reads");
    assert_eq!(complete_audits, 0);
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

/// Delete refuses and keeps the card Active when its decision audit fails.
#[tokio::test(flavor = "current_thread")]
async fn delete_audit_failure_keeps_card_active() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-delete-audit-failure", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("writer bootstrap returned a non-user principal");
    };
    let card_uid = seed_dependency(
        &server,
        server.data_tenant_id(),
        "delete-audit-failure",
        "active",
    )
    .await;
    let superuser = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION vala.test_fail_card_delete_audit()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             IF NEW.operation = 'card.registration.delete' THEN
               RAISE EXCEPTION 'injected delete audit failure';
             END IF;
             RETURN NEW;
           END;
           $$;"#,
    )
    .execute(&superuser)
    .await
    .expect("failure function installs");
    sqlx::query(
        r#"CREATE TRIGGER test_fail_card_delete_audit
           BEFORE INSERT ON vala.audit_staging
           FOR EACH ROW EXECUTE FUNCTION vala.test_fail_card_delete_audit()"#,
    )
    .execute(&superuser)
    .await
    .expect("failure trigger installs");

    let request = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/v1/cards/by-uid/Prompt/{card_uid}"))
        .body(Body::empty())
        .expect("delete request builds");
    let response = server
        .oneshot_authenticated(&jwt, request)
        .await
        .expect("delete responds");
    let status = response.status();
    let body = response_json(response).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], "WYRD_VALA_500_AUDIT_UNAVAILABLE");

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let card_status: String =
        sqlx::query_scalar("SELECT status FROM wyrd.cards WHERE card_uid = $1")
            .bind(card_uid.as_uuid())
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("card state reads");
    let delete_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging \
         WHERE operation = 'card.registration.delete' AND resource = $1",
    )
    .bind(format!("card:{card_uid}"))
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("delete audit count reads");
    assert_eq!(card_status, "active");
    assert_eq!(delete_audits, 0);
    conn.commit().await.expect("assertion transaction commits");
    server.shutdown().await.expect("test server shuts down");
}

/// A delete that finds no Card still records its one received verdict.
///
/// The not-found transaction commits nothing, so its in-transaction append
/// rolls back and the service records the allowed row standalone exactly once.
///
/// # Panics
/// Panics when the server cannot start, the delete is not a 404, or the
/// staged verdict rows are not exactly one `allowed`.
#[tokio::test(flavor = "current_thread")]
async fn delete_by_ref_not_found_records_one_decision() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-delete-missing", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("writer bootstrap returned a non-user principal");
    };

    let request = Request::builder()
        .method(Method::DELETE)
        .uri("/v1/cards/by-ref?kind=Prompt&space=default&name=never-registered&version=1.0.0")
        .body(Body::empty())
        .expect("delete request builds");
    let response = server
        .oneshot_authenticated(&jwt, request)
        .await
        .expect("delete responds");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let decisions: Vec<String> = sqlx::query_scalar(
        "SELECT outcome FROM vala.audit_staging \
         WHERE operation = 'card.registration.delete' AND resource LIKE '%never-registered%'",
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("delete decisions read");
    assert_eq!(decisions, vec!["allowed".to_owned()]);
    conn.commit().await.expect("assertion transaction commits");
    server.shutdown().await.expect("test server shuts down");
}

/// Backend cleanup failure leaves a committed tombstone and retryable blob state.
#[tokio::test(flavor = "current_thread")]
async fn delete_storage_failure_preserves_cleanup_state() {
    let storage_root = tempfile::tempdir().expect("storage root creates");
    let server = WyrdTestServer::builder()
        .with_storage_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: storage_root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some(String::new()),
        })
        .start_bound()
        .await
        .expect("bound test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-delete-storage-failure", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("writer bootstrap returned a non-user principal");
    };
    let card_uid = seed_dependency(
        &server,
        server.data_tenant_id(),
        "delete-storage-failure",
        "active",
    )
    .await;
    let moved_storage_root = storage_root.path().with_extension("moved");
    std::fs::rename(storage_root.path(), &moved_storage_root)
        .expect("storage root moves out of the configured path");
    std::fs::write(storage_root.path(), b"not a directory")
        .expect("broken storage root is created");

    let request = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/v1/cards/by-uid/Prompt/{card_uid}"))
        .body(Body::empty())
        .expect("delete request builds");
    let response = server
        .oneshot_authenticated(&jwt, request)
        .await
        .expect("delete responds");
    let status = response.status();
    let body = response_json(response).await;
    assert_eq!(
        status,
        StatusCode::from_u16(507).expect("507 is a valid status")
    );
    assert_eq!(body["code"], "WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED");

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let card_status: String =
        sqlx::query_scalar("SELECT status FROM wyrd.cards WHERE card_uid = $1")
            .bind(card_uid.as_uuid())
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("card state reads");
    assert_eq!(card_status, "deleted");
    conn.commit().await.expect("assertion transaction commits");
    // Shut down first: a live server's background storage work can recreate
    // the root between removing the broken file and restoring the directory.
    server.shutdown().await.expect("test server shuts down");
    std::fs::remove_file(storage_root.path()).expect("broken storage root removes");
    std::fs::rename(&moved_storage_root, storage_root.path()).expect("storage root restores");
}

/// A blob storage failure refuses registration and leaves durable failure state.
///
/// # Panics
/// Panics when the server cannot start, the registration succeeds, or the Card
/// is not left `failed` with no blob URI and a recorded failure time.
#[tokio::test(flavor = "current_thread")]
async fn blob_storage_failure_leaves_durable_failure_state() {
    let storage_root = tempfile::tempdir().expect("storage root creates");
    let server = WyrdTestServer::builder()
        .with_storage_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: storage_root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some(String::new()),
        })
        .start_bound()
        .await
        .expect("bound test server starts");
    let moved_storage_root = storage_root.path().with_extension("moved");
    std::fs::rename(storage_root.path(), &moved_storage_root)
        .expect("storage root moves out of the configured path");
    std::fs::write(storage_root.path(), b"not a directory")
        .expect("broken storage root is created");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-blob-storage-failure", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let response = server
        .oneshot_authenticated(
            &jwt,
            prompt_registration_request("blob-storage-failure", "blob-storage-failure-001"),
        )
        .await
        .expect("registration responds");
    let status = response.status();
    let body = response_json(response).await;
    assert_ne!(status, StatusCode::CREATED);
    assert!(body["code"].as_str().is_some());

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens after storage failure");
    let failure_state: (
        String,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT status, card_blob_uri, blob_failed_at FROM wyrd.cards \
             WHERE name = 'blob-storage-failure'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("failed card state reads");
    assert_eq!(failure_state.0, "failed");
    assert!(failure_state.1.is_none());
    assert!(failure_state.2.is_some());
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

/// Refuse the whole composite when the registration decision audit fails.
///
/// # Panics
/// Panics when the server cannot start, the registration is not refused, or a
/// Card row survives the failed decision audit.
#[tokio::test(flavor = "current_thread")]
async fn registration_refuses_when_its_decision_audit_fails() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-audit-failure-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let superuser = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION vala.test_fail_registration_decision_audit()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             IF NEW.operation = 'card.registration.create' THEN
               RAISE EXCEPTION 'injected registration audit failure';
             END IF;
             RETURN NEW;
           END;
           $$;"#,
    )
    .execute(&superuser)
    .await
    .expect("failure function installs");
    sqlx::query(
        r#"CREATE TRIGGER test_fail_registration_decision_audit
           BEFORE INSERT ON vala.audit_staging
           FOR EACH ROW EXECUTE FUNCTION vala.test_fail_registration_decision_audit()"#,
    )
    .execute(&superuser)
    .await
    .expect("failure trigger installs");

    let response = server
        .oneshot_authenticated(&jwt, three_card_composite_request("audit-failure-001"))
        .await
        .expect("registration responds");
    let status = response.status();
    let body = response_json(response).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(body["code"], "WYRD_VALA_500_AUDIT_UNAVAILABLE");

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let operation_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.card_registration_operations WHERE idempotency_key = $1",
    )
    .bind("audit-failure-001")
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("operation count reads");
    let card_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.cards WHERE name IN \
         ('composite-prompt', 'composite-agent', 'composite-service')",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("card count reads");
    assert_eq!(operation_count, 0);
    assert_eq!(card_count, 0);
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

/// Reject a cyclic sibling graph before reserving an idempotency operation.
#[tokio::test(flavor = "current_thread")]
async fn dependency_cycle_rejects_before_writes() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-cycle-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let response = server
        .oneshot_authenticated(
            &jwt,
            request_with_body(
                "cycle-001",
                json!({ "submissions": [
                    {
                        "apiVersion": "wyrd/v1", "kind": "Service",
                        "metadata": { "name": "cycle-a", "version": "1.0.0", "space": "default" },
                        "spec": { "components": [{ "alias": "b", "ref": { "sibling": {
                            "kind": "Service", "name": "cycle-b", "version": "1.0.0", "space": "default"
                        }}}] }, "artifacts": []
                    },
                    {
                        "apiVersion": "wyrd/v1", "kind": "Service",
                        "metadata": { "name": "cycle-b", "version": "1.0.0", "space": "default" },
                        "spec": { "components": [{ "alias": "a", "ref": { "sibling": {
                            "kind": "Service", "name": "cycle-a", "version": "1.0.0", "space": "default"
                        }}}] }, "artifacts": []
                    }
                ] }),
            ),
        )
        .await
        .expect("cyclic registration responds");
    let cycle_status = response.status();
    let cycle_body = response_json(response).await;
    assert_eq!(cycle_status, StatusCode::BAD_REQUEST, "{cycle_body}");
    assert_eq!(cycle_body["code"], "WYRD_REGISTRY_400_DEPENDENCY_CYCLE");

    server.shutdown().await.expect("test server shuts down");
}

/// Reject malformed Service peer composition through the raw authenticated HTTP route.
#[tokio::test(flavor = "current_thread")]
async fn service_peer_composition_rejects_before_registry_resolution() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-composition-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let invalid_component = server
        .oneshot_authenticated(
            &jwt,
            request_with_body(
                "invalid-component-001",
                json!({ "submissions": [{
                    "apiVersion": "wyrd/v1",
                    "kind": "Service",
                    "metadata": {
                        "name": "component-service",
                        "version": "1.0.0",
                        "space": "default"
                    },
                    "spec": { "components": [{
                        "alias": "quality",
                        "ref": {
                            "kind": "Verifier",
                            "name": "quality",
                            "version": "1.0.0",
                            "space": "default"
                        }
                    }] },
                    "artifacts": []
                }] }),
            ),
        )
        .await
        .expect("invalid component registration responds");
    assert_eq!(invalid_component.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(invalid_component).await["code"],
        "WYRD_SPEC_400_INVALID_SERVICE_COMPONENT_KIND"
    );

    let orphan_peer = server
        .oneshot_authenticated(
            &jwt,
            request_with_body(
                "orphan-peer-001",
                json!({ "submissions": [
                    {
                        "apiVersion": "wyrd/v1",
                        "kind": "Service",
                        "metadata": {
                            "name": "orphan-service",
                            "version": "1.0.0",
                            "space": "default"
                        },
                        "spec": {},
                        "artifacts": []
                    },
                    {
                        "apiVersion": "wyrd/v1",
                        "kind": "Verifier",
                        "metadata": {
                            "name": "orphan-quality",
                            "version": "1.0.0",
                            "space": "default"
                        },
                        "spec": { "implementation": { "kind": "eval", "spec": { "tasks": {} } } },
                        "artifacts": []
                    }
                ] }),
            ),
        )
        .await
        .expect("orphan peer registration responds");
    assert_eq!(orphan_peer.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(orphan_peer).await["code"],
        "WYRD_SPEC_400_UNBOUND_VERIFIER_PEER"
    );

    server.shutdown().await.expect("test server shuts down");
}

/// Canonical hashing replays an identical graph authored in another wire order.
#[tokio::test(flavor = "current_thread")]
async fn wire_order_permutation_replays_identical_graph() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-order-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };

    let first = server
        .oneshot_authenticated(&jwt, three_card_composite_request("wire-order-001"))
        .await
        .expect("first registration responds");
    let first_status = first.status();
    let first_body = response_json(first).await;
    assert_eq!(first_status, StatusCode::CREATED, "{first_body}");
    let replay = server
        .oneshot_authenticated(
            &jwt,
            permuted_three_card_composite_request("wire-order-001"),
        )
        .await
        .expect("permuted registration responds");
    let replay_status = replay.status();
    let replay_body = response_json(replay).await;
    assert_eq!(replay_status, StatusCode::CREATED, "{replay_body}");
    assert_eq!(replay_body["root"], first_body["root"]);
    assert_eq!(replay_body["outcomes"].as_array().map(Vec::len), Some(3));
    assert!(
        replay_body["outcomes"]
            .as_array()
            .expect("outcomes are an array")
            .iter()
            .all(|outcome| outcome["outcome"] == "idempotent_noop")
    );

    server.shutdown().await.expect("test server shuts down");
}

#[tokio::test(flavor = "current_thread")]
/// Public Card reads preserve graph relationships, pagination, tenant isolation, and tombstones.
async fn card_reads_list_latest_and_delete_are_tenant_safe() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("registry-reads-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("writer bootstrap returned a non-user principal");
    };
    let Bootstrap::User {
        jwt: denied_jwt, ..
    } = server
        .bootstrap_user("registry-reads-denied", &[])
        .await
        .expect("denied user bootstraps")
    else {
        panic!("denied bootstrap returned a non-user principal");
    };

    let registered = server
        .oneshot_authenticated(&jwt, three_card_composite_request("reads-001"))
        .await
        .expect("composite registration responds");
    let registered_status = registered.status();
    let registered_body = response_json(registered).await;
    assert_eq!(registered_status, StatusCode::CREATED, "{registered_body}");
    let outcomes = registered_body["outcomes"]
        .as_array()
        .expect("registration outcomes are an array");
    let service = outcomes
        .iter()
        .find(|outcome| outcome["card_ref"]["kind"] == "Service")
        .expect("service outcome exists");
    let prompt = outcomes
        .iter()
        .find(|outcome| outcome["card_ref"]["kind"] == "Prompt")
        .expect("prompt outcome exists");
    let prompt_name = prompt["card_ref"]["name"]
        .as_str()
        .expect("prompt name exists");
    let service_uid = service["card_ref"]["uid"]
        .as_str()
        .expect("service UID exists")
        .to_owned();

    let by_uid = server
        .oneshot_authenticated(
            &jwt,
            Request::builder()
                .method(Method::GET)
                .uri(format!("/v1/cards/by-uid/Service/{service_uid}"))
                .body(Body::empty())
                .expect("UID read request builds"),
        )
        .await
        .expect("UID read responds");
    assert_eq!(by_uid.status(), StatusCode::OK);
    let service_body = response_json(by_uid).await;
    assert_eq!(service_body["card"]["status"]["phase"], "active");
    assert!(
        !service_body["card"]["relationships"]["outbound"]
            .as_array()
            .expect("outbound relationships are an array")
            .is_empty()
    );

    let by_ref = server
        .oneshot_authenticated(
            &jwt,
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/v1/cards/by-ref?kind=Prompt&space=default&name={prompt_name}&version=1.0.0"
                ))
                .body(Body::empty())
                .expect("ref read request builds"),
        )
        .await
        .expect("ref read responds");
    assert_eq!(by_ref.status(), StatusCode::OK);
    let prompt_body = response_json(by_ref).await;
    assert_eq!(prompt_body["card"]["metadata"]["name"], "composite-prompt");
    assert!(
        !prompt_body["card"]["relationships"]["inbound"]
            .as_array()
            .expect("inbound relationships are an array")
            .is_empty()
    );

    let latest = server
        .oneshot_authenticated(
            &jwt,
            Request::builder()
                .method(Method::GET)
                .uri("/v1/cards/Prompt/default/composite-prompt/latest")
                .body(Body::empty())
                .expect("latest read request builds"),
        )
        .await
        .expect("latest read responds");
    assert_eq!(latest.status(), StatusCode::OK);
    assert_eq!(
        response_json(latest).await["card"]["metadata"]["version"],
        "1.0.0"
    );

    let versions = server
        .oneshot_authenticated(
            &jwt,
            Request::builder()
                .method(Method::GET)
                .uri("/v1/cards/Prompt/default/composite-prompt/versions")
                .body(Body::empty())
                .expect("versions request builds"),
        )
        .await
        .expect("versions responds");
    assert_eq!(versions.status(), StatusCode::OK);
    assert_eq!(
        response_json(versions).await["versions"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    let first_page = server
        .oneshot_authenticated(
            &jwt,
            Request::builder()
                .method(Method::GET)
                .uri("/v1/cards?limit=2")
                .body(Body::empty())
                .expect("list request builds"),
        )
        .await
        .expect("list responds");
    let first_page_status = first_page.status();
    let first_page_body = response_json(first_page).await;
    assert_eq!(first_page_status, StatusCode::OK, "{first_page_body}");
    assert_eq!(first_page_body["items"].as_array().map(Vec::len), Some(2));
    let cursor = first_page_body["next_cursor"]
        .as_str()
        .expect("list returns a cursor");
    let second_page = server
        .oneshot_authenticated(
            &jwt,
            Request::builder()
                .method(Method::GET)
                .uri(format!("/v1/cards?limit=2&cursor={cursor}"))
                .body(Body::empty())
                .expect("cursor list request builds"),
        )
        .await
        .expect("cursor list responds");
    assert_eq!(second_page.status(), StatusCode::OK);
    assert_eq!(
        response_json(second_page).await["items"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    let denied = server
        .oneshot_authenticated(
            &denied_jwt,
            Request::builder()
                .method(Method::GET)
                .uri(format!("/v1/cards/by-uid/Service/{service_uid}"))
                .body(Body::empty())
                .expect("denied read request builds"),
        )
        .await
        .expect("denied read responds");
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let deleted = server
        .oneshot_authenticated(
            &jwt,
            Request::builder()
                .method(Method::DELETE)
                .uri("/v1/cards/by-ref?kind=Service&space=default&name=composite-service&version=1.0.0")
                .body(Body::empty())
                .expect("delete request builds"),
        )
        .await
        .expect("delete responds");
    let deleted_status = deleted.status();
    let deleted_body = response_json(deleted).await;
    assert_eq!(deleted_status, StatusCode::OK, "{deleted_body}");
    assert_eq!(deleted_body["deleted"], true);

    let deleted_read = server
        .oneshot_authenticated(
            &jwt,
            Request::builder()
                .method(Method::GET)
                .uri(format!("/v1/cards/by-uid/Service/{service_uid}"))
                .body(Body::empty())
                .expect("deleted read request builds"),
        )
        .await
        .expect("deleted read responds");
    assert_eq!(deleted_read.status(), StatusCode::NOT_FOUND);

    server.shutdown().await.expect("test server shuts down");
}

/// Seed the active dependencies every bound-Service registration names.
///
/// Inserts the `vb-prompt` Prompt, the `vb-eval` eval Verifier, the
/// `vb-drift` drift Verifier, and the daily `vb-schedule` Trigger, returning
/// the Trigger's UID so a test can prove it is frozen by identity.
///
/// # Panics
/// Panics when a fixture row cannot be inserted.
async fn seed_binding_dependencies(server: &WyrdTestServer) -> CardUid {
    let tenant = server.data_tenant_id();
    seed_dependency(server, tenant, "vb-prompt", "active").await;
    seed_card(
        server,
        tenant,
        "Verifier",
        "vb-eval",
        json!({ "implementation": { "kind": "eval", "spec": { "tasks": {} } } }),
        "active",
    )
    .await;
    seed_card(
        server,
        tenant,
        "Verifier",
        "vb-drift",
        json!({ "implementation": { "kind": "drift", "spec": {
            "method": "Psi",
            "signal": {
                "kind": "Distribution",
                "baseline_ref": {
                    "kind": "Data", "name": "vb-baseline", "space": "default", "version": "1.0.0"
                },
                "features": ["tenure_months"]
            },
            "condition": { "kind": "Statistical" },
            "profile": {
                "kind": "Psi",
                "binning_strategy": { "kind": "Quantile", "n_bins": 10 },
                "threshold": { "kind": "Fixed", "value": 0.25 }
            }
        } } }),
        "active",
    )
    .await;

    seed_card(
        server,
        tenant,
        "Trigger",
        "vb-schedule",
        json!({ "kind": "schedule", "cron": "0 2 * * *" }),
        "active",
    )
    .await
}

/// The reference to the seeded `vb-schedule` Trigger.
fn schedule_trigger_ref() -> Value {
    json!({ "kind": "Trigger", "name": "vb-schedule", "version": "1.0.0", "space": "default" })
}

/// Build a Service-root registration whose Service and component both own a binding.
///
/// The Service-level binding runs the drift Verifier on `runs_on` (a
/// referenced or inline `schedule` Trigger) and dispatches one inline HTTP
/// Operator on failure; the `assistant` component binding runs the eval
/// Verifier on an inline `observations_ready` Trigger. `second_alias` adds a second component occurrence
/// under that alias so the alias-uniqueness refusal can reuse the builder.
fn bound_service_request(
    service_name: &str,
    runs_on: Value,
    second_alias: Option<&str>,
    idempotency_key: &str,
) -> Request<Body> {
    request_with_body(
        idempotency_key,
        bound_service_body(service_name, runs_on, second_alias),
    )
}

/// The JSON registration body [`bound_service_request`] posts.
///
/// Returned separately so a test can rewrite one binding, such as its
/// `on_failure` list, before posting it with [`request_with_body`].
fn bound_service_body(service_name: &str, runs_on: Value, second_alias: Option<&str>) -> Value {
    let agent_ref = json!({ "sibling": {
        "kind": "Agent", "name": "vb-agent", "version": "1.0.0", "space": "default"
    } });
    let mut components = vec![json!({
        "alias": "assistant",
        "ref": agent_ref,
        "verified_by": [{
            "verifier": {
                "kind": "Verifier", "name": "vb-eval", "version": "1.0.0", "space": "default"
            },
            "runs_on": { "kind": "observations_ready" }
        }]
    })];
    if let Some(alias) = second_alias {
        components.push(json!({ "alias": alias, "ref": agent_ref }));
    }
    json!({ "submissions": [
            {
                "apiVersion": "wyrd/v1", "kind": "Service",
                "metadata": { "name": service_name, "version": "1.0.0", "space": "default" },
                "spec": {
                    "components": components,
                    "verified_by": [{
                        "verifier": {
                            "kind": "Verifier", "name": "vb-drift",
                            "version": "1.0.0", "space": "default"
                        },
                        "runs_on": runs_on,
                        "on_failure": [{
                            "kind": "http", "method": "post",
                            "url": "https://hooks.example.test/verification-failed"
                        }]
                    }]
                },
                "artifacts": []
            },
            {
                "apiVersion": "wyrd/v1", "kind": "Agent",
                "metadata": { "name": "vb-agent", "version": "1.0.0", "space": "default" },
                "spec": { "prompt": {
                    "kind": "Prompt", "name": "vb-prompt", "version": "1.0.0", "space": "default"
                } },
                "artifacts": []
            }
        ] })
}

/// Read one Card by UID through the public route as `jwt`.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn read_card(server: &WyrdTestServer, jwt: &str, kind: &str, uid: &str) -> Response<Body> {
    server
        .oneshot_authenticated(
            jwt,
            Request::builder()
                .method(Method::GET)
                .uri(format!("/v1/cards/by-uid/{kind}/{uid}"))
                .body(Body::empty())
                .expect("UID read request builds"),
        )
        .await
        .expect("UID read responds")
}

/// One projected binding's owner activity, activation, and schedule cursor.
type BindingActivityRow = (Option<DateTime<Utc>>, String, Option<DateTime<Utc>>);

/// Read every binding of `owner` with its principal's activity, ordered by activation.
///
/// # Panics
/// Panics when the tenant connection or the read fails.
async fn owner_activity(server: &WyrdTestServer, owner: Uuid) -> Vec<BindingActivityRow> {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let rows = sqlx::query_as(
        "SELECT p.last_authenticated_at, b.activation, b.next_run_at \
           FROM wyrd.verification_bindings b \
           JOIN wyrd.auth_service_accounts p ON p.card_uid = b.owner_card_uid \
          WHERE b.owner_card_uid = $1 \
          ORDER BY b.activation",
    )
    .bind(owner)
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("activity reads");
    conn.commit().await.expect("assertion transaction commits");
    rows
}

/// Served binding IDs are stable, authorized, tenant-isolated, and activate
/// only through the owner's real API-key exchange.
///
/// Registers a Service owning a Service-level binding on a referenced
/// schedule Trigger with an inline Operator, plus a component
/// observations-ready binding; checks the referenced Trigger is frozen by UID
/// and the inline Operator by digest; then reads `status.verification.binding_ids`
/// through the public route, re-applies the identical graph, and checks the
/// under-privileged and cross-tenant reads. Finally it exchanges an API key
/// for the Service's own projected principal through `/auth/token` and proves
/// that exchange stamped the owner's activity and armed only the schedule
/// cursor. An unarmable schedule and a repeated component alias are refused
/// before any write.
///
/// # Panics
/// Panics when the server fails to start or stop, a bootstrap or fixture
/// write fails, a route fails to respond, or any status, error code, binding
/// identity, or activity assertion does not hold.
#[tokio::test(flavor = "current_thread")]
async fn owner_status_serves_stable_binding_ids_and_exchange_activates() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("binding-status-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let Bootstrap::User {
        jwt: denied_jwt, ..
    } = server
        .bootstrap_user("binding-status-denied", &[])
        .await
        .expect("denied user bootstraps")
    else {
        panic!("denied bootstrap returned a non-user principal");
    };
    let tenant = server.data_tenant_id();
    let trigger_uid = seed_binding_dependencies(&server).await;
    let schedule_trigger = schedule_trigger_ref();

    let bad_cron = server
        .oneshot_authenticated(
            &jwt,
            bound_service_request(
                "vb-bad-cron",
                json!({ "kind": "schedule", "cron": "not a cron" }),
                None,
                "vb-bad-cron-operation",
            ),
        )
        .await
        .expect("bad-cron registration responds");
    assert_eq!(bad_cron.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(bad_cron).await["code"],
        "WYRD_REGISTRY_400_INVALID_CARD_SPEC"
    );
    assert_no_registration_writes(&server, "vb-bad-cron-operation", "vb-bad-cron").await;

    let never = server
        .oneshot_authenticated(
            &jwt,
            bound_service_request(
                "vb-never-cron",
                json!({ "kind": "schedule", "cron": "0 0 30 2 *" }),
                None,
                "vb-never-cron-operation",
            ),
        )
        .await
        .expect("never-firing registration responds");
    assert_eq!(never.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(never).await["code"],
        "WYRD_REGISTRY_400_INVALID_CARD_SPEC"
    );
    assert_no_registration_writes(&server, "vb-never-cron-operation", "vb-never-cron").await;

    let reserved = server
        .oneshot_authenticated(
            &jwt,
            bound_service_request(
                "vb-reserved-alias",
                schedule_trigger.clone(),
                Some(wyrd_spec::card::verifier::OWNER_OCCURRENCE_KEY),
                "vb-reserved-alias-operation",
            ),
        )
        .await
        .expect("reserved-alias registration responds");
    assert_eq!(reserved.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(reserved).await["code"],
        "WYRD_REGISTRY_400_INVALID_CARD_SPEC"
    );
    assert_no_registration_writes(&server, "vb-reserved-alias-operation", "vb-reserved-alias")
        .await;

    let duplicate = server
        .oneshot_authenticated(
            &jwt,
            bound_service_request(
                "vb-dup-alias",
                schedule_trigger.clone(),
                Some("assistant"),
                "vb-dup-alias-operation",
            ),
        )
        .await
        .expect("duplicate-alias registration responds");
    assert_eq!(duplicate.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(duplicate).await["code"],
        "WYRD_REGISTRY_400_INVALID_CARD_SPEC"
    );
    assert_no_registration_writes(&server, "vb-dup-alias-operation", "vb-dup-alias").await;

    let registered = server
        .oneshot_authenticated(
            &jwt,
            bound_service_request(
                "vb-service",
                schedule_trigger.clone(),
                None,
                "vb-first-operation",
            ),
        )
        .await
        .expect("bound registration responds");
    let registered_status = registered.status();
    let registered_body = response_json(registered).await;
    assert_eq!(registered_status, StatusCode::CREATED, "{registered_body}");
    let outcome_uid = |kind: &str| {
        registered_body["outcomes"]
            .as_array()
            .expect("registration outcomes are an array")
            .iter()
            .find(|outcome| outcome["card_ref"]["kind"] == kind)
            .and_then(|outcome| outcome["card_ref"]["uid"].as_str())
            .expect("outcome UID exists")
            .to_owned()
    };
    let service_uid = outcome_uid("Service");
    let agent_uid = outcome_uid("Agent");

    let service = response_json(read_card(&server, &jwt, "Service", &service_uid).await).await;
    let binding_ids = service["card"]["status"]["verification"]["binding_ids"].clone();
    let ids = binding_ids
        .as_array()
        .expect("Service status lists binding IDs");
    assert_eq!(ids.len(), 2, "Service-level and component bindings");
    for id in ids {
        let id = Uuid::parse_str(id.as_str().expect("binding ID is a string"))
            .expect("binding ID is a UUID");
        assert_eq!(id.get_version_num(), 7);
    }
    assert!(
        service["card"]["spec"]["verified_by"][0]
            .get("binding_id")
            .is_none(),
        "authored spec is not rewritten"
    );
    let mut conn = server
        .tenant_conn_for(tenant)
        .await
        .expect("tenant connection opens");
    let (frozen_trigger, operators): (Option<Uuid>, Value) = sqlx::query_as(
        "SELECT trigger_uid, operators FROM wyrd.verification_bindings \
          WHERE owner_card_uid = $1 AND activation = 'schedule'",
    )
    .bind(Uuid::parse_str(&service_uid).expect("service UID parses"))
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("frozen targets read");
    conn.commit().await.expect("assertion transaction commits");
    assert_eq!(frozen_trigger, Some(trigger_uid.as_uuid()));
    assert!(
        operators[0]["digest"].is_string(),
        "an inline Operator is frozen as its canonical digest: {operators}"
    );
    let agent = response_json(read_card(&server, &jwt, "Agent", &agent_uid).await).await;
    assert!(
        agent["card"]["status"].get("verification").is_none(),
        "a subject that owns no binding serves no verification status"
    );

    let reapplied = server
        .oneshot_authenticated(
            &jwt,
            bound_service_request(
                "vb-service",
                schedule_trigger.clone(),
                None,
                "vb-reapply-operation",
            ),
        )
        .await
        .expect("re-applied registration responds");
    assert!(reapplied.status().is_success());
    let service = response_json(read_card(&server, &jwt, "Service", &service_uid).await).await;
    assert_eq!(
        service["card"]["status"]["verification"]["binding_ids"], binding_ids,
        "re-apply preserves binding identity"
    );

    let denied = read_card(&server, &denied_jwt, "Service", &service_uid).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let other_tenant = server
        .seed_tenant("binding-status-other")
        .await
        .expect("second tenant seeds");
    let Bootstrap::Machine {
        api_key: other_key, ..
    } = server
        .bootstrap_service_in_tenant(other_tenant, "binding-status-foreign", &["writer"])
        .await
        .expect("foreign reader bootstraps")
    else {
        panic!("service bootstrap returned a non-machine principal");
    };
    let other_jwt = server
        .exchange_api_key(&other_key)
        .await
        .expect("foreign key exchanges");
    let foreign = read_card(&server, &other_jwt, "Service", &service_uid).await;
    assert_eq!(foreign.status(), StatusCode::NOT_FOUND);

    let service_ref: wyrd_spec::reference::CardRef = serde_json::from_value(json!({
        "kind": "Service", "name": "vb-service", "version": "1.0.0", "space": "default"
    }))
    .expect("service ref decodes");
    let Bootstrap::Machine { api_key, .. } = server
        .credential_registered_service(&service_ref, &[])
        .await
        .expect("registered Service is credentialed")
    else {
        panic!("credentialed Service returned a non-machine principal");
    };
    let service_uid = Uuid::parse_str(&service_uid).expect("service UID parses");
    assert!(
        owner_activity(&server, service_uid)
            .await
            .iter()
            .all(|(seen, _, cursor)| seen.is_none() && cursor.is_none()),
        "registration alone never activates the owner"
    );

    server
        .exchange_api_key(&api_key)
        .await
        .expect("owner API key exchanges");
    let rows = owner_activity(&server, service_uid).await;
    assert_eq!(rows.len(), 2);
    let (seen, _, eval_cursor) = &rows[0];
    let seen = seen.expect("API-key exchange records owner activity");
    assert_eq!(rows[0].1, "observations_ready");
    assert_eq!(*eval_cursor, None, "an eval binding never has a cursor");
    assert_eq!(rows[1].1, "schedule");
    assert!(
        rows[1].2.expect("first exchange arms the schedule") > seen,
        "the armed cursor is the next future boundary"
    );

    server.shutdown().await.expect("test server shuts down");
}

/// List `principal`'s registration audit rows as (permission, outcome), sorted.
///
/// # Panics
/// Panics when the tenant connection or the read fails.
async fn registration_audits(server: &WyrdTestServer, principal: Uuid) -> Vec<(String, String)> {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let rows = sqlx::query_as(
        "SELECT permission, outcome FROM vala.audit_staging \
          WHERE operation = 'card.registration.create' AND principal_id = $1 \
          ORDER BY permission, outcome",
    )
    .bind(principal)
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("audit rows read");
    conn.commit().await.expect("assertion transaction commits");
    rows
}

/// Operator-bearing registration also needs `operators:invoke`, audited once.
///
/// A caller holding only `cards:write` registers an Operator-free bound
/// Service with one allowed `cards:write` row, but an inline or referenced
/// `on_failure` Operator is refused with the stable 403, writes no Card,
/// principal, binding, or operation, and audits the allowed `cards:write` and
/// the denied `operators:invoke`. A caller holding both commits the
/// registration with exactly one allowed row per permission.
///
/// # Panics
/// Panics when the server fails to start or stop, a fixture write fails, a
/// route fails to respond, or any status, write, or audit expectation fails.
#[tokio::test(flavor = "current_thread")]
async fn operator_bearing_registration_requires_operator_invoke() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let tenant = server.data_tenant_id();
    seed_binding_dependencies(&server).await;
    seed_card(
        &server,
        tenant,
        "Operator",
        "vb-hook",
        json!({
            "kind": "http", "method": "post",
            "url": "https://hooks.example.test/verification-failed"
        }),
        "active",
    )
    .await;
    let card_write = wyrd_runtime::Permission::card_write();
    let operator_invoke = wyrd_runtime::Permission::operator_invoke();
    server
        .seed_role("vb_cards_only", std::slice::from_ref(&card_write))
        .await
        .expect("cards-only role seeds");
    server
        .seed_role("vb_cards_and_operators", &[card_write, operator_invoke])
        .await
        .expect("cards-and-operators role seeds");
    let Bootstrap::User {
        jwt: cards_only,
        id: cards_only_id,
        ..
    } = server
        .bootstrap_user("vb-cards-only-user", &["vb_cards_only"])
        .await
        .expect("cards-only user bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let Bootstrap::User {
        jwt: both,
        id: both_id,
        ..
    } = server
        .bootstrap_user("vb-both-user", &["vb_cards_and_operators"])
        .await
        .expect("cards-and-operators user bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let with_on_failure = |name: &str, on_failure: Value| {
        let mut body = bound_service_body(name, schedule_trigger_ref(), None);
        body["submissions"][0]["spec"]["verified_by"][0]["on_failure"] = on_failure;
        body
    };
    let allowed = |permission: &str| (permission.to_owned(), "allowed".to_owned());
    let denied = |permission: &str| (permission.to_owned(), "denied".to_owned());

    let free = server
        .oneshot_authenticated(
            &cards_only,
            request_with_body("vb-free-operation", with_on_failure("vb-free", json!([]))),
        )
        .await
        .expect("operator-free registration responds");
    let free_status = free.status();
    assert_eq!(
        free_status,
        StatusCode::CREATED,
        "{}",
        response_json(free).await
    );
    assert_eq!(
        registration_audits(&server, cards_only_id.as_uuid()).await,
        vec![allowed("cards:write")],
        "operator-free registration spends only cards:write"
    );

    let refused = [
        (
            "vb-inline-op",
            json!([{
                "kind": "http", "method": "post",
                "url": "https://hooks.example.test/verification-failed"
            }]),
        ),
        (
            "vb-referenced-op",
            json!([{
                "kind": "Operator", "name": "vb-hook", "version": "1.0.0", "space": "default"
            }]),
        ),
    ];
    for (name, on_failure) in refused {
        let operation = format!("{name}-operation");
        let response = server
            .oneshot_authenticated(
                &cards_only,
                request_with_body(&operation, with_on_failure(name, on_failure)),
            )
            .await
            .expect("operator-bearing registration responds");
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{name}");
        assert_eq!(
            response_json(response).await["code"],
            "WYRD_PERMISSION_403_DENIED_RBAC"
        );
        assert_no_registration_writes(&server, &operation, name).await;
    }
    assert_eq!(
        registration_audits(&server, cards_only_id.as_uuid()).await,
        vec![
            allowed("cards:write"),
            allowed("cards:write"),
            allowed("cards:write"),
            denied("operators:invoke"),
            denied("operators:invoke"),
        ],
        "each refusal records the evaluated cards:write and the operators:invoke denial"
    );

    let accepted = server
        .oneshot_authenticated(
            &both,
            request_with_body(
                "vb-both-operation",
                with_on_failure(
                    "vb-both",
                    json!([{
                        "kind": "Operator", "name": "vb-hook", "version": "1.0.0",
                        "space": "default"
                    }]),
                ),
            ),
        )
        .await
        .expect("authorized operator-bearing registration responds");
    let accepted_status = accepted.status();
    assert_eq!(
        accepted_status,
        StatusCode::CREATED,
        "{}",
        response_json(accepted).await
    );
    assert_eq!(
        registration_audits(&server, both_id.as_uuid()).await,
        vec![allowed("cards:write"), allowed("operators:invoke")],
        "one allowed row per evaluated permission commits with the registration"
    );

    server.shutdown().await.expect("test server shuts down");
}

/// Register the bound `vb-twin` Service in `space` and return its Service UID.
///
/// # Panics
/// Panics when the registration route fails to respond, refuses the graph, or
/// returns no Service outcome UID.
async fn register_twin(server: &WyrdTestServer, jwt: &str, space: &str) -> Uuid {
    let mut body = bound_service_body("vb-twin", schedule_trigger_ref(), None);
    body["submissions"][0]["metadata"]["space"] = json!(space);
    let response = server
        .oneshot_authenticated(
            jwt,
            request_with_body(&format!("vb-twin-{space}-operation"), body),
        )
        .await
        .expect("twin registration responds");
    let status = response.status();
    let body = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let uid = body["outcomes"]
        .as_array()
        .expect("registration outcomes are an array")
        .iter()
        .find(|outcome| outcome["card_ref"]["kind"] == "Service")
        .and_then(|outcome| outcome["card_ref"]["uid"].as_str())
        .expect("Service outcome UID exists");
    Uuid::parse_str(uid).expect("Service UID parses")
}

/// POST `/auth/issue-key` for `card_ref` as `jwt` and return status and body.
///
/// # Panics
/// Panics when the request cannot be built or the route fails to respond.
async fn issue_key(server: &WyrdTestServer, jwt: &str, card_ref: Value) -> (StatusCode, Value) {
    let response = server
        .oneshot_authenticated(
            jwt,
            Request::builder()
                .method(Method::POST)
                .uri("/auth/issue-key")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "card_ref": card_ref }).to_string()))
                .expect("issue-key request builds"),
        )
        .await
        .expect("issue-key responds");
    let status = response.status();
    (status, response_json(response).await)
}

/// Whether every binding row of `owner` still has no recorded activity.
///
/// # Panics
/// Panics when the activity read fails.
async fn never_activated(server: &WyrdTestServer, owner: Uuid) -> bool {
    owner_activity(server, owner)
        .await
        .iter()
        .all(|(seen, _, cursor)| seen.is_none() && cursor.is_none())
}

/// One Card identity in two spaces resolves only through an exact reference.
///
/// Registers the same bound Service kind, name, and version in the `default`
/// and `blue` spaces, which projects two distinct Card-bound principals. An
/// explicit-space `card_ref` issues a key for and — through the real API-key
/// exchange — activates only its own space's owner; the same ref with its
/// space omitted matches both principals and fails closed with the stable
/// principal-not-found refusal, leaving both owners' activity untouched.
///
/// # Panics
/// Panics when the server fails to start or stop, a bootstrap or fixture write
/// fails, a route fails to respond, or any status, code, or activity
/// expectation does not hold.
#[tokio::test(flavor = "current_thread")]
async fn same_card_in_two_spaces_resolves_only_exact_refs() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    seed_binding_dependencies(&server).await;
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("vb-twin-admin", &["writer", "runtime_admin"])
        .await
        .expect("admin bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let default_owner = register_twin(&server, &jwt, "default").await;
    let blue_owner = register_twin(&server, &jwt, "blue").await;
    let twin_ref = |space: Option<&str>| {
        let mut card_ref = json!({ "kind": "Service", "name": "vb-twin", "version": "1.0.0" });
        if let Some(space) = space {
            card_ref["space"] = json!(space);
        }
        card_ref
    };

    let (status, ambiguous) = issue_key(&server, &jwt, twin_ref(None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{ambiguous}");
    assert_eq!(ambiguous["code"], "WYRD_AUTH_404_PRINCIPAL_NOT_FOUND");
    assert!(never_activated(&server, default_owner).await);
    assert!(never_activated(&server, blue_owner).await);

    let (status, issued) = issue_key(&server, &jwt, twin_ref(Some("blue"))).await;
    assert_eq!(status, StatusCode::OK, "{issued}");
    let key = SecretString::from(issued["key"].as_str().expect("issued key").to_owned());
    assert!(
        never_activated(&server, blue_owner).await,
        "issuance never activates the owner"
    );
    server
        .exchange_api_key(&key)
        .await
        .expect("exact owner key exchanges");
    assert!(
        owner_activity(&server, blue_owner)
            .await
            .iter()
            .all(|(seen, _, _)| seen.is_some()),
        "the exchange activates the exact owner"
    );
    assert!(
        never_activated(&server, default_owner).await,
        "the same-named owner in another space is untouched"
    );

    server.shutdown().await.expect("test server shuts down");
}

/// Register the bound `vb-live` Service at `version` and return its Service UID.
///
/// # Panics
/// Panics when the registration route fails to respond, refuses the graph, or
/// returns no Service outcome UID.
async fn register_live(server: &WyrdTestServer, jwt: &str, version: &str) -> Uuid {
    let mut body = bound_service_body("vb-live", schedule_trigger_ref(), None);
    body["submissions"][0]["metadata"]["version"] = json!(version);
    let response = server
        .oneshot_authenticated(
            jwt,
            request_with_body(&format!("vb-live-{version}-operation"), body),
        )
        .await
        .expect("live registration responds");
    let status = response.status();
    let body = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let uid = body["outcomes"]
        .as_array()
        .expect("registration outcomes are an array")
        .iter()
        .find(|outcome| outcome["card_ref"]["kind"] == "Service")
        .and_then(|outcome| outcome["card_ref"]["uid"].as_str())
        .expect("Service outcome UID exists");
    Uuid::parse_str(uid).expect("Service UID parses")
}

/// The `vb-live` Service reference at `version` in the default space.
///
/// # Panics
/// Panics when the reference does not decode.
fn live_ref(version: &str) -> CardRef {
    serde_json::from_value(json!({
        "kind": "Service", "name": "vb-live", "version": version, "space": "default"
    }))
    .expect("live ref decodes")
}

/// Read the machine principals whose id or bound Card UID is `value`.
///
/// Returns each match's id and last qualifying exchange, so a caller can read
/// one principal by id or every principal an owner Card projects and assert
/// how many exist; principal ids and Card UIDs never collide.
///
/// # Panics
/// Panics when the tenant connection or the read fails.
async fn principal_activity(
    server: &WyrdTestServer,
    value: Uuid,
) -> Vec<(Uuid, Option<DateTime<Utc>>)> {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let rows = sqlx::query_as(
        "SELECT id, last_authenticated_at FROM wyrd.auth_service_accounts \
          WHERE id = $1 OR card_uid = $1",
    )
    .bind(value)
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("principal activity reads");
    conn.commit().await.expect("assertion transaction commits");
    rows
}

/// The one principal `owner` projects and its last qualifying exchange.
///
/// # Panics
/// Panics when the read fails or the owner does not project exactly one
/// principal.
async fn owner_principal(server: &WyrdTestServer, owner: Uuid) -> (Uuid, Option<DateTime<Utc>>) {
    let rows = principal_activity(server, owner).await;
    assert_eq!(
        rows.len(),
        1,
        "one exact owner version projects one principal"
    );
    rows[0]
}

/// The schedule cursor of `owner`'s Service-level binding.
///
/// # Panics
/// Panics when the owner has no schedule binding.
async fn schedule_cursor(server: &WyrdTestServer, owner: Uuid) -> Option<DateTime<Utc>> {
    owner_activity(server, owner)
        .await
        .into_iter()
        .find(|(_, activation, _)| activation == "schedule")
        .expect("owner has a schedule binding")
        .2
}

/// Move `principal`'s recorded activity past the default inactivity window,
/// so the gate lapses on the database's own clock, and return the stamp it
/// now carries.
///
/// # Panics
/// Panics when the tenant connection or the update fails.
async fn lapse_activity(server: &WyrdTestServer, principal: Uuid) -> DateTime<Utc> {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let lapsed = sqlx::query_scalar(
        "UPDATE wyrd.auth_service_accounts \
            SET last_authenticated_at = statement_timestamp() - INTERVAL '25 hours' \
          WHERE id = $1 RETURNING last_authenticated_at",
    )
    .bind(principal)
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("activity ages");
    conn.commit().await.expect("assertion transaction commits");
    lapsed
}

/// Evaluate every binding admission gate of `owner` on the database clock.
///
/// # Panics
/// Panics when the tenant connection, the binding list, or an activity read
/// fails, or a listed binding has no activity row.
async fn owner_gates(server: &WyrdTestServer, owner: Uuid) -> Vec<bool> {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT binding_id FROM wyrd.verification_bindings WHERE owner_card_uid = $1",
    )
    .bind(owner)
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("owner bindings list");
    let mut gates = Vec::with_capacity(ids.len());
    for id in ids {
        let id = wyrd_spec::ids::BindingId::new(id).expect("stored binding ID is UUIDv7");
        let activity = binding_activity(&mut conn, id, InactivityTimeout::default())
            .await
            .expect("binding activity reads")
            .expect("listed binding has activity");
        gates.push(activity.active);
    }
    conn.commit().await.expect("assertion transaction commits");
    gates
}

/// Build a real client over the bound server that authenticates with `key`.
///
/// # Panics
/// Panics when the server is not bound or the client auth cannot build.
fn machine_auth(server: &WyrdTestServer, key: &SecretString) -> Arc<AuthMiddleware> {
    let config = ClientConfig {
        http: HttpConfig {
            base_url: server
                .base_url()
                .expect("bound server has a URL")
                .to_owned(),
            ..HttpConfig::default()
        },
        ..ClientConfig::default()
    };
    AuthMiddleware::new(
        &config,
        ResolvedCredential::ApiKey(SecretString::clone(key)),
    )
    .expect("client auth builds")
}

/// Runtime activity follows only qualifying exchanges of the exact owner.
///
/// Registers two A/B versions of one bound Service and drives a real client
/// through `/auth/token`. The first API-key exchange activates only the exact
/// version, arms its schedule cursor, and — through the component binding —
/// activates every binding of that Service. A cached-token request, an idle
/// client whose token went stale, delegation to the other version, and a
/// Card-free automation principal's exchange write no activity. A second
/// replica sharing the principal and a request-driven stale-token re-exchange
/// renew the one shared timestamp without moving the armed cursor. The other
/// version stays independently gated until its own exchange. Idle expiry after
/// the default window, suspension, and Card deletion each close the gate.
///
/// # Panics
/// Panics when the server fails to start or stop, a fixture or route call
/// fails, or any activity, cursor, or gate expectation does not hold.
#[tokio::test(flavor = "current_thread")]
async fn runtime_activity_follows_only_qualifying_exchanges() {
    // 33s access tokens go stale (within the client's 30s skew) after ~3s.
    let server = wyrd_testing::WyrdTestServerBuilder::default()
        .with_access_ttl(chrono::Duration::seconds(33))
        .start_bound()
        .await
        .expect("test server starts");
    seed_binding_dependencies(&server).await;
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("vb-live-admin", &["writer", "runtime_admin"])
        .await
        .expect("admin bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let a_owner = register_live(&server, &jwt, "1.0.0").await;
    let b_owner = register_live(&server, &jwt, "2.0.0").await;
    let credential = |version: &'static str| {
        let server = &server;
        async move {
            let Bootstrap::Machine { api_key, .. } = server
                .credential_registered_service(&live_ref(version), &["writer"])
                .await
                .expect("registered Service is credentialed")
            else {
                panic!("credentialed Service returned a non-machine principal");
            };
            api_key
        }
    };
    let replica_one = credential("1.0.0").await;
    let replica_two = credential("1.0.0").await;
    let b_key = credential("2.0.0").await;
    let (a_principal, seen) = owner_principal(&server, a_owner).await;
    assert_eq!(seen, None, "registration and credentialing never activate");

    let client = machine_auth(&server, &replica_one);
    let token = client.bearer().await.expect("first exchange succeeds");
    let (_, first) = owner_principal(&server, a_owner).await;
    let first = first.expect("the exact owner's first exchange records activity");
    let cursor = schedule_cursor(&server, a_owner)
        .await
        .expect("the first exchange arms the schedule");
    assert!(cursor > first, "the cursor is the next future boundary");
    assert!(
        owner_activity(&server, a_owner)
            .await
            .iter()
            .all(|(stamp, _, _)| *stamp == Some(first)),
        "component bindings inherit the Service activity"
    );
    assert_eq!(owner_principal(&server, b_owner).await.1, None);
    assert!(owner_gates(&server, b_owner).await.iter().all(|g| !g));

    let cached = client.bearer().await.expect("cached token returns");
    assert_eq!(cached.expose(), token.expose(), "a fresh token is reused");
    let read = read_card(&server, token.expose(), "Service", &a_owner.to_string()).await;
    assert_eq!(read.status(), StatusCode::OK);
    let automation = server
        .oneshot_authenticated(
            &jwt,
            Request::builder()
                .method(Method::POST)
                .uri("/v1/principals")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "name": "vb-automation", "roles": ["writer"] }).to_string(),
                ))
                .expect("automation request builds"),
        )
        .await
        .expect("automation creation responds");
    assert_eq!(automation.status(), StatusCode::OK);
    let automation = response_json(automation).await;
    let automation_id = Uuid::parse_str(
        automation["principal_id"]
            .as_str()
            .expect("automation principal id"),
    )
    .expect("automation principal id parses");
    server
        .exchange_api_key(&SecretString::from(
            automation["credential"]
                .as_str()
                .expect("automation credential")
                .to_owned(),
        ))
        .await
        .expect("Card-free automation exchanges");
    assert_eq!(
        principal_activity(&server, automation_id).await,
        vec![(automation_id, None)],
        "a Card-free automation exchange records no activity"
    );
    assert_eq!(owner_principal(&server, a_owner).await.1, Some(first));
    assert_eq!(
        owner_principal(&server, b_owner).await.1,
        None,
        "cached requests never activate an owner"
    );

    let replica = machine_auth(&server, &replica_two);
    replica.bearer().await.expect("replica exchange succeeds");
    assert_eq!(
        principal_activity(&server, a_owner).await.len(),
        1,
        "replicas share the one exact principal"
    );
    let (principal, renewed) = owner_principal(&server, a_owner).await;
    assert_eq!(principal, a_principal);
    let renewed = renewed.expect("replica exchange renews activity");
    assert!(renewed >= first);
    assert_eq!(schedule_cursor(&server, a_owner).await, Some(cursor));

    sleep(Duration::from_secs(4)).await;
    assert_eq!(
        owner_principal(&server, a_owner).await.1,
        Some(renewed),
        "an idle client does not re-exchange on staleness alone"
    );
    let refreshed = client.bearer().await.expect("stale token re-exchanges");
    assert_ne!(
        refreshed.expose(),
        token.expose(),
        "a stale token is replaced"
    );
    let restamped = owner_principal(&server, a_owner)
        .await
        .1
        .expect("re-exchange keeps activity");
    assert!(
        restamped > renewed,
        "request-driven re-exchange renews activity"
    );
    assert_eq!(
        schedule_cursor(&server, a_owner).await,
        Some(cursor),
        "renewal never moves an armed cursor"
    );

    machine_auth(&server, &b_key)
        .bearer()
        .await
        .expect("the other version exchanges");
    let b_seen = owner_principal(&server, b_owner)
        .await
        .1
        .expect("the other version activates on its own exchange");
    assert_eq!(owner_principal(&server, a_owner).await.1, Some(restamped));
    assert!(b_seen >= restamped);

    assert!(owner_gates(&server, a_owner).await.iter().all(|g| *g));
    assert!(owner_gates(&server, b_owner).await.iter().all(|g| *g));
    let lapsed = lapse_activity(&server, a_principal).await;
    assert!(
        owner_gates(&server, a_owner).await.iter().all(|g| !g),
        "the default inactivity window closes the gate"
    );

    let revoke = server
        .oneshot_authenticated(
            &jwt,
            Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/principals/{a_principal}/revoke"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "principal_kind": "service", "reason": "suspend owner" }).to_string(),
                ))
                .expect("revoke request builds"),
        )
        .await
        .expect("revoke responds");
    assert!(revoke.status().is_success(), "{}", revoke.status());
    assert!(
        owner_gates(&server, a_owner).await.iter().all(|g| !g),
        "suspension closes the gate on the next read"
    );
    assert!(server.exchange_api_key(&replica_one).await.is_err());
    assert_eq!(owner_principal(&server, a_owner).await.1, Some(lapsed));

    let delete = server
        .oneshot_authenticated(
            &jwt,
            Request::builder()
                .method(Method::DELETE)
                .uri("/v1/cards/by-ref?kind=Service&space=default&name=vb-live&version=2.0.0")
                .body(Body::empty())
                .expect("delete request builds"),
        )
        .await
        .expect("delete responds");
    assert!(delete.status().is_success(), "{}", delete.status());
    assert!(
        owner_gates(&server, b_owner).await.iter().all(|g| !g),
        "deleting the owner Card closes the gate"
    );

    server.shutdown().await.expect("test server shuts down");
}
