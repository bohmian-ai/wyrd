//! Public card-registration journey through the authenticated HTTP surface.

use std::env;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use base64::Engine;
use secrecy::SecretString;
use serde_json::{Value, json};
use sha2::Digest;
use url::Url;
use uuid::Uuid;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_loader::{build_registration_input, load};
use wyrd_registry::Cards;
use wyrd_spec::envelope::Spec;
use wyrd_spec::ids::{CardUid, DataTenantId};
use wyrd_spec::reference::InlineableRef;
use wyrd_spec::registry::{CardLifecycleStatus, RegistrationOutcomeKind};
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_storage::settings::{BackendConfig, StorageSettings};
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// Return whether the explicitly gated Postgres route tests should run.
fn enabled() -> bool {
    env::var("WYRD_REG_E2E").as_deref() == Ok("1")
}

/// Seed a dependency row directly so the journey can exercise non-Active states.
async fn seed_dependency(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    name: &str,
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
         VALUES ($1, $2, 'Prompt', 'default', $3, '1.0.0', $4, $5, $6)",
    )
    .bind(uid.as_uuid())
    .bind(tenant.as_uuid())
    .bind(name)
    .bind(json!({ "provider": "openai", "model": "gpt-4o", "messages": ["hello"] }))
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
    if !enabled() {
        return;
    }

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
    if !enabled() {
        return;
    }

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
        "SELECT count(*) FROM vala.audit_outbox \
         WHERE operation = 'card.registration.create' AND principal_id = $1",
    )
    .bind(id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("audit count reads");
    assert_eq!(
        audit_count, 1,
        "replay must not append a second create audit"
    );
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

#[tokio::test(flavor = "current_thread")]
/// Registration resolves an external child reference and persists its UID.
async fn registration_resolves_child_card_refs_before_persisting() {
    if !enabled() {
        return;
    }

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
    if !enabled() {
        return;
    }

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
    if !enabled() {
        return;
    }
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
    if !enabled() {
        return;
    }
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
    if !enabled() {
        return;
    }
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

/// Confirm a rejected dependency did not append any durable registration state.
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
    assert_eq!(operation_count, 0);
    assert_eq!(card_count, 0);
    assert_eq!(relationship_count, 0);
    conn.commit().await.expect("assertion transaction commits");
}

/// Reject an artifact-bearing submission when another card shares the request.
#[tokio::test(flavor = "current_thread")]
async fn composite_with_manifest_rejects_before_writes() {
    if !enabled() {
        return;
    }
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
    if !enabled() {
        return;
    }
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
    if !enabled() {
        return;
    }
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
        "SELECT count(*) FROM vala.audit_outbox \
         WHERE operation = 'card.registration.create' AND principal_id = $1",
    )
    .bind(id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("audit count reads");
    assert_eq!(audit_count, 3);
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
    if !enabled() {
        return;
    }
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

/// Activation and its audit row roll back together when the audit append fails.
#[tokio::test(flavor = "current_thread")]
async fn completion_audit_failure_keeps_card_pending() {
    if !enabled() {
        return;
    }
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
           BEFORE INSERT ON vala.audit_outbox
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
        "SELECT count(*) FROM vala.audit_outbox WHERE operation = 'card.registration.complete'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("completion audit count reads");
    assert_eq!(complete_audits, 0);
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

/// Delete audit failure rolls back the tombstone and does not append an audit row.
#[tokio::test(flavor = "current_thread")]
async fn delete_audit_failure_keeps_card_active() {
    if !enabled() {
        return;
    }
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
             IF NEW.operation = 'card.registration' THEN
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
           BEFORE INSERT ON vala.audit_outbox
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
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE");

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
        "SELECT count(*) FROM vala.audit_outbox \
         WHERE operation = 'card.registration' AND resource = $1",
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

/// Backend cleanup failure leaves a committed tombstone and retryable blob state.
#[tokio::test(flavor = "current_thread")]
async fn delete_storage_failure_preserves_cleanup_state() {
    if !enabled() {
        return;
    }
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
    std::fs::remove_file(storage_root.path()).expect("broken storage root removes");
    std::fs::rename(&moved_storage_root, storage_root.path()).expect("storage root restores");
    server.shutdown().await.expect("test server shuts down");
}

/// A blob storage failure leaves durable failure state and an audit record.
#[tokio::test(flavor = "current_thread")]
async fn blob_storage_failure_is_audited_without_leaking_sql() {
    if !enabled() {
        return;
    }
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
    let blob_failure_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_outbox \
         WHERE operation = 'card.registration.blob_write.failed'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("blob failure audit count reads");
    assert_eq!(blob_failure_audits, 1);
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

/// Roll back the entire composite when a later per-node audit append fails.
#[tokio::test(flavor = "current_thread")]
async fn audit_append_failure_rolls_back_composite_transaction() {
    if !enabled() {
        return;
    }
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
        r#"CREATE OR REPLACE FUNCTION vala.test_fail_second_registration_audit()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             IF NEW.operation = 'card.registration.create'
                AND (SELECT count(*) FROM vala.audit_outbox
                     WHERE data_tenant_id = NEW.data_tenant_id
                       AND request_id = NEW.request_id
                       AND operation = 'card.registration.create') >= 1 THEN
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
        r#"CREATE TRIGGER test_fail_second_registration_audit
           BEFORE INSERT ON vala.audit_outbox
           FOR EACH ROW EXECUTE FUNCTION vala.test_fail_second_registration_audit()"#,
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
    if !enabled() {
        return;
    }
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

/// Canonical hashing replays an identical graph authored in another wire order.
#[tokio::test(flavor = "current_thread")]
async fn wire_order_permutation_replays_identical_graph() {
    if !enabled() {
        return;
    }
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
