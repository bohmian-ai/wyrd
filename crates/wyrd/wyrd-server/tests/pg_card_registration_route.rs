//! Public card-registration journey through the authenticated HTTP surface.

use std::env;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
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
