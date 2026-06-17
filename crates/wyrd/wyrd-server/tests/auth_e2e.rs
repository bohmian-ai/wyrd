use std::env;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use base64::Engine;
use serde_json::{Value, json};
use wyrd_testing::env::{Bootstrap, WyrdTestEnv};

fn e2e_enabled() -> bool {
    env::var("WYRD_AUTH_E2E").is_ok()
}

fn authz_check_request(callee: &Bootstrap, action: &str) -> Request<Body> {
    let body = json!({
        "target": callee.card_ref().expect("machine target carries a card_ref"),
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

async fn permission_check_via_delegation(
    env: &WyrdTestEnv,
    initiator_jwt: &str,
    callee: &Bootstrap,
    action: &str,
) -> Value {
    let delegated = env
        .delegate(
            initiator_jwt,
            callee
                .card_ref()
                .expect("machine target carries a card_ref"),
        )
        .await
        .expect("delegate");
    let resp = env
        .call(&delegated, authz_check_request(callee, action))
        .await
        .expect("authz_check call");
    if resp.status() != StatusCode::OK {
        let status = resp.status();
        let body = body_json(resp).await;
        panic!("authz-check journey expected an RBAC decision response: status={status}, body={body}");
    }
    body_json(resp).await
}

fn assert_allow(decision: &Value) {
    assert_eq!(
        decision["decision"], "allow",
        "expected allow, got {decision}"
    );
}

fn assert_deny(decision: &Value) {
    assert_eq!(
        decision["decision"], "deny",
        "expected deny, got {decision}"
    );
    assert_eq!(
        decision["reason"], "missing_permission",
        "RBAC deny must carry the missing_permission reason slug"
    );
}

async fn neutral_initiator(env: &WyrdTestEnv, label: &str) -> Bootstrap {
    env.bootstrap_service(label, &["runtime_admin"])
        .await
        .expect("bootstrap neutral initiator")
}

fn decode_jwt_claims_for_test(jwt: &str) -> Value {
    let payload = jwt.split('.').nth(1).expect("jwt has payload segment");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("payload decodes");
    serde_json::from_slice(&bytes).expect("claims decode")
}

#[tokio::test(flavor = "current_thread")]
async fn journey_user_admin_creates_service_account_and_grants_writer_role() {
    if !e2e_enabled() {
        return;
    }
    let env = WyrdTestEnv::start().await.expect("start");
    let _admin = env
        .bootstrap_user("admin", &["runtime_admin"])
        .await
        .expect("bootstrap admin");
    let sa = env
        .bootstrap_service("svc-admin", &["writer"])
        .await
        .expect("bootstrap sa");
    let initiator = neutral_initiator(&env, "j1-init").await;

    let init_jwt = env
        .exchange_api_key(initiator.api_key().expect("machine"))
        .await
        .expect("initiator jwt");
    let decision = permission_check_via_delegation(&env, &init_jwt, &sa, "card_write").await;
    assert_allow(&decision);
}

#[tokio::test(flavor = "current_thread")]
async fn journey_role_revocation_flips_verdict() {
    if !e2e_enabled() {
        return;
    }
    let env = WyrdTestEnv::start().await.expect("start");
    let sa = env
        .bootstrap_service("sa-rev", &["writer"])
        .await
        .expect("bootstrap");
    let initiator = neutral_initiator(&env, "j2-init").await;
    let init_jwt = env
        .exchange_api_key(initiator.api_key().expect("machine"))
        .await
        .expect("initiator jwt");

    let first = permission_check_via_delegation(&env, &init_jwt, &sa, "card_write").await;
    assert_allow(&first);

    env.revoke_role(&sa, "writer").await.expect("revoke");
    env.force_recheck_principal(&sa).await;

    let second = permission_check_via_delegation(&env, &init_jwt, &sa, "card_write").await;
    assert_deny(&second);
}

#[tokio::test(flavor = "current_thread")]
async fn journey_role_grant_flips_verdict() {
    if !e2e_enabled() {
        return;
    }
    let env = WyrdTestEnv::start().await.expect("start");
    let sa = env
        .bootstrap_service("sa-grant", &[])
        .await
        .expect("bootstrap");
    let initiator = neutral_initiator(&env, "j3-init").await;
    let init_jwt = env
        .exchange_api_key(initiator.api_key().expect("machine"))
        .await
        .expect("initiator jwt");

    let first = permission_check_via_delegation(&env, &init_jwt, &sa, "card_write").await;
    assert_deny(&first);

    env.grant_role(&sa, "writer").await.expect("grant");
    env.force_recheck_principal(&sa).await;

    let second = permission_check_via_delegation(&env, &init_jwt, &sa, "card_write").await;
    assert_allow(&second);
}

#[tokio::test(flavor = "current_thread")]
async fn journey_delegated_call_via_token_exchange() {
    if !e2e_enabled() {
        return;
    }
    let env = WyrdTestEnv::start().await.expect("start");
    let a = env
        .bootstrap_service("svc-a", &["runtime_admin"])
        .await
        .expect("bootstrap a");
    let b = env
        .bootstrap_service("svc-b", &["writer"])
        .await
        .expect("bootstrap b");

    let a_jwt = env
        .exchange_api_key(a.api_key().expect("machine"))
        .await
        .expect("exchange a");
    let delegated = env
        .delegate(&a_jwt, b.card_ref().expect("machine"))
        .await
        .expect("delegate a to b");

    let resp = env
        .call(&delegated, authz_check_request(&b, "card_write"))
        .await
        .expect("call");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_allow(&body);

    let decoded = decode_jwt_claims_for_test(&delegated);
    let act = decoded["act"].as_object().expect("act chain");
    assert_eq!(act["sub"], a.id().to_string());
    assert!(act.get("act").is_none(), "single-hop chain has no parent act");
}

#[tokio::test(flavor = "current_thread")]
async fn journey_delegation_then_revoke_underlying_role() {
    if !e2e_enabled() {
        return;
    }
    let env = WyrdTestEnv::start().await.expect("start");
    let a = env
        .bootstrap_service("svc-a-revoke", &["runtime_admin"])
        .await
        .expect("bootstrap a");
    let b = env
        .bootstrap_service("svc-b-revoke", &["writer"])
        .await
        .expect("bootstrap b");

    let a_jwt = env
        .exchange_api_key(a.api_key().expect("machine"))
        .await
        .expect("exchange a");
    let delegated = env
        .delegate(&a_jwt, b.card_ref().expect("machine"))
        .await
        .expect("delegate");

    let first = env
        .call(&delegated, authz_check_request(&b, "card_write"))
        .await
        .expect("first");
    assert_eq!(first.status(), StatusCode::OK);
    assert_allow(&body_json(first).await);

    env.revoke_role(&b, "writer").await.expect("revoke b");
    env.force_recheck_principal(&b).await;

    let delegated2 = env
        .delegate(&a_jwt, b.card_ref().expect("machine"))
        .await
        .expect("re-delegate");
    let second = env
        .call(&delegated2, authz_check_request(&b, "card_write"))
        .await
        .expect("second");
    assert_eq!(second.status(), StatusCode::OK);
    assert_deny(&body_json(second).await);
}

#[tokio::test(flavor = "current_thread")]
async fn journey_agent_revoke_grant_flip() {
    if !e2e_enabled() {
        return;
    }
    let env = WyrdTestEnv::start().await.expect("start");
    let agent = env
        .bootstrap_agent("agent-flip", &["writer"])
        .await
        .expect("bootstrap agent");
    let initiator = neutral_initiator(&env, "j6-init").await;
    let init_jwt = env
        .exchange_api_key(initiator.api_key().expect("machine"))
        .await
        .expect("initiator jwt");

    let d1 = permission_check_via_delegation(&env, &init_jwt, &agent, "card_write").await;
    assert_allow(&d1);

    env.revoke_role(&agent, "writer").await.expect("revoke");
    env.force_recheck_principal(&agent).await;
    let d2 = permission_check_via_delegation(&env, &init_jwt, &agent, "card_write").await;
    assert_deny(&d2);

    env.grant_role(&agent, "writer").await.expect("grant");
    env.force_recheck_principal(&agent).await;
    let d3 = permission_check_via_delegation(&env, &init_jwt, &agent, "card_write").await;
    assert_allow(&d3);
}

#[tokio::test(flavor = "current_thread")]
async fn journey_cross_principal_kind_isolation_via_independent_bootstrap() {
    if !e2e_enabled() {
        return;
    }
    let env = WyrdTestEnv::start().await.expect("start");
    let sa = env
        .bootstrap_service("sa-only", &["writer"])
        .await
        .expect("sa");
    let agent = env
        .bootstrap_agent("agent-no-role", &[])
        .await
        .expect("agent");
    let initiator = neutral_initiator(&env, "j7-init").await;
    let init_jwt = env
        .exchange_api_key(initiator.api_key().expect("machine"))
        .await
        .expect("initiator jwt");

    let sa_decision = permission_check_via_delegation(&env, &init_jwt, &sa, "card_write").await;
    assert_allow(&sa_decision);

    let agent_decision =
        permission_check_via_delegation(&env, &init_jwt, &agent, "card_write").await;
    assert_deny(&agent_decision);
}

#[tokio::test(flavor = "current_thread")]
async fn cache_ttl_path_also_flips_verdict() {
    if !e2e_enabled() {
        return;
    }
    let env = WyrdTestEnv::start().await.expect("start");
    let sa = env
        .bootstrap_service("sa-ttl", &["writer"])
        .await
        .expect("bootstrap");
    let initiator = neutral_initiator(&env, "j8-init").await;
    let init_jwt = env
        .exchange_api_key(initiator.api_key().expect("machine"))
        .await
        .expect("initiator jwt");

    let first = permission_check_via_delegation(&env, &init_jwt, &sa, "card_write").await;
    assert_allow(&first);

    env.revoke_role(&sa, "writer").await.expect("revoke");
    env.advance(Duration::from_secs(70)).await;

    let second = permission_check_via_delegation(&env, &init_jwt, &sa, "card_write").await;
    assert_deny(&second);
}
