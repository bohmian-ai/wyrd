//! Tier-2 coverage for AC-015: snapshot retention, not reader tracking,
//! protects Oracle reads from Forge destructive maintenance.
//!
//! A query reads its cut in one read-only statement and records nothing.
//! Forge keeps every replaced snapshot for the retention window, measured in
//! `PostgreSQL` time from when it first observed the snapshot replaced, so a
//! query whose deadline fits the window never loses its files. A cut read
//! never waits on a maintenance authority.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::Duration as ChronoDuration;
use uuid::Uuid;

use super::snapshot_expiration::{
    ExpirableTable, assert_files_exist, commit_snapshot, expirable_table, expiry_attempts,
    latest_cleanup_task, leader_only, maintain_once, object_exists, seed_never_published_object,
    seed_running_task, snapshot_files,
};
use super::support::{ForgeTelemetryCheckpoint, PromotionIntegrationFixture};

/// Bound on every wait for a destructive owner to reach its paused effect.
const EFFECT_BOUND: Duration = Duration::from_secs(30);

/// The production retention window the retention scenario runs under.
const RETENTION: Duration = Duration::from_hours(6);

/// Proves replaced snapshots outlive the retention window, and only it.
///
/// A query reads its cut, then two commits replace its snapshot. Inside the
/// window the leader's maintenance pass expires nothing and the cut's files
/// remain. Once the recorded supersessions are older than the window, the very
/// next pass expires every replaced snapshot, drains the cleanup it handed
/// off, and keeps the head's files. Orphan collection is not gated by any
/// read: it deletes an aged orphan while the cut is still held.
///
/// # Panics
///
/// Panics when a replaced snapshot expires inside the window, survives past
/// it, the head loses a file, or the orphan survives collection.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn replaced_snapshots_expire_only_after_the_retention_window() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let mut fixture = PromotionIntegrationFixture::start("retention_window").await;
    fixture.config.snapshot_retention = RETENTION;
    let table = leader_only(fixture).await;
    let fixture = &table.fixture;
    let cut = fixture.read_cut().await;
    let replaced = table.watermark.0;
    let replaced_files = snapshot_files(fixture, replaced).await;
    commit_snapshot(&table).await;
    let head = commit_snapshot(&table).await;

    let (before, after) = maintain_once(&table).await;
    assert_eq!(
        after.snapshots, before.snapshots,
        "no snapshot expires inside the retention window"
    );
    assert_eq!(
        expiry_attempts(fixture).await,
        0,
        "a table with nothing past the window plans no expiry task"
    );
    assert_files_exist(fixture, &replaced_files, "the held cut's snapshot").await;

    // Only orphan collection reads the Forge clock (its object TTL).
    let orphan = seed_never_published_object(fixture).await;
    table
        .control
        .advance(ChronoDuration::hours(48))
        .expect("manual clock advance");
    let report = table
        .supervised
        .forge()
        .run_orphan_gc_report_for_test(&fixture.binding)
        .await
        .expect("orphan cleanup runs");
    assert_eq!(report.deleted, 1, "a held cut does not gate orphan cleanup");
    assert!(!object_exists(fixture, &orphan).await, "the orphan is gone");
    assert_files_exist(fixture, &replaced_files, "the held cut's snapshot").await;

    fixture
        .age_supersessions(RETENTION + Duration::from_secs(60))
        .await;
    let head_files = snapshot_files(fixture, head).await;
    let (_, after) = maintain_once(&table).await;
    assert_eq!(
        after.snapshots,
        BTreeSet::from([head]),
        "the first pass after the window expires every replaced snapshot"
    );
    assert_eq!(
        expiry_attempts(fixture).await,
        1,
        "the unchanged table's first past-window pass plans one expiry task"
    );
    let cleanup = latest_cleanup_task(fixture).await;
    assert_eq!(cleanup.state, "succeeded", "the handed-off cleanup drained");
    for path in &cleanup.paths {
        assert!(
            !object_exists(fixture, path).await,
            "the drained cleanup deleted {path}"
        );
    }
    assert_files_exist(fixture, &head_files, "the retained head").await;
    assert_ne!(
        cut.metadata_location,
        fixture.read_cut().await.metadata_location,
        "the held cut named a replaced pointer"
    );
    table.supervised.shutdown().await;
}

/// Proves a cut read never waits on a live snapshot-expiry authority.
///
/// The expiry is parked inside its catalog commit while it holds the table's
/// exclusive maintenance authority. A cut read started then completes at once
/// with the pointer the expiry is replacing; after the commit is released, the
/// next read names the committed pointer.
///
/// # Panics
///
/// Panics when the cut read waits on the authority, the expiry does not
/// commit, or the later read does not observe its pointer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cut_read_completes_while_snapshot_expiry_holds_the_authority() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let ExpirableTable {
        fixture,
        seam,
        supervised,
        watermark,
        ..
    } = expirable_table("expiry_reader_race").await;
    let forge = supervised.forge();
    let replaced = fixture.read_cut().await.metadata_location;
    let (attempt, worker) = (Uuid::now_v7(), Uuid::now_v7());
    let task = seed_running_task(&fixture, fixture.tenant, attempt, worker, watermark, "22").await;
    seam.park_next_commit();
    let expiry = {
        let forge = Arc::clone(&forge);
        let binding = fixture.binding.clone();
        tokio::spawn(async move {
            forge
                .run_snapshot_expiry_for_test(&binding, task, attempt, worker)
                .await
        })
    };
    tokio::time::timeout(EFFECT_BOUND, seam.wait_for_parked_commit())
        .await
        .expect("the expiry reaches its catalog commit under the held authority");

    let raced = tokio::time::timeout(EFFECT_BOUND, fixture.read_cut())
        .await
        .expect("a cut read never waits on the expiry's authority");
    assert_eq!(
        raced.metadata_location, replaced,
        "the raced read observes the pointer still current"
    );
    seam.release_parked_commit();
    expiry
        .await
        .expect("the expiry task joins")
        .expect("the corroborated expiration commits")
        .expect("the committed expiration settles its own task");
    assert_ne!(
        fixture.read_cut().await.metadata_location,
        replaced,
        "a read after the commit observes the expiry's pointer"
    );
    supervised.shutdown().await;
}

/// Proves a cut read never waits on a live orphan delete.
///
/// Orphan cleanup is paused inside its object delete while it holds the
/// table's exclusive maintenance authority; a cut read started then completes
/// at once.
///
/// # Panics
///
/// Panics when the cut read waits on the authority or the orphan pass fails.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cut_read_completes_while_orphan_delete_holds_the_authority() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let ExpirableTable {
        fixture,
        store,
        supervised,
        ..
    } = expirable_table("orphan_reader_race").await;
    let forge = supervised.forge();
    seed_never_published_object(&fixture).await;
    store.pause_delete_at(1);
    let collection = {
        let forge = Arc::clone(&forge);
        let binding = fixture.binding.clone();
        tokio::spawn(async move { forge.run_orphan_gc_report_for_test(&binding).await })
    };
    tokio::time::timeout(EFFECT_BOUND, store.delete_paused())
        .await
        .expect("orphan cleanup reaches its delete under the held authority");

    tokio::time::timeout(EFFECT_BOUND, fixture.read_cut())
        .await
        .expect("a cut read never waits on orphan cleanup's authority");
    store.release_delete();
    let report = collection
        .await
        .expect("the orphan task joins")
        .expect("orphan cleanup runs");
    assert!(
        report.deleted >= 1,
        "the paused delete completed: {report:?}"
    );
    supervised.shutdown().await;
}
