use std::env;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, Response, StatusCode, header};
use base64::Engine;
use serde_json::{Value, json};
use wyrd_spec::auth::TokenAudience;
use wyrd_testing::{Bootstrap, WyrdTestServer};

fn e2e_enabled() -> bool {
    env::var("WYRD_AUTH_E2E").is_ok()
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

/// Exchange a service's API key for its direct Wyrd access token.
///
/// # Panics
/// Panics when the bootstrap carries no key or the exchange fails.
async fn machine_jwt(srv: &WyrdTestServer, machine: &Bootstrap) -> String {
    srv.exchange_api_key(machine.api_key().expect("machine"))
        .await
        .expect("machine key exchanges")
}

/// Have `actor` act for the holder of `subject_jwt`, then check `action` on
/// the actor's own Card with the delegated token.
///
/// The verdict reflects the intersection of the subject's token permissions
/// and the actor's current grants, which is read at exchange time.
///
/// # Panics
/// Panics when the exchange fails or the check does not return an RBAC
/// decision body.
async fn permission_check_via_delegation(
    srv: &WyrdTestServer,
    subject_jwt: &str,
    actor: &Bootstrap,
    action: &str,
) -> Value {
    let delegated = srv
        .delegate(
            subject_jwt,
            &machine_jwt(srv, actor).await,
            TokenAudience::Wyrd,
        )
        .await
        .expect("delegate");
    let resp = srv
        .oneshot_authenticated(&delegated, authz_check_request(actor, action))
        .await
        .expect("authz_check call");
    if resp.status() != StatusCode::OK {
        let status = resp.status();
        let body = body_json(resp).await;
        panic!(
            "authz-check journey expected an RBAC decision response: status={status}, body={body}"
        );
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

/// Bootstrap a subject that holds the `writer` authority an actor may use on
/// its behalf, so the actor's own grants decide each delegated verdict.
async fn neutral_initiator(srv: &WyrdTestServer, label: &str) -> Bootstrap {
    srv.bootstrap_service(label, &["writer"])
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
    let srv = WyrdTestServer::start_in_process().await.expect("start");
    let _admin = srv
        .bootstrap_user("admin", &["runtime_admin"])
        .await
        .expect("bootstrap admin");
    let sa = srv
        .bootstrap_service("svc-admin", &["writer"])
        .await
        .expect("bootstrap sa");
    let initiator = neutral_initiator(&srv, "j1-init").await;

    let init_jwt = srv
        .exchange_api_key(initiator.api_key().expect("machine"))
        .await
        .expect("initiator jwt");
    let decision = permission_check_via_delegation(&srv, &init_jwt, &sa, "card_write").await;
    assert_allow(&decision);
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test(flavor = "current_thread")]
async fn journey_role_revocation_flips_verdict() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process().await.expect("start");
    let sa = srv
        .bootstrap_service("sa-rev", &["writer"])
        .await
        .expect("bootstrap");
    let initiator = neutral_initiator(&srv, "j2-init").await;
    let init_jwt = srv
        .exchange_api_key(initiator.api_key().expect("machine"))
        .await
        .expect("initiator jwt");

    let first = permission_check_via_delegation(&srv, &init_jwt, &sa, "card_write").await;
    assert_allow(&first);

    srv.revoke_role(&sa, "writer").await.expect("revoke");

    let second = permission_check_via_delegation(&srv, &init_jwt, &sa, "card_write").await;
    assert_deny(&second);
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test(flavor = "current_thread")]
async fn journey_role_grant_flips_verdict() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process().await.expect("start");
    let sa = srv
        .bootstrap_service("sa-grant", &[])
        .await
        .expect("bootstrap");
    let initiator = neutral_initiator(&srv, "j3-init").await;
    let init_jwt = srv
        .exchange_api_key(initiator.api_key().expect("machine"))
        .await
        .expect("initiator jwt");

    let first = permission_check_via_delegation(&srv, &init_jwt, &sa, "card_write").await;
    assert_deny(&first);

    srv.grant_role(&sa, "writer").await.expect("grant");

    let second = permission_check_via_delegation(&srv, &init_jwt, &sa, "card_write").await;
    assert_allow(&second);
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test(flavor = "current_thread")]
async fn journey_delegated_call_via_token_exchange() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process().await.expect("start");
    let a = srv
        .bootstrap_service("svc-a", &["writer"])
        .await
        .expect("bootstrap a");
    let b = srv
        .bootstrap_service("svc-b", &["writer"])
        .await
        .expect("bootstrap b");

    let delegated = srv
        .delegate(
            &machine_jwt(&srv, &a).await,
            &machine_jwt(&srv, &b).await,
            TokenAudience::Wyrd,
        )
        .await
        .expect("b acts for a");

    let resp = srv
        .oneshot_authenticated(&delegated, authz_check_request(&b, "card_write"))
        .await
        .expect("call");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_allow(&body);

    let decoded = decode_jwt_claims_for_test(&delegated);
    assert_eq!(decoded["sub"], a.id().to_string(), "subject is A");
    let act = decoded["act"].as_object().expect("act chain");
    assert_eq!(act["sub"], b.id().to_string(), "current actor is B");
    assert!(
        act.get("act").is_none(),
        "single-hop chain has no parent act"
    );
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test(flavor = "current_thread")]
async fn journey_delegation_then_revoke_underlying_role() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process().await.expect("start");
    let a = srv
        .bootstrap_service("svc-a-revoke", &["writer"])
        .await
        .expect("bootstrap a");
    let b = srv
        .bootstrap_service("svc-b-revoke", &["writer"])
        .await
        .expect("bootstrap b");

    let a_jwt = machine_jwt(&srv, &a).await;
    let b_jwt = machine_jwt(&srv, &b).await;
    let delegated = srv
        .delegate(&a_jwt, &b_jwt, TokenAudience::Wyrd)
        .await
        .expect("delegate");

    let first = srv
        .oneshot_authenticated(&delegated, authz_check_request(&b, "card_write"))
        .await
        .expect("first");
    assert_eq!(first.status(), StatusCode::OK);
    assert_allow(&body_json(first).await);

    srv.revoke_role(&b, "writer").await.expect("revoke b");

    let delegated2 = srv
        .delegate(&a_jwt, &b_jwt, TokenAudience::Wyrd)
        .await
        .expect("re-delegate");
    let second = srv
        .oneshot_authenticated(&delegated2, authz_check_request(&b, "card_write"))
        .await
        .expect("second");
    assert_eq!(second.status(), StatusCode::OK);
    assert_deny(&body_json(second).await);
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test(flavor = "current_thread")]
async fn journey_agent_revoke_grant_flip() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process().await.expect("start");
    let agent = srv
        .bootstrap_agent("agent-flip", &["writer"])
        .await
        .expect("bootstrap agent");
    let initiator = neutral_initiator(&srv, "j6-init").await;
    let init_jwt = srv
        .exchange_api_key(initiator.api_key().expect("machine"))
        .await
        .expect("initiator jwt");

    let d1 = permission_check_via_delegation(&srv, &init_jwt, &agent, "card_write").await;
    assert_allow(&d1);

    srv.revoke_role(&agent, "writer").await.expect("revoke");
    let d2 = permission_check_via_delegation(&srv, &init_jwt, &agent, "card_write").await;
    assert_deny(&d2);

    srv.grant_role(&agent, "writer").await.expect("grant");
    let d3 = permission_check_via_delegation(&srv, &init_jwt, &agent, "card_write").await;
    assert_allow(&d3);
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test(flavor = "current_thread")]
async fn journey_cross_principal_kind_isolation_via_independent_bootstrap() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process().await.expect("start");
    let sa = srv
        .bootstrap_service("sa-only", &["writer"])
        .await
        .expect("sa");
    let agent = srv
        .bootstrap_agent("agent-no-role", &[])
        .await
        .expect("agent");
    let initiator = neutral_initiator(&srv, "j7-init").await;
    let init_jwt = srv
        .exchange_api_key(initiator.api_key().expect("machine"))
        .await
        .expect("initiator jwt");

    let sa_decision = permission_check_via_delegation(&srv, &init_jwt, &sa, "card_write").await;
    assert_allow(&sa_decision);

    let agent_decision =
        permission_check_via_delegation(&srv, &init_jwt, &agent, "card_write").await;
    assert_deny(&agent_decision);
    srv.shutdown().await.expect("shutdown");
}

/// Delegation never amplifies: an actor holding `writer` that acts for a
/// subject holding nothing receives a delegated token that cannot write cards.
#[tokio::test(flavor = "current_thread")]
async fn journey_delegation_cannot_amplify_the_subject() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process().await.expect("start");
    let actor = srv
        .bootstrap_service("svc-amplify-actor", &["writer"])
        .await
        .expect("bootstrap actor");
    let subject = srv
        .bootstrap_service("svc-amplify-subject", &[])
        .await
        .expect("bootstrap subject");
    let subject_jwt = machine_jwt(&srv, &subject).await;

    let decision = permission_check_via_delegation(&srv, &subject_jwt, &actor, "card_write").await;
    assert_deny(&decision);
    srv.shutdown().await.expect("shutdown");
}
