use arrow::datatypes::{DataType, Field};
use futures_util::StreamExt as _;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::oracle::analytical::AnalyticalLiveInspection;
use vala_bifrost_redux::oracle::{AuthorizedQueryContext, QueryIpcDecoder};
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::{Permission, Principal, PrincipalKind};
use wyrd_server::query::scheduled::ScheduledQueryCaller;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuthMethod, BifrostQueryRequest, FreshnessPolicy, QueryExecutionPath, VisibilityMode,
};
use wyrd_testing::WyrdTestServer;
use wyrd_tonic::wyrd::v1 as proto;
use wyrd_tonic::wyrd::v1::bifrost_query_service_client::BifrostQueryServiceClient;

/// Boxed error carried by every helper in this journey.
type ServerJourneyError = Box<dyn std::error::Error + Send + Sync>;

/// Rows the fixture publishes, and therefore the exact result of every read.
const FIXTURE_VALUES: [i64; 4] = [1, 2, 3, 4];

/// Builds one published-only strict request with an explicit deadline.
fn request(sql: &str) -> BifrostQueryRequest {
    BifrostQueryRequest {
        sql: sql.to_owned(),
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(30_000),
    }
}

/// Builds the same authorized context the public query service builds.
///
/// # Errors
///
/// Returns the tenant invariant error when the principal and tenant disagree.
fn scheduled_context(tenant: DataTenantId) -> Result<AuthorizedQueryContext, ServerJourneyError> {
    let permission = Permission::bifrost_query_read();
    let principal = Principal::new(
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::from_iter([permission.clone()]),
    );
    Ok(AuthorizedQueryContext::try_new(
        principal,
        tenant,
        RequestId::now_v7(),
        None,
        AuthMethod::Internal,
        permission.to_string(),
    )?)
}

/// Counts audit rows written under one operation name.
///
/// # Errors
///
/// Returns the tenant-connection or SQL error.
async fn audit_rows(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    operation: &str,
) -> Result<i64, ServerJourneyError> {
    let mut conn = server.tenant_conn_for(tenant).await?;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox WHERE operation LIKE $1")
            .bind(operation)
            .fetch_one(&mut **conn.transaction())
            .await?;
    conn.commit().await?;
    Ok(count)
}

/// The public gRPC surface and the server's own scheduled caller share one
/// audit acceptance, one terminal contract, and one cleanup.
///
/// # Panics
///
/// Panics when the shared query journey cannot be driven to its claims.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup() {
    prove_shared_query_surfaces()
        .await
        .expect("shared gRPC and scheduled query journey");
}

/// Drives both server-side query entries against one real bound server.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_shared_query_surfaces() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let table = format!("server_query_{}", uuid::Uuid::now_v7().simple());
    server
        .state()
        .bifrost_catalog()
        .ok_or("server composed no Bifrost catalog")?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, &table),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await?;
    let sql = format!("SELECT value FROM vala.bifrost.{table} ORDER BY value");

    // Readiness first: every claim below is about a node that is actually
    // serving, not one that answered before its dependencies resolved.
    let base = server.base_url().ok_or("missing HTTP URL")?.to_owned();
    let ready = reqwest::get(format!("{base}/readyz")).await?;
    if !ready.status().is_success() {
        return Err(format!("the server reported {} on /readyz", ready.status()).into());
    }

    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, "server-query-caller", &["admin"])
        .await?;
    let api_key = bootstrap
        .api_key()
        .ok_or("machine bootstrap returned no key")?
        .clone();
    let writer = wyrd_client::WyrdClient::with_config(wyrd_client::config::ClientConfig {
        grpc: wyrd_client::transport::GrpcConfig {
            endpoint: server.grpc_url().ok_or("missing gRPC URL")?,
            connect_retries: 0,
            ..wyrd_client::transport::GrpcConfig::default()
        },
        http: wyrd_client::transport::HttpConfig {
            base_url: base.clone(),
            ..wyrd_client::transport::HttpConfig::default()
        },
        api_key: Some(api_key.clone()),
        ..wyrd_client::config::ClientConfig::default()
    })?;
    let transport = vala_sdk::BifrostGrpcTransport::connect(&writer).await?;
    for value in FIXTURE_VALUES {
        transport
            .insert_batch(
                &format!("vala.bifrost.{table}"),
                uuid::Uuid::now_v7().into_bytes(),
                ipc_value(value),
            )
            .await?;
    }
    server.flush_bifrost().await?;
    let channel = wyrd_tonic::tonic::transport::Endpoint::from_shared(
        server.grpc_url().ok_or("missing gRPC URL")?,
    )?
    .connect()
    .await?;

    // The private plane must not be reachable on the public listener.
    let mut peer = proto::oracle_peer_service_client::OraclePeerServiceClient::new(channel.clone());
    match peer
        .reserve_slots(proto::ReserveNodeSlotsRequest::default())
        .await
    {
        Err(status) if status.code() == wyrd_tonic::tonic::Code::Unimplemented => {}
        Err(status) => {
            return Err(format!(
                "OraclePeerService answered {:?} on the public listener",
                status.code()
            )
            .into());
        }
        Ok(_) => return Err("OraclePeerService served a request on the public listener".into()),
    }

    let reads_before = audit_rows(&server, tenant, "bifrost.query.read_decision").await?;
    let mut grpc = BifrostQueryServiceClient::new(channel);
    let mut authenticated =
        wyrd_tonic::tonic::Request::new(proto::BifrostQueryRequest::from(request(&sql)));
    authenticated.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {}", writer.auth().bearer().await?.expose()).parse()?,
    );
    let response = grpc.query(authenticated).await?;
    let deadline_ms: i64 = response
        .metadata()
        .get("x-wyrd-query-deadline-ms")
        .ok_or("the gRPC query response omits its initial deadline metadata")?
        .to_str()?
        .parse()?;
    if deadline_ms <= 0 {
        return Err(format!("the gRPC response pinned deadline {deadline_ms}").into());
    }
    let (rows, path) = drain_grpc(response.into_inner()).await?;
    if rows != FIXTURE_VALUES.len() {
        return Err(format!("the public gRPC query returned {rows} rows").into());
    }

    // One accepted logical read is audited; the stages that served it are not.
    if audit_rows(&server, tenant, "bifrost.query.read_decision").await? != reads_before + 1 {
        return Err("the public query did not relay exactly one logical read row".into());
    }
    if audit_rows(&server, tenant, "bifrost.query.stage%").await? != 0 {
        return Err("a query stage appended its own audit row".into());
    }

    // The same statement through the server's own caller: same entry, same
    // terminal contract, same selected path.
    let caller = ScheduledQueryCaller::new(
        server.state().clone(),
        scheduled_context(tenant)?,
        tokio_util::sync::CancellationToken::new(),
    );
    let scheduled = caller.run(request(&sql)).await?;
    if scheduled.rows != FIXTURE_VALUES.len() as u64 {
        return Err(format!("the scheduled query returned {} rows", scheduled.rows).into());
    }
    if scheduled.terminal.execution_path != path {
        return Err(format!(
            "the scheduled query settled {:?} where the public query settled {path:?}",
            scheduled.terminal.execution_path
        )
        .into());
    }
    // The same operation, once more: one relayed logical read per statement,
    // whichever entry ran it.
    if audit_rows(&server, tenant, "bifrost.query.read_decision").await? != reads_before + 2 {
        return Err("the scheduled caller did not relay its own logical read row".into());
    }
    if audit_rows(&server, tenant, "bifrost.query.stage%").await? != 0 {
        return Err("the scheduled caller appended a stage audit row".into());
    }

    // Cancellation settles the stream rather than abandoning it.
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();
    let cancelled = ScheduledQueryCaller::new(
        server.state().clone(),
        scheduled_context(tenant)?,
        cancellation,
    )
    .run(request(&sql))
    .await;
    match cancelled {
        Err(error) if error.status() == 502 || error.code().contains("STREAM_INCOMPLETE") => {}
        Err(error) => return Err(format!("cancellation reported {}", error.code()).into()),
        Ok(outcome) => {
            return Err(
                format!("a cancelled scheduled query returned {} rows", outcome.rows).into(),
            );
        }
    }

    // A refused public request carries the canonical Wyrd problem document.
    let mut unknown = wyrd_tonic::tonic::Request::new(proto::BifrostQueryRequest::from(request(
        "SELECT value FROM vala.bifrost.no_such_journey_table",
    )));
    unknown.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {}", writer.auth().bearer().await?.expose()).parse()?,
    );
    let refused = grpc
        .query(unknown)
        .await
        .err()
        .ok_or("a query over an unregistered table was served")?;
    let bytes = refused
        .metadata()
        .get_bin(wyrd_tonic::error::WYRD_ERROR_HEADER)
        .ok_or("the refusal carried no canonical problem envelope")?
        .to_bytes()?;
    let problem: serde_json::Value = serde_json::from_slice(&bytes)?;
    for field in ["code", "status", "title", "detail"] {
        if problem.get(field).is_none() {
            return Err(format!("the problem envelope omits {field}: {problem}").into());
        }
    }

    if let Some(query) = server.state().bifrost_query()
        && let Some(handle) = query.engine().analytical_execution()
        && !AnalyticalLiveInspection::is_clean(&handle.live()?)
    {
        return Err("the server retained Analytical ownership after both callers".into());
    }
    server.shutdown().await?;
    Ok(())
}

/// Decodes one public gRPC query stream and reports its rows and settled path.
///
/// # Errors
///
/// Returns a status, decode, or missing-terminal error.
async fn drain_grpc(
    mut frames: wyrd_tonic::tonic::Streaming<proto::QueryStreamFrame>,
) -> Result<(usize, QueryExecutionPath), ServerJourneyError> {
    let mut decoder = QueryIpcDecoder::new();
    let mut rows = 0_usize;
    let mut path = None;
    while let Some(frame) = frames.next().await {
        match frame?.frame {
            Some(proto::query_stream_frame::Frame::Schema(schema)) => {
                decoder.accept_schema(&schema.arrow_ipc_schema)?;
            }
            Some(proto::query_stream_frame::Frame::Batch(batch)) => {
                rows += decoder.accept_batch(&batch.arrow_ipc_batch)?.num_rows();
            }
            Some(proto::query_stream_frame::Frame::Terminal(terminal)) => {
                path = Some(
                    match proto::QueryExecutionPath::try_from(terminal.execution_path)? {
                        proto::QueryExecutionPath::Analytical => QueryExecutionPath::Analytical,
                        proto::QueryExecutionPath::Interactive => QueryExecutionPath::Interactive,
                        proto::QueryExecutionPath::Unspecified => {
                            return Err("the terminal named no execution path".into());
                        }
                    },
                );
            }
            None => return Err("the gRPC stream carried an empty frame".into()),
        }
    }
    Ok((
        rows,
        path.ok_or("the gRPC stream emitted no terminal frame")?,
    ))
}

/// Encodes one deterministic `(value)` row as an Arrow IPC stream.
fn ipc_value(value: i64) -> Vec<u8> {
    let schema = std::sync::Arc::new(arrow::datatypes::Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    let batch = arrow::record_batch::RecordBatch::try_new(
        std::sync::Arc::clone(&schema),
        vec![std::sync::Arc::new(arrow::array::Int64Array::from(vec![
            value,
        ]))],
    )
    .expect("one column of one row");
    let mut bytes = Vec::new();
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut bytes, schema.as_ref())
        .expect("valid schema");
    writer.write(&batch).expect("in-memory IPC write");
    writer.finish().expect("in-memory IPC finish");
    bytes
}
