//! Peer-mode capture journey: a gateway served by a pod without Scribe.
//!
//! An `oracle`-target pod serves the public API and the gateway beside a
//! `scribe`-target pod, both on the cluster's `wyrd-peer` mTLS plane. The
//! oracle pod runs no Scribe, so its one capture writer must deliver through
//! the capture-only peer RPC to the live Scribe. The listener-level refusal of
//! a leaf that is not the `wyrd-peer` identity is proven once for every peer
//! service by the `bifrost_oracle` peer-network listener journeys.

use std::io::Cursor;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Array, StringArray};
use arrow::datatypes::Schema;
use arrow::ipc::writer::StreamWriter;
use arrow::json::ReaderBuilder;
use serde_json::{Value, json};
use url::Url;
use vala_bifrost_redux::tables::{CallsTable, DomainTable};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_runtime::Permission;
use wyrd_server::config::BifrostTarget;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{GATEWAY_CAPTURE_PRINCIPAL, GatewayAccess};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::BifrostError;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};
use wyrd_testing::server::WyrdTestServer;
use wyrd_tonic::wyrd::v1::IngestCaptureRequest;
use wyrd_tonic::wyrd::v1::scribe_capture_peer_service_client::ScribeCapturePeerServiceClient;

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

/// Submits one `vala.gateway.calls` row whose present `resolved_model` lacks
/// its `model` child to `scribe`'s capture peer RPC, as a cluster member.
///
/// The row is otherwise a valid capture, so the refusal can come only from
/// the partial Struct. Returns the row's call id and the stable error the
/// peer plane answered with.
///
/// # Errors
///
/// Returns an encoding, TLS, or connection error, or a description when the
/// Scribe acknowledges the row.
async fn submit_partial_resolved_model(
    cluster: &WyrdTestCluster,
    scribe: &WyrdTestServer,
    tenant: DataTenantId,
) -> Result<(uuid::Uuid, WyrdError), Box<dyn std::error::Error>> {
    let call_id = uuid::Uuid::now_v7();
    let now = chrono::Utc::now().to_rfc3339();
    let row = json!({
        "schema_version": 1,
        "call_id": call_id.to_string(),
        "caller_principal_id": uuid::Uuid::now_v7().to_string(),
        "operation": "chat_completions",
        "ingress_dialect": "openai",
        "requested_model": {"provider": "openai", "model": "gpt-4o"},
        "resolved_model": {"provider": "openai", "model": null},
        "streaming": false,
        "started_at": now,
        "terminal_at": now,
        "outcome": "succeeded",
        "attempt_count": 1,
        "pricing_versions": [],
        "capture_policy_version": 1,
        "payload_object_refs": [],
    });
    let schema = Arc::new(Schema::new(CallsTable::arrow_fields()));
    let batch = ReaderBuilder::new(Arc::clone(&schema))
        .build(Cursor::new(row.to_string()))?
        .next()
        .ok_or("the partial row decodes to no batch")??;
    let mut writer = StreamWriter::try_new(Vec::new(), &schema)?;
    writer.write(&batch)?;
    writer.finish()?;
    let authority = cluster.peer_ca();
    let member = authority.issue_leaf("gateway-peer-journey")?;
    let channel = wyrd_tonic::transport::mutually_authenticated_tls_endpoint(
        scribe
            .peer_url()
            .ok_or("the scribe pod has no peer listener")?,
        authority.ca_certificate_pem().as_bytes(),
        authority.server_name().to_owned(),
        member.certificate_pem().as_bytes(),
        member.private_key_pem().as_bytes(),
    )?
    .connect()
    .await?;
    let refused = ScribeCapturePeerServiceClient::new(channel)
        .ingest_capture(IngestCaptureRequest {
            tenant_id: tenant.to_string(),
            table: "vala.gateway.calls".to_owned(),
            batch_id: uuid::Uuid::now_v7().to_string(),
            request_id: RequestId::now_v7().as_str().to_owned(),
            arrow_ipc: writer.into_inner()?.into(),
        })
        .await
        .err()
        .ok_or("the scribe pod acknowledged a partial resolved_model")?;
    Ok((call_id, wyrd_client::error::from_grpc_status(&refused)))
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
/// Before that call, a calls row whose present `resolved_model` lacks a child
/// is sent straight to the scribe pod's capture peer RPC; it is refused before
/// any ACK with the exact partial-Struct problem, no row for it is ever
/// readable, and the following real capture still lands.
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
    let (partial_call, refusal) =
        submit_partial_resolved_model(&cluster, scribe, cluster.data_tenant_id())
            .await
            .expect("the partial capture is refused");
    let WyrdError::Vala { error: refusal } = refusal else {
        panic!("the partial capture was refused as {refusal:?}");
    };
    assert_eq!(
        refusal,
        BifrostError::SchemaParse {
            detail: "row 0: resolved_model.model must be null exactly when resolved_model is null"
                .to_owned(),
        },
        "the peer plane carries the exact partial-Struct problem"
    );

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
    assert_eq!(
        strings(
            gateway,
            &admin_key,
            &format!("SELECT call_id FROM vala.gateway.calls WHERE call_id = '{partial_call}'"),
            "call_id",
        )
        .await
        .expect("the refused call id queries"),
        Vec::<String>::new(),
        "the refused partial capture retained no row"
    );
    cluster.shutdown().await.expect("cluster shuts down");
}
