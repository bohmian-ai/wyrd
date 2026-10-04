//! Shutdown ownership: what a pod publishes before it stops and what it keeps.

use std::time::Duration;

use uuid::Uuid;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::geometry::{
    DEFAULT_GENERATION_ROTATION_BYTES, DEFAULT_SHARD_COUNT, DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
    DEFAULT_WAL_SEGMENT_BYTES, ScribeGeometry,
};
use vala_bifrost_redux::scribe::persistence::PersistenceFaults;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use super::support::{
    append_values, await_persistence_drained, published_rows, register_table, sorted_values,
    tenant_client, unique_table,
};

/// Retention the idle-publication case configures.
///
/// The value an operator sets with `WYRD_MAX_FILE_RETENTION_TIME`; it bounds
/// both how long a generation stays writable and how long a staged member
/// waits for its object target, so a short value keeps the case fast without
/// touching any size threshold.
const RETENTION: Duration = Duration::from_secs(2);

/// Longest the idle-publication case waits for the pod to publish on its own.
///
/// Two retention periods (seal, then staging dwell) plus the one-second
/// lifecycle tick, with headroom for the publication itself.
const IDLE_PUBLICATION_DEADLINE: Duration = Duration::from_secs(30);

/// Rows a table stops receiving writes with still publish on the pod's own clock.
///
/// Every other production control stays at its default, so the 512 MiB object
/// target is never reached: the only way these rows publish is the pod's
/// lifecycle tick expiring the generation's age and then the staged member's
/// dwell. No flush, snapshot refresh, drain, or further write is issued; the
/// case only waits and observes, exactly as an application that wrote once and
/// went quiet would. The audit publisher is kept off because it is the one
/// other writer in the pod, and its generations would publish the table
/// without the tick.
///
/// # Panics
///
/// Panics when the server cannot start, when an append or read fails, or when
/// the acknowledged rows are not published exactly once within
/// [`IDLE_PUBLICATION_DEADLINE`].
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_publishes_idle_rows_on_its_own_clock() {
    let geometry = ScribeGeometry::new(
        DEFAULT_SHARD_COUNT,
        DEFAULT_WAL_SEGMENT_BYTES,
        DEFAULT_GENERATION_ROTATION_BYTES,
        RETENTION,
        None,
        None,
        DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
    )
    .expect("default geometry with a short retention is valid");
    // The audit publisher writes its own table into the same Scribe, and each
    // of its durable generations re-checks every key's dwell. With it running
    // this case would pass on audit traffic alone; without it, the lifecycle
    // tick is the only thing that can publish the idle table.
    let server = WyrdTestServer::builder()
        .with_scribe_geometry_for_test(geometry)
        .without_audit_publication_for_test()
        .start_bound()
        .await
        .expect("the Scribe production harness starts");
    let tenant = server.data_tenant_id();
    let name = unique_table("idle_publication");
    let table = register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
    let client = tenant_client(&server, tenant).await;
    let expected: Vec<i64> = (0..64).collect();
    append_values(&client, &table, Uuid::now_v7(), &expected)
        .await
        .expect("the append is acknowledged");

    let deadline = tokio::time::Instant::now() + IDLE_PUBLICATION_DEADLINE;
    while published_rows(&server, tenant, &name).await < expected.len() as u64 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "rows of an idle table were not published within {IDLE_PUBLICATION_DEADLINE:?}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        expected.len() as u64,
        "idle publication must publish every acknowledged row exactly once"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "publication may not change which rows are readable"
    );

    server.shutdown().await.expect("the server drains cleanly");
}

/// Claims the pod may hold at once: the harness runs the default Scribe
/// configuration, whose four WAL IO workers size the claim budget.
const CLAIM_BUDGET: usize = 4;

/// Tables the concurrent-publication case makes due together: twice the
/// budget, so a publisher that honours the budget must queue half of them.
const CONCURRENT_TABLES: usize = 2 * CLAIM_BUDGET;

/// How long each claim's publication is held open in the concurrent case, so
/// claims that run together are observed overlapping.
const HELD_PUBLICATION: Duration = Duration::from_millis(1_500);

/// Due claims publish together, never more at once than the claim budget.
///
/// Every table here is written once and goes quiet, so each becomes one claim
/// that the pod's own lifecycle tick makes due when its dwell expires — the
/// production path a busy pod with many tables takes every few seconds. Each
/// publication is held open at the real object-write seam, so the fault
/// controls observe how many claims are in flight together: more than one
/// proves due claims do not wait for each other, and no more than the budget
/// proves the bound holds. Every acknowledged row must still publish exactly
/// once.
///
/// # Panics
///
/// Panics when the server cannot start, when an append or read fails, when
/// the due claims do not all publish within [`IDLE_PUBLICATION_DEADLINE`],
/// when no two claims overlap, when more than [`CLAIM_BUDGET`] run at once, or
/// when a row is lost or duplicated.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_publishes_due_claims_concurrently_within_the_claim_budget() {
    let geometry = ScribeGeometry::new(
        DEFAULT_SHARD_COUNT,
        DEFAULT_WAL_SEGMENT_BYTES,
        DEFAULT_GENERATION_ROTATION_BYTES,
        RETENTION,
        None,
        None,
        DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
    )
    .expect("default geometry with a short retention is valid");
    let faults = PersistenceFaults::default();
    faults.set_object_write_delay_for_test(HELD_PUBLICATION);
    let server = WyrdTestServer::builder()
        .with_scribe_geometry_for_test(geometry)
        .with_scribe_persistence_faults_for_test(faults.clone())
        .without_audit_publication_for_test()
        .start_bound()
        .await
        .expect("the Scribe production harness starts");
    let tenant = server.data_tenant_id();
    let client = tenant_client(&server, tenant).await;
    let expected: Vec<i64> = (0..16).collect();
    let mut tables = Vec::with_capacity(CONCURRENT_TABLES);
    for index in 0..CONCURRENT_TABLES {
        let name = unique_table(&format!("concurrent_due_{index}"));
        let table = register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
        append_values(&client, &table, Uuid::now_v7(), &expected)
            .await
            .expect("the append is acknowledged");
        tables.push((name, table));
    }

    let deadline = tokio::time::Instant::now() + IDLE_PUBLICATION_DEADLINE;
    for (name, _) in &tables {
        while published_rows(&server, tenant, name).await < expected.len() as u64 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "due claims were not all published within {IDLE_PUBLICATION_DEADLINE:?}"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    let overlap = faults.max_concurrent_object_writes_for_test();
    assert!(
        overlap > 1,
        "due claims must publish together, but at most {overlap} was in flight at once"
    );
    assert!(
        overlap <= CLAIM_BUDGET,
        "{overlap} claims were in flight at once; the claim budget is {CLAIM_BUDGET}"
    );
    for (name, table) in &tables {
        assert_eq!(
            published_rows(&server, tenant, name).await,
            expected.len() as u64,
            "concurrent publication must publish every acknowledged row exactly once"
        );
        assert_eq!(
            sorted_values(&client, table).await,
            expected,
            "publication may not change which rows are readable"
        );
    }

    server.shutdown().await.expect("the server drains cleanly");
}

/// Two flushes that meet over a full claim budget publish every claim once.
///
/// The first flush takes every claim slot and its publications are held at
/// the real object-write seam. The second flush then finds each slot held by
/// a claim another publication is driving: it must neither drive those claims
/// a second time nor fail because no slot is free, but wait for a slot and
/// publish the rest. Both flushes succeed, no more claims than the budget are
/// ever in flight, and every acknowledged row publishes exactly once.
///
/// # Panics
///
/// Panics when the server cannot start, when an append, flush or read fails,
/// when more than [`CLAIM_BUDGET`] claim publications run at once, or when a
/// row is lost or duplicated.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn concurrent_flushes_share_the_claim_budget_and_publish_each_claim_once() {
    let faults = PersistenceFaults::default();
    faults.hold_object_writes_for_test();
    let server = WyrdTestServer::builder()
        .with_scribe_persistence_faults_for_test(faults.clone())
        .without_audit_publication_for_test()
        .start_bound()
        .await
        .expect("the Scribe production harness starts");
    let tenant = server.data_tenant_id();
    let client = tenant_client(&server, tenant).await;
    let expected: Vec<i64> = (0..16).collect();
    let mut tables = Vec::with_capacity(CONCURRENT_TABLES);
    for index in 0..CONCURRENT_TABLES {
        let name = unique_table(&format!("contended_flush_{index}"));
        let table = register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
        append_values(&client, &table, Uuid::now_v7(), &expected)
            .await
            .expect("the append is acknowledged");
        tables.push((name, table));
    }

    let contending = async {
        faults
            .wait_for_held_object_writes_for_test(CLAIM_BUDGET)
            .await;
        let flush = server.flush_bifrost();
        tokio::pin!(flush);
        let settled_early = tokio::select! {
            result = &mut flush => Some(result),
            () = faults.wait_for_claim_contention_for_test(CLAIM_BUDGET) => None,
        };
        faults.release_object_writes_for_test();
        match settled_early {
            Some(result) => result,
            None => flush.await,
        }
    };
    let (first, second) = tokio::join!(server.flush_bifrost(), contending);
    first.expect("the flush holding every claim slot publishes its claims");
    second.expect("a flush that meets a full claim budget waits for a slot");
    let overlap = faults.max_concurrent_object_writes_for_test();
    assert!(
        overlap <= CLAIM_BUDGET,
        "{overlap} claim publications ran at once; the claim budget is {CLAIM_BUDGET}"
    );
    for (name, table) in &tables {
        assert_eq!(
            published_rows(&server, tenant, name).await,
            expected.len() as u64,
            "every acknowledged row publishes exactly once"
        );
        assert_eq!(sorted_values(&client, table).await, expected);
    }

    server.shutdown().await.expect("the server drains cleanly");
}

/// A due claim whose publication fails is retried by the pod's own tick.
///
/// The first claim object write is refused, the way an object-store error
/// refuses it. The claim keeps its slot and its members, and nothing but the
/// lifecycle tick runs: no flush, drain, or further write. The tick must pick
/// the refused claim up again and publish it, or a pod whose every slot has
/// failed once would never publish again.
///
/// # Panics
///
/// Panics when the server cannot start, when an append or read fails, when
/// the claim does not publish within [`IDLE_PUBLICATION_DEADLINE`], or when a
/// row is lost or duplicated.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_tick_retries_a_failed_due_claim() {
    let geometry = ScribeGeometry::new(
        DEFAULT_SHARD_COUNT,
        DEFAULT_WAL_SEGMENT_BYTES,
        DEFAULT_GENERATION_ROTATION_BYTES,
        RETENTION,
        None,
        None,
        DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
    )
    .expect("default geometry with a short retention is valid");
    let faults = PersistenceFaults::default();
    faults.fail_next_object_write();
    let server = WyrdTestServer::builder()
        .with_scribe_geometry_for_test(geometry)
        .with_scribe_persistence_faults_for_test(faults.clone())
        .without_audit_publication_for_test()
        .start_bound()
        .await
        .expect("the Scribe production harness starts");
    let tenant = server.data_tenant_id();
    let name = unique_table("tick_retry");
    let table = register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
    let client = tenant_client(&server, tenant).await;
    let expected: Vec<i64> = (0..32).collect();
    append_values(&client, &table, Uuid::now_v7(), &expected)
        .await
        .expect("the append is acknowledged");

    tokio::time::timeout(
        IDLE_PUBLICATION_DEADLINE,
        faults.wait_for_published_claims_for_test(1),
    )
    .await
    .expect("the tick retries the refused claim and publishes it");
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        expected.len() as u64,
        "the retried claim publishes every acknowledged row exactly once"
    );
    assert_eq!(sorted_values(&client, &table).await, expected);

    server.shutdown().await.expect("the server drains cleanly");
}

/// A claim that fails after its commit landed is finished by the pod's own
/// tick, frees its claim slot, and lets publication continue.
///
/// The claim's fenced commit lands and every member records it as
/// `Published`, then retirement fails before any member moves to cleanup.
/// Nothing but the lifecycle tick runs: no flush, drain, or restart. The tick
/// must finish the claim from its durable state without publishing its rows
/// again, return its slot, and publish a second table written afterwards.
///
/// # Panics
///
/// Panics when the server cannot start, when an append or read fails, when
/// the injected failure is never reached, when either claim does not settle
/// within [`IDLE_PUBLICATION_DEADLINE`], when a claim stays outstanding or a
/// staged member survives, or when a row is lost or duplicated.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_tick_finishes_a_claim_that_failed_after_its_commit() {
    let geometry = ScribeGeometry::new(
        DEFAULT_SHARD_COUNT,
        DEFAULT_WAL_SEGMENT_BYTES,
        DEFAULT_GENERATION_ROTATION_BYTES,
        RETENTION,
        None,
        None,
        DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
    )
    .expect("default geometry with a short retention is valid");
    let faults = PersistenceFaults::default();
    faults.fail_next_claim_retirement();
    let server = WyrdTestServer::builder()
        .with_scribe_geometry_for_test(geometry)
        .with_scribe_persistence_faults_for_test(faults.clone())
        .without_audit_publication_for_test()
        .start_bound()
        .await
        .expect("the Scribe production harness starts");
    let tenant = server.data_tenant_id();
    let client = tenant_client(&server, tenant).await;
    let expected: Vec<i64> = (0..32).collect();

    let first_name = unique_table("post_commit_failure");
    let first = register_table(&server, tenant, BifrostNamespace::Datasets, &first_name).await;
    append_values(&client, &first, Uuid::now_v7(), &expected)
        .await
        .expect("the first append is acknowledged");
    tokio::time::timeout(
        IDLE_PUBLICATION_DEADLINE,
        faults.wait_for_published_claims_for_test(1),
    )
    .await
    .expect("the tick finishes the claim whose retirement failed after its commit");
    assert!(
        !faults.claim_retirement_failure_armed_for_test(),
        "the claim failed after its commit before the tick finished it"
    );
    assert_eq!(
        published_rows(&server, tenant, &first_name).await,
        expected.len() as u64,
        "the finished claim's rows are published exactly once"
    );
    assert_eq!(sorted_values(&client, &first).await, expected);
    let settled = server
        .scribe_staging_backlog_for_test()
        .expect("the staging owner is inspectable");
    assert_eq!(
        (settled.outstanding_claims, settled.live_members),
        (0, 0),
        "the finished claim returned its slot and retired its members"
    );

    let second_name = unique_table("after_post_commit_failure");
    let second = register_table(&server, tenant, BifrostNamespace::Datasets, &second_name).await;
    append_values(&client, &second, Uuid::now_v7(), &expected)
        .await
        .expect("the second append is acknowledged");
    tokio::time::timeout(
        IDLE_PUBLICATION_DEADLINE,
        faults.wait_for_published_claims_for_test(2),
    )
    .await
    .expect("publication continues after the finished claim");
    assert_eq!(
        published_rows(&server, tenant, &second_name).await,
        expected.len() as u64,
        "the next claim publishes every acknowledged row exactly once"
    );
    assert_eq!(sorted_values(&client, &second).await, expected);

    server.shutdown().await.expect("the server drains cleanly");
}

/// A stopping pod retains its acknowledged rows for the next process.
///
/// Acknowledged rows outlive the process that accepted them. A pod told to
/// stop drains admitted work and leaves its staged members durable; the
/// restarted process restores them and the publish tick publishes them later.
/// A pod that is killed cannot drain, so the WAL it already fsynced stays
/// authoritative and its replacement replays it. This owner drives both against one table and holds
/// the same invariant across them — a strict public read returns exactly the
/// acknowledged rows, once each — because a pod that loses rows on the way down
/// and a pod that resurrects them on the way up are the same defect seen from
/// opposite ends.
///
/// The abrupt half restarts on a new address deliberately: a replacement pod is
/// not the same endpoint, and replay must be driven by the retained WAL and
/// staging roots rather than by anything the old process still held.
///
/// # Panics
///
/// Panics when the cluster cannot start, stop, or restart a pod, when a public
/// append or read fails, or when a shutdown loses or duplicates an acknowledged
/// row or publishes part of a claim.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_shutdown_drains_or_preserves_replay() {
    let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("the one-pod mixed cluster starts");
    let tenant = cluster.data_tenant_id();
    let name = unique_table("shutdown_replay");
    let node = {
        let server = cluster.server(0).expect("the mixed pod is running");
        register_table(server, tenant, BifrostNamespace::Datasets, &name).await;
        server.node_id()
    };
    let table = format!("{}.{name}", BifrostNamespace::Datasets.as_str());

    let drained: Vec<i64> = (0..24).collect();
    {
        let server = cluster.server(0).expect("the mixed pod is running");
        let client = tenant_client(server, tenant).await;
        append_values(&client, &table, Uuid::now_v7(), &drained)
            .await
            .expect("the first append is acknowledged");
    }

    // A pod told to stop keeps its staged rows durable for the restart.
    cluster
        .stop_node(node)
        .await
        .expect("the pod shuts down gracefully");
    cluster
        .restart_node(node)
        .await
        .expect("the pod restarts on its retained roots");
    let preserved: Vec<i64> = (100..124).collect();
    {
        let server = cluster.server(0).expect("the restarted pod is running");
        // After restart the staged rows are either still staged or already
        // published by the tick, depending on timing. What may never happen is
        // publishing part of a claim, so that is what this holds.
        let published = published_rows(server, tenant, &name).await;
        assert!(
            published == 0 || published == drained.len() as u64,
            "a shutdown publishes all of a claim's rows or none of them; published {published} of {}",
            drained.len()
        );
        let client = tenant_client(server, tenant).await;
        assert_eq!(
            sorted_values(&client, &table).await,
            drained,
            "a drained pod's rows must read back exactly once after restart"
        );
        append_values(&client, &table, Uuid::now_v7(), &preserved)
            .await
            .expect("the second append is acknowledged");
    }

    // A pod that is killed owes those rows to its WAL and its staging volume.
    // Both are represented deliberately: `staged` is frozen into a durable
    // member and left unpublished, while `writable` is only acknowledged, so
    // the replacement pod has to restore one durable member from the volume and
    // replay the other rows from the WAL before either can publish.
    let staged: Vec<i64> = (200..224).collect();
    let writable: Vec<i64> = (300..324).collect();
    {
        let server = cluster.server(0).expect("the restarted pod is running");
        let client = tenant_client(server, tenant).await;
        append_values(&client, &table, Uuid::now_v7(), &staged)
            .await
            .expect("the staged append is acknowledged");
        let scribe = server.bifrost_scribe().expect("the pod owns a Scribe");
        scribe
            .flush_writable_for_test()
            .await
            .expect("every writable bucket freezes");
        await_persistence_drained(&scribe).await;
        append_values(&client, &table, Uuid::now_v7(), &writable)
            .await
            .expect("the writable append is acknowledged");
    }

    let roots = cluster
        .terminate_node_abruptly_for_test(node)
        .await
        .expect("the pod is terminated without a drain");
    cluster
        .restart_terminated_node_at_new_address(node, roots)
        .await
        .expect("a replacement pod restarts on the retained roots");

    let mut expected = drained.clone();
    expected.extend_from_slice(&preserved);
    expected.extend_from_slice(&staged);
    expected.extend_from_slice(&writable);
    expected.sort_unstable();
    let server = cluster.server(0).expect("the replacement pod is running");
    let client = tenant_client(server, tenant).await;
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "an abrupt termination must preserve every acknowledged row for replay"
    );

    // What the replacement pod restored is ordinary staged work, so it
    // publishes once, under a fence the rows were never written by.
    server
        .flush_bifrost()
        .await
        .expect("the restored rows publish");
    assert_eq!(
        published_rows(server, tenant, &name).await,
        expected.len() as u64,
        "rows a replacement pod restored must publish exactly once"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "publication after replay may not change which rows are readable"
    );

    cluster
        .shutdown()
        .await
        .expect("the cluster drains cleanly");
}
