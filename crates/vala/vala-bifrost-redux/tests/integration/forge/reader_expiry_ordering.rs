//! Tier-2 coverage for the ordering between Oracle active table reads and
//! Forge destructive maintenance.
//!
//! Cut acquisition and every destructive preparation take the same
//! `vala.bifrost_table_maintenance_authority` row, so an active read is
//! observed by snapshot expiration, expired-object cleanup, and orphan
//! cleanup alike. Nothing waits for snapshot age: the first maintenance pass
//! after the table's last reader releases may destroy what it replaced.

use std::collections::BTreeSet;

use chrono::Duration as ChronoDuration;

use super::snapshot_expiration::{
    assert_files_exist, commit_snapshot, latest_cleanup_task, leader_only_table, maintain_once,
    object_exists, seed_never_published_object, snapshot_files,
};
use super::support::ForgeTelemetryCheckpoint;

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
        assert_eq!(report.deleted, 0, "orphan cleanup deletes nothing while {why} is held");
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
    assert_eq!(report.deleted, 1, "orphan cleanup deletes exactly the orphan");
    assert!(
        !object_exists(fixture, &orphan).await,
        "the orphan is gone after the last reader"
    );
    table.supervised.shutdown().await;
}
