//! Public card-registration journey through the authenticated HTTP surface.

use std::env;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use wyrd_spec::envelope::Spec;
use wyrd_spec::reference::PromptRef;
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_testing::{Bootstrap, WyrdTestServer};

fn enabled() -> bool {
    env::var("WYRD_REGISTRY_E2E").as_deref() == Ok("1")
}

async fn response_json(response: axum::http::Response<Body>) -> Value {
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("response body reads");
    serde_json::from_slice(&bytes).expect("response body is JSON")
}

fn registration_request() -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/v1/cards")
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", "route-journey-001")
        .body(Body::from(
            json!({
                "card": {
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
                    }
                },
                "artifacts": []
            })
            .to_string(),
        ))
        .expect("registration request builds")
}

fn prompt_registration_request(name: &str, idempotency_key: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/v1/cards")
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", idempotency_key)
        .body(Body::from(
            json!({
                "card": {
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
                    }
                },
                "artifacts": []
            })
            .to_string(),
        ))
        .expect("prompt registration request builds")
}

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
                "card": {
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
                    }
                },
                "artifacts": []
            })
            .to_string(),
        ))
        .expect("agent registration request builds")
}

#[tokio::test(flavor = "current_thread")]
async fn registration_replays_through_public_authenticated_route() {
    if !enabled() {
        return;
    }

    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let Bootstrap::User { jwt, .. } = server
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
    assert_eq!(first_body["outcome"], "created");

    let replay = server
        .oneshot_authenticated(&jwt, registration_request())
        .await
        .expect("replay responds");
    assert_eq!(replay.status(), StatusCode::CREATED);
    let replay_body = response_json(replay).await;
    assert_eq!(replay_body["outcome"], "idempotent_noop");
    assert_eq!(replay_body["card_uid"], first_body["card_uid"]);

    server.shutdown().await.expect("test server shuts down");
}

#[tokio::test(flavor = "current_thread")]
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
    assert_eq!(parent.status(), StatusCode::CREATED);
    let parent_body = response_json(parent).await;
    let parent_uid = serde_json::from_value(parent_body["card_uid"].clone())
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
    let PromptRef::Card(child_ref) = agent.prompt else {
        panic!("persisted agent prompt is not a card reference");
    };
    assert_eq!(child_ref.name.as_str(), "resolved-prompt");
    assert_eq!(
        child_ref.uid.as_ref().map(ToString::to_string),
        child_body["card_uid"].as_str().map(ToOwned::to_owned),
    );
    conn.commit().await.expect("assertion transaction commits");

    server.shutdown().await.expect("test server shuts down");
}
