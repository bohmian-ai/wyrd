//! Reconciliation of the pod's one production observation owner.
//!
//! Scribe publishes every admission, contention, staging and publication
//! transition through a single `ScribeTelemetry` owner. A counter on its own is
//! not evidence that the transition it names happened, so this module never
//! asserts a count in isolation: each total is checked against the production
//! state the same run can observe independently — writable buckets, durable
//! staged members, published hot objects, and the rows a public strict read
//! returns.

use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::telemetry::ScribeStagingSnapshot;
use wyrd_testing::WyrdTestServer;

use super::support::{
    append_values, await_persistence_drained, published_object_count, register_table,
    sorted_values, start_scribe_server, tenant_client, unique_table,
};

/// Longest a settled pod may take to retire the members its publication replaced.
///
/// Retirement waits for a published member's readers to drain, so it is not
/// complete at the instant `flush_bifrost` returns. The deadline turns a stuck
/// settlement into a diagnosable failure; nothing here measures elapsed time as
/// evidence.
const SETTLEMENT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

/// Polls the retained owner until publication has settled, or the deadline passes.
///
/// Returns the first snapshot in which no claim is outstanding and no staged
/// member survives its published object, which is the terminal durability state
/// the run's published objects and exact read-back are then checked against.
///
/// # Panics
///
/// Panics when the staging totals are not inspectable, or when the pod has not
/// settled inside [`SETTLEMENT_DEADLINE`].
async fn await_staging_settled(server: &WyrdTestServer) -> ScribeStagingSnapshot {
    let deadline = tokio::time::Instant::now() + SETTLEMENT_DEADLINE;
    loop {
        let staging = server
            .scribe_staging_totals_for_test()
            .expect("the pod's staged and claim totals are inspectable");
        if staging.outstanding_claims() == 0 && staging.live_members() == 0 {
            return staging;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "publication did not settle inside {SETTLEMENT_DEADLINE:?}: \
             {} claims outstanding and {} staged members still live",
            staging.outstanding_claims(),
            staging.live_members()
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// The one retained telemetry owner reconciles with the state it observed.
///
/// The case walks an ordinary hot path — append, freeze, publish, read — and at
/// every boundary checks the owner's reconcilable totals against a fact the run
/// can establish without the owner: an idle pod owns no vector and no member; an
/// settled append has returned the lifecycle vector it borrowed while its rows
/// stay owned by a writable bucket and nothing is published; the freeze makes at
/// least one member durable with no claim yet outstanding; and publication
/// settles every
/// claim and retires every member it replaced while the same strict read stays
/// exact.
///
/// What makes this a duplicate-projection check rather than a counter tour is
/// that the append phase is measured as an exact delta. Four appends open four
/// admission transitions, close four, and serve four bounded demand records on
/// the one retained owner. A second registry projecting the same transitions
/// would double every one of those deltas while every other fact in the run —
/// the rows, the buckets, the objects — stayed identical.
///
/// # Panics
///
/// Panics when a public append, freeze, publication or read fails, when the
/// totals are not inspectable, when any total disagrees with the production
/// state observed beside it, or when the read returns other than exactly the
/// acknowledged rows.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_hot_path_telemetry_reconciles() {
    let server = start_scribe_server().await;
    let tenant = server.data_tenant_id();
    let name = unique_table("telemetry_reconcile");
    let table = register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
    let client = tenant_client(&server, tenant).await;

    // Idle: the owner has nothing to report because the pod owns nothing.
    let idle = server
        .scribe_contention_totals_for_test()
        .expect("the pod's contention totals are inspectable");
    assert_eq!(
        idle.active_transitions(),
        0,
        "an idle pod holds no admission transition in flight"
    );
    assert_eq!(
        idle.live_vectors(),
        0,
        "an idle pod lends no lifecycle vector"
    );
    let idle_staging = server
        .scribe_staging_totals_for_test()
        .expect("the pod's staged and claim totals are inspectable");
    assert_eq!(
        idle_staging.live_members(),
        0,
        "an idle pod owns no durable staged member"
    );
    assert_eq!(
        idle_staging.outstanding_claims(),
        0,
        "an idle pod has no publication in flight"
    );

    let expected: Vec<i64> = (0..64).collect();
    let appends = expected.len() / 16;
    for (ordinal, chunk) in expected.chunks(16).enumerate() {
        append_values(&client, &table, uuid::Uuid::now_v7(), chunk)
            .await
            .unwrap_or_else(|error| panic!("append {ordinal} is acknowledged: {error:?}"));
    }

    // Active: every append settled, so the owner reports the exact transition
    // deltas beside the writable bucket that still holds the acknowledged rows.
    let active = server
        .scribe_contention_totals_for_test()
        .expect("the pod's contention totals are inspectable");
    assert_eq!(
        active.starts() - idle.starts(),
        appends as u64,
        "each admitted append opens exactly one admission transition on the \
         retained owner"
    );
    assert_eq!(
        active.terminals() - idle.terminals(),
        appends as u64,
        "each admitted append closes exactly the transition it opened"
    );
    assert_eq!(
        active.demand_transitions() - idle.demand_transitions(),
        appends as u64,
        "each admitted append serves exactly one bounded demand record"
    );
    assert_eq!(
        active.active_transitions(),
        0,
        "every acknowledged append closed the admission transition it opened"
    );
    assert_eq!(
        active.live_vectors(),
        0,
        "a settled append returns the lifecycle vector it borrowed, so no \
         vector stays lent between appends"
    );
    let snapshot = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");
    assert!(
        snapshot.writable_bucket_count > 0,
        "the acknowledged rows stay owned by a writable bucket after their \
         admission transitions settled"
    );
    let staged_before_freeze = server
        .scribe_staging_totals_for_test()
        .expect("the pod's staged and claim totals are inspectable");
    assert_eq!(
        staged_before_freeze.live_members(),
        0,
        "no member is durable before the freeze that stages it"
    );

    // Freeze: the writable buckets become durable staged members and the owner
    // counts them, while publication has not been asked for.
    let scribe = server.bifrost_scribe().expect("the server owns a Scribe");
    scribe
        .flush_writable_for_test()
        .await
        .expect("every writable bucket freezes");
    await_persistence_drained(&scribe).await;
    let staged = server
        .scribe_staging_totals_for_test()
        .expect("the pod's staged and claim totals are inspectable");
    assert!(
        staged.live_members() > 0,
        "the freeze made at least one member durable and the owner must count it"
    );
    assert_eq!(
        staged.outstanding_claims(),
        0,
        "no claim is outstanding before publication is asked for"
    );
    assert_eq!(
        published_object_count(&server, tenant, BifrostNamespace::Datasets, &name).await,
        0,
        "the freeze is a local staging boundary and publishes nothing"
    );

    // Publication: every claim settles and every member the published objects
    // replaced is retired, with the strict read still exact.
    server
        .flush_bifrost()
        .await
        .expect("the staged members publish");
    let published = server
        .published_hot_files_for_test(tenant, BifrostNamespace::Datasets.as_str(), &name)
        .await
        .expect("published hot files are inspectable");
    assert!(
        !published.is_empty(),
        "publication must commit at least one hot object"
    );
    let published_rows: u64 = published.iter().map(|file| file.row_count).sum();
    assert_eq!(
        published_rows,
        expected.len() as u64,
        "the published objects must account for every acknowledged row exactly once"
    );
    let settled = await_staging_settled(&server).await;
    assert_eq!(
        settled.outstanding_claims(),
        0,
        "a settled pod reports no publication in flight"
    );
    assert_eq!(
        settled.live_members(),
        0,
        "every staged member the published objects replaced is retired"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "the reconciled run must still read back exactly the acknowledged rows"
    );

    // Terminal: the admission ledger closes everything it opened, and a pod that
    // was never refused publishes no demand transition at all.
    let terminal = server
        .scribe_contention_totals_for_test()
        .expect("the pod's contention totals are inspectable");
    assert_eq!(
        terminal.starts(),
        terminal.terminals(),
        "every admission transition the pod opened was closed exactly once"
    );
    assert_eq!(
        terminal.active_transitions(),
        0,
        "a settled pod holds no admission transition in flight"
    );
    assert_eq!(
        terminal.live_vectors(),
        0,
        "a settled pod lends no lifecycle vector"
    );

    server.shutdown().await.expect("the server drains cleanly");
}
