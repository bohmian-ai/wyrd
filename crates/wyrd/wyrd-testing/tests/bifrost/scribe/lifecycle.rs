//! Shutdown ownership: what a pod publishes before it stops and what it keeps.

use uuid::Uuid;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use super::support::{
    append_values, await_persistence_drained, published_rows, register_table, sorted_values,
    tenant_client, unique_table,
};

/// A stopping pod either publishes its acknowledged rows or retains them.
///
/// Acknowledged rows outlive the process that accepted them, and there are
/// exactly two honest ways for that to be true. A pod told to stop drains: it
/// sweeps every staged member it is still holding into published objects, so
/// nothing is left for the next process to rediscover. A pod that is killed
/// cannot drain, so the WAL it already fsynced stays authoritative and its
/// replacement replays it. This owner drives both against one table and holds
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
/// append or read fails, or when a shutdown loses, duplicates, or fails to
/// publish an acknowledged row.
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

    // A pod told to stop owes its staged rows a publication.
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
        // A stopping pod either drains its staged rows into published objects
        // or leaves all of them for replay, and which one happens depends on
        // whether the bounded drain outlives the work. What it may never do is
        // publish part of a claim, so that is what this holds.
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
