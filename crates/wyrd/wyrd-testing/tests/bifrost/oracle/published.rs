//! Oracle journeys — published-cut governance: the production storage owner's
//! cache reuse, immutable identity, pruning, bounded cancellation, and the
//! process lifecycle that must close it.
//!
//! Module of the `oracle` binary; see `main.rs` for the capability it proves
//! and `support.rs` for the fixtures it shares.

use std::sync::Arc;
use std::time::Duration;

use vala_bifrost_redux::storage::{
    BifrostStorageError, StorageLifecycle, StorageRequestOutcome,
};
use wyrd_testing::WyrdTestServer;

/// An already-expired process drain aborts storage and reports the failure.
///
/// The branch under test is the one a real pod takes when its supervisor drain
/// consumed the whole budget: production must not treat "no time left" as a
/// clean teardown. Before this journey existed, that branch returned a
/// successful all-`false` report and never touched the storage owner, so a
/// process could exit reporting success while its metadata cache was still
/// open and its object I/O still admissible. The assertions are therefore
/// three: the terminal is the exact stable lifecycle failure, the retained
/// production owner is closed and settled, and a governed read issued
/// afterwards is refused by the owner rather than reaching the backend.
///
/// # Panics
///
/// Panics when the bound server does not start, when the join returns a
/// successful report or a different failure, when the retained storage snapshot
/// is not closed and quiescent, or when a post-shutdown read is admitted.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn expired_process_shutdown_aborts_storage_and_returns_failure() {
    let mut server = WyrdTestServer::builder()
        .with_shutdown_drain_for_test(Duration::ZERO)
        .start_bound()
        .await
        .expect("the bound production server starts");
    let storage = Arc::clone(
        server
            .state()
            .bifrost_storage()
            .expect("a composed server owns one Bifrost storage owner"),
    );

    let failure = server
        .cancel_and_join_for_test()
        .await
        .expect_err("an already-expired drain must not report a successful shutdown");
    let rendered = format!("{failure:?}");
    assert!(
        rendered.contains("Bifrost shutdown deadline elapsed before role drain"),
        "the expired branch must project its stable lifecycle failure, got {rendered}"
    );

    let snapshot = storage.telemetry_snapshot();
    assert_eq!(
        snapshot.lifecycle(),
        StorageLifecycle::Closed,
        "an aborted process must leave the storage owner closed"
    );
    assert_eq!(snapshot.load_starts(), snapshot.load_terminals());
    assert_eq!(snapshot.request_starts(), snapshot.request_terminals());
    assert_eq!(snapshot.active_requests(), 0);
    assert_eq!(snapshot.inflight_loads(), 0);
    assert_eq!(snapshot.waiters(), 0);
    assert_eq!(snapshot.resident_entries(), 0);
    assert_eq!(snapshot.resident_bytes(), 0);
    assert_eq!(snapshot.anomalies(), 0);

    let refused = storage
        .read("bifrost/journey/after-shutdown.parquet")
        .await
        .expect_err("a closed owner admits no governed read");
    assert_eq!(
        refused,
        BifrostStorageError::Closed,
        "the refusal must come from the owner, before any backend call"
    );
    let after = storage.telemetry_snapshot();
    assert_eq!(
        after.request_terminal(StorageRequestOutcome::Closed),
        snapshot.request_terminal(StorageRequestOutcome::Closed) + 1,
        "the refusal must publish exactly one closed request terminal"
    );
    assert_eq!(after.anomalies(), 0);

    server
        .shutdown()
        .await
        .expect("the harness releases its fixtures");
}
