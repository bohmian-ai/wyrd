//! Running-query lifecycle controls and describe admission, audited end to end.
//!
//! The public gRPC and HTTP lifecycle controls share one server service, so the
//! retained audit evidence each decision leaves is asserted here once, through
//! the generated gRPC client, against a real server and real principals. The
//! SDK's own tenant-scoping journey lives in `wyrd-client`'s `pg_bifrost_e2e`.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema};

use wyrd_client::WyrdClient;
use wyrd_client::bifrost::{Bifrost, TableConfig};
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_testing::WyrdTestServer;
use wyrd_tonic::tonic::{Code, Request};
use wyrd_tonic::wyrd::v1 as proto;
use wyrd_tonic::wyrd::v1::bifrost_query_service_client::BifrostQueryServiceClient;

/// One server with a published table and three callers across two tenants.
///
/// The owner runs the query under test; `other` is an admin of a second tenant
/// who must see nothing of it; `denied` belongs to that second tenant with no
/// role at all, so every lifecycle call it makes is an RBAC refusal.
struct LifecycleFixture {
    /// The running server.
    server: WyrdTestServer,
    /// Admin of the server's default tenant, which owns the running query.
    owner: WyrdClient,
    /// Admin of the second tenant.
    other: WyrdClient,
    /// Role-less service of the second tenant.
    denied: WyrdClient,
    /// The second tenant.
    other_tenant: DataTenantId,
    /// The published table the stalled query reads.
    table_fqn: String,
}

impl LifecycleFixture {
    /// Starts the server, publishes one row, and bootstraps the three callers.
    ///
    /// # Panics
    ///
    /// Panics when the server, the table, the second tenant, or any caller
    /// cannot be provisioned.
    async fn start() -> Self {
        let server = WyrdTestServer::start_bound()
            .await
            .expect("lifecycle server starts");
        let table_fqn = format!(
            "vala.datasets.query_lifecycle_{}",
            uuid::Uuid::now_v7().simple()
        );
        let owner = Self::client(&server, None, "query-lifecycle-owner", &["admin"]).await;
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        Bifrost::connect_with_table(
            &owner,
            TableConfig::from_arrow(&table_fqn, schema).expect("declared lifecycle table"),
        )
        .await
        .expect("the owner connects")
        .register()
        .await
        .expect("register the lifecycle table");
        server
            .seed_bifrost_rows(&table_fqn, &[1])
            .await
            .expect("publish the lifecycle row");
        let other_tenant = server
            .seed_tenant(&format!(
                "query-lifecycle-other-{}",
                uuid::Uuid::now_v7().simple()
            ))
            .await
            .expect("seed the second tenant");
        let other = Self::client(
            &server,
            Some(other_tenant),
            "query-lifecycle-other",
            &["admin"],
        )
        .await;
        let denied = Self::client(&server, Some(other_tenant), "query-lifecycle-denied", &[]).await;
        Self {
            server,
            owner,
            other,
            denied,
            other_tenant,
            table_fqn,
        }
    }

    /// A client for one freshly bootstrapped service holding `roles`.
    ///
    /// # Panics
    ///
    /// Panics when the service cannot be bootstrapped or the client assembled.
    async fn client(
        server: &WyrdTestServer,
        tenant: Option<DataTenantId>,
        name: &str,
        roles: &[&str],
    ) -> WyrdClient {
        let bootstrap = match tenant {
            Some(tenant) => {
                server
                    .bootstrap_service_in_tenant(tenant, name, roles)
                    .await
            }
            None => server.bootstrap_service(name, roles).await,
        }
        .expect("bootstrap the lifecycle caller");
        WyrdClient::with_config(ClientConfig {
            grpc: GrpcConfig {
                endpoint: server.grpc_url().expect("gRPC URL"),
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: server.base_url().expect("HTTP URL").to_owned(),
                ..HttpConfig::default()
            },
            credential: Some(bootstrap.api_key().expect("machine API key").clone()),
            ..ClientConfig::default()
        })
        .expect("lifecycle client")
    }

    /// Starts one owner query into the deterministic schema stall.
    ///
    /// Returns the request id and the task that owns the stream, which the
    /// caller aborts once the lifecycle assertions are done.
    ///
    /// # Panics
    ///
    /// Panics when the query does not start or does not reach the stall.
    async fn stalled_query(&self) -> (RequestId, tokio::task::JoinHandle<()>) {
        self.server.stall_next_query_after_schema();
        let stream = Bifrost::query_only(&self.owner)
            .query(&BifrostQueryRequest {
                params: Vec::new(),
                sql: format!("SELECT value FROM {}", self.table_fqn),
                deadline_ms: Some(30_000),
            })
            .await
            .expect("the owner query starts");
        let request_id = stream.request_id().clone();
        let task = tokio::spawn(async move {
            let mut stream = stream;
            let _ = stream.next_batch().await;
        });
        let stalled = self
            .server
            .wait_query_schema_stall()
            .await
            .expect("the query stalls after its schema");
        assert_eq!(stalled, request_id.as_str());
        (request_id, task)
    }
}

/// Adds one Wyrd access token to a typed public gRPC request.
///
/// # Panics
///
/// Panics when the bearer is not valid metadata.
async fn authorized<T>(client: &WyrdClient, value: T) -> Request<T> {
    let bearer = client.auth().bearer().await.expect("bearer token");
    let mut request = Request::new(value);
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {}", bearer.expose())
            .parse()
            .expect("access-token metadata"),
    );
    request
}

/// Asserts retained history holds exactly these lifecycle decisions.
///
/// The operation total converges first, so every row for the operation is
/// retained; each distinct resource/outcome pair is then one settled count.
///
/// # Panics
///
/// Panics when a count does not converge within the harness budget.
async fn assert_lifecycle_audit(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    operation: &str,
    expected: &[(&RequestId, &str)],
) {
    server
        .await_retained_audit_count(
            tenant,
            &format!("operation = '{operation}'"),
            expected.len() as u64,
        )
        .await
        .expect("retained lifecycle decision total");
    let mut pairs: BTreeMap<(String, &str), u64> = BTreeMap::new();
    for (id, outcome) in expected {
        *pairs
            .entry((format!("vala.query.lifecycle/{id}"), outcome))
            .or_default() += 1;
    }
    for ((resource, outcome), count) in pairs {
        server
            .await_retained_audit_count(
                tenant,
                &format!(
                    "operation = '{operation}' AND resource = '{resource}' \
                     AND outcome = '{outcome}'"
                ),
                count,
            )
            .await
            .expect("retained lifecycle decision");
    }
}

/// The gRPC lifecycle controls are tenant-opaque, RBAC-gated, idempotent, and
/// leave exactly one retained decision per call.
///
/// A foreign tenant's lookup and cancel of a running query are
/// indistinguishable from an unknown id; a role-less caller is refused; the
/// owner's second cancel reports nothing started. Every get and cancel is
/// audited in the caller's own tenant, including the refusal.
///
/// # Panics
///
/// Panics when any control answers differently or any retained decision is
/// missing or duplicated.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn grpc_lifecycle_controls_are_tenant_scoped_and_audited() {
    let fixture = LifecycleFixture::start().await;
    let (request_id, task) = fixture.stalled_query().await;
    let unknown_id = RequestId::now_v7();
    let channel = wyrd_tonic::tonic::transport::Endpoint::from_shared(
        fixture.server.grpc_url().expect("gRPC URL"),
    )
    .expect("gRPC endpoint")
    .connect()
    .await
    .expect("gRPC connects");
    let mut grpc = BifrostQueryServiceClient::new(channel);
    let get = |id: &RequestId| proto::GetRunningQueryRequest {
        request_id: id.to_string(),
    };
    let cancel = |id: &RequestId| proto::CancelRunningQueryRequest {
        request_id: id.to_string(),
    };

    let listed = grpc
        .list_running_queries(authorized(&fixture.owner, proto::ListRunningQueriesRequest {}).await)
        .await
        .expect("the owner lists its query")
        .into_inner();
    assert_eq!(listed.queries.len(), 1);
    assert_eq!(listed.queries[0].request_id, request_id.as_str());
    let got = grpc
        .get_running_query(authorized(&fixture.owner, get(&request_id)).await)
        .await
        .expect("the owner reads its query")
        .into_inner();
    assert_eq!(got.request_id, request_id.as_str());

    let foreign = grpc
        .get_running_query(authorized(&fixture.other, get(&request_id)).await)
        .await
        .expect_err("a foreign tenant cannot inspect the query");
    let absent = grpc
        .get_running_query(authorized(&fixture.other, get(&unknown_id)).await)
        .await
        .expect_err("an unknown query is not found");
    assert_eq!(foreign.code(), Code::NotFound);
    assert_eq!(
        (foreign.code(), foreign.message()),
        (absent.code(), absent.message())
    );
    let denied_list = grpc
        .list_running_queries(
            authorized(&fixture.denied, proto::ListRunningQueriesRequest {}).await,
        )
        .await
        .expect_err("a role-less caller cannot list");
    assert_eq!(denied_list.code(), Code::PermissionDenied);
    let denied_get = grpc
        .get_running_query(authorized(&fixture.denied, get(&request_id)).await)
        .await
        .expect_err("a role-less caller cannot inspect");
    assert_eq!(denied_get.code(), Code::PermissionDenied);

    let foreign = grpc
        .cancel_running_query(authorized(&fixture.other, cancel(&request_id)).await)
        .await
        .expect_err("a foreign tenant cannot cancel the query");
    let absent = grpc
        .cancel_running_query(authorized(&fixture.other, cancel(&unknown_id)).await)
        .await
        .expect_err("an unknown cancellation is not found");
    assert_eq!(foreign.code(), Code::NotFound);
    assert_eq!(
        (foreign.code(), foreign.message()),
        (absent.code(), absent.message())
    );
    for started in [true, false] {
        let response = grpc
            .cancel_running_query(authorized(&fixture.owner, cancel(&request_id)).await)
            .await
            .expect("the owner cancels")
            .into_inner();
        assert_eq!(response.cancellation_started, started);
    }

    let owner_tenant = fixture.server.data_tenant_id();
    assert_lifecycle_audit(
        &fixture.server,
        owner_tenant,
        "vala.query.running.get",
        &[(&request_id, "allowed")],
    )
    .await;
    assert_lifecycle_audit(
        &fixture.server,
        owner_tenant,
        "vala.query.running.cancel",
        &[(&request_id, "allowed"), (&request_id, "allowed")],
    )
    .await;
    assert_lifecycle_audit(
        &fixture.server,
        fixture.other_tenant,
        "vala.query.running.get",
        &[
            (&request_id, "allowed"),
            (&unknown_id, "allowed"),
            (&request_id, "denied"),
        ],
    )
    .await;
    assert_lifecycle_audit(
        &fixture.server,
        fixture.other_tenant,
        "vala.query.running.cancel",
        &[(&request_id, "allowed"), (&unknown_id, "allowed")],
    )
    .await;

    task.abort();
    let _ = task.await;
    fixture.server.shutdown().await.expect("server shutdown");
}

/// A denied table describe is refused, audited, and admits nothing.
///
/// `WyrdState::start_bifrost` and `observe.record` reach a table only through
/// `Bifrost::writer_table`, so this is the describe every scoped observation
/// depends on. A principal holding query but not `bifrost_table:read` receives
/// the stable RBAC refusal, the server retains one denied decision naming the
/// requested table, and the writer holds no cached destination and no producer.
///
/// # Panics
///
/// Panics when the describe is admitted, the refusal carries another code, the
/// retained decision is missing, or the writer cached or pooled anything.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn denied_describe_is_audited_before_admission() {
    let server = WyrdTestServer::start_bound()
        .await
        .expect("describe server starts");
    let permission: wyrd_runtime::Permission =
        "bifrost_query:read".parse().expect("a declared permission");
    server
        .seed_role("describe_denied", &[permission])
        .await
        .expect("seed the query-only role");
    let writer = Bifrost::connect(
        &LifecycleFixture::client(&server, None, "describe_denied", &["describe_denied"]).await,
    )
    .await
    .expect("the query-only writer connects");

    let denied = writer
        .writer_table("vala.drift.observations")
        .await
        .expect_err("a caller without bifrost_table:read cannot describe");
    assert_eq!(
        wyrd_spec::error::WyrdError::from(&denied).code(),
        "WYRD_PERMISSION_403_DENIED_RBAC"
    );
    server
        .await_retained_audit_count(
            server.data_tenant_id(),
            "operation = 'vala.bifrost.describe' \
             AND resource = 'vala.drift.observations' AND outcome = 'denied'",
            1,
        )
        .await
        .expect("one retained denied decision names the requested table");
    assert!(
        writer
            .cached_writer_table("vala.drift.observations")
            .is_none()
    );
    assert_eq!(
        writer.producer_count(),
        0,
        "a denied describe admits nothing"
    );

    server.shutdown().await.expect("server shutdown");
}
