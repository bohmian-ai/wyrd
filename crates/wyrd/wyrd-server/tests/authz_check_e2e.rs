use std::env;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use insta::assert_json_snapshot;
use serde_json::{Value, json};
use wyrd_auth_check::{DenyAllPolicyHook, RecordingPolicyHook};
use wyrd_testing::env::{Bootstrap, WyrdTestEnv};

fn e2e_enabled() -> bool {
    env::var("WYRD_AUTHZ_CHECK_E2E").is_ok()
}

async fn env_with_recorder() -> (WyrdTestEnv, Arc<RecordingPolicyHook>) {
    let recorder = Arc::new(RecordingPolicyHook::default());
    let env = WyrdTestEnv::start()
        .await
        .expect("start env")
        .with_policy_hook(recorder.clone());
    (env, recorder)
}

async fn env_with_deny(reason: &str) -> WyrdTestEnv {
    WyrdTestEnv::start()
        .await
        .expect("start env")
        .with_policy_hook(Arc::new(DenyAllPolicyHook {
            reason: reason.to_owned(),
        }))
}

fn authz_check_request(target: &Bootstrap, action: &str) -> Request<Body> {
    let body = json!({
        "target": target.card_ref().expect("machine target carries a card_ref"),
        "action": action,
        "context": {},
    });
    Request::builder()
        .method(Method::POST)
        .uri("/v1/authz/check")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&body).expect("body serializes"),
        ))
        .expect("request builds")
}

async fn body_json(resp: Response<Body>) -> Value {
    let bytes = to_bytes(resp.into_body(), 1 << 20)
        .await
        .expect("read body");
    serde_json::from_slice(&bytes).expect("decode body")
}

fn redact_volatile(body: &Value) -> Value {
    let mut redacted = body.clone();
    if let Some(obj) = redacted.as_object_mut()
        && obj.contains_key("wyrd_request_id")
    {
        obj["wyrd_request_id"] = json!("[redacted]");
    }
    redacted
}

#[tokio::test(flavor = "current_thread")]
async fn service_b_calls_c_on_behalf_of_a_extends_chain_correctly() {
    if !e2e_enabled() {
        return;
    }
    let (env, recorder) = env_with_recorder().await;
    let a = env
        .bootstrap_service("svc-a", &["runtime_admin"])
        .await
        .expect("a");
    let b = env
        .bootstrap_service("svc-b", &["runtime_admin"])
        .await
        .expect("b");
    let c = env
        .bootstrap_service("svc-c", &["writer"])
        .await
        .expect("c");

    let a_jwt = env
        .exchange_api_key(a.api_key().expect("machine has key"))
        .await
        .expect("a jwt");
    let a_to_b = env
        .delegate(&a_jwt, b.card_ref().expect("machine has card ref"))
        .await
        .expect("a to b");
    let b_to_c = env
        .delegate(&a_to_b, c.card_ref().expect("machine has card ref"))
        .await
        .expect("b to c");

    let resp = env
        .call(&b_to_c, authz_check_request(&c, "card_write"))
        .await
        .expect("call");
    assert_eq!(resp.status(), StatusCode::OK);

    let last = recorder.last().expect("hook called");
    assert_eq!(last.chain.len(), 2, "two-hop chain");
    assert_eq!(
        last.chain[0].id.to_string(),
        a.id().to_string(),
        "initiator-first chain"
    );
    assert_eq!(
        last.chain[1].id.to_string(),
        b.id().to_string(),
        "intermediate caller"
    );
    assert_eq!(
        last.callee.id.to_string(),
        c.id().to_string(),
        "callee identity"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn single_hop_allow_records_caller_and_callee() {
    if !e2e_enabled() {
        return;
    }
    let (env, recorder) = env_with_recorder().await;
    let a = env
        .bootstrap_service("svc-a-allow", &["runtime_admin"])
        .await
        .expect("a");
    let b = env
        .bootstrap_service("svc-b-allow", &["writer"])
        .await
        .expect("b");

    let a_jwt = env
        .exchange_api_key(a.api_key().expect("machine has key"))
        .await
        .expect("a jwt");
    let delegated = env
        .delegate(&a_jwt, b.card_ref().expect("machine has card ref"))
        .await
        .expect("delegate");

    let resp = env
        .call(&delegated, authz_check_request(&b, "card_write"))
        .await
        .expect("call");
    assert_eq!(resp.status(), StatusCode::OK);
    assert_json_snapshot!("single_hop_allow_response", body_json(resp).await);

    let last = recorder.last().expect("hook called");
    assert_eq!(last.chain.len(), 1);
    assert_eq!(last.chain[0].id.to_string(), a.id().to_string());
    assert_eq!(last.callee.id.to_string(), b.id().to_string());
}

#[tokio::test(flavor = "current_thread")]
async fn non_delegated_token_rejected_before_hook() {
    if !e2e_enabled() {
        return;
    }
    let (env, recorder) = env_with_recorder().await;
    let service = env
        .bootstrap_service("sa-direct", &["writer"])
        .await
        .expect("service");
    let jwt = env
        .exchange_api_key(service.api_key().expect("machine has key"))
        .await
        .expect("jwt");

    let resp = env
        .call(&jwt, authz_check_request(&service, "card_write"))
        .await
        .expect("call");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let body = body_json(resp).await;
    assert_eq!(body["code"], "WYRD_AUTHZ_403_REQUIRES_DELEGATED_TOKEN");
    assert_eq!(body["details"]["reason"], "chain_empty");
    assert_eq!(
        recorder.calls().len(),
        0,
        "hook short-circuited before invocation"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn deny_decision_returns_403_with_reason() {
    if !e2e_enabled() {
        return;
    }
    let env = env_with_deny("policy_x").await;
    let a = env
        .bootstrap_service("svc-a-deny", &["runtime_admin"])
        .await
        .expect("a");
    let b = env
        .bootstrap_service("svc-b-deny", &["writer"])
        .await
        .expect("b");

    let a_jwt = env
        .exchange_api_key(a.api_key().expect("machine has key"))
        .await
        .expect("a jwt");
    let delegated = env
        .delegate(&a_jwt, b.card_ref().expect("machine has card ref"))
        .await
        .expect("delegate");

    let resp = env
        .call(&delegated, authz_check_request(&b, "card_write"))
        .await
        .expect("call");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert_json_snapshot!(
        "deny_decision_response",
        redact_volatile(&body_json(resp).await)
    );
}
