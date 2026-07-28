//! Replay the real Forge tick through bounded phase schedules.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use opendal::{Buffer, Entry, Metadata};
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::ForgeObjectStore;
use vala_bifrost_redux::maintenance::{StagingFileCommitted, StagingPublishOutcome};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{
    CommitUncertaintyCatalog, ForgeObjectStoreControl, seed_forge_group,
    seed_forge_group_for_tenant_with_schema, seed_forge_group_for_tenant_with_schema_and_days,
};

/// Pauses the rewrite fence boundary so a test can take over the lease before
/// the production staging PUT is attempted.
#[derive(Debug)]
struct OutputPutBarrier {
    /// Real object-store seam used for all source reads and cleanup deletes.
    inner: Arc<ForgeObjectStoreControl>,
    /// Signals that the next output reached the pre-fence boundary.
    reached: tokio::sync::Notify,
    /// Retains the reached signal for waiters that start after the callback.
    reached_flag: AtomicBool,
    /// Counts output boundaries so the first output boundary is paused.
    calls: std::sync::atomic::AtomicUsize,
    /// Releases the paused pre-fence boundary.
    release: tokio::sync::Notify,
}

impl OutputPutBarrier {
    /// Wrap a real Forge object-store control with one output barrier.
    fn new(inner: Arc<ForgeObjectStoreControl>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            reached: tokio::sync::Notify::new(),
            reached_flag: AtomicBool::new(false),
            calls: std::sync::atomic::AtomicUsize::new(0),
            release: tokio::sync::Notify::new(),
        })
    }

    /// Wait until the stale rewrite reaches its first output boundary.
    async fn wait_until_reached(&self) {
        while !self.reached_flag.load(Ordering::Acquire) {
            self.reached.notified().await;
        }
    }

    /// Resume the stale rewrite after the successor acquires the lease.
    fn release(&self) {
        self.release.notify_waiters();
    }
}

#[async_trait]
impl vala_bifrost_redux::forge::ForgeObjectStore for OutputPutBarrier {
    async fn before_output_put(&self, _path: &str) -> opendal::Result<()> {
        if self.calls.fetch_add(1, Ordering::AcqRel) != 0 {
            return Ok(());
        }
        self.reached_flag.store(true, Ordering::Release);
        self.reached.notify_waiters();
        self.release.notified().await;
        Ok(())
    }

    async fn read(&self, path: &str) -> opendal::Result<Buffer> {
        self.inner.read(path).await
    }

    async fn read_range(&self, path: &str, range: std::ops::Range<u64>) -> opendal::Result<Buffer> {
        self.inner.read_range(path, range).await
    }

    async fn list(&self, prefix: &str) -> opendal::Result<Vec<Entry>> {
        self.inner.list(prefix).await
    }

    async fn stat(&self, path: &str) -> opendal::Result<Metadata> {
        self.inner.stat(path).await
    }

    async fn delete(&self, path: &str) -> opendal::Result<()> {
        self.inner.delete(path).await
    }
}

async fn steal_forge_lease(fixture: &wyrd_testing::bifrost::ForgeFixture) -> (uuid::Uuid, i64) {
    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    sqlx::query("UPDATE vala.maintenance_leases SET expires_at = now() - interval '1 second' WHERE lease_key = $1")
        .bind(&lease_key)
        .execute(fixture.operator_pool.pool())
        .await
        .expect("expire Forge lease for deterministic takeover");
    let owner = uuid::Uuid::now_v7();
    let token = vala_sql::queries::maintenance_leases::try_acquire_lease(
        &fixture.operator_pool,
        &lease_key,
        owner,
        i64::try_from(fixture.config.lease_ttl.as_secs()).expect("lease seconds"),
    )
    .await
    .expect("successor lease acquisition")
    .expect("successor owns expired Forge lease");
    (owner, token)
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests replay convergence of compaction bookkeeping using real durable state.
///
/// Steps:
/// 1. Start a bound Wyrd test server and seed a real Iceberg table, staging
///    Parquet files, and aged `vala.file_list` rows with the shared fixture.
/// 2. Replay 24 bounded schedules of one-shot ticks, alternating sequential
///    calls with concurrent calls that contend for the same production lease.
/// 3. Query `vala.audit_outbox` after all replays and assert exactly one terminal
///    compaction event exists.
///
/// The schedules are deliberately driven through the production API, not a fake
/// Forge state machine. One terminal audit row after repeated durable ticks
/// verifies idempotent bookkeeping and no duplicate Iceberg commit under retry.
async fn forge_compaction_pg_iceberg_bookkeeping_matrix_never_duplicates() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "compaction_matrix_rows").await;
    for schedule in 0..24 {
        if schedule % 3 == 0 {
            let (left, right) = tokio::join!(fixture.forge.run_once(), fixture.forge.run_once(),);
            assert!(left.expect("left durable Forge replay").bins_committed <= 1);
            assert!(right.expect("right durable Forge replay").bins_committed <= 1);
        } else {
            assert!(
                fixture
                    .forge
                    .run_once()
                    .await
                    .expect("durable Forge replay")
                    .bins_committed
                    <= 1
            );
        }
    }
    let committed = fixture
        .operation_count("forge.file_compact.committed")
        .await;
    assert_eq!(committed, 1);
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests tenant, physical-table, partition, and schema isolation in one
/// production discovery tick.
///
/// Steps:
/// 1. Start one real Wyrd server, provision a second tenant, and seed two
///    physical tables with different Iceberg schemas and tenant-qualified
///    object prefixes.
/// 2. Run the public one-shot scheduler against both groups.
/// 3. Assert each table has exactly one snapshot, two input rows stamped with
///    that table's snapshot, and an audit resource/path scoped to its tenant.
///
/// The durable catalog, file-list, audit, and object-prefix assertions prove
/// that grouping does not merge tenants, physical tables, partition days, or
/// incompatible schema fingerprints.
async fn forge_compaction_tick_isolates_tenants_tables_and_schemas() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let tenant_a = server.data_tenant_id();
    let tenant_b = server
        .seed_tenant("forge-tenant-b")
        .await
        .expect("second tenant");
    let first = seed_forge_group_for_tenant_with_schema_and_days(
        &server,
        tenant_a,
        "isolation_base",
        false,
        &[
            chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day"),
            chrono::NaiveDate::from_ymd_opt(2026, 7, 15).expect("partition day"),
        ],
    )
    .await;
    let second =
        seed_forge_group_for_tenant_with_schema(&server, tenant_b, "isolation_variant", true).await;
    assert_ne!(first.tenant, second.tenant);
    let discovered_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM vala.file_list WHERE data_tenant_id IN ($1, $2)")
            .bind(first.tenant.as_uuid())
            .bind(second.tenant.as_uuid())
            .fetch_one(first.operator_pool.pool())
            .await
            .expect("isolated discovery rows");
    assert_eq!(discovered_rows, 6);
    let outcome = first
        .forge
        .run_once()
        .await
        .expect("isolated production tick");
    assert_eq!(outcome.bins_committed, 3, "tick outcome: {outcome:?}");

    for fixture in [&first, &second] {
        let state: (i64, i64, Option<i64>) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE compacted), min(committed_snapshot_id) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("isolated file-list state");
        let expected_files = if fixture.tenant == first.tenant { 4 } else { 2 };
        let expected_bins: i64 = if fixture.tenant == first.tenant { 2 } else { 1 };
        assert_eq!(state.0, expected_files);
        assert_eq!(state.1, expected_files);
        assert!(state.2.is_some());
        let table = fixture
            .catalog
            .load_table(&fixture.binding.table_ident())
            .await
            .expect("isolated catalog table");
        assert_eq!(table.metadata().snapshots().len(), expected_bins as usize);
        let audit_count = fixture
            .operation_count("forge.file_compact.committed")
            .await;
        assert_eq!(audit_count, expected_bins);
        let prepared_count = fixture.operation_count("forge.file_compact.prepared").await;
        assert_eq!(prepared_count, expected_bins);
        assert!(
            fixture
                .binding
                .object_prefix
                .contains(&fixture.tenant.to_string())
        );
    }
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests public-tick behavior when a competing worker already owns the table.
///
/// Steps:
/// 1. Start the real bound server and seed a compaction-ready table.
/// 2. Acquire the production lease row directly with a distinct owner/token.
/// 3. Invoke the public tick, assert it reports a skip and zero committed bins,
///    then release the test owner with the fenced SQL release operation.
///
/// The durable outcome proves the public path stops before Iceberg/file-list
/// mutation when the fence is unavailable. Exact mid-commit theft remains a
/// separate fault-injection gap recorded in the closeout ledger.
async fn forge_compaction_lease_contention_fails_closed() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "compaction_lease_rows").await;
    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    let owner = uuid::Uuid::now_v7();
    let token = vala_sql::queries::maintenance_leases::try_acquire_lease(
        &fixture.operator_pool,
        &lease_key,
        owner,
        i64::try_from(fixture.config.lease_ttl.as_secs()).expect("lease seconds"),
    )
    .await
    .expect("lease acquisition")
    .expect("lease token");
    let outcome = fixture.forge.run_once().await.expect("fenced tick");
    assert_eq!(outcome.bins_committed, 0);
    assert!(outcome.tables_skipped > 0);
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            owner,
            token,
        )
        .await
        .expect("lease release")
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests reconciliation after Iceberg commits but the caller receives an
/// injected retryable uncertainty response.
///
/// Steps:
/// 1. Seed a real compaction-ready table and wrap its real Catalog so the next
///    successful `update_table` returns a retryable error after delegation.
/// 2. Run one public tick and then a successor tick against the same durable
///    catalog and SQL state.
/// 3. Assert reconciliation settles one prepared operation, the successor
///    commits no new bin, the catalog contains one snapshot, and the audit
///    table contains one terminal committed/recovered event.
///
/// The test proves the observable retry result is idempotent. It is the durable
/// The post-commit wrapper is scoped to this test and delegates the real
/// Iceberg update, so the assertion covers the production uncertainty path
/// rather than a fake state transition.
async fn forge_compaction_replay_after_commit_is_idempotent() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "compaction_retry_rows").await;
    let uncertain_catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    uncertain_catalog.fail_after_next_commit();
    let context = fixture.context_with_catalog(fixture.config.clone(), uncertain_catalog);
    let first = context.run_once().await.expect("uncertain tick outcome");
    assert_eq!(
        first.tables_failed, 1,
        "uncertain commit must fail its table"
    );
    assert_eq!(first.bins_committed, 0);
    let second = fixture
        .forge
        .run_once()
        .await
        .expect("successor durable tick");
    assert_eq!(second.bins_committed, 0, "successor outcome: {second:?}");
    let committed = fixture
        .operation_count("forge.file_compact.committed")
        .await
        + fixture
            .operation_count("forge.file_compact.recovered")
            .await;
    assert_eq!(committed, 1, "successor outcome: {second:?}");
    let prepared = fixture.operation_count("forge.file_compact.prepared").await;
    assert_eq!(prepared, 1, "uncertain commit must reconcile one operation");
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("committed Iceberg table");
    assert_eq!(table.metadata().snapshots().count(), 1);
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests fencing immediately after a real Iceberg commit and before SQL
/// bookkeeping.
///
/// Steps:
/// 1. Seed real staged Parquet and file-list rows, then wrap the real catalog
///    so it pauses after accepting the Iceberg commit.
/// 2. Take over the durable table lease while the stale worker is paused and
///    resume the wrapper with a retryable stale-fence response.
/// 3. Assert the stale worker leaves the source rows without a committed
///    snapshot and writes no terminal audit row; release the successor lease
///    and run a normal tick to reconcile the committed snapshot.
async fn forge_compaction_lease_theft_after_catalog_commit_fails_closed() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "compaction_fence_rows").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    control.pause_after_commit();
    let context = fixture.context_with_catalog(fixture.config.clone(), control.clone());
    let task = tokio::spawn(async move { context.run_once().await });
    control.wait_for_commit().await;
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    control.reject_paused_commit();
    let outcome = task
        .await
        .expect("stale compaction task")
        .expect("stale tick reports table failure");
    assert_eq!(outcome.tables_failed, 1);
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        0
    );
    assert!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 AND committed_snapshot_id IS NULL",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(&fixture.binding.logical_namespace)
        .bind(&fixture.binding.table_name)
        .fetch_one(fixture.operator_pool.pool())
        .await
        .expect("uncommitted source rows")
        == 2
    );
    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            successor_owner,
            successor_token,
        )
        .await
        .expect("successor release")
    );
    fixture
        .forge
        .run_once()
        .await
        .expect("successor reconciliation");
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.recovered")
            .await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests fencing immediately before a real Iceberg commit.
///
/// The catalog wrapper pauses at the existing `Catalog::update_table` seam,
/// the test takes over the durable lease, and the wrapper rejects the stale
/// commit without delegating it. The source rows remain prepared but no
/// snapshot or terminal bookkeeping is created.
async fn forge_compaction_lease_theft_before_catalog_commit_fails_closed() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "compaction_precommit_fence_rows").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    control.pause_before_commit();
    let context = fixture.context_with_catalog(fixture.config.clone(), control.clone());
    let task = tokio::spawn(async move { context.run_once().await });
    control.wait_for_before_commit().await;
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    control.reject_paused_before_commit();
    let outcome = task
        .await
        .expect("stale precommit task")
        .expect("stale precommit tick reports failure");
    assert_eq!(outcome.tables_failed, 1);
    let snapshots = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("precommit catalog table")
        .metadata()
        .snapshots()
        .len();
    assert_eq!(snapshots, 0);
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        0
    );
    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            successor_owner,
            successor_token,
        )
        .await
        .expect("successor release")
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests that lease loss immediately before an output PUT prevents every
/// stale output and leaves no rewrite-owned object for the successor.
async fn forge_compaction_lease_theft_before_output_put_cleans_rewrite_outputs() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "compaction_output_fence_rows").await;
    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    let barrier = OutputPutBarrier::new(Arc::clone(&control));
    let mut config = fixture.config.clone();
    config.output_file_bytes = 1;
    let context = fixture.context_with_object_store(config, Arc::clone(&barrier));
    let task = tokio::spawn(async move { context.run_once().await });
    tokio::time::timeout(Duration::from_secs(30), barrier.wait_until_reached())
        .await
        .expect("rewrite reached bounded pre-PUT barrier");
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    barrier.release();
    let outcome = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("stale output task completed after barrier release")
        .expect("stale output task")
        .expect("stale output tick reports failure");
    assert_eq!(outcome.tables_failed, 1);
    let entries = control
        .list(&fixture.binding.object_prefix)
        .await
        .expect("list rewrite-owned objects");
    assert!(
        !entries
            .iter()
            .any(|entry| entry.path().contains("/data/forge-")),
        "stale worker left output objects: {entries:?}"
    );
    assert_eq!(
        fixture.operation_count("forge.file_compact.prepared").await,
        0
    );
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        0
    );

    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            successor_owner,
            successor_token,
        )
        .await
        .expect("successor release")
    );
    fixture
        .forge
        .run_once()
        .await
        .expect("successor convergence");
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("successor table");
    assert_eq!(table.metadata().snapshots().len(), 1);
    assert_eq!(
        fixture.operation_count("forge.file_compact.prepared").await,
        1
    );
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge incremental interleaving lane"]
/// Replays competing hinted/periodic-shaped one-shot ticks against one real
/// table and then verifies lease theft fails closed before a successor tick
/// converges the durable state.
///
/// # Errors
///
/// The journey fails when durable lease, SQL, or Iceberg operations diverge
/// from the production Forge contract.
async fn forge_incremental_interleaving() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "incremental_interleaving_rows").await;
    let forge = fixture.context_with_config(fixture.config.clone());
    let (first, second) = tokio::join!(forge.run_once(), forge.run_once());
    assert!(first.expect("first interleaving tick").bins_committed <= 1);
    assert!(second.expect("second interleaving tick").bins_committed <= 1);
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        1
    );

    let (owner, token) = steal_forge_lease(&fixture).await;
    let blocked = forge.run_once().await.expect("blocked tick");
    assert_eq!(blocked.bins_committed, 0);
    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            owner,
            token,
        )
        .await
        .expect("release interleaving lease")
    );
    forge.run_once().await.expect("successor convergence");
    assert_eq!(
        fixture
            .operation_count("forge.file_compact.committed")
            .await,
        1
    );
    let hinted = seed_forge_group(&server, "hint_periodic_rows").await;
    let hint_control = CommitUncertaintyCatalog::new(Arc::clone(&hinted.catalog));
    hint_control.pause_before_commit();
    let (hint_forge, hint_publisher) =
        hinted.context_with_catalog_and_publisher(hinted.config.clone(), hint_control.clone());
    let day = chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day");
    assert_eq!(
        hint_publisher.try_publish(StagingFileCommitted::new(hinted.binding.clone(), day)),
        StagingPublishOutcome::Published
    );
    let hint_stop = CancellationToken::new();
    let hint_task = tokio::spawn({
        let forge = hint_forge.clone();
        let stop = hint_stop.clone();
        async move { forge.run(stop).await }
    });
    hint_control.wait_for_before_commit().await;
    hint_stop.cancel();
    hint_control.reject_paused_before_commit();
    hint_task
        .await
        .expect("hint scheduler task")
        .expect("hint scheduler shutdown");
    assert_eq!(
        hinted.operation_count("forge.file_compact.committed").await,
        0
    );
    hint_forge.run_once().await.expect("periodic recovery tick");
    assert_eq!(
        hinted.operation_count("forge.file_compact.committed").await,
        1
    );

    let uncertain_fixture = seed_forge_group(&server, "multi_output_uncertain_rows").await;
    let mut multi_config = uncertain_fixture.config.clone();
    multi_config.output_file_bytes = 1;
    let uncertain = CommitUncertaintyCatalog::new(Arc::clone(&uncertain_fixture.catalog));
    uncertain.fail_after_next_commit();
    let uncertain_forge = uncertain_fixture.context_with_catalog(multi_config.clone(), uncertain);
    let _ = uncertain_forge.run_once().await;
    let successor = uncertain_fixture.context_with_config(multi_config);
    let _ = successor.run_once().await;
    assert_eq!(
        uncertain_fixture
            .operation_count("forge.file_compact.prepared")
            .await,
        1
    );
    assert_eq!(
        uncertain_fixture
            .operation_count("forge.file_compact.committed")
            .await
            + uncertain_fixture
                .operation_count("forge.file_compact.recovered")
                .await,
        1
    );
    let cancel_stop = CancellationToken::new();
    let cancel_forge = hinted.context_with_config(hinted.config.clone());
    let cancel_task = tokio::spawn({
        let stop = cancel_stop.clone();
        async move { cancel_forge.run(stop).await }
    });
    cancel_stop.cancel();
    cancel_task
        .await
        .expect("cancelled Forge")
        .expect("cancelled Forge shutdown");
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Tests expiry fencing after the real snapshot-expiry commit and before its
/// recovered terminal audit append.
///
/// Steps:
/// 1. Create two real Iceberg snapshots through normal Forge compaction, then
///    configure a zero retention window so the older snapshot is eligible.
/// 2. Pause a real catalog response after expiry has committed, take over the
///    table lease, and resume with a stale-fence error.
/// 3. Assert no terminal expiry audit is written by the stale worker, then
///    release the successor lease and let the next production tick reconcile
///    the already-applied expiry exactly once.
async fn forge_expiry_lease_theft_before_terminal_audit_fails_closed() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "expiry_fence_rows").await;
    fixture.forge.run_once().await.expect("first snapshot");
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    fixture.forge.run_once().await.expect("second snapshot");

    let mut config = fixture.config.clone();
    config.snapshot_retention = Duration::from_millis(1);
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    control.pause_after_commit();
    let context = fixture.context_with_catalog(config, control.clone());
    let task = tokio::spawn(async move { context.run_once().await });
    control.wait_for_commit().await;
    let (successor_owner, successor_token) = steal_forge_lease(&fixture).await;
    control.reject_paused_commit();
    let outcome = task
        .await
        .expect("stale expiry task")
        .expect("stale expiry tick reports failure");
    assert_eq!(outcome.tables_failed, 1);
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.committed")
            .await,
        0
    );
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.recovered")
            .await,
        0
    );
    let snapshots = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("post-expiry table")
        .metadata()
        .snapshots()
        .len();
    assert_eq!(snapshots, 1);

    let lease_key = format!(
        "forge:table:{}:{}:{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    );
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &lease_key,
            successor_owner,
            successor_token,
        )
        .await
        .expect("successor release")
    );
    fixture
        .forge
        .run_once()
        .await
        .expect("expiry recovery tick");
    assert_eq!(
        fixture
            .operation_count("forge.snapshot_expire.recovered")
            .await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}
