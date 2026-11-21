//! Shutdown ownership: what a pod publishes before it stops and what it keeps,
//! and how a fenced execution state recovers across takeover and restart.

use std::time::Duration;

use uuid::Uuid;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::geometry::{
    DEFAULT_GENERATION_ROTATION_BYTES, DEFAULT_SHARD_COUNT, DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
    DEFAULT_WAL_SEGMENT_BYTES, ScribeGeometry,
};
use vala_bifrost_redux::scribe::persistence::PersistenceFaults;
use vala_verify::blocks::Checkpoints as _;
use wyrd_server::verification::state::{RunState, StateError, revision_fence};
use wyrd_spec::DataTenantId;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::Producer;
use wyrd_spec::verification::state::ExecutionState;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};
use wyrd_testing::verification::VerificationFixture;

use super::support::{
    INGEST_BUSY, append_values, await_persistence_drained, endpoint_client, published_rows,
    read_sql, register_table, sorted_values, tenant_client, tenant_key, unique_table,
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
/// went quiet would.
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

/// Claims the pod may hold at once: each journey pins
/// `scribe.persistence_cpu_threads`, which sizes the claim budget.
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
        .with_scribe_persistence_cpu_threads_for_test(CLAIM_BUDGET)
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
        .with_scribe_persistence_cpu_threads_for_test(CLAIM_BUDGET)
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
        .start_bound()
        .await
        .expect("the Scribe production harness starts");
    let tenant = server.data_tenant_id();
    let name = unique_table("tick_retry");
    faults.target_table_for_test(&name);
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
        .start_bound()
        .await
        .expect("the Scribe production harness starts");
    let tenant = server.data_tenant_id();
    let client = tenant_client(&server, tenant).await;
    let expected: Vec<i64> = (0..32).collect();

    let first_name = unique_table("post_commit_failure");
    faults.target_table_for_test(&first_name);
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
        .scribe_staging_backlog_for_test(Some(&first_name))
        .expect("the staging owner is inspectable");
    assert_eq!(
        (settled.outstanding_claims, settled.live_members),
        (0, 0),
        "the finished claim returned its slot and retired its members"
    );

    let second_name = unique_table("after_post_commit_failure");
    faults.target_table_for_test(&second_name);
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

/// A complete WAL attempt whose batch was never accepted restores no rows
/// after an abrupt restart, on an ordinary table as on a domain table.
///
/// One batch is accepted. While the tenant role cannot record batch
/// receipts, a second batch is written to the WAL and refused, so its
/// producer never sees an acknowledgement. After the pod is terminated and
/// replaced on its retained roots, only the accepted rows read back.
///
/// # Panics
///
/// Panics when the cluster cannot start, the accepted append fails, the
/// unrecorded append is acknowledged, or the never-accepted rows reappear.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn replay_restores_only_accepted_batches() {
    let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("the one-pod mixed cluster starts");
    let tenant = cluster.data_tenant_id();
    let name = unique_table("accepted_only_replay");
    let node = {
        let server = cluster.server(0).expect("the mixed pod is running");
        register_table(server, tenant, BifrostNamespace::Datasets, &name).await;
        server.node_id()
    };
    let table = format!("{}.{name}", BifrostNamespace::Datasets.as_str());
    let accepted: Vec<i64> = (0..24).collect();
    let pool = cluster
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool");
    {
        let server = cluster.server(0).expect("the mixed pod is running");
        let client = tenant_client(server, tenant).await;
        append_values(&client, &table, Uuid::now_v7(), &accepted)
            .await
            .expect("the first append is acknowledged");
        sqlx::query("REVOKE INSERT ON vala.scribe_batch_commits FROM wyrd_tenant")
            .execute(&pool)
            .await
            .expect("receipt recording is revoked");
        let refused = append_values(
            &client,
            &table,
            Uuid::now_v7(),
            &(100..124).collect::<Vec<_>>(),
        )
        .await;
        sqlx::query("GRANT INSERT ON vala.scribe_batch_commits TO wyrd_tenant")
            .execute(&pool)
            .await
            .expect("receipt recording is granted again");
        assert!(
            refused.is_err(),
            "an unrecorded batch is never acknowledged"
        );
    }

    let roots = cluster
        .terminate_node_abruptly_for_test(node)
        .await
        .expect("the pod is terminated without a drain");
    cluster
        .restart_terminated_node_at_new_address(node, roots)
        .await
        .expect("a replacement pod restarts on the retained roots");
    let server = cluster.server(0).expect("the replacement pod is running");
    let client = tenant_client(server, tenant).await;
    assert_eq!(
        sorted_values(&client, &table).await,
        accepted,
        "a never-accepted WAL attempt restores no rows"
    );

    cluster
        .shutdown()
        .await
        .expect("the cluster drains cleanly");
}

/// A published batch's receipt is deleted once its shard's persisted safe
/// frontier covers it, and a restart still restores exactly the accepted rows.
///
/// The first batch is published, which hands its WAL segment off and lets
/// the shard manifest record a frontier at or past its receipt's COMMIT
/// segment. The second is accepted but still WAL-dependent, and the third is
/// never accepted. Of the table's receipts the real deletion query removes
/// only the first.
/// The abrupt stop leaves a torn record after the last write. After the
/// restart the published rows come from their own files, the WAL-dependent
/// rows from the WAL under their kept receipt, the torn record is cut off and
/// the never-accepted rows do not appear.
///
/// # Panics
///
/// Panics when the cluster cannot start, an accepted append fails, the
/// unrecorded append is acknowledged, no frontier covers the published
/// receipt, the deletion keeps it or removes the WAL-dependent one, the restart
/// refuses the torn tail, or the restored rows differ from the accepted ones.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn restart_restores_accepted_rows_after_handed_off_receipts_are_deleted() {
    let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("the one-pod mixed cluster starts");
    let tenant = cluster.data_tenant_id();
    let name = unique_table("handed_off_receipts");
    let node = {
        let server = cluster.server(0).expect("the mixed pod is running");
        register_table(server, tenant, BifrostNamespace::Datasets, &name).await;
        server.node_id()
    };
    let table = format!("{}.{name}", BifrostNamespace::Datasets.as_str());
    let published: Vec<i64> = (0..24).collect();
    let wal_only: Vec<i64> = (24..48).collect();
    let (published_batch, wal_only_batch) = (Uuid::now_v7(), Uuid::now_v7());
    let pool = cluster
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool");
    {
        let server = cluster.server(0).expect("the mixed pod is running");
        let client = tenant_client(server, tenant).await;
        append_values(&client, &table, published_batch, &published)
            .await
            .expect("the published append is acknowledged");
        server
            .flush_bifrost()
            .await
            .expect("the first batch publishes");
        append_values(&client, &table, wal_only_batch, &wal_only)
            .await
            .expect("the WAL-only append is acknowledged");
        sqlx::query("REVOKE INSERT ON vala.scribe_batch_commits FROM wyrd_tenant")
            .execute(&pool)
            .await
            .expect("receipt recording is revoked");
        let refused = append_values(
            &client,
            &table,
            Uuid::now_v7(),
            &(100..124).collect::<Vec<_>>(),
        )
        .await;
        sqlx::query("GRANT INSERT ON vala.scribe_batch_commits TO wyrd_tenant")
            .execute(&pool)
            .await
            .expect("receipt recording is granted again");
        assert!(
            refused.is_err(),
            "an unrecorded batch is never acknowledged"
        );
    }

    delete_handed_off_receipt(&cluster, tenant, published_batch).await;
    let mut conn = vala_sql::TenantConn::acquire(cluster.pg_fixture().app_pool(), tenant)
        .await
        .expect("tenant connection");
    let kept = vala_sql::queries::scribe_batch_commits::committed_batches(
        &mut conn,
        &table,
        &[published_batch, wal_only_batch],
    )
    .await
    .expect("the receipts read");
    conn.commit().await.expect("the read commits");
    assert_eq!(
        kept,
        [wal_only_batch],
        "only the WAL-dependent receipt of the table is kept"
    );

    let roots = cluster
        .terminate_node_abruptly_for_test(node)
        .await
        .expect("the pod is terminated without a drain");
    tear_wal_tail(&cluster, tenant, wal_only_batch).await;
    cluster
        .restart_terminated_node_at_new_address(node, roots)
        .await
        .expect("a replacement pod restarts on the retained roots");
    let server = cluster.server(0).expect("the replacement pod is running");
    let client = tenant_client(server, tenant).await;
    assert_eq!(
        sorted_values(&client, &table).await,
        (0..48).collect::<Vec<_>>(),
        "exactly the accepted rows restore"
    );

    cluster
        .shutdown()
        .await
        .expect("the cluster drains cleanly");
}

/// Another pod's losing copy of a batch restores nothing after the winning
/// receipt is deleted.
///
/// The first pod accepts and publishes the batch. The second pod is then sent
/// the same batch: it writes its own WAL attempt, loses the receipt to the
/// first pod's and acknowledges it as already accepted. Once the first pod's
/// frontier covers its receipt, the receipt is deleted, so the second pod's
/// attempt has no receipt at all. That pod restarts abruptly and flushes, and
/// the table still holds the batch exactly once.
///
/// # Panics
///
/// Panics when the cluster cannot start, either append is refused, the
/// receipt cannot be deleted, or the batch is read back other than once.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn a_losing_duplicate_restores_nothing_after_the_winning_receipt_is_deleted() {
    let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::two_mixed())
        .await
        .expect("the two-pod mixed cluster starts");
    let tenant = cluster.data_tenant_id();
    let name = unique_table("losing_duplicate");
    let (winner, loser) = {
        let winner = cluster.server(0).expect("the first pod is running");
        register_table(winner, tenant, BifrostNamespace::Datasets, &name).await;
        let loser = cluster.server(1).expect("the second pod is running");
        (winner.node_id(), loser.node_id())
    };
    let table = format!("{}.{name}", BifrostNamespace::Datasets.as_str());
    let batch_id = Uuid::now_v7();
    let values: Vec<i64> = (0..24).collect();
    {
        let winner = cluster
            .server_by_node(winner)
            .expect("the first pod is running");
        let key = tenant_key(winner, tenant).await;
        let client = endpoint_client(winner, &key);
        append_values(&client, &table, batch_id, &values)
            .await
            .expect("the first pod accepts the batch");
        winner.flush_bifrost().await.expect("the batch publishes");
        let loser = cluster
            .server_by_node(loser)
            .expect("the second pod is running");
        let client = endpoint_client(loser, &key);
        append_values(&client, &table, batch_id, &values)
            .await
            .expect("the second pod acknowledges the batch as already accepted");
    }
    delete_handed_off_receipt(&cluster, tenant, batch_id).await;

    let roots = cluster
        .terminate_node_abruptly_for_test(loser)
        .await
        .expect("the losing pod is terminated without a drain");
    cluster
        .restart_terminated_node_at_new_address(loser, roots)
        .await
        .expect("a replacement pod restarts on the retained roots");
    let restarted = cluster
        .server_by_node(loser)
        .expect("the replacement pod is running");
    restarted
        .flush_bifrost()
        .await
        .expect("anything the replacement restored publishes");
    let client = tenant_client(restarted, tenant).await;
    assert_eq!(
        sorted_values(&client, &table).await,
        values,
        "the losing WAL attempt restores no second copy"
    );

    cluster
        .shutdown()
        .await
        .expect("the cluster drains cleanly");
}

/// A running pod that acknowledged another pod's batch as a duplicate accepts
/// that batch id anew once the accepting pod's cleaner deletes its receipt.
///
/// The first pod accepts and publishes the batch. The second pod is sent the
/// same batch and acknowledges it as already accepted, adding no copy. The
/// first pod's receipt is aged past retry history and that pod's own cleaner
/// deletes it. The second pod, still running, is sent the batch again: with
/// no receipt left it accepts the batch as new under its own receipt, and the
/// table holds the rows twice.
///
/// # Panics
///
/// Panics when the cluster cannot start, an append or publication fails, the
/// duplicate adds a copy, the first pod's cleaner does not delete the receipt
/// within [`CLEANUP_DEADLINE`], or the second pod acknowledges the last retry
/// without accepting it under its own receipt.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn a_running_duplicate_pod_accepts_anew_after_the_winning_receipt_is_deleted() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::two_mixed())
        .await
        .expect("the two-pod mixed cluster starts");
    let tenant = cluster.data_tenant_id();
    let name = unique_table("running_duplicate");
    let accepting = cluster.server(0).expect("the first pod is running");
    let duplicate = cluster.server(1).expect("the second pod is running");
    register_table(accepting, tenant, BifrostNamespace::Datasets, &name).await;
    let table = format!("{}.{name}", BifrostNamespace::Datasets.as_str());
    let key = tenant_key(accepting, tenant).await;
    let first = endpoint_client(accepting, &key);
    let second = endpoint_client(duplicate, &key);
    let pool = cluster
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool");
    let batch_id = Uuid::now_v7();
    let values: Vec<i64> = (0..24).collect();
    append_values(&first, &table, batch_id, &values)
        .await
        .expect("the first pod accepts the batch");
    accepting
        .flush_bifrost()
        .await
        .expect("the batch publishes");
    append_values(&second, &table, batch_id, &values)
        .await
        .expect("the second pod acknowledges the batch as already accepted");
    duplicate
        .flush_bifrost()
        .await
        .expect("the second pod publishes anything it holds");
    assert_eq!(
        sorted_values(&second, &table).await,
        values,
        "the duplicate adds no copy"
    );
    let receipt = "SELECT wal_node_id FROM vala.scribe_batch_commits \
                    WHERE data_tenant_id = $1 AND batch_id = $2";
    let (winner,): (Uuid,) = sqlx::query_as(receipt)
        .bind(tenant.as_uuid())
        .bind(batch_id)
        .fetch_one(&pool)
        .await
        .expect("the first pod's receipt holds the batch");

    sqlx::query(
        "UPDATE vala.scribe_batch_commits SET ingested_at = ingested_at - interval '2 hours' \
          WHERE data_tenant_id = $1 AND batch_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(batch_id)
    .execute(&pool)
    .await
    .expect("the receipt ages past retry history");
    let deadline = tokio::time::Instant::now() + CLEANUP_DEADLINE;
    loop {
        let left: Option<(Uuid,)> = sqlx::query_as(receipt)
            .bind(tenant.as_uuid())
            .bind(batch_id)
            .fetch_optional(&pool)
            .await
            .expect("receipts read");
        if left.is_none() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the first pod's cleaner never deleted its aged, handed-off receipt"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    append_values(&second, &table, batch_id, &values)
        .await
        .expect("the second pod answers the retry");
    let renewed: Option<(Uuid,)> = sqlx::query_as(receipt)
        .bind(tenant.as_uuid())
        .bind(batch_id)
        .fetch_optional(&pool)
        .await
        .expect("receipts read");
    let (owner,) = renewed.expect("the second pod accepts the retry anew under a receipt");
    assert_ne!(owner, winner, "the new receipt is the second pod's");
    duplicate
        .flush_bifrost()
        .await
        .expect("the second acceptance publishes");
    let mut twice = [values.clone(), values].concat();
    twice.sort_unstable();
    assert_eq!(
        sorted_values(&second, &table).await,
        twice,
        "the batch id accepted anew adds its rows a second time"
    );

    cluster
        .shutdown()
        .await
        .expect("the cluster drains cleanly");
}

/// Staged rows restore and retire exactly once after their receipt is deleted.
///
/// The batch is staged locally but not published, so its stage records are
/// its only copy once its WAL is handed off and its receipt deleted. An abrupt
/// restart serves the rows from those records. Publishing then retires the
/// records, and a second abrupt restart serves the published rows once, with
/// no stage record or WAL left to restore a second copy.
///
/// # Panics
///
/// Panics when the cluster cannot start or restart, the append or a
/// publication fails, the rows publish before the restart, the receipt cannot
/// be deleted, or any read differs from the batch.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn staged_rows_restore_and_retire_once_after_their_receipt_is_deleted() {
    let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("the one-pod mixed cluster starts");
    let tenant = cluster.data_tenant_id();
    let name = unique_table("staged_after_receipt");
    let table = format!("{}.{name}", BifrostNamespace::Datasets.as_str());
    let rows: Vec<i64> = (0..48).collect();
    let batch_id = Uuid::now_v7();
    let (node, key) = {
        let server = cluster.server(0).expect("the mixed pod is running");
        register_table(server, tenant, BifrostNamespace::Datasets, &name).await;
        let key = tenant_key(server, tenant).await;
        let client = endpoint_client(server, &key);
        append_values(&client, &table, batch_id, &rows)
            .await
            .expect("the batch is acknowledged");
        let scribe = server.bifrost_scribe().expect("the pod owns a Scribe");
        scribe
            .flush_writable_for_test()
            .await
            .expect("every writable bucket freezes");
        await_persistence_drained(&scribe).await;
        assert_eq!(
            published_rows(server, tenant, &name).await,
            0,
            "the rows are staged, not published"
        );
        (server.node_id(), key)
    };
    delete_handed_off_receipt(&cluster, tenant, batch_id).await;

    for phase in ["staged", "published"] {
        let roots = cluster
            .terminate_node_abruptly_for_test(node)
            .await
            .expect("the pod is terminated without a drain");
        cluster
            .restart_terminated_node_at_new_address(node, roots)
            .await
            .expect("a replacement pod restarts on the retained roots");
        let server = cluster.server(0).expect("the replacement pod is running");
        let client = endpoint_client(server, &key);
        assert_eq!(
            sorted_values(&client, &table).await,
            rows,
            "the {phase} rows restore exactly once"
        );
        if phase == "staged" {
            server
                .flush_bifrost()
                .await
                .expect("the restored rows publish");
            assert_eq!(
                published_rows(server, tenant, &name).await,
                rows.len() as u64,
                "the committed files hold the batch once"
            );
        }
    }

    cluster
        .shutdown()
        .await
        .expect("the cluster drains cleanly");
}

/// Appends a torn record to the WAL segment holding `batch_id`'s COMMIT, as an
/// abrupt stop in the middle of a write leaves it.
///
/// # Panics
///
/// Panics when the batch holds no receipt or its segment cannot be found or
/// written.
async fn tear_wal_tail(cluster: &WyrdTestCluster, tenant: DataTenantId, batch_id: Uuid) {
    let pool = cluster
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool");
    let (node, epoch, shard, segment): (Uuid, i64, i16, i64) = sqlx::query_as(
        "SELECT wal_node_id, wal_writer_epoch, wal_shard_id, wal_segment_sequence \
           FROM vala.scribe_batch_commits WHERE data_tenant_id = $1 AND batch_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(batch_id)
    .fetch_one(&pool)
    .await
    .expect("the batch holds a receipt");
    let suffix = std::path::PathBuf::from(node.simple().to_string())
        .join(epoch.to_string())
        .join(format!("shard-{shard:02}"))
        .join(format!("{segment}.wal"));
    let path = cluster
        .wal_dirs()
        .find_map(|root| find_file(root, &suffix))
        .expect("the batch's WAL segment exists");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("the WAL segment opens");
    std::io::Write::write_all(&mut file, &[0xff; 13]).expect("the torn bytes are written");
    file.sync_all().expect("the torn bytes sync");
}

/// Deletes `batch_id`'s receipt through the real deletion query once its
/// shard's persisted safe frontier covers the receipt's COMMIT segment.
///
/// Waits for Scribe's own retirement pass to persist that frontier. History
/// is shrunk to zero age so only the frontier decides; other covered receipts
/// of the same shard may go with it.
///
/// # Panics
///
/// Panics when the batch holds no receipt, no persisted frontier covers it
/// within the case deadline, or the deletion fails or removes nothing.
async fn delete_handed_off_receipt(
    cluster: &WyrdTestCluster,
    tenant: DataTenantId,
    batch_id: Uuid,
) {
    let pool = cluster
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool");
    let (stream_node, epoch, shard, segment): (Uuid, i64, i16, i64) = sqlx::query_as(
        "SELECT wal_node_id, wal_writer_epoch, wal_shard_id, wal_segment_sequence \
           FROM vala.scribe_batch_commits WHERE data_tenant_id = $1 AND batch_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(batch_id)
    .fetch_one(&pool)
    .await
    .expect("the batch holds a receipt");
    let manifest_suffix = std::path::PathBuf::from(stream_node.simple().to_string())
        .join(epoch.to_string())
        .join(format!("shard-{shard:02}"))
        .join("manifest");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let safe_segment = loop {
        let safe_segment = cluster
            .wal_dirs()
            .find_map(|root| find_file(root, &manifest_suffix))
            .and_then(|manifest| {
                serde_json::from_slice::<serde_json::Value>(&std::fs::read(manifest).ok()?).ok()?
                    ["safe_segment"]
                    .as_i64()
            });
        match safe_segment {
            Some(safe_segment) if safe_segment >= segment => break safe_segment,
            _ => {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "the shard's persisted frontier never covered the receipt's COMMIT segment"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    };

    let mut conn = vala_sql::TenantConn::acquire(cluster.pg_fixture().app_pool(), tenant)
        .await
        .expect("tenant connection");
    let deleted = vala_sql::queries::scribe_batch_commits::delete_handed_off(
        &mut conn,
        &vala_sql::queries::scribe_batch_commits::HandedOffShard {
            node_id: stream_node,
            writer_epoch: epoch,
            shard_id: shard,
            safe_segment,
        },
        &vala_sql::queries::scribe_batch_commits::RetryHistory {
            max_age: Duration::ZERO,
            ..Default::default()
        },
        chrono::Utc::now(),
        100,
    )
    .await
    .expect("the handed-off receipts are deleted");
    conn.commit().await.expect("the deletion commits");
    assert!(!deleted.is_empty(), "the handed-off receipt is deleted");
}

/// Longest a receipt-cleanup case waits for the pod's own cleaner.
///
/// Its scheduled cadence, with headroom for the sweep itself.
const CLEANUP_DEADLINE: Duration = Duration::from_secs(60);

/// The pod deletes its own aged receipts, and refuses fresh writes as
/// retryable busy while it cannot, without losing reads or accepted rows.
///
/// An accepted, published batch's receipt is aged past recent retry history.
/// The pod's background cleaner deletes it without any test-driven deletion,
/// and the same batch id is then accepted again as a new batch. With receipt
/// deletion revoked the cleaner fails: fresh appends are refused with the
/// retryable busy code while the accepted rows stay readable. Granting
/// deletion again lets the cleaner recover and the pod admit again.
///
/// # Panics
///
/// Panics when the cluster cannot start, an accepted append or a read fails,
/// the cleaner does not delete the aged receipt, a refusal carries another
/// code, or admission does not stop and resume within [`CLEANUP_DEADLINE`].
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn receipt_cleanup_failure_pauses_fresh_writes_until_it_recovers() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("the one-pod mixed cluster starts");
    let tenant = cluster.data_tenant_id();
    let name = unique_table("receipt_cleanup");
    let server = cluster.server(0).expect("the mixed pod is running");
    register_table(server, tenant, BifrostNamespace::Datasets, &name).await;
    let table = format!("{}.{name}", BifrostNamespace::Datasets.as_str());
    let probe_name = unique_table("receipt_cleanup_probe");
    register_table(server, tenant, BifrostNamespace::Datasets, &probe_name).await;
    let probe = format!("{}.{probe_name}", BifrostNamespace::Datasets.as_str());
    let pool = cluster
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool");
    let client = tenant_client(server, tenant).await;
    let batch_id = Uuid::now_v7();
    let values: Vec<i64> = (0..24).collect();
    append_values(&client, &table, batch_id, &values)
        .await
        .expect("the batch is accepted");
    server.flush_bifrost().await.expect("the batch publishes");
    sqlx::query(
        "UPDATE vala.scribe_batch_commits SET ingested_at = ingested_at - interval '2 hours' \
          WHERE data_tenant_id = $1 AND batch_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(batch_id)
    .execute(&pool)
    .await
    .expect("the receipt ages past retry history");

    let deadline = tokio::time::Instant::now() + CLEANUP_DEADLINE;
    loop {
        let (left,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM vala.scribe_batch_commits \
              WHERE data_tenant_id = $1 AND batch_id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(batch_id)
        .fetch_one(&pool)
        .await
        .expect("receipts read");
        if left == 0 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the pod's cleaner never deleted the aged, handed-off receipt"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    append_values(&client, &table, batch_id, &values)
        .await
        .expect("a batch id whose receipt is gone is accepted as a new batch");
    server
        .flush_bifrost()
        .await
        .expect("the second batch publishes");
    let twice = [values.clone(), values.clone()].concat();
    let mut twice_sorted = twice.clone();
    twice_sorted.sort_unstable();
    assert_eq!(sorted_values(&client, &table).await, twice_sorted);

    sqlx::query("REVOKE DELETE ON vala.scribe_batch_commits FROM wyrd_tenant")
        .execute(&pool)
        .await
        .expect("receipt deletion is revoked");
    let deadline = tokio::time::Instant::now() + CLEANUP_DEADLINE;
    let refusal = loop {
        if let Err(error) = append_values(&client, &probe, Uuid::now_v7(), &[100]).await {
            break error;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "fresh writes were still admitted after cleanup could not delete"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert_eq!(
        refusal.code(),
        INGEST_BUSY,
        "cleanup failure refuses as retryable busy"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        twice_sorted,
        "accepted rows stay readable while writes are refused"
    );

    sqlx::query("GRANT DELETE ON vala.scribe_batch_commits TO wyrd_tenant")
        .execute(&pool)
        .await
        .expect("receipt deletion is granted again");
    let deadline = tokio::time::Instant::now() + CLEANUP_DEADLINE;
    loop {
        match append_values(&client, &probe, Uuid::now_v7(), &[100]).await {
            Ok(()) => break,
            Err(error) if error.code() == INGEST_BUSY => {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "the pod never admitted again after cleanup recovered"
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            Err(error) => panic!("a recovered pod admits the write: {error:?}"),
        }
    }

    cluster
        .shutdown()
        .await
        .expect("the cluster drains cleanly");
}

/// Finds the file under `root` whose path ends with `suffix`.
fn find_file(root: &std::path::Path, suffix: &std::path::Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(root).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        if path.is_dir() {
            find_file(&path, suffix)
        } else {
            path.ends_with(suffix).then_some(path)
        }
    })
}

/// Starts one bound pod over a durable Bifrost data root and provisions one
/// queued Verifier execution in its tenant.
///
/// # Panics
///
/// Panics when the pod cannot start or the fixture, subject, Verifier, or
/// submission fails.
async fn execution_pod(
    root: &std::path::Path,
) -> (
    WyrdTestServer,
    VerificationFixture,
    wyrd_spec::ids::ExecutionId,
) {
    let server = WyrdTestServer::builder()
        .with_durable_bifrost_data_root(root.to_path_buf())
        .start_bound()
        .await
        .expect("the pod starts on a durable data root");
    let fixture = VerificationFixture::provision(server.state(), server.data_tenant_id())
        .await
        .expect("the tenant provisions");
    let (subject, _) = fixture.service("recovered").await.expect("service");
    let verifier = fixture
        .drift_verifier("recovering")
        .await
        .expect("verifier");
    let execution = fixture
        .submit(&verifier, &subject, std::collections::BTreeMap::new())
        .await
        .expect("the execution is admitted");
    (server, fixture, execution)
}

/// Takes over `execution` as the next claim of its lease and returns that
/// writer's recovery of the Bifrost state.
///
/// # Errors
///
/// Returns the [`StateError`] refusing the recovery: an accepted revision
/// that cannot be found or read is unavailable, and one in another format is
/// incompatible.
///
/// # Panics
///
/// Panics when the lease cannot be expired or the execution is not claimable.
async fn take_over(
    fixture: &VerificationFixture,
    server: &WyrdTestServer,
    execution: wyrd_spec::ids::ExecutionId,
) -> Result<RunState, StateError> {
    fixture
        .expire_lease(execution)
        .await
        .expect("the lease expires");
    fixture
        .claim_state(server.state())
        .await
        .expect("the execution is reclaimable")
        .1
}

/// The (step, attempt) of every invocation in `state`, in commit order.
fn invocations(state: &ExecutionState) -> Vec<(String, u32)> {
    state
        .invocations
        .iter()
        .map(|invocation| {
            (
                invocation.producer.step_id.as_str().to_owned(),
                invocation.producer.attempt,
            )
        })
        .collect()
}

/// Recovery loads the newest readable revision and never overwrites a newer one.
///
/// A writer accepts two invocations, which background persistence may
/// coalesce into one revision or write as two. When the next revision's
/// batch is held but its row is not visible, as when the Scribe holding it
/// was unavailable before the read discovered it, a takeover recovers the
/// newest readable revision, and its own write of the next revision is
/// refused, so it never overwrites the held one. Once nothing else holds
/// the revision, a takeover loads the accepted revision exactly, and the
/// same revision reads back after the pod restarts on its retained data
/// root and replays its WAL.
///
/// # Panics
///
/// Panics when the pod cannot start or restart, a revision is not accepted,
/// recovery starts fresh, a takeover overwrites a held revision, or a recovered state
/// differs from the accepted one.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn execution_recovery_requires_exact_accepted_state() {
    let root = tempfile::tempdir().expect("durable Bifrost data root");
    let (server, fixture, execution) = execution_pod(root.path()).await;
    let (_, first) = fixture
        .claim_state(server.state())
        .await
        .expect("the execution is claimable");
    let first = first.expect("a fresh execution's state claims");
    for step in ["review", "judge"] {
        first
            .begin(&Producer::plain(&TaskId::new(step).expect("task"), 1))
            .await
            .expect("the revision is accepted");
    }
    first.persisted().await.expect("the revisions are accepted");
    let accepted = first.snapshot();
    drop(first);

    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool");
    let fences: Vec<Uuid> = (1..=2)
        .map(|revision| revision_fence(server.data_tenant_id(), execution, revision))
        .collect();
    let (last,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM vala.scribe_batch_commits WHERE batch_id = ANY($1)")
            .bind(&fences)
            .fetch_one(&pool)
            .await
            .expect("the accepted fences read");
    assert!(last >= 1, "the accepted invocations are fenced");
    let unseen = revision_fence(server.data_tenant_id(), execution, last + 1);
    let fenced = sqlx::query(
        "INSERT INTO vala.scribe_batch_commits \
         SELECT (jsonb_populate_record(c, jsonb_build_object('batch_id', $2::uuid))).* \
           FROM vala.scribe_batch_commits c \
          WHERE logical_table_fqn = 'bifrost.verification.execution_events' AND batch_id = $1",
    )
    .bind(revision_fence(server.data_tenant_id(), execution, last))
    .bind(unseen)
    .execute(&pool)
    .await
    .expect("the next revision's fence is durable");
    assert_eq!(
        fenced.rows_affected(),
        1,
        "exactly the next fence is copied"
    );
    let behind = take_over(&fixture, &server, execution)
        .await
        .expect("the newest readable revision recovers");
    assert_eq!(behind.snapshot(), accepted);
    behind
        .begin(&Producer::plain(&TaskId::new("verdict").expect("task"), 1))
        .await
        .expect("the change is recorded in memory");
    assert!(
        matches!(behind.persisted().await, Err(StateError::Stale)),
        "a revision another batch holds is never overwritten"
    );
    drop(behind);
    sqlx::query("DELETE FROM vala.scribe_batch_commits WHERE batch_id = $1")
        .bind(unseen)
        .execute(&pool)
        .await
        .expect("the unseen fence is withdrawn");
    let recovered = take_over(&fixture, &server, execution)
        .await
        .expect("the accepted revision recovers");
    assert_eq!(recovered.snapshot(), accepted);
    assert_eq!(
        invocations(&accepted),
        [("review".to_owned(), 1), ("judge".to_owned(), 1)]
    );
    drop(recovered);

    let server = Box::pin(server.restart_bound(
        WyrdTestServer::builder().with_durable_bifrost_data_root(root.path().to_path_buf()),
    ))
    .await
    .expect("the pod restarts on its retained data root");
    assert_eq!(
        fixture
            .execution_state(server.state(), execution)
            .await
            .expect("the accepted revision reads after replay"),
        Some(accepted)
    );
    server.shutdown().await.expect("the server drains cleanly");
}

/// An execution's accepted revisions outlive their retry receipts.
///
/// A writer accepts two invocations and the pod publishes them, so their
/// rows have custody of their own. Every revision receipt is then removed,
/// as cleanup removes receipts outside recent history: a takeover still
/// recovers the exact accepted state from its Bifrost records instead of
/// starting fresh. A receipt for the revision after the newest visible one
/// stands for an accepted row no read can see yet: a takeover recovers the
/// newest readable revision, and its write of the held revision is refused.
///
/// # Panics
///
/// Panics when the pod cannot start or publish, a revision is not accepted,
/// recovery starts fresh, a takeover overwrites a held revision, or a recovered
/// state differs from the accepted one.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn execution_recovery_outlives_its_revision_receipts() {
    let root = tempfile::tempdir().expect("durable Bifrost data root");
    let (server, fixture, execution) = execution_pod(root.path()).await;
    let (_, first) = fixture
        .claim_state(server.state())
        .await
        .expect("the execution is claimable");
    let first = first.expect("a fresh execution's state claims");
    for step in ["review", "judge"] {
        first
            .begin(&Producer::plain(&TaskId::new(step).expect("task"), 1))
            .await
            .expect("the revision is accepted");
    }
    first.persisted().await.expect("the revisions are accepted");
    let accepted = first.snapshot();
    drop(first);
    server
        .flush_bifrost()
        .await
        .expect("the accepted revisions publish");

    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("fixture exposes a superuser pool");
    let tenant = server.data_tenant_id();
    let fences: Vec<Uuid> = (1..=2)
        .map(|revision| revision_fence(tenant, execution, revision))
        .collect();
    let (last,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM vala.scribe_batch_commits WHERE batch_id = ANY($1)")
            .bind(&fences)
            .fetch_one(&pool)
            .await
            .expect("the accepted fences read");
    assert!(last >= 1, "the accepted invocations are fenced");
    let expired = sqlx::query("DELETE FROM vala.scribe_batch_commits WHERE batch_id = ANY($1)")
        .bind(&fences)
        .execute(&pool)
        .await
        .expect("the revision receipts are removed");
    assert_eq!(
        expired.rows_affected(),
        u64::try_from(last).expect("count"),
        "every receipt is gone"
    );
    let recovered = take_over(&fixture, &server, execution)
        .await
        .expect("the accepted revision recovers without its receipts");
    assert_eq!(
        recovered.snapshot(),
        accepted,
        "recovery neither resets nor truncates the accepted revisions"
    );
    drop(recovered);

    let unseen = revision_fence(tenant, execution, last + 1);
    sqlx::query(
        "INSERT INTO vala.scribe_batch_commits \
         (data_tenant_id, logical_table_fqn, batch_id, slice_set_digest, slice_count, \
          wal_node_id, wal_writer_epoch, wal_shard_id, wal_segment_sequence, wal_lsn_min, \
          wal_lsn_max, request_id, ingested_at, table_version) \
         VALUES ($1, 'bifrost.verification.execution_events', $2, \
                 decode(repeat('00', 32), 'hex'), 1, gen_random_uuid(), 1, 0, 0, 0, 0, \
                 gen_random_uuid(), statement_timestamp(), 1)",
    )
    .bind(tenant.as_uuid())
    .bind(unseen)
    .execute(&pool)
    .await
    .expect("the next revision's receipt is recent");
    let behind = take_over(&fixture, &server, execution)
        .await
        .expect("the newest readable revision recovers");
    assert_eq!(behind.snapshot(), accepted);
    behind
        .begin(&Producer::plain(&TaskId::new("verdict").expect("task"), 1))
        .await
        .expect("the change is recorded in memory");
    assert!(
        matches!(behind.persisted().await, Err(StateError::Stale)),
        "a revision another batch holds is never overwritten"
    );
    drop(behind);
    sqlx::query("DELETE FROM vala.scribe_batch_commits WHERE batch_id = $1")
        .bind(unseen)
        .execute(&pool)
        .await
        .expect("the unseen receipt is withdrawn");
    let recovered = take_over(&fixture, &server, execution)
        .await
        .expect("the accepted revision recovers again");
    assert_eq!(recovered.snapshot(), accepted);
    drop(recovered);
    server.shutdown().await.expect("the server drains cleanly");
}

/// A restarted pod keeps its own Bifrost data root, so a checkpoint
/// acknowledged into its WAL but never published survives a shutdown that
/// aborts at its budget and recovers exactly.
///
/// Every object write is held and a publication of the acknowledged
/// revisions is in flight at the hold when the pod stops, so its Bifrost
/// drain cannot finish within the shutdown budget and is aborted. The harness
/// restart mounts no explicit data root; the replacement reopens the original
/// one, reads the accepted revision, and a takeover recovers it without
/// running an accepted invocation again.
///
/// # Panics
///
/// Panics when the pod cannot start or restart, the revisions are not
/// accepted, or the state after restart differs from the accepted one.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn restart_recovers_an_acknowledged_unpublished_checkpoint() {
    let faults = PersistenceFaults::default();
    faults.hold_object_writes_for_test();
    let server = WyrdTestServer::builder()
        .with_scribe_persistence_faults_for_test(faults.clone())
        .start_bound()
        .await
        .expect("the pod starts");
    let fixture = VerificationFixture::provision(server.state(), server.data_tenant_id())
        .await
        .expect("the tenant provisions");
    let (subject, _) = fixture.service("unpublished").await.expect("service");
    let verifier = fixture
        .drift_verifier("unpublished-recovery")
        .await
        .expect("verifier");
    let execution = fixture
        .submit(&verifier, &subject, std::collections::BTreeMap::new())
        .await
        .expect("the execution is admitted");
    let (_, writer) = fixture
        .claim_state(server.state())
        .await
        .expect("the execution is claimable");
    let writer = writer.expect("a fresh execution's state claims");
    for step in ["review", "judge"] {
        writer
            .begin(&Producer::plain(&TaskId::new(step).expect("task"), 1))
            .await
            .expect("the revision is accepted");
    }
    writer
        .persisted()
        .await
        .expect("the revisions are acknowledged before any publishes");
    let accepted = writer.snapshot();
    drop(writer);
    let state = server.state().clone();
    let publishing = tokio::spawn(async move { state.flush_scribe_for_test().await });
    tokio::time::timeout(
        Duration::from_secs(30),
        faults.wait_for_held_object_writes_for_test(1),
    )
    .await
    .expect("the acknowledged revisions start publishing");

    let server = Box::pin(server.restart_bound(WyrdTestServer::builder()))
        .await
        .expect("the pod restarts on its own data root");
    publishing.abort();
    assert_eq!(
        fixture
            .execution_state(server.state(), execution)
            .await
            .expect("the acknowledged revision reads after restart"),
        Some(accepted.clone())
    );
    let recovered = take_over(&fixture, &server, execution)
        .await
        .expect("the acknowledged revision recovers");
    assert_eq!(recovered.snapshot(), accepted);
    assert_eq!(
        invocations(&recovered.snapshot()),
        [("review".to_owned(), 1), ("judge".to_owned(), 1)],
        "no accepted invocation is recomputed"
    );
    drop(recovered);
    server.shutdown().await.expect("the server drains cleanly");
}

/// A takeover inherits progress accepted before it, the first accepted
/// writer of a revision keeps it, and the refused write never resurrects
/// through WAL replay.
///
/// Writer A accepts one revision and is taken over by writer B, which
/// inherits it. B accepts the next revision; A, delayed past the takeover,
/// offers its own rows for that same revision and is refused. After the pod
/// restarts on its retained data root and replays its WAL, the execution
/// holds exactly B's two revisions, one row each, and a further takeover
/// recovers B's state.
///
/// # Panics
///
/// Panics when the pod cannot start or restart, an accepted revision is
/// refused or lost, the replaced writer's revision is accepted or replayed,
/// or a recovered state differs from the accepted one.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn execution_takeover_rejects_late_writes_after_replay() {
    let root = tempfile::tempdir().expect("durable Bifrost data root");
    let (server, fixture, execution) = execution_pod(root.path()).await;
    let (_, replaced) = fixture
        .claim_state(server.state())
        .await
        .expect("the execution is claimable");
    let replaced = replaced.expect("a fresh execution's state claims");
    replaced
        .begin(&Producer::plain(&TaskId::new("review").expect("task"), 1))
        .await
        .expect("A's first revision is recorded");
    replaced
        .persisted()
        .await
        .expect("A's first revision is accepted");

    let current = take_over(&fixture, &server, execution)
        .await
        .expect("B takes the state over");
    assert_eq!(
        invocations(&current.snapshot()),
        [("review".to_owned(), 1)],
        "B inherits the progress accepted before its takeover"
    );
    current
        .begin(&Producer::plain(&TaskId::new("judge").expect("task"), 1))
        .await
        .expect("B's revision is recorded");
    current.persisted().await.expect("B's revision is accepted");
    replaced
        .begin(&Producer::plain(&TaskId::new("late").expect("task"), 1))
        .await
        .expect("the replaced writer records its revision in memory");
    replaced
        .persisted()
        .await
        .expect_err("the replaced writer cannot take B's revision");
    let accepted = current.snapshot();
    drop((replaced, current));

    let server = Box::pin(server.restart_bound(
        WyrdTestServer::builder().with_durable_bifrost_data_root(root.path().to_path_buf()),
    ))
    .await
    .expect("the pod restarts on its retained data root");
    server.flush_bifrost().await.expect("the replay publishes");
    let client = tenant_client(&server, server.data_tenant_id()).await;
    assert_eq!(
        read_sql(
            &client,
            &format!(
                "SELECT sequence AS value FROM bifrost.verification.execution_events \
                 WHERE execution_id = '{execution}' ORDER BY sequence"
            ),
        )
        .await,
        [1, 2],
        "exactly the accepted revisions are retained once each"
    );
    let recovered = take_over(&fixture, &server, execution)
        .await
        .expect("the accepted revision recovers after replay");
    assert_eq!(recovered.snapshot(), accepted);
    assert_eq!(
        invocations(&accepted),
        [("review".to_owned(), 1), ("judge".to_owned(), 1)]
    );
    drop(recovered);
    server.shutdown().await.expect("the server drains cleanly");
}
