use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use insta::assert_json_snapshot;
use serde_json::{Value, json};
use wyrd_auth_check::{DenyAllPolicyHook, RecordingPolicyHook};
use wyrd_spec::auth::TokenAudience;
use wyrd_testing::{Bootstrap, WyrdTestServer, WyrdTestServerError};

async fn srv_with_recorder() -> (WyrdTestServer, Arc<RecordingPolicyHook>) {
    let recorder = Arc::new(RecordingPolicyHook::default());
    let srv = WyrdTestServer::builder()
        .with_policy_hook(recorder.clone())
        .start_in_process()
        .await
        .expect("start srv");
    (srv, recorder)
}

async fn srv_with_deny(reason: &str) -> WyrdTestServer {
    WyrdTestServer::builder()
        .with_policy_hook(Arc::new(DenyAllPolicyHook {
            reason: reason.to_owned(),
        }))
        .start_in_process()
        .await
        .expect("start srv")
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

/// Exchange one service's API key for its Wyrd access token.
///
/// # Panics
/// Panics when the bootstrap carries no API key or the exchange fails.
async fn service_jwt(srv: &WyrdTestServer, service: &Bootstrap) -> String {
    srv.exchange_api_key(service.api_key().expect("machine has key"))
        .await
        .expect("service key exchanges")
}

/// C acting for A through B re-exchanges the delegated subject token, so the
/// verified chain names B then C (earliest first), C is the current actor, and
/// A stays the subject the check authorizes.
///
/// # Panics
/// Panics when bootstrap, either exchange, the check call, or shutdown fails,
/// the check is not `200 OK`, the hook was never called, or the recorded
/// chain, actor, or subject differs.
#[tokio::test(flavor = "current_thread")]
async fn service_c_acts_for_a_through_b_extends_chain_correctly() {
    let (srv, recorder) = srv_with_recorder().await;
    let a = srv
        .bootstrap_service("svc-a", &["writer"])
        .await
        .expect("a");
    let b = srv
        .bootstrap_service("svc-b", &["writer"])
        .await
        .expect("b");
    let c = srv
        .bootstrap_service("svc-c", &["writer"])
        .await
        .expect("c");

    let b_for_a = srv
        .delegate(
            &service_jwt(&srv, &a).await,
            &service_jwt(&srv, &b).await,
            TokenAudience::Wyrd,
        )
        .await
        .expect("b acts for a");
    let c_for_a = srv
        .delegate(&b_for_a, &service_jwt(&srv, &c).await, TokenAudience::Wyrd)
        .await
        .expect("c acts for a through b");

    let resp = srv
        .oneshot_authenticated(&c_for_a, authz_check_request(&a, "card_write"))
        .await
        .expect("call");
    assert_eq!(resp.status(), StatusCode::OK);

    let last = recorder.last().expect("hook called");
    let chain: Vec<String> = last.chain.iter().map(|step| step.id.to_string()).collect();
    assert_eq!(
        chain,
        [b.id().to_string(), c.id().to_string()],
        "earliest actor first"
    );
    assert_eq!(
        last.actor.id.to_string(),
        c.id().to_string(),
        "current actor"
    );
    assert_eq!(last.subject.id.to_string(), a.id().to_string(), "subject");
    srv.shutdown().await.expect("shutdown");
}

/// One exchange yields subject A and actor B; the check hook sees exactly
/// that pair.
///
/// # Panics
/// Panics when bootstrap, the exchange, the check call, or shutdown fails,
/// the check is not `200 OK`, the response snapshot differs, or the hook did
/// not record a one-step chain with actor B and subject A.
#[tokio::test(flavor = "current_thread")]
async fn single_hop_allow_records_subject_and_actor() {
    let (srv, recorder) = srv_with_recorder().await;
    let a = srv
        .bootstrap_service("svc-a-allow", &["writer"])
        .await
        .expect("a");
    let b = srv
        .bootstrap_service("svc-b-allow", &["writer"])
        .await
        .expect("b");

    let delegated = srv
        .delegate(
            &service_jwt(&srv, &a).await,
            &service_jwt(&srv, &b).await,
            TokenAudience::Wyrd,
        )
        .await
        .expect("delegate");

    let resp = srv
        .oneshot_authenticated(&delegated, authz_check_request(&a, "card_write"))
        .await
        .expect("call");
    assert_eq!(resp.status(), StatusCode::OK);
    assert_json_snapshot!("single_hop_allow_response", body_json(resp).await);

    let last = recorder.last().expect("hook called");
    assert_eq!(last.chain.len(), 1);
    assert_eq!(last.actor.id.to_string(), b.id().to_string());
    assert_eq!(last.subject.id.to_string(), a.id().to_string());
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test(flavor = "current_thread")]
async fn non_delegated_token_rejected_before_hook() {
    let (srv, recorder) = srv_with_recorder().await;
    let service = srv
        .bootstrap_service("sa-direct", &["writer"])
        .await
        .expect("service");
    let jwt = srv
        .exchange_api_key(service.api_key().expect("machine has key"))
        .await
        .expect("jwt");

    let resp = srv
        .oneshot_authenticated(&jwt, authz_check_request(&service, "card_write"))
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
    srv.shutdown().await.expect("shutdown");
}

/// A denying invoke policy refuses the exchange itself, with the policy's
/// reason, so no delegated token ever exists to present to the check.
///
/// # Panics
/// Panics when bootstrap or shutdown fails, the exchange succeeds or fails
/// with a non-HTTP error, or the refusal is not `403`
/// `WYRD_AUTHZ_403_POLICY_DENIED` carrying reason `policy_x`.
#[tokio::test(flavor = "current_thread")]
async fn deny_decision_refuses_the_exchange_with_reason() {
    let srv = srv_with_deny("policy_x").await;
    let a = srv
        .bootstrap_service("svc-a-deny", &["writer"])
        .await
        .expect("a");
    let b = srv
        .bootstrap_service("svc-b-deny", &["writer"])
        .await
        .expect("b");

    let refusal = srv
        .delegate(
            &service_jwt(&srv, &a).await,
            &service_jwt(&srv, &b).await,
            TokenAudience::Wyrd,
        )
        .await
        .expect_err("policy denies the exchange");
    let WyrdTestServerError::Http { status, code, body } = refusal else {
        panic!("expected an HTTP refusal, got {refusal:?}");
    };
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(code, "WYRD_AUTHZ_403_POLICY_DENIED");
    let body: Value = serde_json::from_str(&body).expect("problem json");
    assert_eq!(body["details"]["reason"], "policy_x");
    srv.shutdown().await.expect("shutdown");
}
