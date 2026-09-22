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
pub(super) type ServerJourneyError = Box<dyn std::error::Error + Send + Sync>;

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
pub(super) fn scheduled_context(
    tenant: DataTenantId,
) -> Result<AuthorizedQueryContext, ServerJourneyError> {
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
        permission,
    )?)
}

/// Builds one two-hop verified delegation chain for attribution assertions.
///
/// The chain is a stand-in for a verified `act` claim: the scheduled caller has
/// no HTTP edge to mint one, but Oracle receives a chain the same way either
/// path delivers it — on the authorized context — so a forwarded query is the
/// honest place to prove the chain survives the signed envelope.
fn journey_delegation_chain() -> Vec<wyrd_runtime::DelegationStep> {
    [uuid::Uuid::from_u128(0xA), uuid::Uuid::from_u128(0xB)]
        .into_iter()
        .map(|id| wyrd_runtime::DelegationStep {
            principal: wyrd_runtime::PrincipalRef {
                id: PrincipalId::new(id),
                kind: PrincipalKind::User,
            },
        })
        .collect()
}

/// Reads the exact committed read-decision detail for one request id.
///
/// # Errors
///
/// Returns a SQL or JSON failure, or the absence of the expected audit row.
async fn read_decision_detail(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    request_id: &str,
) -> Result<serde_json::Value, ServerJourneyError> {
    let mut conn = server.tenant_conn_for(tenant).await?;
    let detail: String = sqlx::query_scalar(
        "SELECT detail FROM vala.audit_staging \
         WHERE operation = 'bifrost.query.read_decision' AND request_id = $1 \
         ORDER BY seq DESC LIMIT 1",
    )
    .bind(request_id)
    .fetch_one(&mut **conn.transaction())
    .await?;
    conn.commit().await?;
    Ok(serde_json::from_str(&detail)?)
}

/// Bounded budget for in-flight read-audit commits to finish before a count.
const AUDIT_STAGED_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

/// Bounded budget for every selected role to publish its readiness bit.
const READINESS_CEILING: std::time::Duration = std::time::Duration::from_secs(30);

/// Counts audit rows written under one operation name.
///
/// Waits for in-flight Oracle audit commits first so the count reflects every
/// read decision. A server hosting no Oracle role has none to wait for.
///
/// # Errors
///
/// Returns a failure naming the residual when commits do not finish within
/// [`AUDIT_STAGED_BUDGET`], or the tenant-connection or SQL error.
async fn audit_rows(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    operation: &str,
) -> Result<i64, ServerJourneyError> {
    if server.oracle_runtime_inspection().is_ok() {
        let pending = server.wait_oracle_audit_staged(AUDIT_STAGED_BUDGET).await?;
        if pending != 0 {
            return Err(format!(
                "read-audit commits did not finish: {pending} pending after {AUDIT_STAGED_BUDGET:?}"
            )
            .into());
        }
    }
    let mut conn = server.tenant_conn_for(tenant).await?;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM vala.audit_staging WHERE operation LIKE $1")
            .bind(operation)
            .fetch_one(&mut **conn.transaction())
            .await?;
    conn.commit().await?;
    Ok(count)
}

/// Real lifecycle controls require terminal proof even when the queried owner is absent.
///
/// Synthetic frame gates force transport timing without replacing the server's
/// authenticated controls. The process MCP journey supplies real owner cleanup.
///
/// # Errors
/// Returns server setup, control, channel or bounded-wait failures.
///
/// # Panics
/// Panics if cancellation is delayed, absence becomes proof, or terminal proof is lost.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn consumer_cancellation_requires_owner_terminal() -> Result<(), ServerJourneyError> {
    use vala_bifrost_redux::oracle::{OracleQueryStream, failed_terminal};
    use wyrd_spec::vala::BifrostError;
    use wyrd_spec::vala::api::{QueryStreamFrame, QueryTerminalErrorCode};

    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let controls = server
        .state()
        .bifrost
        .query_controls()
        .ok_or("missing query controls")?;
    for case in [
        "delayed",
        "EOF",
        "transport",
        "deadline",
        "not found",
        "consumed failed",
    ] {
        let request_id = RequestId::now_v7();
        assert_eq!(
            controls
                .get(tenant, request_id.clone())
                .await
                .expect_err("synthetic query has no owner")
                .code(),
            wyrd_spec::error::WyrdError::from(BifrostError::RunningQueryNotFound).code()
        );
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let token = tokio_util::sync::CancellationToken::new();
        let frames = Box::pin(futures_util::stream::poll_fn(move |context| {
            receiver.poll_recv(context)
        }));
        let mut stream =
            OracleQueryStream::test_new("settlement-fixture".to_owned(), frames, token.clone());
        stream.deadline_ms =
            chrono::Utc::now().timestamp_millis() + if case == "deadline" { 200 } else { 5_000 };
        let terminal = failed_terminal(QueryTerminalErrorCode::QueryExecutionFailed, 0);
        let observed = (case == "consumed failed").then(|| terminal.clone());
        let controls = controls.clone();
        let settlement = tokio::spawn(async move {
            controls
                .cancel_and_settle(
                    tenant,
                    request_id,
                    stream,
                    VisibilityMode::PublishedOnly,
                    observed,
                )
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), token.cancelled()).await?;
        assert!(
            token.is_cancelled(),
            "{case}: signal precedes terminal waiting"
        );
        match case {
            "delayed" => {
                tokio::time::sleep(std::time::Duration::from_millis(2_200)).await;
                assert!(
                    !settlement.is_finished(),
                    "two-second best effort is not settlement"
                );
                sender.send(Ok(QueryStreamFrame::Terminal(terminal)))?;
            }
            "not found" => {
                sender.send(Ok(QueryStreamFrame::Terminal(terminal)))?;
            }
            "transport" => {
                sender.send(Err(BifrostError::QueryExecutionFailed))?;
            }
            "EOF" => drop(sender),
            "deadline" | "consumed failed" => {}
            _ => unreachable!("closed fixture cases"),
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(4), settlement).await??;
        match case {
            "EOF" | "transport" => assert_eq!(
                result.expect_err("unconfirmed owner").code(),
                wyrd_spec::error::WyrdError::from(BifrostError::RunningQueryControlUnavailable)
                    .code()
            ),
            "deadline" => assert_eq!(
                result.expect_err("original deadline expires").code(),
                wyrd_spec::error::WyrdError::from(BifrostError::QueryStreamIncomplete).code()
            ),
            _ => result?,
        }
    }
    server.shutdown().await?;
    Ok(())
}

/// A verified forwarding envelope retains a shorter deadline than its request budget.
///
/// # Errors
/// Returns setup, catalog, acceptance or stream errors from the real server.
///
/// # Panics
/// Panics if acceptance replaces the signed instant or the empty-table query fails.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn signed_forwarding_preserves_shorter_absolute_deadline() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let table = format!("signed_deadline_{}", uuid::Uuid::now_v7().simple());
    server
        .state()
        .bifrost_catalog()
        .ok_or("missing catalog")?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, &table),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await?;
    let query = request(&format!("SELECT value FROM vala.bifrost.{table}"));
    assert_eq!(query.deadline_ms, Some(30_000));
    let deadline = chrono::Utc::now().timestamp_millis() + 5_000;
    let mut stream = server
        .state()
        .bifrost
        .query_with_signed_deadline_for_test(scheduled_context(tenant)?, query, deadline)
        .await?;
    assert_eq!(stream.deadline_ms, deadline);
    let mut terminal = None;
    while let Some(frame) = stream.frames.next().await {
        if let wyrd_spec::vala::api::QueryStreamFrame::Terminal(frame) = frame? {
            terminal = Some(frame);
        }
    }
    let terminal = terminal.expect("empty-table query still settles");
    assert_eq!(
        terminal.outcome,
        wyrd_spec::vala::api::QueryTerminalOutcome::Success
    );
    assert_eq!(terminal.row_count, 0);
    server.shutdown().await?;
    Ok(())
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
    prove_scheduled_analytical_peer_loss()
        .await
        .expect("scheduled Analytical peer-loss journey");
}

/// Waits for a bound server to publish readiness on `/readyz`.
///
/// `WyrdTestServer::start_bound` returns once the process answers `/healthz`,
/// which is liveness. Readiness additionally requires each selected role to
/// publish its own bit, and the Forge coordinator publishes only after its
/// first planning pass completes. Polling is therefore the claim: a single
/// probe races roles that are still converging and proves nothing about a
/// server that would have been ready a moment later.
///
/// # Errors
///
/// Returns the last `/readyz` body when readiness does not arrive within the
/// bounded window, so the failing probe and its reason code are visible.
async fn await_server_ready(base: &str) -> Result<(), ServerJourneyError> {
    let deadline = std::time::Instant::now() + READINESS_CEILING;
    let mut last = String::new();
    while std::time::Instant::now() < deadline {
        let response = reqwest::get(format!("{base}/readyz")).await?;
        if response.status().is_success() {
            return Ok(());
        }
        last = response.text().await.unwrap_or_default();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(format!("the server never reported ready on /readyz: {last}").into())
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
    await_server_ready(&base).await?;

    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, "server-query-caller", &["admin"])
        .await?;
    let api_key = bootstrap
        .api_key()
        .ok_or("machine bootstrap returned no key")?
        .clone();
    let writer = wyrd_testing::bifrost::write::BifrostWriter::connect(
        wyrd_client::config::ClientConfig {
            grpc: wyrd_client::transport::GrpcConfig {
                endpoint: server.grpc_url().ok_or("missing gRPC URL")?,
                connect_retries: 0,
                ..wyrd_client::transport::GrpcConfig::default()
            },
            http: wyrd_client::transport::HttpConfig {
                base_url: base.clone(),
                ..wyrd_client::transport::HttpConfig::default()
            },
            credential: Some(api_key.clone()),
            ..wyrd_client::config::ClientConfig::default()
        },
        bootstrap
            .card_ref()
            .ok_or("machine bootstrap returned no Card scope")?
            .clone(),
    )
    .await?;
    writer
        .write(
            &format!("vala.bifrost.{table}"),
            &value_schema(),
            FIXTURE_VALUES.map(value_row),
        )
        .await?;
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
        format!("Bearer {}", writer.client().auth().bearer().await?.expose()).parse()?,
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
        format!("Bearer {}", writer.client().auth().bearer().await?.expose()).parse()?,
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
fn value_schema() -> arrow::datatypes::SchemaRef {
    std::sync::Arc::new(arrow::datatypes::Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]))
}

/// Encodes one fixture row as the JSON the client write door buffers.
fn value_row(value: i64) -> Vec<u8> {
    format!(r#"{{"value": {value}}}"#).into_bytes()
}

/// Bounded polls the peer-loss phase waits on an Analytical lifecycle change.
const ANALYTICAL_POLLS: usize = 100;

/// A scheduled Analytical query whose peer dies produces no outcome and no leak.
///
/// The scheduled caller is the server's own entry, so a peer loss under it must
/// settle exactly like one under the public entry: a mapped stable failure, no
/// `ScheduledQueryOutcome`, the same single audited read decision the successful
/// phase above proves, and no retained Analytical ownership once cleanup joins.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_scheduled_analytical_peer_loss() -> Result<(), ServerJourneyError> {
    let mut cluster = wyrd_testing::bifrost::WyrdTestCluster::start_spec(
        wyrd_testing::bifrost::BifrostClusterSpec::three_oracles_one_scribe(),
    )
    .await?;
    let tenant = cluster.data_tenant_id();
    let table = format!("scheduled_peer_loss_{}", uuid::Uuid::now_v7().simple());
    let ingest = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("the cluster composed no Scribe")?;
    ingest
        .state()
        .bifrost_catalog()
        .ok_or("the Scribe composed no Bifrost catalog")?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, &table),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await?;

    let bootstrap = ingest
        .bootstrap_service_in_tenant(tenant, "scheduled-peer-loss-writer", &["admin"])
        .await?;
    let api_key = bootstrap
        .api_key()
        .ok_or("machine bootstrap returned no key")?
        .clone();
    let writer = wyrd_testing::bifrost::write::BifrostWriter::connect(
        wyrd_client::config::ClientConfig {
            grpc: wyrd_client::transport::GrpcConfig {
                endpoint: ingest.grpc_url().ok_or("missing gRPC URL")?,
                connect_retries: 0,
                ..wyrd_client::transport::GrpcConfig::default()
            },
            http: wyrd_client::transport::HttpConfig {
                base_url: ingest.base_url().ok_or("missing HTTP URL")?.to_owned(),
                ..wyrd_client::transport::HttpConfig::default()
            },
            credential: Some(api_key),
            ..wyrd_client::config::ClientConfig::default()
        },
        bootstrap
            .card_ref()
            .ok_or("machine bootstrap returned no Card scope")?
            .clone(),
    )
    .await?;
    // Two published objects rather than one: a single-partition leaf is
    // leader-executable, and this phase needs a root the planner distributes.
    for _ in 0..2 {
        writer
            .write(
                &format!("vala.bifrost.{table}"),
                &value_schema(),
                FIXTURE_VALUES.map(value_row),
            )
            .await?;
        ingest.flush_bifrost().await?;
    }
    cluster.refresh_oracle_snapshots().await?;
    let sql = format!(
        "SELECT value, COUNT(*) AS matched FROM vala.bifrost.{table} GROUP BY value ORDER BY value"
    );
    prove_scheduled_analytical_completion(&cluster, &sql).await?;

    // The pause is armed on one follower before the statement runs, so the
    // peer this phase kills is provably holding an activated stage rather than
    // racing the query's own completion.
    let victim_server = cluster.server(1).ok_or("missing follower node")?;
    let victim = victim_server.node_id();
    let pause = std::sync::Arc::new(
        vala_bifrost_redux::oracle::analytical::AnalyticalExecutePause::default(),
    );
    victim_server
        .state()
        .bifrost_query()
        .ok_or("the follower composed no Bifrost query surface")?
        .engine()
        .analytical_execution()
        .ok_or("the follower composed no Analytical handle")?
        .worker()
        .bind_execute_pause_for_test(std::sync::Arc::clone(&pause));

    let leader = cluster.server(0).ok_or("missing leader node")?;
    let leader_state = leader.state().clone();
    let reads_before = audit_rows(leader, tenant, "bifrost.query.read_decision").await?;

    let scheduled = {
        let context = scheduled_context(tenant)?;
        tokio::spawn(async move {
            ScheduledQueryCaller::new(
                leader_state,
                context,
                tokio_util::sync::CancellationToken::new(),
            )
            .run(request(&sql))
            .await
        })
    };

    // The peer is killed only once it actually holds an activated stage, so the
    // loss is a real mid-execution failure rather than a race with selection.
    tokio::time::timeout(std::time::Duration::from_secs(30), pause.wait_paused())
        .await
        .map_err(|_| "no follower ever held an activated Analytical stage")?;
    cluster.terminate_node_abruptly_for_test(victim).await?;

    match scheduled.await? {
        Ok(outcome) => {
            return Err(format!(
                "a lost peer still produced a scheduled outcome of {} rows",
                outcome.rows
            )
            .into());
        }
        Err(error) if error.code().starts_with("WYRD_") => {}
        Err(error) => {
            return Err(format!("the lost peer surfaced {error} as {}", error.code()).into());
        }
    }

    // One accepted logical read, exactly as the successful phase proves: a
    // failed execution neither skips its audited decision nor writes a second.
    let leader = cluster.server(0).ok_or("missing leader node")?;
    if audit_rows(leader, tenant, "bifrost.query.read_decision").await? != reads_before + 1 {
        return Err("the failed scheduled query did not relay exactly one logical read".into());
    }
    if audit_rows(leader, tenant, "bifrost.query.stage%").await? != 0 {
        return Err("a failed query stage appended its own audit row".into());
    }

    await_clean_analytical(&cluster).await?;
    cluster.shutdown().await?;
    Ok(())
}

/// Runs successful and actively cancelled scheduled Analytical queries through Scribe ingress.
///
/// # Errors
/// Returns dispatch, audit, activation or cleanup failures from the real cluster.
///
/// # Panics
/// Panics if either phase skips Analytical execution, duplicates audit, or retains ownership.
async fn prove_scheduled_analytical_completion(
    cluster: &wyrd_testing::bifrost::WyrdTestCluster,
    sql: &str,
) -> Result<(), ServerJourneyError> {
    let tenant = cluster.data_tenant_id();
    let ingress = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing Scribe ingress")?;
    assert!(
        ingress.state().bifrost.oracle().is_none(),
        "the scheduled ingress must forward"
    );
    assert!(
        ingress.state().bifrost.query_controls().is_some(),
        "forwarded ingress retains lifecycle routing"
    );
    let before = audit_rows(ingress, tenant, "bifrost.query.read_decision").await?;
    // Forwarding-only ingress: this node owns no Oracle, so the read decision
    // below is built and committed by the remote leader from the context the
    // signed forwarding envelope carried.
    let chain = journey_delegation_chain();
    let delegated = scheduled_context(tenant)?.with_delegation_chain(chain.clone());
    let delegated_request_id = delegated.request_id.to_string();
    let outcome = ScheduledQueryCaller::new(
        ingress.state().clone(),
        delegated,
        tokio_util::sync::CancellationToken::new(),
    )
    .run(request(sql))
    .await?;
    assert_eq!(
        outcome.terminal.outcome,
        wyrd_spec::vala::api::QueryTerminalOutcome::Success
    );
    assert_eq!(
        outcome.terminal.execution_path,
        QueryExecutionPath::Analytical
    );
    assert_eq!(outcome.rows, u64::try_from(FIXTURE_VALUES.len())?);
    assert_eq!(outcome.terminal.row_count, outcome.rows);
    assert_scheduled_owners_released(cluster)?;
    assert_eq!(
        audit_rows(ingress, tenant, "bifrost.query.read_decision").await?,
        before + 1
    );
    let detail = read_decision_detail(ingress, tenant, &delegated_request_id).await?;
    assert_eq!(
        detail["delegation_chain"]
            .as_array()
            .ok_or("the forwarded read decision records its delegation chain")?
            .iter()
            .map(|step| step["principal_id"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>(),
        chain
            .iter()
            .map(|step| step.principal.id.to_string())
            .collect::<Vec<_>>(),
        "remote execution audits the same ordered chain local execution would: {detail}"
    );

    let leader_id = cluster
        .servers()
        .filter(|server| server.state().bifrost.oracle().is_some())
        .map(WyrdTestServer::node_id)
        .min()
        .ok_or("missing ready Oracle")?;
    let follower = cluster
        .servers()
        .find(|server| server.node_id() != leader_id && server.state().bifrost.oracle().is_some())
        .ok_or("missing remote follower")?;
    let pause = std::sync::Arc::new(
        vala_bifrost_redux::oracle::analytical::AnalyticalExecutePause::default(),
    );
    follower
        .state()
        .bifrost_query()
        .ok_or("missing follower Oracle")?
        .engine()
        .analytical_execution()
        .ok_or("missing follower execution")?
        .worker()
        .bind_execute_pause_for_test(std::sync::Arc::clone(&pause));
    let cancellation = tokio_util::sync::CancellationToken::new();
    let caller = ScheduledQueryCaller::new(
        ingress.state().clone(),
        scheduled_context(tenant)?,
        cancellation.clone(),
    );
    let query = request(sql);
    let scheduled = tokio::spawn(async move { caller.run(query).await });
    let entered =
        tokio::time::timeout(std::time::Duration::from_secs(10), pause.wait_paused()).await;
    cancellation.cancel();
    pause.release();
    entered?;
    let error = scheduled
        .await?
        .expect_err("an actively cancelled schedule has no outcome");
    assert_eq!(error.code(), "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE");
    assert_scheduled_owners_released(cluster)?;
    assert_eq!(
        audit_rows(ingress, tenant, "bifrost.query.read_decision").await?,
        before + 2
    );
    Ok(())
}

/// Checks ownership at the scheduled return boundary, without a later polling grace period.
///
/// # Errors
/// Returns inspection failure or retained query/graph/admission/resource evidence.
fn assert_scheduled_owners_released(
    cluster: &wyrd_testing::bifrost::WyrdTestCluster,
) -> Result<(), ServerJourneyError> {
    for server in cluster.servers() {
        let Some(query) = server.state().bifrost_query() else {
            continue;
        };
        let engine = query.engine();
        let live = engine
            .analytical_execution()
            .ok_or("missing analytical owner")?
            .live()?;
        let root = engine.role_resources().snapshot()?;
        if !live.is_clean()
            || !engine
                .running_queries()
                .list(cluster.data_tenant_id())
                .is_empty()
            || root.oracle_query_active
            || root.oracle_query_slot_units != 0
            || root.oracle_query_memory_used_bytes != 0
            || root.oracle_query_scratch_used_bytes != 0
        {
            return Err(format!("scheduled return retained ownership: {live:?}, {root:?}").into());
        }
    }
    Ok(())
}

/// Waits until every running node retains no Analytical ownership.
///
/// # Errors
///
/// Returns an inspection error, or a description of what a node still held.
async fn await_clean_analytical(
    cluster: &wyrd_testing::bifrost::WyrdTestCluster,
) -> Result<(), ServerJourneyError> {
    let mut last = String::new();
    for _ in 0..ANALYTICAL_POLLS {
        let mut clean = true;
        last.clear();
        for server in cluster.servers() {
            let Some(query) = server.state().bifrost_query() else {
                continue;
            };
            let Some(handle) = query.engine().analytical_execution() else {
                continue;
            };
            let live = handle.live()?;
            if !AnalyticalLiveInspection::is_clean(&live) {
                clean = false;
                last = format!("{live:?}");
            }
        }
        if clean {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!("a node still retained Analytical ownership: {last}").into())
}

/// Tenant-local roles reach exactly the Bifrost tables their scope names.
///
/// Both halves of the approved role matrix run against one real bound server
/// through the public SDK, so the decision under test is the production one:
/// the route admits the coarse Bifrost read capability, Oracle resolves the
/// complete scan set from the catalog, and the object decision is taken against
/// the resolved tables before any row is read.
///
/// The journey also proves the denial is durable: one refusal writes exactly one
/// tenant-bound denial event and no accepted-read event, and a refusal whose own
/// audit append fails is reported as audit-unavailable rather than as a plain
/// rejection. Finally a schema-scoped bearer is presented directly on the
/// generated gRPC query service: its covered query drains and an uncovered one
/// is refused before a response stream opens.
///
/// # Panics
///
/// Panics when a scoped role reaches a table it was not granted, is refused one
/// it was granted, a refusal returns rows, or a refusal is not durably audited.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn tenant_scoped_roles_reach_only_their_granted_bifrost_tables() {
    prove_object_scoped_role_matrix()
        .await
        .expect("object-scoped Bifrost role matrix journey");
}

/// Stable code a principal receives for a table its grants do not cover.
const QUERY_FORBIDDEN: &str = "WYRD_VALA_403_QUERY_FORBIDDEN";
/// Stable code substituted when a refusal's own audit append cannot commit.
const AUDIT_UNAVAILABLE: &str = "WYRD_VALA_500_AUDIT_UNAVAILABLE";
/// Audited operation name the public query route decides under.
const QUERY_OPERATION: &str = "vala.query.sync";
/// Audited operation name Oracle commits one accepted read decision under.
const READ_DECISION_OPERATION: &str = "bifrost.query.read_decision";

/// Non-sensitive projection of `vala.logs.records`.
const LOGS_SQL: &str = "SELECT severity_text FROM vala.logs.records";
/// Non-sensitive projection of `vala.traces.spans`.
const TRACES_SQL: &str = "SELECT name FROM vala.traces.spans";
/// One plan whose resolved scan set spans both schemas.
const MIXED_SQL: &str = "SELECT r.severity_text, s.name \
     FROM vala.logs.records r JOIN vala.traces.spans s ON r.trace_id = s.trace_id";

/// Drives the complete `analyst` / `data_scientist` access matrix.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_object_scoped_role_matrix() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let catalog = server
        .state()
        .bifrost_catalog()
        .ok_or("server composed no Bifrost catalog")?;

    // Both canonical built-ins exist before any grant is written: a grant names
    // the UID the catalog assigned, so the registration is what the scope is
    // derived from rather than a name the test invented.
    let logs_uid = catalog
        .ensure_builtin(
            tenant,
            vala_bifrost_redux::tables::builtin_table("logs", "records")
                .ok_or("no canonical vala.logs.records definition")?,
        )
        .await?;
    let spans_uid = catalog
        .ensure_builtin(
            tenant,
            vala_bifrost_redux::tables::builtin_table("traces", "spans")
                .ok_or("no canonical vala.traces.spans definition")?,
        )
        .await?;

    let base = server.base_url().ok_or("missing HTTP URL")?.to_owned();
    await_server_ready(&base).await?;

    // `analyst` holds the whole `vala.logs` schema; `data_scientist` holds the
    // single resolved `vala.traces.spans` UID and nothing else.
    let analyst = scoped_client(
        &server,
        "analyst",
        wyrd_runtime::PermissionScope::Bifrost(wyrd_runtime::BifrostPermissionScope::Schema(
            wyrd_runtime::BifrostSchemaScope {
                catalog: "vala".to_owned(),
                schema: "logs".to_owned(),
            },
        )),
    )
    .await?;
    let data_scientist = scoped_client(
        &server,
        "data_scientist",
        wyrd_runtime::PermissionScope::Bifrost(wyrd_runtime::BifrostPermissionScope::Table(
            wyrd_runtime::BifrostTableScope {
                catalog: "vala".to_owned(),
                schema: "traces".to_owned(),
                table_uid: uuid::Uuid::from_bytes(*spans_uid.as_bytes()),
            },
        )),
    )
    .await?;

    assert_ne!(
        uuid::Uuid::from_bytes(*logs_uid.as_bytes()),
        uuid::Uuid::from_bytes(*spans_uid.as_bytes()),
        "the two built-ins must carry distinct registered identities"
    );

    accepts(&analyst, LOGS_SQL).await?;
    refuses(&analyst, TRACES_SQL).await?;
    refuses(&analyst, MIXED_SQL).await?;

    accepts(&data_scientist, TRACES_SQL).await?;
    refuses(&data_scientist, LOGS_SQL).await?;
    refuses(&data_scientist, MIXED_SQL).await?;

    // One public scoped-object denial is a durable, tenant-bound audit event.
    // The principal passed coarse route admission, so without this the tenant's
    // chain would hold no record that it repeatedly probed tables it does not
    // hold. A denial must also never look like an accepted read.
    let denials_before = audit_rows(&server, tenant, QUERY_OPERATION).await?;
    let reads_before = audit_rows(&server, tenant, READ_DECISION_OPERATION).await?;
    refuses(&analyst, TRACES_SQL).await?;
    let denials_after = audit_rows(&server, tenant, QUERY_OPERATION).await?;
    let reads_after = audit_rows(&server, tenant, READ_DECISION_OPERATION).await?;
    if denials_after != denials_before + 1 {
        return Err(format!(
            "one scoped-object denial must write exactly one durable denial event, \
             got {denials_before} -> {denials_after}"
        )
        .into());
    }
    if reads_after != reads_before {
        return Err(format!(
            "a denied object decision must write no accepted-read event, \
             got {reads_before} -> {reads_after}"
        )
        .into());
    }

    // The denial fails closed on its own append: an unrecordable refusal is
    // reported as audit-unavailable, never as a plain rejection.
    server.fail_query_object_denial_audit();
    refuses_with(&analyst, TRACES_SQL, AUDIT_UNAVAILABLE).await?;
    server.restore_query_object_denial_audit();
    refuses(&analyst, TRACES_SQL).await?;

    prove_scoped_bearer_over_grpc(&server, &analyst).await?;

    server.shutdown().await?;
    Ok(())
}

/// Presents one schema-scoped bearer on the generated gRPC query service.
///
/// The HTTP matrix above proves Oracle's table decision; this proves the gRPC
/// adapter hands the same signed `permissions` to it. The `vala.logs` bearer
/// drains its covered query, and an uncovered `vala.traces` query is refused
/// with `WYRD_VALA_403_QUERY_FORBIDDEN` from the call itself, before any
/// response stream exists.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_scoped_bearer_over_grpc(
    server: &WyrdTestServer,
    analyst: &wyrd_client::WyrdClient,
) -> Result<(), ServerJourneyError> {
    let bearer = format!("Bearer {}", analyst.auth().bearer().await?.expose());
    let mut grpc = BifrostQueryServiceClient::new(
        wyrd_tonic::tonic::transport::Endpoint::from_shared(
            server.grpc_url().ok_or("missing gRPC URL")?,
        )?
        .connect()
        .await?,
    );
    let query = |sql: &str| -> Result<_, ServerJourneyError> {
        let mut request =
            wyrd_tonic::tonic::Request::new(proto::BifrostQueryRequest::from(request(sql)));
        request
            .metadata_mut()
            .insert("x-wyrd-access-token", bearer.parse()?);
        Ok(request)
    };

    let covered = grpc
        .query(query(LOGS_SQL)?)
        .await
        .map_err(|status| format!("the covered gRPC query must be admitted: {status:?}"))?;
    drain_grpc(covered.into_inner())
        .await
        .map_err(|error| format!("the covered gRPC query must drain: {error}"))?;

    match grpc.query(query(TRACES_SQL)?).await {
        Err(status)
            if status.code() == wyrd_tonic::tonic::Code::PermissionDenied
                && wyrd_client::error::from_grpc_status(&status).code() == QUERY_FORBIDDEN => {}
        Err(status) => {
            return Err(format!(
                "the uncovered gRPC query must be refused with {QUERY_FORBIDDEN}, got {status:?}"
            )
            .into());
        }
        Ok(_) => return Err("the uncovered gRPC query opened a response stream".into()),
    }
    Ok(())
}

/// Service B acts for Service A against Bifrost and holds only A's authority.
///
/// A holds read on one concrete table; B holds read on that table plus table
/// write. B, configured as an ordinary shared client, calls `on_behalf_of` with
/// A's token: the delegated token names A as subject, B as the outer actor, and
/// the Bifrost audience, and carries only the exact-table read. On the real
/// Bifrost surface the read succeeds and a table registration is refused before
/// effect, while B's own token then creates that same table. The invoke policy saw
/// A-to-B, and both the exchange and the read decision are audited under A
/// with B as the actor. The same token is refused on a non-Bifrost route.
///
/// # Panics
///
/// Panics when any contract claim, access decision, or attribution breaks.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn service_b_acts_for_service_a_with_only_a_table_authority() {
    prove_service_b_acts_for_service_a()
        .await
        .expect("service B acts for service A with only A's table authority");
}

/// Drives the A-to-B delegation journey.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_service_b_acts_for_service_a() -> Result<(), ServerJourneyError> {
    let policy = std::sync::Arc::new(wyrd_auth_check::RecordingPolicyHook::default());
    let server = WyrdTestServer::builder()
        .with_policy_hook(policy.clone())
        .start_bound()
        .await?;
    let tenant = server.data_tenant_id();
    let catalog = server
        .state()
        .bifrost_catalog()
        .ok_or("server composed no Bifrost catalog")?;
    let spans_uid = catalog
        .ensure_builtin(
            tenant,
            vala_bifrost_redux::tables::builtin_table("traces", "spans")
                .ok_or("no canonical vala.traces.spans definition")?,
        )
        .await?;
    await_server_ready(server.base_url().ok_or("missing HTTP URL")?).await?;

    let spans_read = Permission {
        resource: wyrd_runtime::Resource::BifrostQuery,
        action: wyrd_runtime::Action::Read,
        scope: wyrd_runtime::PermissionScope::Bifrost(wyrd_runtime::BifrostPermissionScope::Table(
            wyrd_runtime::BifrostTableScope {
                catalog: "vala".to_owned(),
                schema: "traces".to_owned(),
                table_uid: uuid::Uuid::from_bytes(*spans_uid.as_bytes()),
            },
        )),
    };
    let write = Permission::bifrost_table_write();
    server
        .seed_role("delegation_subject", std::slice::from_ref(&spans_read))
        .await?;
    server
        .seed_role("delegation_actor", &[spans_read.clone(), write.clone()])
        .await?;
    let a = server
        .bootstrap_service_in_tenant(tenant, "delegation-a", &["delegation_subject"])
        .await?;
    let b = server
        .bootstrap_service_in_tenant(tenant, "delegation-b", &["delegation_actor"])
        .await?;
    let a_token = server
        .exchange_api_key(a.api_key().ok_or("A carries no API key")?)
        .await?;
    let b_client = client_for(&server, b.api_key().ok_or("B carries no API key")?)?;

    let delegated = b_client
        .on_behalf_of(
            secrecy::SecretString::from(a_token),
            wyrd_spec::auth::TokenAudience::Bifrost,
        )
        .await?;

    let bearer = delegated.auth().bearer().await?;
    let claims = decode_claims(bearer.expose())?;
    assert_eq!(claims["sub"], a.id().to_string(), "subject is A: {claims}");
    assert_eq!(
        claims["act"]["sub"],
        b.id().to_string(),
        "outer actor is B: {claims}"
    );
    assert!(claims["act"].get("act").is_none(), "one actor: {claims}");
    assert_eq!(claims["aud"], "bifrost", "Bifrost audience: {claims}");
    let permissions: PermissionSet = serde_json::from_value(claims["permissions"].clone())?;
    assert!(
        permissions.contains(&spans_read),
        "exact-table read: {claims}"
    );
    assert!(!permissions.contains(&write), "no write: {claims}");

    let invoke = policy.last().ok_or("the invoke policy was not asked")?;
    assert_eq!(invoke.subject.id.to_string(), a.id().to_string());
    assert_eq!(invoke.actor.id.to_string(), b.id().to_string());

    accepts(&delegated, TRACES_SQL).await?;
    let register = wyrd_spec::vala::api::RegisterTableRequest {
        namespace: "vala.datasets".to_owned(),
        name: "delegation_events".to_owned(),
        fields: vec![wyrd_spec::vala::api::FieldSpec {
            name: "value".to_owned(),
            data_type: wyrd_spec::vala::api::DataTypeSpec::Int64,
            nullable: true,
            metadata: std::collections::BTreeMap::new(),
        }],
        physical_layout: None,
    };
    let refused = delegated
        .request_json::<_, serde_json::Value>(
            reqwest::Method::POST,
            "/v1/bifrost/tables",
            Some(&register),
        )
        .await
        .err()
        .ok_or("the delegated token must not register a table")?;
    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    // The Bifrost-bound token cannot be replayed against a non-Bifrost route.
    let replayed = delegated
        .request_json::<(), serde_json::Value>(
            reqwest::Method::GET,
            &format!("/v1/principals/{}/credentials", a.id()),
            None,
        )
        .await
        .err()
        .ok_or("a Bifrost-audience token must not reach a Wyrd route")?;
    assert!(
        replayed.code().starts_with("WYRD_AUTH_401"),
        "audience mismatch is unauthenticated: {replayed:?}"
    );

    // B's own token then creates the table, so the refused call left nothing.
    let own: wyrd_spec::vala::api::RegisterTableResponse = b_client
        .request_json(reqwest::Method::POST, "/v1/bifrost/tables", Some(&register))
        .await?;
    assert_eq!(
        own.outcome,
        wyrd_spec::vala::api::RegisterOutcome::Created,
        "the refused registration had no effect"
    );

    audit_rows(&server, tenant, READ_DECISION_OPERATION).await?;
    let mut conn = server.tenant_conn_for(tenant).await?;
    let attributed: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT operation, resource, detail FROM vala.audit_staging \
         WHERE principal_id = $1 AND operation IN ('auth.token.exchange', $2) ORDER BY seq",
    )
    .bind(uuid::Uuid::parse_str(&a.id().to_string())?)
    .bind(READ_DECISION_OPERATION)
    .fetch_all(&mut **conn.transaction())
    .await?;
    conn.commit().await?;
    let b_id = b.id().to_string();
    let exchange = attributed
        .iter()
        .filter(|(operation, ..)| operation == "auth.token.exchange")
        .map(|(_, resource, detail)| Ok((resource, serde_json::from_str(detail)?)))
        .collect::<Result<Vec<(&String, serde_json::Value)>, ServerJourneyError>>()?
        .into_iter()
        .find(|(_, detail)| detail["actor_principal_id"] == b_id.as_str())
        .ok_or("the delegated exchange is audited under A with B as actor")?;
    assert_eq!(exchange.0, "bifrost", "the exchange targets Bifrost");
    assert_eq!(exchange.1["subject_principal_id"], a.id().to_string());
    let read = attributed
        .iter()
        .find(|(operation, ..)| operation == READ_DECISION_OPERATION)
        .ok_or("the delegated read is audited under A")?;
    let read_detail: serde_json::Value = serde_json::from_str(&read.2)?;
    assert_eq!(
        read_detail["delegation_chain"][0]["principal_id"], b_id,
        "the read names B as actor: {read_detail}"
    );

    server.shutdown().await?;
    Ok(())
}

/// Decodes a JWT payload for contract assertions only; never verifies it.
///
/// # Errors
///
/// Returns a failure when the token has no payload segment or it is not JSON.
fn decode_claims(jwt: &str) -> Result<serde_json::Value, ServerJourneyError> {
    use base64::Engine as _;
    let payload = jwt.split('.').nth(1).ok_or("the JWT has no payload")?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload)?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Seeds one tenant-local role at `scope` and returns a client bound to it.
///
/// The role exists only here. Neither name is a builtin: the matrix is a
/// statement about scoped grants, not about Wyrd shipping an analyst role.
///
/// # Errors
///
/// Returns a role-seed, bootstrap, or client-construction failure.
async fn scoped_client(
    server: &WyrdTestServer,
    role: &str,
    scope: wyrd_runtime::PermissionScope,
) -> Result<wyrd_client::WyrdClient, ServerJourneyError> {
    server
        .seed_role(
            role,
            &[Permission {
                resource: wyrd_runtime::Resource::BifrostQuery,
                action: wyrd_runtime::Action::Read,
                scope,
            }],
        )
        .await?;
    let bootstrap = server
        .bootstrap_service_in_tenant(server.data_tenant_id(), role, &[role])
        .await?;
    client_for(
        server,
        bootstrap
            .api_key()
            .ok_or("the bootstrapped service carries no API key")?,
    )
}

/// Builds an ordinary shared client authenticating with `api_key` against
/// `server`'s HTTP and gRPC listeners.
///
/// # Errors
///
/// Returns a missing-listener or client-construction failure.
fn client_for(
    server: &WyrdTestServer,
    api_key: &secrecy::SecretString,
) -> Result<wyrd_client::WyrdClient, ServerJourneyError> {
    Ok(wyrd_client::WyrdClient::with_config(
        wyrd_client::config::ClientConfig {
            grpc: wyrd_client::transport::GrpcConfig {
                endpoint: server.grpc_url().ok_or("missing gRPC URL")?,
                connect_retries: 0,
                ..wyrd_client::transport::GrpcConfig::default()
            },
            http: wyrd_client::transport::HttpConfig {
                base_url: server.base_url().ok_or("missing HTTP URL")?.to_owned(),
                ..wyrd_client::transport::HttpConfig::default()
            },
            credential: Some(api_key.clone()),
            ..wyrd_client::config::ClientConfig::default()
        },
    )?)
}

/// Asserts one query is authorized and streams to completion.
///
/// # Errors
///
/// Returns the refusal when the granted table is denied, or the streaming
/// failure when an authorized query cannot be drained.
async fn accepts(client: &wyrd_client::WyrdClient, sql: &str) -> Result<(), ServerJourneyError> {
    let mut stream = wyrd_client::Bifrost::query_only(client)
        .query(&request(sql))
        .await
        .map_err(|error| format!("`{sql}` must be authorized: {error}"))?;
    while stream.next_batch().await?.is_some() {}
    Ok(())
}

/// Asserts one query is refused before its stream opens and returns no rows.
///
/// A refusal that arrives after an open stream has already told the caller the
/// query was accepted, so the claim is specifically that the SDK never hands
/// back a stream at all.
///
/// # Errors
///
/// Returns a failure when the query is accepted, or when it fails with anything
/// other than the stable query-forbidden refusal.
async fn refuses(client: &wyrd_client::WyrdClient, sql: &str) -> Result<(), ServerJourneyError> {
    refuses_with(client, sql, QUERY_FORBIDDEN).await
}

/// Asserts one query is refused before its stream opens with the exact `code`.
///
/// # Errors
///
/// Returns a failure when the query is accepted, or when it fails with anything
/// other than `code`.
async fn refuses_with(
    client: &wyrd_client::WyrdClient,
    sql: &str,
    code: &str,
) -> Result<(), ServerJourneyError> {
    match wyrd_client::Bifrost::query_only(client)
        .query(&request(sql))
        .await
    {
        Ok(mut stream) => {
            let mut rows = 0_usize;
            while let Some(batch) = stream.next_batch().await? {
                rows += batch.num_rows();
            }
            Err(
                format!("`{sql}` must be refused before its stream opens, streamed {rows} rows")
                    .into(),
            )
        }
        Err(wyrd_client::bifrost::BifrostClientError::Transport(error)) if error.code() == code => {
            Ok(())
        }
        Err(other) => Err(format!("`{sql}` must be refused with {code}, got {other}").into()),
    }
}

/// Access-token lifetime the stream-admission journey mints under.
const SHORT_ACCESS_TTL: chrono::Duration = chrono::Duration::seconds(3);

/// A bearer is checked once, when a query stream is admitted.
///
/// The server mints three-second tokens with no clock-skew allowance, so a real
/// `exp` is crossed in-test. One stream is opened while the bearer is valid and
/// drained only after it lapses: admitted work finishes under its own query
/// deadline because nothing re-verifies mid-stream. The same bearer then cannot
/// open another stream, which is refused as unauthenticated before planning.
///
/// # Panics
///
/// Panics when the admitted stream fails after expiry or the expired bearer
/// opens a new stream.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn an_expired_bearer_finishes_its_admitted_stream_but_opens_no_other() {
    prove_stream_admission_expiry()
        .await
        .expect("stream-admission expiry journey");
}

/// Drives one admitted stream past its bearer's expiry, then retries admission.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_stream_admission_expiry() -> Result<(), ServerJourneyError> {
    let server = wyrd_testing::WyrdTestServerBuilder::default()
        .with_access_ttl(SHORT_ACCESS_TTL)
        .with_auth_verify_settings(wyrd_auth_verify::WyrdAuthVerifySettings {
            allowed_clock_skew: std::time::Duration::ZERO,
        })
        .start_bound()
        .await?;
    let tenant = server.data_tenant_id();
    server
        .state()
        .bifrost_catalog()
        .ok_or("server composed no Bifrost catalog")?
        .ensure_builtin(
            tenant,
            vala_bifrost_redux::tables::builtin_table("logs", "records")
                .ok_or("no canonical vala.logs.records definition")?,
        )
        .await?;
    await_server_ready(server.base_url().ok_or("missing HTTP URL")?).await?;

    let client =
        scoped_client(&server, "stream_reader", wyrd_runtime::PermissionScope::All).await?;
    let bearer = format!("Bearer {}", client.auth().bearer().await?.expose());
    let mut grpc = BifrostQueryServiceClient::new(
        wyrd_tonic::tonic::transport::Endpoint::from_shared(
            server.grpc_url().ok_or("missing gRPC URL")?,
        )?
        .connect()
        .await?,
    );
    let query = |bearer: &str| -> Result<_, ServerJourneyError> {
        let mut request =
            wyrd_tonic::tonic::Request::new(proto::BifrostQueryRequest::from(request(LOGS_SQL)));
        request
            .metadata_mut()
            .insert("x-wyrd-access-token", bearer.parse()?);
        Ok(request)
    };

    let admitted = grpc.query(query(&bearer)?).await?.into_inner();
    tokio::time::sleep((SHORT_ACCESS_TTL + chrono::Duration::seconds(1)).to_std()?).await;
    drain_grpc(admitted)
        .await
        .map_err(|error| format!("an admitted stream must finish after expiry: {error}"))?;

    match grpc.query(query(&bearer)?).await {
        Err(status) if status.code() == wyrd_tonic::tonic::Code::Unauthenticated => {}
        Err(status) => {
            return Err(
                format!("an expired bearer must be unauthenticated, got {status:?}").into(),
            );
        }
        Ok(_) => return Err("an expired bearer opened a new query stream".into()),
    }

    server.shutdown().await?;
    Ok(())
}

/// Generic protected-edge limit used by the staged query timeout journey.
const EDGE_LIMIT: std::time::Duration = std::time::Duration::from_secs(1);

/// Oracle preparation can outlive the generic edge limit but not its own deadline.
///
/// The server's edge timeout is shortened below explicit query deadlines. A
/// request-scoped preparation pause holds one query past the edge limit and
/// releases it before Oracle expiry, which must still stream a successful
/// terminal; a second query held past its own deadline must fail with Oracle's
/// typed timeout. A stalled query body and a stalled non-query body must still
/// receive the generic edge timeout, proving only post-handoff query work is
/// exempt.
///
/// # Errors
/// Returns server setup, catalog, authentication, HTTP or frame decoding errors.
///
/// # Panics
/// Panics if a timeout is attributed to the wrong owner or the released query
/// does not settle successfully.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn query_edge_timeout_yields_to_oracle_deadline() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::builder()
        .with_limits_for_test(wyrd_server::state::LimitsConfig {
            timeout: EDGE_LIMIT,
            ..wyrd_server::state::LimitsConfig::default()
        })
        .start_bound()
        .await?;
    let tenant = server.data_tenant_id();
    let table = format!("edge_timeout_{}", uuid::Uuid::now_v7().simple());
    server
        .state()
        .bifrost_catalog()
        .ok_or("missing catalog")?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, &table),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await?;
    let base = server.base_url().ok_or("missing HTTP URL")?.to_owned();
    await_server_ready(&base).await?;
    let api_key = server
        .bootstrap_service_in_tenant(tenant, "edge-timeout-caller", &["admin"])
        .await?
        .api_key()
        .ok_or("machine bootstrap returned no key")?
        .clone();
    let edge = EdgeTimeoutClient {
        http: reqwest::Client::new(),
        base,
        token: server.exchange_api_key(&api_key).await?,
    };
    let oracle = server
        .state()
        .bifrost_query()
        .map(|query| std::sync::Arc::clone(query.engine()))
        .ok_or("missing Oracle")?;
    let sql = format!("SELECT value FROM vala.bifrost.{table}");

    // Released before Oracle expiry: the edge limit no longer applies.
    let (pause, response) = edge.paused_query(&oracle, &sql, 10_000).await?;
    tokio::time::sleep(EDGE_LIMIT * 2).await;
    assert!(
        !response.is_finished(),
        "edge timer must not end preparation"
    );
    pause.release();
    let response = response.await??;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let mut decoder = wyrd_tonic::frame_codec::FrameDecoder::new(16 * 1024 * 1024);
    let mut terminal = None;
    for frame in decoder.push::<proto::QueryStreamFrame>(&response.bytes().await?)? {
        if let Some(proto::query_stream_frame::Frame::Terminal(frame)) = frame.frame {
            terminal = Some(frame.outcome);
        }
    }
    assert_eq!(
        terminal,
        Some(proto::QueryTerminalOutcome::Success as i32),
        "released query settles successfully"
    );

    // Held past Oracle expiry: the typed query timeout survives.
    let (pause, response) = edge.paused_query(&oracle, &sql, 3_000).await?;
    let response = tokio::time::timeout(std::time::Duration::from_secs(10), response).await???;
    pause.release();
    assert_eq!(problem_code(response).await?, "WYRD_VALA_504_QUERY_TIMEOUT");
    oracle.bind_preparation_pause_for_test(None)?;

    // Stalled bodies before Oracle handoff keep the generic edge timeout.
    let query_body = serde_json::to_vec(&request(&sql))?;
    for (path, body) in [
        ("/v1/query", query_body),
        ("/v1/cards", b"{\"apiVersion\":\"wyrd/v1\"}".to_vec()),
    ] {
        let response = edge.stalled_post(path, body).await?;
        assert_eq!(
            problem_code(response).await?,
            "WYRD_SERVER_504_REQUEST_TIMEOUT",
            "{path} body collection stays edge-bounded"
        );
    }
    server.shutdown().await?;
    Ok(())
}

/// A silent selected remote Oracle cannot hold an HTTP query past its deadline.
///
/// On a role-separated cluster the Scribe-only ingress owns no Oracle, so the
/// public query is forwarded to a remote leader. Every Oracle is armed to park
/// the forwarded envelope before acceptance, making the selected peer a
/// connected replica that never answers. The HTTP request must still settle
/// with the typed query timeout, exactly one envelope may arrive, and the
/// ingress must cancel that parked call. Once disarmed, the same ingress serves
/// a forwarded query successfully, proving the protected request was released.
///
/// # Errors
/// Returns cluster setup, catalog, authentication, HTTP or frame decoding errors.
///
/// # Panics
/// Panics if the silent peer receives other than one envelope or the follow-up
/// query does not succeed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn silent_remote_oracle_delivery_yields_typed_query_timeout() -> Result<(), ServerJourneyError>
{
    let cluster = wyrd_testing::bifrost::WyrdTestCluster::start_spec(
        wyrd_testing::bifrost::BifrostClusterSpec::role_separated(),
    )
    .await?;
    let tenant = cluster.data_tenant_id();
    let table = format!("silent_peer_{}", uuid::Uuid::now_v7().simple());
    let ingress = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("missing Scribe ingress")?;
    assert!(
        ingress.state().bifrost.oracle().is_none(),
        "the ingress must forward"
    );
    ingress
        .state()
        .bifrost_catalog()
        .ok_or("missing catalog")?
        .create_table(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, &table),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await?;
    cluster.refresh_oracle_snapshots().await?;
    let base = ingress.base_url().ok_or("missing HTTP URL")?.to_owned();
    await_server_ready(&base).await?;
    let api_key = ingress
        .bootstrap_service_in_tenant(tenant, "silent-peer-caller", &["admin"])
        .await?
        .api_key()
        .ok_or("machine bootstrap returned no key")?
        .clone();
    let edge = EdgeTimeoutClient {
        http: reqwest::Client::new(),
        base,
        token: ingress.exchange_api_key(&api_key).await?,
    };
    let peers = cluster
        .servers()
        .filter(|server| server.state().bifrost.oracle().is_some())
        .map(|server| {
            server
                .state()
                .bifrost
                .silent_forward_peer_for_test()
                .ok_or("an Oracle composed no forwarder")
        })
        .collect::<Result<Vec<_>, _>>()?;
    for peer in &peers {
        peer.set_armed(true);
    }

    let mut body = request(&format!("SELECT value FROM vala.bifrost.{table}"));
    body.deadline_ms = Some(2_000);
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        edge.post("/v1/query").json(&body).send(),
    )
    .await
    .map_err(|_| "a silent remote peer held the HTTP query past its deadline")??;
    assert_eq!(problem_code(response).await?, "WYRD_VALA_504_QUERY_TIMEOUT");
    let selected = peers
        .iter()
        .find(|peer| peer.counts().0 > 0)
        .ok_or("no Oracle received the forwarded envelope")?;
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        selected.wait_abandoned(1),
    )
    .await
    .map_err(|_| "the ingress left the silent delivery parked")??;
    assert_eq!(
        peers.iter().map(|peer| peer.counts().0).sum::<usize>(),
        1,
        "one selected envelope and no successor delivery"
    );

    for peer in &peers {
        peer.set_armed(false);
    }
    let body = request(&format!("SELECT value FROM vala.bifrost.{table}"));
    let response = edge.post("/v1/query").json(&body).send().await?;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let mut decoder = wyrd_tonic::frame_codec::FrameDecoder::new(16 * 1024 * 1024);
    let terminal = decoder
        .push::<proto::QueryStreamFrame>(&response.bytes().await?)?
        .into_iter()
        .find_map(|frame| match frame.frame {
            Some(proto::query_stream_frame::Frame::Terminal(terminal)) => Some(terminal.outcome),
            _ => None,
        });
    assert_eq!(
        terminal,
        Some(proto::QueryTerminalOutcome::Success as i32),
        "the released ingress still forwards successfully"
    );
    cluster.shutdown().await?;
    Ok(())
}

/// Authenticated raw HTTP caller for the staged edge-timeout journey.
struct EdgeTimeoutClient {
    /// Plain client without its own total timeout.
    http: reqwest::Client,
    /// Bound server origin.
    base: String,
    /// Exchanged bearer token for the admin service.
    token: String,
}

impl EdgeTimeoutClient {
    /// Arms a request-scoped preparation pause and starts that query.
    ///
    /// Returns once Oracle has reached the paused boundary, so the edge handoff
    /// has already happened and the caller controls the remaining wait.
    ///
    /// # Errors
    /// Returns pause binding errors, or a failure when the query settles or does
    /// not reach the pause within the bounded window.
    async fn paused_query(
        &self,
        oracle: &vala_bifrost_redux::oracle::Oracle,
        sql: &str,
        deadline_ms: i64,
    ) -> Result<
        (
            std::sync::Arc<vala_bifrost_redux::oracle::OraclePreparationPause>,
            tokio::task::JoinHandle<reqwest::Result<reqwest::Response>>,
        ),
        ServerJourneyError,
    > {
        let request_id = RequestId::now_v7();
        let pause = std::sync::Arc::new(vala_bifrost_redux::oracle::OraclePreparationPause::new(
            request_id.clone(),
        ));
        oracle.bind_preparation_pause_for_test(Some(std::sync::Arc::clone(&pause)))?;
        let mut body = request(sql);
        body.deadline_ms = Some(deadline_ms);
        let response = tokio::spawn(
            self.post("/v1/query")
                .header("wyrd-request-id", request_id.to_string())
                .json(&body)
                .send(),
        );
        let reached = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while pause.observed_deadline_ms().is_none() {
            if response.is_finished() || std::time::Instant::now() > reached {
                pause.release();
                return Err("query did not reach the preparation pause".into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        Ok((pause, response))
    }

    /// Sends a body that stalls longer than the edge limit before completing.
    ///
    /// # Errors
    /// Returns transport errors other than the server's timeout response.
    async fn stalled_post(
        &self,
        path: &str,
        body: Vec<u8>,
    ) -> Result<reqwest::Response, ServerJourneyError> {
        let (head, tail) = body.split_at(1);
        let chunks = [
            bytes::Bytes::copy_from_slice(head),
            bytes::Bytes::copy_from_slice(tail),
        ];
        let stream = futures_util::stream::iter(chunks.into_iter().enumerate()).then(
            |(index, chunk)| async move {
                if index == 1 {
                    tokio::time::sleep(EDGE_LIMIT * 3).await;
                }
                Ok::<_, std::io::Error>(chunk)
            },
        );
        Ok(self
            .post(path)
            .header("content-type", "application/json")
            .header("idempotency-key", uuid::Uuid::now_v7().to_string())
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await?)
    }

    /// Starts one authenticated POST to `path`.
    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .post(format!("{}{path}", self.base))
            .header("x-wyrd-access-token", format!("Bearer {}", self.token))
    }
}

/// Reads the stable problem code from an error response.
///
/// # Errors
/// Returns body or JSON decoding errors, or a failure when `code` is absent.
async fn problem_code(response: reqwest::Response) -> Result<String, ServerJourneyError> {
    let problem: serde_json::Value = response.json().await?;
    Ok(problem["code"]
        .as_str()
        .ok_or_else(|| format!("problem without code: {problem}"))?
        .to_owned())
}
