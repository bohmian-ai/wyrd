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
        "SELECT detail FROM vala.audit_outbox \
         WHERE operation = 'bifrost.query.read_decision' AND request_id = $1 \
         ORDER BY seq DESC LIMIT 1",
    )
    .bind(request_id)
    .fetch_one(&mut **conn.transaction())
    .await?;
    conn.commit().await?;
    Ok(serde_json::from_str(&detail)?)
}

/// Bounded budget for the read-audit relay to drain before a durable count.
const AUDIT_RELAY_CONVERGENCE_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

/// Waits until the Oracle read-audit WAL residual reaches zero.
///
/// A query's read decision is durable at its local WAL fsync and reaches
/// `vala.audit_outbox` through the production relay, so a count taken the
/// instant a query settles can precede the relay. This polls the production
/// residual counter — not a sleep, and not a retry over the assertion itself —
/// so the asserted row counts observe a fully relayed server.
///
/// A server that hosts no Oracle role owns no read-audit WAL, so there is
/// nothing to converge and the wait returns immediately.
///
/// # Errors
///
/// Returns a convergence failure naming the residual and oldest record age
/// when the relay does not drain within [`AUDIT_RELAY_CONVERGENCE_BUDGET`].
async fn await_audit_relay_convergence(server: &WyrdTestServer) -> Result<(), ServerJourneyError> {
    let deadline = std::time::Instant::now() + AUDIT_RELAY_CONVERGENCE_BUDGET;
    loop {
        let Ok(inspection) = server.oracle_runtime_inspection() else {
            return Ok(());
        };
        if inspection.audit_wal_records == 0 {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "read-audit relay did not converge: {} WAL records pending (oldest {:?}) after {AUDIT_RELAY_CONVERGENCE_BUDGET:?}",
                inspection.audit_wal_records, inspection.audit_oldest_age,
            )
            .into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Counts audit rows written under one operation name.
///
/// Waits for read-audit relay convergence first so the count reflects every
/// accepted read decision rather than the relay's current progress.
///
/// # Errors
///
/// Returns the relay convergence, tenant-connection, or SQL error.
async fn audit_rows(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    operation: &str,
) -> Result<i64, ServerJourneyError> {
    await_audit_relay_convergence(server).await?;
    let mut conn = server.tenant_conn_for(tenant).await?;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox WHERE operation LIKE $1")
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
            api_key: Some(api_key.clone()),
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
            api_key: Some(api_key),
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
