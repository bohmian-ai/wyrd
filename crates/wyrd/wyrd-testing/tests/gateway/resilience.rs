//! Failover, limit, and cancellation journeys against a real bound server.
//!
//! These cover the call lifecycle a successful journey never reaches: a
//! deployment that refuses retryably hands the call to its sibling, an
//! administered limit refuses the next call before any dispatch, and a caller
//! that goes away mid-call still leaves settled accounting behind.
//!
//! Shutdown drain is a journey here: a call whose invocation audit append is
//! still pending when graceful shutdown begins drains inside the production
//! deadline. A draining server's `503` refusal is not observable through the
//! public surface — the listener is shut down gracefully, so the connection is
//! simply refused. That refusal, and the terminal error frame an open stream
//! receives on drain, are proven in-process by
//! `wyrd_server::components::gateway::pg_invocation_tests::gateway_onboards_compatible_provider_at_runtime`,
//! which drives the same handlers over real HTTP dispatch.

use std::time::Duration;

use serde_json::{Value, json};
use sqlx::PgPool;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, ResponseTemplate};
use wyrd_spec::ids::DataTenantId;
use wyrd_sql::TenantConn;

use crate::harness::{Journey, openai_code, refusal, tokens};

/// Model both failover deployments serve.
const PROJECTION: &str = "acme/m";

/// Buffered Chat Completions answer the healthy deployment returns.
fn completion() -> Value {
    json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "created": 1,
        "model": "m",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15},
    })
}

/// One Chat Completions request body for [`PROJECTION`].
fn chat() -> Value {
    json!({
        "model": PROJECTION,
        "max_completion_tokens": 16,
        "messages": [{"role": "user", "content": "hi"}],
    })
}

/// Stores one `openai_compatible` deployment named `name` serving `model` of
/// provider `acme`, whose routes live under `prefix` of the journey's mock
/// upstream.
///
/// The adapter needs no upstream credential, so the deployment authenticates
/// with `none` and the mock records exactly what the gateway sent.
async fn deploy_compatible(journey: &Journey, name: &str, model: &str, prefix: &str) {
    journey
        .put(
            &format!("provider-deployments/{name}"),
            json!({
                "name": name,
                "model": {"provider": "acme", "model": model},
                "adapter": {"openai_compatible": {
                    "base_url": format!("{}{prefix}/v1", journey.upstream.uri()),
                }},
                "auth": "none",
                "capabilities": ["chat_completions"],
                "routing_weight": 1,
            }),
        )
        .await;
}

/// Proves deployment failover and an administered rate limit through the
/// public server.
///
/// One model is served by two deployments; the first provider exchange of the
/// call refuses with a retryable `503`, so the call completes on the sibling
/// deployment and the caller never sees the refusal. Both attempts are
/// accounted, and because the refused attempt reached its provider with no
/// reported usage the call total stays unknown rather than understating what
/// the refused exchange may have consumed. A tenant limit of one request per
/// minute then refuses the next call with the stable limit code and dispatches
/// nothing.
///
/// # Panics
/// Panics when a status, body, ledger entry, or dispatch-count expectation
/// fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn a_retryable_refusal_fails_over_and_a_limit_refuses_before_dispatch() {
    let journey = Journey::start().await;
    // Exactly one provider exchange refuses retryably, whichever deployment
    // weighted selection reaches first, so failover is proven without
    // depending on the selection order.
    Mock::given(method("POST"))
        .and(path_regex(r"^/(first|second)/v1/chat/completions$"))
        .respond_with(
            ResponseTemplate::new(503).set_body_json(
                json!({"error": {"message": "upstream busy", "type": "server_error"}}),
            ),
        )
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&journey.upstream)
        .await;
    for prefix in ["/first", "/second"] {
        Mock::given(method("POST"))
            .and(path(format!("{prefix}/v1/chat/completions")))
            .respond_with(ResponseTemplate::new(200).set_body_json(completion()))
            .with_priority(2)
            .mount(&journey.upstream)
            .await;
    }
    deploy_compatible(&journey, "acme-first", "m", "/first").await;
    deploy_compatible(&journey, "acme-second", "m", "/second").await;
    // Administered before any call, so the limit counts this journey's own
    // calls: the failover call is the tenant's one admitted request.
    journey
        .put(
            "governance-policy",
            json!({
                "limits": [{"subject": "tenant", "target": "all", "requests_per_minute": 1,
                            "tokens_per_minute": null, "concurrent_calls": null}],
                "budgets": [],
                "pricing": [],
                "unknown_cost": "allow_unpriced",
            }),
        )
        .await;
    let bearer = vec![("authorization", format!("Bearer {}", journey.caller))];

    let answered = journey.post("/v1/chat/completions", &bearer, &chat()).await;
    assert_eq!(
        answered.status().as_u16(),
        200,
        "a retryable refusal never reaches the caller"
    );
    assert_eq!(
        answered.json::<Value>().await.expect("answer json"),
        completion(),
        "the sibling deployment's answer is the caller's"
    );
    assert_eq!(
        journey.upstream_calls().await.len(),
        2,
        "the refused attempt and the completing one both dispatched"
    );
    assert_eq!(
        journey.settled_usage().await,
        vec![Value::Null],
        "the call is accounted once; a dispatched attempt whose usage the \
         provider never reported leaves the call total unknown rather than \
         understated"
    );
    assert_eq!(
        journey.accounted_attempts().await,
        vec![
            (json!("failed"), Value::Null),
            (json!("succeeded"), tokens(11, 4)),
        ],
        "both attempts are accounted, the completing one with its usage"
    );

    let dispatched = journey.upstream_calls().await.len();
    refusal(
        journey.post("/v1/chat/completions", &bearer, &chat()).await,
        429,
        "WYRD_GATEWAY_429_LIMIT_EXCEEDED",
        openai_code,
    )
    .await;
    assert_eq!(
        journey.upstream_calls().await.len(),
        dispatched,
        "a limited call reaches no provider"
    );
}

/// Proves that a caller who goes away mid-call leaves settled accounting.
///
/// The provider holds the exchange open; once the upstream has recorded the
/// dispatch the caller's request is dropped, which cancels the call. The
/// gateway still settles it on its tracked task, so the tenant ledger carries
/// the call with no usage rather than losing it, and the server keeps serving.
///
/// # Panics
/// Panics when the dispatch is not recorded within the bound, or a ledger or
/// status expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn a_cancelled_call_is_still_settled_and_the_server_keeps_serving() {
    let journey = Journey::start().await;
    Mock::given(method("POST"))
        .and(path("/held/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(completion())
                .set_delay(Duration::from_secs(60)),
        )
        .mount(&journey.upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/prompt/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion()))
        .mount(&journey.upstream)
        .await;
    deploy_compatible(&journey, "acme-held", "held", "/held").await;
    deploy_compatible(&journey, "acme-prompt", "m", "/prompt").await;
    let bearer = vec![("authorization", format!("Bearer {}", journey.caller))];

    let (http, base, token) = (
        journey.http.clone(),
        journey.base.clone(),
        journey.caller.clone(),
    );
    let abandoned = tokio::spawn(async move {
        http.post(format!("{base}/v1/chat/completions"))
            .bearer_auth(token)
            .json(&json!({
                "model": "acme/held",
                "max_completion_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}],
            }))
            .send()
            .await
    });
    tokio::time::timeout(Duration::from_secs(30), async {
        while journey.upstream_calls().await.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the held call dispatched");
    abandoned.abort();

    // The gateway settles a cancelled call on the task tracker, so the ledger
    // carries it once the tracker drains.
    assert_eq!(
        journey.settled_usage().await,
        vec![Value::Null],
        "the cancelled call is accounted with no usage"
    );

    // The server still serves the next caller.
    let answered = journey.post("/v1/chat/completions", &bearer, &chat()).await;
    assert_eq!(
        answered.status().as_u16(),
        200,
        "a cancelled call never wedges the server"
    );
    assert_eq!(
        answered.json::<Value>().await.expect("answer json"),
        completion()
    );
}

/// Reads the outcome of every `gateway.invoke` audit decision staged for
/// `tenant`, oldest first.
///
/// Uses its own short transaction on the same database the server audits into,
/// so it observes only committed rows: an append still holding its transaction
/// open is invisible here, which is what makes a pending append observable as
/// an absent row rather than as a blocked read.
///
/// # Panics
/// Panics when the tenant transaction cannot be opened or the read fails.
async fn invoke_decisions(pool: &PgPool, tenant: DataTenantId) -> Vec<String> {
    let mut conn = TenantConn::acquire(pool, tenant)
        .await
        .expect("vala tenant conn");
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT outcome FROM vala.audit_staging \
          WHERE operation = 'gateway.invoke' ORDER BY seq",
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("audit rows");
    conn.commit().await.expect("audit read commits");
    rows
}

/// Proves that graceful shutdown drains an invocation audit append that is
/// still pending when it begins.
///
/// The tenant's audit chain head is held under `FOR UPDATE` by this test, which
/// is exactly what the canonical append takes, so the call's staged append
/// blocks on it. The caller's completion still arrives — the invocation audit
/// is deliberately non-blocking — and the decision row is absent while the lock
/// is held. Production shutdown then runs: `BoundServer::run` closes the gateway
/// task tracker and waits on it inside `shutdown.drain_ms`, so releasing the
/// lock after the listener has stopped accepting leaves the append to finish
/// inside that budget. A clean return proves it did; had the tracker still held
/// the append at the deadline, the server would have exited terminally with
/// `gateway call accounting did not drain before the shutdown deadline`.
///
/// # Panics
/// Panics when the call, the lock, the drain, or an audit expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn a_pending_invocation_audit_append_drains_before_shutdown_completes() {
    let mut journey = Journey::start().await;
    Mock::given(method("POST"))
        .and(path("/drain/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion()))
        .mount(&journey.upstream)
        .await;
    deploy_compatible(&journey, "acme-drain", "m", "/drain").await;

    // Owned pool: the held transaction must outlive the `&mut` borrow the
    // in-place teardown below takes on the server.
    let pool = journey.server.pg_fixture().vala_postgres().pool().clone();
    let tenant = journey.server.data_tenant_id();
    let mut head = TenantConn::acquire(&pool, tenant)
        .await
        .expect("vala tenant conn");
    sqlx::query("SELECT last_seq FROM vala.audit_chain_head FOR UPDATE")
        .fetch_one(&mut **head.transaction())
        .await
        .expect("the tenant chain head is held");
    let before = invoke_decisions(&pool, tenant).await.len();

    let bearer = vec![("authorization", format!("Bearer {}", journey.caller))];
    let answered = journey.post("/v1/chat/completions", &bearer, &chat()).await;
    assert_eq!(
        answered.status().as_u16(),
        200,
        "the call never waits for its own audit append"
    );
    assert_eq!(
        answered.json::<Value>().await.expect("answer json"),
        completion(),
        "the provider response completed while the append was pending"
    );
    assert_eq!(
        invoke_decisions(&pool, tenant).await.len(),
        before,
        "the append is pending while the chain head is held"
    );

    let (http, base) = (journey.http.clone(), journey.base.clone());
    let release = async {
        // Readiness stops being advertised as the first step of production
        // shutdown, so an unready or refused probe means the drain has begun.
        tokio::time::timeout(Duration::from_secs(30), async {
            while http
                .get(format!("{base}/readyz"))
                .send()
                .await
                .is_ok_and(|ready| ready.status().is_success())
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("shutdown begins");
        assert_eq!(
            invoke_decisions(&pool, tenant).await.len(),
            before,
            "the append is still pending when shutdown begins"
        );
        head.commit().await.expect("the chain head releases");
    };
    let (drained, ()) = tokio::join!(journey.server.cancel_and_join_for_test(), release);
    drained.expect("the pending append drains inside the shutdown deadline");

    assert_eq!(
        invoke_decisions(&pool, tenant).await.len(),
        before + 1,
        "the decision committed before shutdown completed"
    );
}
