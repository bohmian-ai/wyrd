//! A second replica joins a running peer and serves remote work over mTLS.

use std::time::{Duration, Instant};

use wyrd_server::config::BifrostTarget;
use wyrd_spec::vala::api::BifrostQueryRequest;

use super::support::PeerJourneyError;
use crate::peer_cluster::{MembershipEntry, PeerCluster};
use crate::support::public_client;

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
/// The joiner selects only the Oracle role, so the journey proves a read-tier
/// pod with no Scribe of its own executes against the running topology.
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

/// Drives the join, remote work, and departure against live pods.
///
/// # Errors
///
/// Returns the first claim that broke.
async fn prove_peer_join_and_remote_query() -> Result<(), PeerJourneyError> {
    let mut cluster = PeerCluster::start_with_joiner(&[
        BifrostTarget::All,
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
    ])
    .await?;
    let members = [
        cluster.node_id(FIRST).as_uuid(),
        cluster.node_id(SECOND).as_uuid(),
    ];
    let joined_node_id = cluster.node_id(JOINED).as_uuid();

    // A replica that fails before its private listener serves must never be
    // routable: its reserved roles stay out of every ready snapshot.
    {
        let _held = std::net::TcpListener::bind(cluster.peer_addr(JOINED)?)?;
        if cluster.join(JOINED).await.is_ok() {
            return Err("a replica whose private peer socket is held started serving".into());
        }
    }
    let snapshot = cluster.membership(FIRST).await?;
    if let Some(entry) = snapshot
        .iter()
        .find(|entry| !members.contains(&entry.node_id))
    {
        return Err(
            format!("a replica that never served its peer listener is ready: {entry:?}").into(),
        );
    }

    // The first pod's fenced incarnations: a restart would re-register every
    // role under an advanced fence.
    let first_node_id = cluster.node_id(FIRST).as_uuid();
    let first_fences = |membership: &[MembershipEntry]| -> Vec<(String, u64)> {
        membership
            .iter()
            .filter(|entry| entry.node_id == first_node_id)
            .map(|entry| (entry.role.clone(), entry.fencing_token))
            .collect()
    };
    let fences_before = first_fences(&snapshot);
    cluster.join(JOINED).await?;
    let joined_addr = format!("https://{}", cluster.peer_addr(JOINED)?);
    if joined_addr == cluster.advertise_addr(FIRST).await? {
        return Err("the joined pod advertises the first pod's address".into());
    }

    await_membership(&cluster, "the join", |membership| {
        membership.iter().any(|entry| {
            entry.node_id == joined_node_id
                && entry.role == "oracle"
                && entry.ready
                && entry.address == joined_addr
        })
    })
    .await?;
    if first_fences(&cluster.membership(FIRST).await?) != fences_before {
        return Err("the first pod was restarted to discover the joined pod".into());
    }

    let table = format!("join_{}", uuid::Uuid::now_v7().simple());
    cluster.register_table(FIRST, &table).await?;
    let batch_rows = ROWS / BATCHES;
    for batch in 0..BATCHES {
        cluster
            .ingest_rows(FIRST, &table, batch * batch_rows, batch_rows, GROUPS)
            .await?;
    }
    for index in [FIRST, SECOND, JOINED] {
        cluster.refresh_snapshot(index).await?;
    }
    let sql = format!("SELECT id FROM vala.bifrost.{table} WHERE id < {ROWS}");
    let expected = usize::try_from(ROWS)?;

    // The coordinator is never a participant in its own worker set, so every
    // stage the joiner plans lands on the other two Oracles: a graph lease on
    // the first pod is the worker-ID-bearing remote Oracle RPC.
    let grouped = format!(
        "SELECT filter_key, COUNT(*) AS matched FROM vala.bifrost.{table} GROUP BY filter_key"
    );
    let leases_before = cluster.graph_leases(FIRST)?.0;
    let evidence = cluster
        .execute_analytical_baseline(JOINED, &grouped)
        .await?;
    if evidence.rows != usize::try_from(GROUPS)? {
        return Err(format!(
            "the remote graph returned {} groups, not {GROUPS}",
            evidence.rows
        )
        .into());
    }
    if cluster.graph_leases(FIRST)?.0 <= leases_before {
        return Err("the joined pod ran the graph without leasing the first pod".into());
    }

    // A fused public read discovers live Scribe tail on every Scribe member;
    // the joiner has none, so its read must cross to the first pod over mTLS.
    // The body-poll counter is process-wide; the only Scribe peer answering
    // tail requests is the first pod, so an advance is its tail being read.
    let api_key = cluster.provision_public_api_key("peer-join").await?;
    let polls_before = cluster.peer_body_polls();
    let rows = fused_row_count(cluster.server(JOINED)?, &api_key, &sql).await?;
    if rows != expected {
        return Err(format!("the fused read returned {rows} rows, not {expected}").into());
    }
    if cluster.peer_body_polls() == polls_before {
        return Err("the joined pod's fused read never reached the first pod's tail".into());
    }

    // An abrupt departure: a query issued before membership notices must fail
    // or return everything, never a partial result.
    cluster.kill(JOINED).await?;
    match fused_row_count(cluster.server(FIRST)?, &api_key, &sql).await {
        Ok(rows) if rows != expected => {
            return Err(format!("a broken remote call returned {rows} of {expected} rows").into());
        }
        Ok(_) | Err(_) => {}
    }
    await_membership(&cluster, "the departure", |membership| {
        membership
            .iter()
            .all(|entry| entry.node_id != joined_node_id)
    })
    .await?;

    cluster.shutdown().await?;
    Ok(())
}

/// Polls the first pod's live membership cut until `observed` holds.
///
/// # Errors
///
/// Returns a failure naming `change` and the last cut seen when the deadline
/// passes, or the membership refresh failure unchanged.
async fn await_membership(
    cluster: &PeerCluster,
    change: &str,
    observed: impl Fn(&[MembershipEntry]) -> bool,
) -> Result<(), PeerJourneyError> {
    let deadline = Instant::now() + MEMBERSHIP_DEADLINE;
    loop {
        let membership = cluster.membership(FIRST).await?;
        if observed(&membership) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "the first pod never observed {change}; last membership {membership:?}"
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
    node: &wyrd_testing::WyrdTestServer,
    api_key: &secrecy::SecretString,
    sql: &str,
) -> Result<usize, PeerJourneyError> {
    let client = public_client(node, api_key)?;
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
