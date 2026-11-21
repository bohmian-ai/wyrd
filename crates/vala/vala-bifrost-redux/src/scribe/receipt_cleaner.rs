//! Background deletion of retry receipts and the cleanup health that gates
//! fresh write admission.
//!
//! One cleaner per Scribe process deletes receipts this node wrote, across
//! every writer epoch, once they are outside recent retry history and covered
//! by their shard's persisted safe frontier. Admission refuses fresh writes as
//! retryable busy unless a whole sweep completed recently, so failed, hung or
//! lagging cleanup pauses acceptance instead of letting receipts pile up.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use vala_sql::queries::scribe_batch_commits::{
    HandedOffShard, ReceiptStream, RetryHistory, delete_handed_off, ingest_instant, receipt_streams,
};

use crate::contracts::ScribeError;
use crate::scribe::committed_batches::CommittedBatches;
use crate::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};

/// Interval between scheduled sweeps when nothing wakes the cleaner sooner.
const CADENCE: Duration = Duration::from_secs(30);

/// Pause before retrying a failed sweep.
const RETRY: Duration = Duration::from_secs(1);

/// Oldest sweep boundary admission still accepts as current coverage.
///
/// Four scheduled sweeps: a sweep that cannot finish within this of its own
/// start leaves admission busy, however busy the cleaner looks meanwhile.
const COVERAGE_LIMIT: Duration = Duration::from_secs(120);

/// Receipts one DELETE removes at most.
const BATCH: i64 = 1_000;

/// Whether receipt cleanup is current enough to admit fresh writes.
///
/// Holds the start instant of the newest sweep that completed every due
/// portion of its scope. Startup and every failure clear it, so a process
/// starts cleanup-unverified and a failed or ambiguous sweep refuses at once;
/// a hung or exited cleaner refuses once that boundary ages past
/// [`COVERAGE_LIMIT`].
#[derive(Debug, Default)]
pub(crate) struct CleanupHealth {
    /// Start of the newest completed sweep, if one completed since the last
    /// failure.
    covered: Mutex<Option<Instant>>,
}

impl CleanupHealth {
    /// Whether a sweep started within [`COVERAGE_LIMIT`] of `now` completed.
    ///
    /// A local check only: it runs on every admission and touches no SQL.
    pub(crate) fn admits(&self, now: Instant) -> bool {
        self.lock()
            .is_some_and(|boundary| now.saturating_duration_since(boundary) <= COVERAGE_LIMIT)
    }

    /// Records that the sweep started at `boundary` completed. Coverage never
    /// moves backwards, so an old sweep finishing late renews nothing.
    pub(crate) fn covered(&self, boundary: Instant) {
        let mut covered = self.lock();
        if covered.is_none_or(|current| boundary > current) {
            *covered = Some(boundary);
        }
    }

    /// Records a failed or ambiguous sweep; admission refuses until a later
    /// sweep completes.
    pub(crate) fn failed(&self) {
        *self.lock() = None;
    }

    /// Takes the lock, recovering it if a holder panicked; every update leaves
    /// one consistent value.
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Instant>> {
        self.covered
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The one background receipt cleaner of a Scribe process.
///
/// Its scope is rebuilt from durable state on every sweep: the receipts
/// PostgreSQL holds for this node and each shard's persisted safe frontier,
/// so a missed wake or a restart loses nothing.
pub(crate) struct ReceiptCleaner {
    /// Cross-tenant session that lists this node's receipt streams.
    operator: vala_sql::OperatorPool,
    /// Tenant-scoped pool the deletions run on.
    postgres: Arc<vala_sql::ValaPostgres>,
    /// WAL root holding every epoch's shard manifests.
    wal_root: PathBuf,
    /// Stable node whose receipts this cleaner owns.
    node: NodeId,
    /// This pod's committed-batch mirror, which must forget deleted batches.
    committed: Arc<CommittedBatches>,
    /// Health admission reads.
    health: Arc<CleanupHealth>,
    /// Coalesced request for an early sweep.
    wake: Arc<Notify>,
    /// Stops the background loop at shutdown.
    cancel: CancellationToken,
    /// Whether the background loop was started.
    started: AtomicBool,
    /// Receipts one DELETE removes at most.
    batch: i64,
    /// Held for the whole of one sweep.
    sweeping: tokio::sync::Mutex<()>,
}

impl ReceiptCleaner {
    /// Builds the cleaner for `node`, sharing `health` with admission and
    /// `wake` with the WAL frontier owner.
    pub(crate) fn new(
        operator: vala_sql::OperatorPool,
        postgres: Arc<vala_sql::ValaPostgres>,
        wal_root: PathBuf,
        node: NodeId,
        committed: Arc<CommittedBatches>,
        health: Arc<CleanupHealth>,
        wake: Arc<Notify>,
    ) -> Self {
        Self {
            operator,
            postgres,
            wal_root,
            node,
            committed,
            health,
            wake,
            cancel: CancellationToken::new(),
            started: AtomicBool::new(false),
            batch: BATCH,
            sweeping: tokio::sync::Mutex::new(()),
        }
    }

    /// Runs one sweep and publishes its outcome to cleanup health.
    ///
    /// The startup sweep and the background loop share this entry point and
    /// never overlap.
    ///
    /// # Errors
    ///
    /// Returns the sweep's error after marking cleanup unhealthy.
    pub(crate) async fn sweep_and_report(&self) -> Result<(), ScribeError> {
        // Sweeps run one at a time, so each outcome is published in boundary
        // order and an older success cannot overwrite a newer failure.
        let _sweeping = self.sweeping.lock().await;
        let boundary = Instant::now();
        match self.sweep().await {
            Ok(deleted) => {
                self.health.covered(boundary);
                tracing::debug!(deleted, node = %self.node, "Scribe receipt sweep completed");
                Ok(())
            }
            Err(error) => {
                self.health.failed();
                tracing::warn!(error = %error, node = %self.node, "Scribe receipt sweep failed");
                Err(error)
            }
        }
    }

    /// Deletes every receipt of this node that is due at the sweep's cutoff,
    /// returning how many went.
    ///
    /// The cutoff is PostgreSQL time read once, so receipts accepted later
    /// belong to later sweeps and the sweep is finite. A stream without a
    /// persisted frontier is recovery-pinned and skipped. Deleted batches
    /// leave the committed-batch mirror before their deletion commits, so an
    /// ambiguous commit can only make the mirror forget too much.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when discovery, a manifest read, a deletion or
    /// its commit fails.
    async fn sweep(&self) -> Result<usize, ScribeError> {
        let streams = receipt_streams(&self.operator, self.node.as_uuid()).await?;
        let Some(first) = streams.first() else {
            return Ok(0);
        };
        let cutoff = {
            let mut conn = self.postgres.tenant_conn(first.tenant).await?;
            ingest_instant(&mut conn).await?
        };
        let frontiers = self.frontiers(&streams).await?;
        let history = RetryHistory::default();
        let mut deleted = 0;
        for (stream, safe_segment) in streams.iter().zip(frontiers) {
            let Some(safe_segment) = safe_segment else {
                continue;
            };
            let shard = HandedOffShard {
                node_id: self.node.as_uuid(),
                writer_epoch: stream.writer_epoch,
                shard_id: stream.shard_id,
                safe_segment: i64::try_from(safe_segment).unwrap_or(i64::MAX),
            };
            loop {
                let mut conn = self.postgres.tenant_conn(stream.tenant).await?;
                let batch_ids =
                    delete_handed_off(&mut conn, &shard, &history, cutoff, self.batch).await?;
                self.committed.forget(stream.tenant, &batch_ids);
                conn.commit().await?;
                deleted += batch_ids.len();
                if i64::try_from(batch_ids.len()).unwrap_or(i64::MAX) < self.batch {
                    break;
                }
            }
        }
        Ok(deleted)
    }

    /// Reads each stream's persisted safe frontier, `None` for a stream with
    /// none yet, off the async workers.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a shard id is out of range, a
    /// manifest cannot be read, or the blocking read panics.
    async fn frontiers(&self, streams: &[ReceiptStream]) -> Result<Vec<Option<u64>>, ScribeError> {
        let manifests = streams
            .iter()
            .map(|stream| {
                let shard_id =
                    u8::try_from(stream.shard_id).map_err(|_| ScribeError::Internal {
                        detail: format!("receipt shard {} is out of range", stream.shard_id),
                    })?;
                Ok(crate::scribe::replay::shard_manifest_path(
                    &self.wal_root,
                    StreamIdentity::new(self.node, WriterEpoch::new(stream.writer_epoch)),
                    shard_id,
                ))
            })
            .collect::<Result<Vec<_>, ScribeError>>()?;
        tokio::task::spawn_blocking(move || {
            manifests
                .iter()
                .map(|manifest| {
                    Ok(crate::scribe::manifest::read_manifest(manifest)?
                        .and_then(|manifest| manifest.safe_segment))
                })
                .collect()
        })
        .await
        .map_err(|error| ScribeError::Internal {
            detail: format!("receipt frontier read failed: {error}"),
        })?
    }

    /// Starts the background loop once on `runtime`.
    ///
    /// Each pass sweeps, then waits for a wake, the cadence, cancellation, or
    /// after a failure the shorter retry pause.
    pub(crate) fn start(self: &Arc<Self>, runtime: &tokio::runtime::Handle) {
        if self.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let cleaner = Arc::clone(self);
        runtime.spawn(async move {
            loop {
                let pause = if cleaner.sweep_and_report().await.is_ok() {
                    CADENCE
                } else {
                    RETRY
                };
                tokio::select! {
                    () = cleaner.cancel.cancelled() => break,
                    () = cleaner.wake.notified() => {}
                    () = tokio::time::sleep(pause) => {}
                }
            }
        });
    }

    /// Stops the background loop; a sweep in progress ends at its next await.
    pub(crate) fn stop(&self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Admission needs a completed sweep that started recently: none at
    /// startup, none after a failure, none once the newest boundary ages past
    /// the limit, and a late-finishing old sweep cannot renew it.
    #[test]
    fn cleanup_health_admits_only_a_fresh_completed_sweep() {
        let health = CleanupHealth::default();
        let start = Instant::now();
        assert!(!health.admits(start), "startup is cleanup-unverified");

        health.covered(start);
        assert!(health.admits(start + COVERAGE_LIMIT));
        assert!(
            !health.admits(start + COVERAGE_LIMIT + Duration::from_secs(1)),
            "a hung or exited cleaner goes stale"
        );

        health.covered(start + Duration::from_secs(10));
        health.covered(start);
        assert!(
            !health.admits(start + COVERAGE_LIMIT + Duration::from_secs(11)),
            "an older sweep finishing late does not renew coverage"
        );
        assert!(health.admits(start + COVERAGE_LIMIT + Duration::from_secs(10)));

        health.failed();
        assert!(
            !health.admits(start + Duration::from_secs(10)),
            "failure refuses at once"
        );
    }
}

#[cfg(test)]
mod pg_tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;
    use vala_sql::queries::scribe_batch_commits::{ScribeBatchCommit, committed_batches, record};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;

    /// One receipt of `node`'s stream `(epoch, shard)` at `segment`.
    fn receipt(
        tenant: DataTenantId,
        node: NodeId,
        epoch: i64,
        shard: i16,
        segment: i64,
    ) -> ScribeBatchCommit {
        ScribeBatchCommit {
            tenant,
            logical_table_fqn: "bifrost.cleaner".to_owned(),
            batch_id: Uuid::now_v7(),
            table_version: wyrd_spec::vala::ids::TableVersion::INITIAL,
            slice_set_digest: [1; 32],
            slice_count: 1,
            wal_node_id: node.as_uuid(),
            wal_writer_epoch: epoch,
            wal_shard_id: shard,
            wal_segment_sequence: segment,
            wal_lsn_min: segment * 10,
            wal_lsn_max: segment * 10 + 1,
            request_id: Uuid::now_v7(),
        }
    }

    /// Records `receipts` two hours old, outside age history.
    async fn record_old(fixture: &PgFixture, receipts: &[ScribeBatchCommit]) {
        let tenant = receipts.first().expect("one receipt").tenant;
        let old = Utc::now() - chrono::Duration::hours(2);
        let offered = receipts
            .iter()
            .map(|receipt| (receipt.clone(), old))
            .collect::<Vec<_>>();
        let mut conn = fixture
            .vala_postgres()
            .tenant_conn(tenant)
            .await
            .expect("tenant connection");
        record(&mut conn, &offered).await.expect("receipts record");
        conn.commit().await.expect("receipts commit");
    }

    /// Batch ids among `receipts` that still hold a receipt.
    async fn kept(fixture: &PgFixture, receipts: &[ScribeBatchCommit]) -> Vec<Uuid> {
        let tenant = receipts.first().expect("one receipt").tenant;
        let mut conn = fixture
            .vala_postgres()
            .tenant_conn(tenant)
            .await
            .expect("tenant connection");
        let mut kept = committed_batches(
            &mut conn,
            "bifrost.cleaner",
            &receipts
                .iter()
                .map(|receipt| receipt.batch_id)
                .collect::<Vec<_>>(),
        )
        .await
        .expect("receipts read");
        kept.sort();
        kept
    }

    /// Persists frontier `segment` for `stream`'s `shard` under `wal_root`.
    fn frontier(wal_root: &std::path::Path, stream: StreamIdentity, shard: u8, segment: u64) {
        let path = crate::scribe::replay::shard_manifest_path(wal_root, stream, shard);
        std::fs::create_dir_all(path.parent().expect("shard directory")).expect("shard directory");
        crate::scribe::manifest::advance_safe_segment(&path, stream, segment).expect("frontier");
    }

    /// Builds a cleaner over `fixture` for `node` with `batch`-sized DELETEs.
    fn cleaner(
        fixture: &PgFixture,
        wal_root: &std::path::Path,
        node: NodeId,
        committed: &Arc<CommittedBatches>,
        batch: i64,
    ) -> ReceiptCleaner {
        ReceiptCleaner {
            batch,
            ..ReceiptCleaner::new(
                fixture.operator_pool().clone(),
                Arc::new(fixture.vala_postgres().clone()),
                wal_root.to_path_buf(),
                node,
                Arc::clone(committed),
                Arc::new(CleanupHealth::default()),
                Arc::new(Notify::new()),
            )
        }
    }

    /// One sweep deletes every due receipt of the node, across tenants and
    /// writer epochs, in bounded batches, and then reports healthy cleanup.
    ///
    /// Tenant A holds five covered receipts of an earlier epoch, more than two
    /// DELETE batches, and one above its frontier. Tenant B holds one covered
    /// receipt of the current epoch and one in a shard with no frontier.
    /// Another node's covered receipt is not this cleaner's. Every covered
    /// receipt goes and leaves the committed-batch mirror; the pinned ones
    /// stay and do not fail the sweep.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start, the sweep fails, or the kept
    /// receipts, mirror or health differ from the rule.
    #[tokio::test]
    async fn a_sweep_deletes_every_due_receipt_of_the_node_and_reports_healthy() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant_a = fixture.data_tenant_id();
        let tenant_b = fixture
            .seed_additional_tenant("receipt-cleaner-b")
            .await
            .expect("second tenant");
        let wal_root = tempfile::tempdir().expect("WAL root");
        let node = NodeId::generate();
        let other_node = NodeId::generate();
        for (stream_node, epoch, shard) in [(node, 1, 0), (node, 2, 1), (other_node, 1, 0)] {
            frontier(
                wal_root.path(),
                StreamIdentity::new(stream_node, WriterEpoch::new(epoch)),
                shard,
                3,
            );
        }
        let a_covered = (0..5)
            .map(|_| receipt(tenant_a, node, 1, 0, 2))
            .collect::<Vec<_>>();
        let a_pinned = receipt(tenant_a, node, 1, 0, 4);
        let a_foreign = receipt(tenant_a, other_node, 1, 0, 1);
        let b_covered = receipt(tenant_b, node, 2, 1, 3);
        let b_unframed = receipt(tenant_b, node, 2, 5, 0);
        let a_all = [a_covered.clone(), vec![a_pinned.clone(), a_foreign.clone()]].concat();
        let b_all = vec![b_covered.clone(), b_unframed.clone()];
        record_old(&fixture, &a_all).await;
        record_old(&fixture, &b_all).await;
        let committed = Arc::new(CommittedBatches::default());
        let mirrored = crate::scribe::committed_batches::CommittedBatch {
            table_fqn: "bifrost.cleaner".to_owned(),
            table_version: wyrd_spec::vala::ids::TableVersion::INITIAL,
            logical_digest: [1; 32],
            slice_count: 1,
        };
        committed.record(tenant_a, a_covered[0].batch_id, mirrored.clone());
        committed.record(tenant_a, a_pinned.batch_id, mirrored);
        let cleaner = cleaner(&fixture, wal_root.path(), node, &committed, 2);
        assert!(
            !cleaner.health.admits(Instant::now()),
            "startup is unverified"
        );

        cleaner
            .sweep_and_report()
            .await
            .expect("the sweep completes");

        let mut a_left = vec![a_pinned.batch_id, a_foreign.batch_id];
        a_left.sort();
        assert_eq!(kept(&fixture, &a_all).await, a_left);
        assert_eq!(kept(&fixture, &b_all).await, [b_unframed.batch_id]);
        assert!(
            !committed.knows(tenant_a, a_covered[0].batch_id),
            "deleted batches are forgotten"
        );
        assert!(
            committed.knows(tenant_a, a_pinned.batch_id),
            "kept batches stay mirrored"
        );
        assert!(
            cleaner.health.admits(Instant::now()),
            "a completed sweep is healthy"
        );
    }

    /// A failed sweep refuses admission at once, a sweep with nothing due
    /// succeeds, and a later complete sweep restores health.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start, the injected failure does not
    /// fail the sweep, or health does not follow the sweep outcomes.
    #[tokio::test]
    async fn a_failed_sweep_is_unhealthy_until_a_later_sweep_completes() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let wal_root = tempfile::tempdir().expect("WAL root");
        let node = NodeId::generate();
        let committed = Arc::new(CommittedBatches::default());
        let cleaner = cleaner(&fixture, wal_root.path(), node, &committed, 100);
        cleaner
            .sweep_and_report()
            .await
            .expect("an empty scope completes");
        assert!(cleaner.health.admits(Instant::now()));

        let stream = StreamIdentity::new(node, WriterEpoch::new(1));
        frontier(wal_root.path(), stream, 0, 3);
        let due = receipt(tenant, node, 1, 0, 1);
        record_old(&fixture, std::slice::from_ref(&due)).await;
        let superuser = fixture.superuser_pool().expect("superuser pool");
        sqlx::query("REVOKE DELETE ON vala.scribe_batch_commits FROM wyrd_tenant")
            .execute(&superuser)
            .await
            .expect("deletion is revoked");
        cleaner
            .sweep_and_report()
            .await
            .expect_err("the sweep cannot delete");
        assert!(
            !cleaner.health.admits(Instant::now()),
            "failure refuses at once"
        );

        sqlx::query("GRANT DELETE ON vala.scribe_batch_commits TO wyrd_tenant")
            .execute(&superuser)
            .await
            .expect("deletion is granted again");
        cleaner
            .sweep_and_report()
            .await
            .expect("the sweep catches up");
        assert!(cleaner.health.admits(Instant::now()));
        assert!(kept(&fixture, std::slice::from_ref(&due)).await.is_empty());
    }
}
