//! Public card-registration journey through the authenticated HTTP surface.

use std::env;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use secrecy::SecretString;
use serde_json::{Value, json};
use url::Url;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_loader::{build_registration_input, load};
use wyrd_registry::Cards;
use wyrd_spec::envelope::Spec;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::InlineableRef;
use wyrd_spec::registry::{CardLifecycleStatus, RegistrationOutcomeKind};
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// Return whether the explicitly gated Postgres route tests should run.
fn enabled() -> bool {
    env::var("WYRD_REG_E2E").as_deref() == Ok("1")
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
    std::fs::write(
        &prompt_path,
        "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: client-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: [hello]\n",
    )
    .expect("prompt card writes");
    let input = build_registration_input(load(&prompt_path).expect("loader tree builds"))
        .expect("registration input builds");

    let server = WyrdTestServer::start_bound()
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
/// The offline loader hands its wire projection to the existing 02a route.
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
        panic!("02a must bind the loader sibling before persistence");
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
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}

/// Initialize a sole heavy card after commit and advance its manifest row.
#[tokio::test(flavor = "current_thread")]
async fn heavy_registration_initializes_upload_after_commit() {
    if !enabled() {
        return;
    }
    let server = WyrdTestServer::start_in_process()
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
    let upload_response = server
        .oneshot_authenticated(&jwt, upload)
        .await
        .expect("local upload responds");
    assert_eq!(upload_response.status(), StatusCode::OK);

    let complete = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/cards/{card_uid}/complete"))
        .header("Idempotency-Key", "heavy-only-001")
        .body(Body::empty())
        .expect("card completion request builds");
    let complete_response = server
        .oneshot_authenticated(&jwt, complete)
        .await
        .expect("card completion responds");
    assert_eq!(complete_response.status(), StatusCode::OK);
    let complete_body = response_json(complete_response).await;
    assert_eq!(complete_body["outcomes"][0]["status"], "active");
    assert_eq!(complete_body["outcomes"][0]["outcome"], "registered");

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
