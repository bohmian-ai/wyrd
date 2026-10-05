//! Tier-2 coverage for the ordering between Oracle active table reads and
//! Forge destructive maintenance.
//!
//! Cut acquisition and every destructive effect take the same
//! `vala.bifrost_table_maintenance_authority` row, and each destructive effect
//! holds it exclusively through its known outcome, so an active read is
//! observed by snapshot expiration, expired-object cleanup, and orphan
//! cleanup alike, and a racing reader either commits first or observes the
//! finished effect. Nothing waits for snapshot age: the first maintenance pass
//! after the table's last reader releases may destroy what it replaced.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Duration as ChronoDuration;
use uuid::Uuid;
use vala_sql::queries::oracle_reader_authority::ActiveReadOwner;

use super::snapshot_expiration::{
    ExpirableTable, assert_files_exist, commit_snapshot, expirable_table, latest_cleanup_task,
    leader_only_table, maintain_once, object_exists, seed_never_published_object,
    seed_running_task, snapshot_files,
};
use super::support::{ForgeTelemetryCheckpoint, PromotionIntegrationFixture};

/// Bound on every wait for a destructive owner to reach its paused effect.
const EFFECT_BOUND: Duration = Duration::from_secs(30);

/// Proves the table's last active reader, not snapshot age, gates destruction.
///
/// Two queries hold active reads while two commits replace every older
/// snapshot and an aged never-published object waits for orphan cleanup.
/// While either read is held, the leader's maintenance pass expires nothing
/// and orphan cleanup deletes nothing. Releasing the first reader changes
/// nothing; releasing the last lets the very next pass expire every replaced
/// snapshot without any clock advance, drain the expired-object cleanup it
/// handed off, and keep the head's files. Orphan cleanup then deletes the
/// orphan.
///
/// # Panics
///
/// Panics when anything is destroyed while a read is held, a replaced
/// snapshot or the orphan survives after release, or the head loses a file.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn last_table_reader_controls_destructive_cleanup() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let table = leader_only_table("last_table_reader").await;
    let fixture = &table.fixture;
    let readers = [
        fixture.hold_active_read().await,
        fixture.hold_active_read().await,
    ];
    let replaced = table.watermark.0;
    let replaced_files = snapshot_files(fixture, replaced).await;
    commit_snapshot(&table).await;
    let head = commit_snapshot(&table).await;
    // Only orphan collection has an age rule (its object TTL), so only the
    // orphan needs the Forge clock moved; snapshot expiry never reads it.
    let orphan = seed_never_published_object(fixture).await;
    table
        .control
        .advance(ChronoDuration::hours(48))
        .expect("manual clock advance");
    let forge = table.supervised.forge();
    let deletes_before = table.store.deletes();

    for (released, why) in [(None, "two reads"), (Some(readers[0]), "the last read")] {
        if let Some(query_id) = released {
            fixture.release_active_read(query_id).await;
        }
        let (before, after) = maintain_once(&table).await;
        assert_eq!(
            after.snapshots, before.snapshots,
            "no snapshot expires while {why} is held"
        );
        let report = forge
            .run_orphan_gc_report_for_test(&fixture.binding)
            .await
            .expect("orphan cleanup runs and refuses");
        assert_eq!(
            report.deleted, 0,
            "orphan cleanup deletes nothing while {why} is held"
        );
        assert!(
            object_exists(fixture, &orphan).await,
            "the orphan survives while {why} is held"
        );
        assert_eq!(
            table.store.deletes(),
            deletes_before,
            "no object is deleted while {why} is held"
        );
        assert_files_exist(fixture, &replaced_files, "a replaced snapshot under read").await;
    }

    fixture.release_active_read(readers[1]).await;
    let head_files = snapshot_files(fixture, head).await;
    let (_, after) = maintain_once(&table).await;
    assert_eq!(
        after.snapshots,
        BTreeSet::from([head]),
        "the first pass after the last reader expires every replaced snapshot"
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
    let report = forge
        .run_orphan_gc_report_for_test(&fixture.binding)
        .await
        .expect("orphan cleanup runs");
    assert_eq!(
        report.deleted, 1,
        "orphan cleanup deletes exactly the orphan"
    );
    assert!(
        !object_exists(fixture, &orphan).await,
        "the orphan is gone after the last reader"
    );
    table.supervised.shutdown().await;
}

/// Proves a reader racing a live snapshot-expiry authority observes its commit.
///
/// The expiry is parked inside its catalog commit while it holds the table's
/// exclusive maintenance authority. A cut acquisition started then blocks on
/// that authority in `PostgreSQL` rather than reading the old pointer; once the
/// commit is released and the authority yields, the reader's cut names the
/// pointer the expiry committed, never the one it replaced.
///
/// # Panics
///
/// Panics when the reader commits while the authority is live, the expiry
/// does not commit, or the reader observes the replaced pointer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reader_racing_snapshot_expiry_observes_the_committed_pointer() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let ExpirableTable {
        fixture,
        seam,
        supervised,
        watermark,
        ..
    } = expirable_table("expiry_reader_race").await;
    let forge = supervised.forge();
    let replaced = current_pointer(&fixture).await;
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

    let (reader, acquisition) = fixture.spawn_active_read();
    fixture.await_blocked_cut_acquisition().await;
    assert!(
        !acquisition.is_finished(),
        "no reader commits while the expiry's exclusive authority is live"
    );
    seam.release_parked_commit();
    expiry
        .await
        .expect("the expiry task joins")
        .expect("the corroborated expiration commits")
        .expect("the committed expiration settles its own task");
    let observed = acquisition.await.expect("the raced acquisition joins");
    assert_ne!(
        observed.metadata_location, replaced,
        "the raced reader never observes the pointer the expiry replaced"
    );
    fixture.release_active_read(reader).await;
    assert_eq!(
        observed.metadata_location,
        current_pointer(&fixture).await,
        "the raced reader observes exactly the pointer the expiry committed"
    );
    supervised.shutdown().await;
}

/// Proves a reader racing a live orphan delete commits only after it.
///
/// Orphan cleanup is paused inside its object delete while it holds the
/// table's exclusive maintenance authority. A cut acquisition started then
/// blocks on that authority; it commits only after the delete's outcome is
/// known and the authority yields, by which time the orphan is gone.
///
/// # Panics
///
/// Panics when the reader commits while the authority is live, the orphan
/// pass fails, or the orphan survives the reader's commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reader_racing_orphan_delete_commits_after_it() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let ExpirableTable {
        fixture,
        store,
        supervised,
        ..
    } = expirable_table("orphan_reader_race").await;
    let forge = supervised.forge();
    let orphan = seed_never_published_object(&fixture).await;
    store.pause_delete_at(1);
    let collection = {
        let forge = Arc::clone(&forge);
        let binding = fixture.binding.clone();
        tokio::spawn(async move { forge.run_orphan_gc_report_for_test(&binding).await })
    };
    tokio::time::timeout(EFFECT_BOUND, store.delete_paused())
        .await
        .expect("orphan cleanup reaches its delete under the held authority");

    let (reader, acquisition) = fixture.spawn_active_read();
    fixture.await_blocked_cut_acquisition().await;
    assert!(
        !acquisition.is_finished(),
        "no reader commits while orphan cleanup's exclusive authority is live"
    );
    store.release_delete();
    acquisition.await.expect("the raced acquisition joins");
    assert!(
        !object_exists(&fixture, &orphan).await,
        "the raced reader committed only after the orphan delete finished"
    );
    fixture.release_active_read(reader).await;
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

/// Proves every acquisition expires at the query's one original deadline.
///
/// The deadline is fixed against `PostgreSQL`'s clock, then the first
/// acquisition is delayed and a replayed acquisition for the same query is
/// delayed again. Each stamps `abandon_after` at that original deadline in
/// `PostgreSQL` time instead of rebasing it by the delay, and an acquisition
/// started with no time left records nothing.
///
/// # Panics
///
/// Panics when an acquisition fails, `abandon_after` drifts from the original
/// deadline, or an expired deadline still records a read.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delayed_and_replayed_acquisitions_expire_at_the_original_deadline() {
    let _telemetry = ForgeTelemetryCheckpoint::install();
    let fixture = PromotionIntegrationFixture::start("deadline_replay").await;
    let budget = Duration::from_hours(1);
    let delay = Duration::from_secs(2);
    let pool = fixture.operator_pool.pool();
    let origin: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .expect("PostgreSQL clock");
    let deadline = Instant::now() + budget;
    let expected = origin + ChronoDuration::from_std(budget).expect("budget fits");
    let owner = ActiveReadOwner {
        query_id: Uuid::now_v7(),
        node_id: Uuid::now_v7(),
        fencing_token: 1,
    };
    let tables = std::slice::from_ref(&fixture.binding.table_ref);
    for attempt in ["the delayed acquisition", "the delayed replay"] {
        // The delay is the scenario under test, not synchronization.
        tokio::time::sleep(delay).await;
        fixture
            .catalog
            .acquire_active_cut(fixture.tenant, owner, deadline, tables)
            .await
            .expect("the registered table acquires")
            .expect("most of the hour remains");
        let abandon_after: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT abandon_after FROM vala.oracle_active_table_reads WHERE query_id = $1",
        )
        .bind(owner.query_id)
        .fetch_one(pool)
        .await
        .expect("one active read row");
        let drift = (abandon_after - expected).abs();
        assert!(
            drift < ChronoDuration::milliseconds(750),
            "{attempt} expires at the original deadline, drifting {drift} from {expected}"
        );
    }
    fixture.release_active_read(owner.query_id).await;

    let late = ActiveReadOwner {
        query_id: Uuid::now_v7(),
        ..owner
    };
    assert!(
        fixture
            .catalog
            .acquire_active_cut(fixture.tenant, late, Instant::now(), tables)
            .await
            .expect("an exhausted deadline is not a catalog failure")
            .is_none(),
        "no acquisition starts once the deadline has passed"
    );
    let recorded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.oracle_active_table_reads WHERE query_id = $1",
    )
    .bind(late.query_id)
    .fetch_one(pool)
    .await
    .expect("active read count");
    assert_eq!(recorded, 0, "an exhausted deadline records no active read");
}

/// Reads the fixture table's current catalog pointer through a production
/// cut acquisition, releasing it at once.
///
/// # Panics
///
/// Panics when the acquisition or its release fails.
async fn current_pointer(fixture: &PromotionIntegrationFixture) -> String {
    let (query_id, acquisition) = fixture.spawn_active_read();
    let cut = acquisition.await.expect("the probe acquisition joins");
    fixture.release_active_read(query_id).await;
    cut.metadata_location
}
