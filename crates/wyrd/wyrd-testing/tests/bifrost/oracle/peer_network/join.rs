//! A second replica joins a running peer and serves remote work over mTLS.

use std::time::{Duration, Instant};

use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_testing::bifrost::process_cluster::{
    BifrostProcessCluster, NodeReport, ProcessNode, ProcessNodeTarget,
};

use super::support::{PeerJourneyError, polls_at};

/// Path of the compiled child every simulated pod runs.
const NODE_BINARY: &str = env!("CARGO_BIN_EXE_bifrost_peer_test_node");

/// `all` pod running before the join; it owns the only Scribe and Forge roles.
const FIRST: usize = 0;

/// `oracle` pod running beside the first before the join.
///
/// A coordinator plans a shuffle with one task per remote participant, and a
/// single-task boundary is elided back onto the leader. The joiner therefore
/// needs two remote Oracles for its grouped aggregation to leave the process.
const SECOND: usize = 1;

/// `oracle` pod launched into the running topology.
///
/// A second `all` replica would be a standby Forge coordinator, which is
/// unready by design, so the joiner selects exactly the role that scales out.
const JOINED: usize = 2;

/// Rows the first pod writes and publishes.
const ROWS: i64 = 64;

/// Ingest batches the rows arrive in, one hot object each.
const BATCHES: i64 = 4;

/// Distinct `filter_key` groups the fixture generator cycles through.
const GROUPS: i64 = 8;

/// Longest a membership change may take to become observable.
///
/// Membership is heartbeat-driven: a join appears within one heartbeat, and a
/// stopped member leaves once its last heartbeat ages past the fifteen-second
/// liveness cutoff. The deadline turns a member that never appears or never
/// leaves into a diagnosable failure; elapsed time is never itself evidence.
const MEMBERSHIP_DEADLINE: Duration = Duration::from_secs(45);

/// A running `all` and `oracle` pair discovers a third `oracle` replica that
/// joins with its own runtime-supplied address and the shared cluster bundle.
/// Queries the joiner coordinates lease the first pod's Oracle and read its
/// Scribe tail over mTLS, and the first pod forgets the joiner once it stops,
/// without a restart and without a partial result in between. A first attempt
/// whose private peer socket is held fails before serving and never appears in
/// the first pod's ready membership; the retry after release joins normally.
///
/// # Panics
///
/// Panics when any step of the join journey fails, naming the step.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn peer_join_and_remote_query() {
    prove_peer_join_and_remote_query()
        .await
        .expect("peer join journey");
}

/// Drives the join, remote work, and departure against real processes.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_peer_join_and_remote_query() -> Result<(), PeerJourneyError> {
    let mut cluster = BifrostProcessCluster::start(
        NODE_BINARY,
        &[ProcessNodeTarget::All, ProcessNodeTarget::Oracle],
    )
    .await?;
    let first_pid = cluster.nodes()[FIRST].pid();
    let members: Vec<uuid::Uuid> = cluster
        .nodes()
        .iter()
        .map(|node| node.ready_report().node_id)
        .collect();

    // A replica that fails before its private listener serves must never be
    // routable: its reserved roles stay out of every ready snapshot.
    cluster.join_with_peer_socket_held(ProcessNodeTarget::Oracle)?;
    let snapshot = cluster.nodes_mut()[FIRST].inspect()?.membership;
    if let Some(entry) = snapshot
        .iter()
        .find(|entry| !members.contains(&entry.node_id))
    {
        return Err(
            format!("a replica that never served its peer listener is ready: {entry:?}").into(),
        );
    }

    let joined = cluster.join(ProcessNodeTarget::Oracle)?.clone();
    if joined.advertise_addr == cluster.nodes()[FIRST].ready_report().advertise_addr {
        return Err("the joined pod advertises the first pod's address".into());
    }

    await_membership(&mut cluster, "the join", |report| {
        report.membership.iter().any(|entry| {
            entry.node_id == joined.node_id
                && entry.role == "oracle"
                && entry.ready
                && entry.address == joined.advertise_addr
        })
    })
    .await?;
    if cluster.nodes()[FIRST].pid() != first_pid {
        return Err("the first pod was restarted to discover the joined pod".into());
    }

    let table = format!("join_{}", uuid::Uuid::now_v7().simple());
    cluster.nodes_mut()[FIRST].register_table(&table)?;
    let batch_rows = ROWS / BATCHES;
    for batch in 0..BATCHES {
        cluster.nodes_mut()[FIRST].ingest_rows(&table, batch * batch_rows, batch_rows, GROUPS)?;
    }
    for index in [FIRST, SECOND, JOINED] {
        cluster.nodes_mut()[index].refresh_snapshot()?;
    }
    let sql = format!("SELECT id FROM vala.bifrost.{table} WHERE id < {ROWS}");
    let expected = usize::try_from(ROWS)?;

    // The coordinator is never a participant in its own worker set, so every
    // stage the joiner plans lands on the other two Oracles: a graph lease on
    // the first pod is the worker-ID-bearing remote Oracle RPC.
    let grouped = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} GROUP BY filter_key"
    );
    let leases_before = cluster.nodes_mut()[FIRST].graph_leases()?.0;
    let evidence = cluster.nodes_mut()[JOINED].execute_analytical_baseline(&grouped)?;
    if evidence.rows != usize::try_from(GROUPS)? {
        return Err(format!(
            "the remote graph returned {} groups, not {GROUPS}",
            evidence.rows
        )
        .into());
    }
    if cluster.nodes_mut()[FIRST].graph_leases()?.0 <= leases_before {
        return Err("the joined pod ran the graph without leasing the first pod".into());
    }

    // A fused public read discovers live Scribe tail on every Scribe member;
    // the joiner has none, so its read must cross to the first pod over mTLS.
    let api_key = cluster.provision_public_api_key("peer-join").await?;
    let polls_before = polls_at(&mut cluster, FIRST)?;
    let rows = fused_row_count(&cluster.nodes()[JOINED], &api_key, &sql).await?;
    if rows != expected {
        return Err(format!("the fused read returned {rows} rows, not {expected}").into());
    }
    if polls_at(&mut cluster, FIRST)? == polls_before {
        return Err("the joined pod's fused read never reached the first pod's tail".into());
    }

    // An abrupt departure: a query issued before membership notices must fail
    // or return everything, never a partial result.
    let mut departed = cluster.remove(JOINED)?;
    departed.kill()?;
    match fused_row_count(&cluster.nodes()[FIRST], &api_key, &sql).await {
        Ok(rows) if rows != expected => {
            return Err(format!("a broken remote call returned {rows} of {expected} rows").into());
        }
        Ok(_) | Err(_) => {}
    }
    await_membership(&mut cluster, "the departure", |report| {
        report
            .membership
            .iter()
            .all(|entry| entry.node_id != joined.node_id)
    })
    .await?;

    cluster.shutdown()?;
    Ok(())
}

/// Polls the first pod's live membership cut until `observed` holds.
///
/// # Errors
///
/// Returns a failure naming `change` and the last cut seen when the deadline
/// passes, or the control-protocol failure unchanged.
async fn await_membership(
    cluster: &mut BifrostProcessCluster,
    change: &str,
    observed: impl Fn(&NodeReport) -> bool,
) -> Result<(), PeerJourneyError> {
    let deadline = Instant::now() + MEMBERSHIP_DEADLINE;
    loop {
        let report = cluster.nodes_mut()[FIRST].inspect()?;
        if observed(&report) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "the first pod never observed {change}; last membership {:?}",
                report.membership
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Runs one strict fused public query on `node` and counts its rows.
///
/// # Errors
///
/// Returns a failure when the query is refused, a batch fails, or the stream
/// ends without a terminal frame.
async fn fused_row_count(
    node: &ProcessNode,
    api_key: &secrecy::SecretString,
    sql: &str,
) -> Result<usize, PeerJourneyError> {
    let client = WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: format!("http://{}", node.grpc_addr()),
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: format!("http://{}", node.http_addr()),
            ..HttpConfig::default()
        },
        credential: Some(api_key.clone()),
        ..ClientConfig::default()
    })?;
    let mut stream = wyrd_client::Bifrost::query_only(&client)
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
            deadline_ms: None,
        })
        .await?;
    let mut rows = 0;
    while let Some(batch) = stream.next_batch().await? {
        rows += batch.num_rows();
    }
    stream
        .terminal()
        .ok_or("the fused query produced no terminal frame")?;
    Ok(rows)
}
