//! Peer-mode capture journey: a gateway served by a pod without Scribe.
//!
//! An `oracle`-target pod serves the public API and the gateway beside a
//! `scribe`-target pod, both on the cluster's `wyrd-peer` mTLS plane. The
//! oracle pod runs no Scribe, so its one capture writer must deliver through
//! the capture-only peer RPC to the live Scribe. The listener-level refusal of
//! a leaf that is not the `wyrd-peer` identity is proven once for every peer
//! service by the `bifrost_oracle` peer-network listener journeys.

use std::time::{Duration, Instant};

use arrow::array::{Array, StringArray};
use serde_json::{Value, json};
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_runtime::Permission;
use wyrd_server::config::BifrostTarget;
use wyrd_spec::auth::{GATEWAY_CAPTURE_PRINCIPAL, GatewayAccess};
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};
use wyrd_testing::server::WyrdTestServer;

use crate::harness::{PROVIDER_KEY, api_key, exchange};

/// Buffered Chat Completions answer the mock upstream returns.
fn completion() -> Value {
    json!({
        "id": "chatcmpl-peer",
        "object": "chat.completion",
        "created": 1,
        "model": "gpt-4o",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop", "logprobs": null}],
        "usage": {"prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15},
    })
}

/// Puts one gateway administration document at `route` on `base` as `admin`.
///
/// # Panics
/// Panics when the request fails or is refused.
async fn put(http: &reqwest::Client, base: &str, admin: &str, route: &str, body: Value) {
    let response = http
        .put(format!("{base}/v1/admin/gateway/{route}"))
        .header("x-wyrd-access-token", format!("Bearer {admin}"))
        .json(&body)
        .send()
        .await
        .expect("admin request sends");
    let status = response.status();
    assert!(
        status.is_success(),
        "{route}: {status} {}",
        response.text().await.unwrap_or_default()
    );
}

/// Reads one string column of the rows `sql` returns through `gateway`'s
/// public query path, as `credential`'s principal.
///
/// # Errors
/// Returns the client, transport, or query error.
async fn strings(
    gateway: &WyrdTestServer,
    credential: &secrecy::SecretString,
    sql: &str,
    column: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let client = wyrd_client::WyrdClient::with_config(wyrd_client::config::ClientConfig {
        grpc: wyrd_client::transport::GrpcConfig {
            endpoint: gateway.grpc_url().ok_or("missing gRPC URL")?,
            connect_retries: 0,
            ..wyrd_client::transport::GrpcConfig::default()
        },
        http: wyrd_client::transport::HttpConfig {
            base_url: gateway.base_url().ok_or("missing HTTP URL")?.to_owned(),
            ..wyrd_client::transport::HttpConfig::default()
        },
        credential: Some(credential.clone()),
        ..wyrd_client::config::ClientConfig::default()
    })?;
    let result = wyrd_client::Bifrost::connect(&client)
        .await?
        .sql(sql)
        .await?;
    let mut values = Vec::new();
    for batch in result.batches() {
        let array = batch
            .column_by_name(column)
            .ok_or_else(|| format!("no {column} column"))?;
        let strings = array
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| format!("{column} is not utf8"))?;
        values.extend((0..strings.len()).map(|row| strings.value(row).to_owned()));
    }
    Ok(values)
}

/// Proves AC-043's peer-mode path: a gateway call served by an `oracle`-target
/// pod, which runs no Scribe, still lands its capture.
///
/// The tenant administrator configures a provider and a `Metadata` capture
/// policy on the oracle pod, and an ordinary caller invokes one chat
/// completion there. The call row reaches `vala.gateway.calls` and its attempt
/// span `vala.traces.spans` on the scribe pod through the capture-only peer
/// RPC, both stamped with the reserved capture principal and the admitting
/// request, and read back through the oracle pod's public query path.
///
/// # Panics
///
/// Panics when the cluster does not start, the call fails, or the captured
/// rows do not land within thirty seconds.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn oracle_only_gateway_captures_through_the_peer_scribe() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion()))
        .mount(&upstream)
        .await;
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::for_targets(&[BifrostTarget::Oracle, BifrostTarget::Scribe])
            .with_gateway_provider_root_for_test(Url::parse(&upstream.uri()).expect("mock url")),
    )
    .await
    .expect("peer-mode cluster starts");
    let gateway = cluster.server(0).expect("oracle pod");
    let scribe = cluster.server(1).expect("scribe pod");
    assert!(
        gateway.state().bifrost_ingest().is_none(),
        "the gateway pod runs no Scribe"
    );

    let invoke = [Permission::gateway_invoke(GatewayAccess::Provider {
        provider: "openai".parse().expect("provider id"),
    })];
    gateway
        .seed_role("gateway_invoker", &invoke)
        .await
        .expect("invoker role seeds");
    let admin_key = api_key(gateway, "peer_admin", &["admin"]).await;
    let admin = exchange(gateway, &admin_key).await;
    let caller = exchange(
        gateway,
        &api_key(gateway, "peer_caller", &["gateway_invoker"]).await,
    )
    .await;
    let http = reqwest::Client::new();
    let base = gateway.base_url().expect("bound url").to_owned();
    put(
        &http,
        &base,
        &admin,
        "provider-credentials/openai-key",
        json!({
            "name": "openai-key",
            "provider": "openai",
            "source": {"managed_secret": {"secret": PROVIDER_KEY}},
        }),
    )
    .await;
    put(
        &http,
        &base,
        &admin,
        "provider-deployments/gpt-4o",
        json!({
            "name": "gpt-4o",
            "model": {"provider": "openai", "model": "gpt-4o"},
            "adapter": "openai",
            "auth": {"bearer": {"credential": "openai-key"}},
            "capabilities": ["chat_completions"],
            "routing_weight": 1,
        }),
    )
    .await;
    put(
        &http,
        &base,
        &admin,
        "capture-policy",
        json!({"mode": "metadata", "payload_fields": []}),
    )
    .await;

    let answer = http
        .post(format!("{base}/v1/chat/completions"))
        .header("authorization", format!("Bearer {caller}"))
        .json(&json!({
            "model": "openai/gpt-4o",
            "max_completion_tokens": 16,
            "messages": [{"role": "user", "content": "hi"}],
        }))
        .send()
        .await
        .expect("call sends");
    assert_eq!(answer.status().as_u16(), 200);
    let request_id = answer
        .headers()
        .get("wyrd-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("the answer carries its request id")
        .to_owned();
    assert_eq!(answer.json::<Value>().await.expect("answer"), completion());

    let deadline = Instant::now() + Duration::from_secs(30);
    for table in ["vala.gateway.calls", "vala.traces.spans"] {
        let sql =
            format!("SELECT principal_id FROM {table} WHERE wyrd_request_id = '{request_id}'");
        let principals = loop {
            scribe
                .flush_bifrost()
                .await
                .expect("the scribe pod flushes");
            match strings(gateway, &admin_key, &sql, "principal_id").await {
                Ok(rows) if !rows.is_empty() => break rows,
                outcome => assert!(
                    Instant::now() < deadline,
                    "{table} capture never landed through the peer Scribe: {outcome:?}"
                ),
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        };
        assert_eq!(
            principals,
            [GATEWAY_CAPTURE_PRINCIPAL.to_string()],
            "{table} holds exactly the call's capture, stamped with the capture principal"
        );
    }
    cluster.shutdown().await.expect("cluster shuts down");
}
