//! Replay the real Forge tick through bounded phase schedules.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use arrow::util::display::array_value_to_string;
use async_trait::async_trait;
use iceberg::Catalog;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use opendal::{Buffer, Entry, Metadata};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::forge::{
    ForgeLease, ForgeObjectStore, IcebergRewriteDisposition, forge_lease_key,
};
use vala_bifrost_redux::maintenance::{StagingFileCommitted, StagingPublishOutcome};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{CommitUncertaintyCatalog, ForgeObjectStoreControl, seed_forge_group};

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
    (owner, token.fencing_token)
}

/// Fold exactly the fixture's two staged inputs without consuming a live rewrite.
///
/// The real scheduler still runs the complete production stage order. Limiting
/// the per-tick file budget to the two staged inputs deliberately exhausts the
/// shared budget before live replacement, leaving the resulting snapshot for a
/// test that needs to control that replacement separately.
///
/// # Panics
///
/// Panics when the production-shaped tick cannot compact the fixture's exact
/// staged pair or unexpectedly commits a live replacement.
async fn fold_staged_pair_without_live_replacement(fixture: &wyrd_testing::bifrost::ForgeFixture) {
    let mut config = fixture.config.clone();
    config.max_files_per_tick = 2;
    let outcome = fixture
        .context_with_config(config)
        .run_once()
        .await
        .expect("staged-pair preparation tick");
    assert_eq!(outcome.staging_input_files, 2);
    assert_eq!(outcome.bins_committed, 1);
    assert_eq!(outcome.live_groups_committed, 0);
}

/// Advance the fixture's shared manual Forge clock beyond short uncertainty windows.
///
/// Live-rewrite recovery is intentionally based on Forge wall time rather than
/// elapsed Tokio time, so prepared-operation fixtures advance this control
/// explicitly to a full second beyond wall time before asking a fresh scheduler
/// to classify their state. The margin exceeds both Forge's millisecond clock
/// storage and PostgreSQL's microsecond audit timestamps.
///
/// # Panics
///
/// Panics when a checked future UTC instant cannot be represented or the
/// test-owned monotonic clock cannot reach it.
fn advance_forge_clock_past_uncertainty(server: &WyrdTestServer) {
    let future = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(1))
        .expect("future Forge clock instant is representable");
    server
        .forge_clock()
        .set(future)
        .expect("advance Forge clock beyond uncertainty window");
}

/// Build a recovery tick configuration that cannot start a two-file replacement.
///
/// Reset and recovered-state tests inspect one terminal transition. Their
/// scheduler tick must still execute production reconciliation, but a one-file
/// budget keeps the next eligible two-file live rewrite for a later test step;
/// a one-year snapshot window prevents unrelated expiry commits from consuming
/// an uncertainty-injection catalog seam.
fn recovery_config_without_live_replacement(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> vala_bifrost_redux::forge::ForgeConfig {
    let mut config = fixture.config.clone();
    config.max_files_per_tick = 1;
    config.uncertainty_bound = Duration::from_micros(1);
    config.snapshot_retention = Duration::from_secs(31_536_000);
    config
}

/// Build two eligible live files, then discover one exact replacement through
/// the supplied catalog graph.
async fn live_replacement_context(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    control: Arc<CommitUncertaintyCatalog>,
) -> (
    Arc<vala_bifrost_redux::forge::Forge>,
    iceberg::table::Table,
    vala_bifrost_redux::forge::IcebergTablePlan,
    vala_bifrost_redux::forge::ForgeLease,
) {
    fold_staged_pair_without_live_replacement(fixture).await;
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    fold_staged_pair_without_live_replacement(fixture).await;

    let context = fixture.context_with_catalog(fixture.config.clone(), control.clone());
    let table = control
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("wrapped catalog table");
    let plan = context
        .discover_live_rewrites_for_test(
            &fixture.binding,
            &table,
            chrono::NaiveDate::from_ymd_opt(2026, 7, 15).expect("current day"),
        )
        .await
        .expect("wrapped catalog live rewrite discovery");
    assert!(
        plan.groups_for_test()
            .iter()
            .any(|group| group.files_for_test().len() >= 2),
        "fixture must produce an eligible live replacement group"
    );
    let lease = ForgeLease::acquire(
        &fixture.operator_pool,
        forge_lease_key(
            fixture.tenant,
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        ),
        uuid::Uuid::now_v7(),
        fixture.config.lease_ttl,
    )
    .await
    .expect("live replacement lease query")
    .expect("live replacement lease");
    (context, table, plan, lease)
}

/// Read the exact prepared output paths from durable audit state.
async fn prepared_live_output_paths(fixture: &wyrd_testing::bifrost::ForgeFixture) -> Vec<String> {
    let mut conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("live replacement audit tenant connection");
    let detail: String = sqlx::query_scalar(
        "SELECT detail FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND resource = $1 \
           AND operation = 'forge.iceberg_rewrite.prepared' \
         ORDER BY seq DESC LIMIT 1",
    )
    .bind(format!(
        "bifrost://{}/{}/{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    ))
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("prepared live replacement audit detail");
    let detail: serde_json::Value =
        serde_json::from_str(&detail).expect("prepared audit detail JSON");
    detail["output_paths"]
        .as_array()
        .expect("prepared audit output paths")
        .iter()
        .map(|path| {
            path.as_str()
                .expect("prepared output path string")
                .to_owned()
        })
        .collect()
}

/// Assert every exact prepared output remains in the real object store.
async fn assert_prepared_live_outputs_exist(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> Vec<String> {
    let paths = prepared_live_output_paths(fixture).await;
    assert!(
        !paths.is_empty(),
        "prepared replacement must persist at least one output path"
    );
    for path in &paths {
        let object_path = path
            .find(&fixture.binding.object_prefix)
            .map(|offset| &path[offset..])
            .expect("prepared output path belongs to the fixture table");
        fixture
            .staging
            .stat(object_path)
            .await
            .unwrap_or_else(|error| panic!("prepared output {object_path} is absent: {error}"));
    }
    paths
}

/// Read the exact live data-file membership of the current Iceberg snapshot.
async fn current_live_paths(table: &iceberg::table::Table) -> BTreeSet<String> {
    let snapshot = table
        .metadata()
        .current_snapshot()
        .expect("live replacement table snapshot");
    let manifest_list = table
        .manifest_list_reader(snapshot)
        .load()
        .await
        .expect("current manifest list");
    let mut paths = BTreeSet::new();
    for manifest_file in manifest_list.entries() {
        let manifest = manifest_file
            .load_manifest(table.file_io())
            .await
            .expect("current manifest");
        paths.extend(
            manifest
                .entries()
                .iter()
                .filter(|entry| entry.is_alive())
                .map(|entry| entry.file_path().to_owned()),
        );
    }
    paths
}

/// Read the exact logical row multiset from every current-snapshot Parquet file.
async fn current_logical_rows(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
    table: &iceberg::table::Table,
) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for catalog_path in current_live_paths(table).await {
        let object_path = catalog_path
            .find(&fixture.binding.object_prefix)
            .map(|offset| &catalog_path[offset..])
            .expect("live catalog path belongs to the fixture table");
        let bytes = fixture
            .staging
            .read(object_path)
            .await
            .expect("live Parquet object")
            .to_bytes();
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes)
            .expect("live Parquet reader")
            .build()
            .expect("live Parquet batch reader");
        for batch in reader {
            let batch = batch.expect("live Parquet batch");
            for row in 0..batch.num_rows() {
                rows.push(
                    batch
                        .columns()
                        .iter()
                        .map(|column| {
                            array_value_to_string(column.as_ref(), row)
                                .expect("logical row value formatting")
                        })
                        .collect(),
                );
            }
        }
    }
    rows.sort();
    rows
}

/// Assert that one replacement snapshot directly advances the selected plan base.
fn assert_single_snapshot_advance(table: &iceberg::table::Table, base_snapshot_id: i64) {
    let current = table
        .metadata()
        .current_snapshot()
        .expect("replacement current snapshot");
    assert_ne!(current.snapshot_id(), base_snapshot_id);
    assert_eq!(current.parent_snapshot_id(), Some(base_snapshot_id));
}

/// Read the typed live-replacement audit payloads in durable transition order.
async fn live_rewrite_details(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> Vec<serde_json::Value> {
    let mut conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("live replacement audit tenant connection");
    let details: Vec<String> = sqlx::query_scalar(
        "SELECT detail FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND resource = $1 \
           AND operation IN ('forge.iceberg_rewrite.prepared', 'forge.iceberg_rewrite.committed') \
         ORDER BY seq",
    )
    .bind(format!(
        "bifrost://{}/{}/{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    ))
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("live replacement audit details");
    details
        .into_iter()
        .map(|detail| serde_json::from_str(&detail).expect("typed live replacement audit JSON"))
        .collect()
}

/// Read every durable live-rewrite state row in deterministic operation order.
async fn live_rewrite_state_rows(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> Vec<(String, uuid::Uuid, serde_json::Value, serde_json::Value)> {
    let mut conn = fixture
        .vala
        .tenant_conn(fixture.tenant)
        .await
        .expect("live replacement state tenant connection");
    sqlx::query_as(
        "SELECT phase, operation_id, prepared_detail, current_detail \
         FROM vala.forge_operation_state \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND resource = $1 AND family = 'iceberg_rewrite' \
         ORDER BY operation_id",
    )
    .bind(format!(
        "bifrost://{}/{}/{}",
        fixture.tenant, fixture.binding.logical_namespace, fixture.binding.table_name
    ))
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("live replacement state rows")
}

/// Read the phase only when the fixture has exactly one live-rewrite state row.
async fn live_rewrite_state_phase(fixture: &wyrd_testing::bifrost::ForgeFixture) -> Option<String> {
    let rows = live_rewrite_state_rows(fixture).await;
    match rows.as_slice() {
        [(phase, _, _, _)] => Some(phase.clone()),
        [] => None,
        _ => panic!("fixture must not contain multiple live-rewrite operations: {rows:?}"),
    }
}

/// Produce one durable Prepared live rewrite by cancelling at the successful PUT boundary.
async fn prepare_cancelled_live_rewrite(
    fixture: &wyrd_testing::bifrost::ForgeFixture,
) -> (Arc<ForgeObjectStoreControl>, BTreeSet<String>) {
    fold_staged_pair_without_live_replacement(fixture).await;
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    fold_staged_pair_without_live_replacement(fixture).await;

    let control = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    let context = fixture.context_with_object_store(fixture.config.clone(), Arc::clone(&control));
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("live rewrite table");
    let action = Transaction::new(&table).update_table_properties().set(
        "write.target-file-size-bytes".to_owned(),
        (16 * 1024 * 1024).to_string(),
    );
    ApplyTransactionAction::apply(action, Transaction::new(&table))
        .expect("target-size property update")
        .commit(fixture.catalog.as_ref())
        .await
        .expect("target-size property commit");
    let table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("target-sized live rewrite table");
    let before_paths = current_live_paths(&table).await;
    let plan = context
        .discover_live_rewrites_for_test(
            &fixture.binding,
            &table,
            chrono::NaiveDate::from_ymd_opt(2026, 7, 15).expect("current day"),
        )
        .await
        .expect("live rewrite discovery");
    assert_eq!(plan.groups_for_test().len(), 1, "exactly one rewrite group");
    let group = plan.groups_for_test()[0].clone();
    assert_eq!(group.files_for_test().len(), 2, "exactly two inputs");
    let input_bytes: u64 = group
        .files_for_test()
        .iter()
        .map(|file| file.file_size_bytes_for_test())
        .sum();
    assert!(
        input_bytes < 1024 * 1024,
        "fixture inputs must stay below 1 MiB"
    );
    assert_eq!(
        table
            .metadata()
            .table_properties()
            .expect("Iceberg table properties")
            .write_target_file_size_bytes,
        16 * 1024 * 1024
    );
    let base = plan.base_snapshot_id_for_test();
    let mut lease = ForgeLease::acquire(
        &fixture.operator_pool,
        forge_lease_key(
            fixture.tenant,
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        ),
        uuid::Uuid::now_v7(),
        fixture.config.lease_ttl,
    )
    .await
    .expect("post-PUT lease query")
    .expect("post-PUT lease");
    control.pause_after_next_output_put();
    let stop = CancellationToken::new();
    let operator_pool = fixture.operator_pool.clone();
    let old_context = Arc::downgrade(&context);
    let task = tokio::spawn({
        let binding = fixture.binding.clone();
        let stop = stop.clone();
        async move {
            let result = context
                .replace_live_group_for_test(&mut lease, &binding, &table, base, &group, &stop)
                .await;
            let release = lease.release(&operator_pool).await;
            (result, release)
        }
    });
    tokio::time::timeout(Duration::from_secs(30), control.wait_for_output_put())
        .await
        .expect("successful output PUT boundary");
    let output = control
        .last_output_path()
        .expect("armed output PUT records its exact path");
    fixture
        .staging
        .stat(&output)
        .await
        .expect("output is durable while notification is paused");
    stop.cancel();
    control.release_output_put();
    let (result, release) = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("cancelled replacement shutdown bound")
        .expect("cancelled replacement task");
    assert!(
        matches!(result, Err(vala_bifrost_redux::forge::ForgeError::Shutdown)),
        "post-PUT cancellation must stop after Prepared: {result:?}"
    );
    assert!(release.expect("cancelled replacement lease release"));
    assert!(
        old_context.upgrade().is_none(),
        "the first-process Forge allocation must be dropped after bounded shutdown"
    );
    assert_eq!(control.output_put_calls(), 1);
    assert_eq!(
        live_rewrite_state_phase(fixture).await.as_deref(),
        Some("prepared")
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        1
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.committed")
            .await,
        0
    );
    let details = live_rewrite_details(fixture).await;
    assert_eq!(details.len(), 1, "Prepared is the only durable phase");
    assert_eq!(
        details[0]["output_paths"]
            .as_array()
            .expect("Prepared output paths")
            .len(),
        1,
        "the forced replacement produces exactly one durable output"
    );
    (control, before_paths)
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Cancellation after the real output PUT persists Prepared before bounded shutdown.
async fn live_rewrite_post_put_cancellation_persists_prepared_before_shutdown() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_post_put_cancel").await;
    let (control, before_paths) = prepare_cancelled_live_rewrite(&fixture).await;
    assert_eq!(control.output_put_calls(), 1);
    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("unchanged base table");
    assert_eq!(current_live_paths(&retained).await, before_paths);
    assert_prepared_live_outputs_exist(&fixture).await;
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A fresh Forge resets one abandoned output and preserves the original live set.
async fn restart_resets_abandoned_live_output_exactly_once() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_restart_reset").await;
    let (control, before_paths) = prepare_cancelled_live_rewrite(&fixture).await;
    let prepared_rows = live_rewrite_state_rows(&fixture).await;
    assert_eq!(prepared_rows.len(), 1);
    assert_eq!(prepared_rows[0].0, "prepared");
    assert_eq!(prepared_rows[0].2, prepared_rows[0].3);
    let base_table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("pre-reset logical table");
    let expected_rows = current_logical_rows(&fixture, &base_table).await;
    assert!(!expected_rows.is_empty());
    let output = control.last_output_path().expect("prepared output path");
    let config = recovery_config_without_live_replacement(&fixture);
    advance_forge_clock_past_uncertainty(&server);
    let restarted = fixture.context_with_object_store(config, Arc::clone(&control));
    let restart_task = tokio::spawn({
        let restarted = Arc::clone(&restarted);
        async move { restarted.run_once().await }
    });
    let outcome = tokio::time::timeout(Duration::from_secs(30), restart_task)
        .await
        .expect("fresh reset tick completion bound")
        .expect("fresh reset tick task")
        .expect("fresh reset tick");
    assert_eq!(outcome.tables_succeeded, 1, "reset outcome: {outcome:?}");
    assert_eq!(outcome.live_reset, 1);
    assert_eq!(outcome.live_recovered, 0);
    assert_eq!(outcome.live_pending, 0);
    assert_eq!(outcome.live_unresolved, 0);
    assert_eq!(outcome.open_operation_overflows, 0);
    let rows = live_rewrite_state_rows(&fixture).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "reset");
    assert_eq!(rows[0].1, prepared_rows[0].1);
    assert_eq!(rows[0].2, prepared_rows[0].2);
    assert_eq!(rows[0].3["operation_id"], prepared_rows[0].1.to_string());
    assert_eq!(rows[0].3["phase"], "reset");
    assert_eq!(rows[0].3["input_paths"], prepared_rows[0].2["input_paths"]);
    assert_eq!(
        rows[0].3["output_paths"],
        prepared_rows[0].2["output_paths"]
    );
    assert_eq!(rows[0].3["committed_snapshot_id"], serde_json::Value::Null);
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        1
    );
    assert!(fixture.staging.stat(&output).await.is_err());
    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("reset table");
    assert_eq!(current_live_paths(&retained).await, before_paths);
    assert_eq!(
        current_logical_rows(&fixture, &retained).await,
        expected_rows
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Replaying a fresh Forge after Reset does not append a second terminal transition.
async fn restart_replay_does_not_duplicate_live_terminal_transition() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_restart_replay").await;
    let (control, _) = prepare_cancelled_live_rewrite(&fixture).await;
    let config = recovery_config_without_live_replacement(&fixture);
    advance_forge_clock_past_uncertainty(&server);
    let restarted = fixture.context_with_object_store(config, control);
    assert_eq!(
        {
            let first = restarted.run_once().await.expect("first replay tick");
            assert_eq!(first.live_recovered, 0);
            assert_eq!(first.live_pending, 0);
            assert_eq!(first.live_unresolved, 0);
            assert_eq!(first.open_operation_overflows, 0);
            assert_eq!(first.tables_succeeded, 1);
            first.live_reset
        },
        1
    );
    let replay = restarted.run_once().await.expect("second replay tick");
    assert_eq!(replay.live_reset, 0);
    assert_eq!(replay.live_recovered, 0);
    assert_eq!(replay.live_pending, 0);
    assert_eq!(replay.live_unresolved, 0);
    assert_eq!(replay.open_operation_overflows, 0);
    assert_eq!(replay.tables_succeeded, 1);
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A fresh Forge recovers one catalog-accepted uncertain replacement exactly once.
async fn restart_recovers_catalog_accepted_live_rewrite_exactly_once() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_restart_recovered").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, table, plan, mut lease) =
        live_replacement_context(&fixture, Arc::clone(&control)).await;
    let old_context = Arc::downgrade(&context);
    let base = plan.base_snapshot_id_for_test();
    assert_eq!(
        plan.groups_for_test().len(),
        1,
        "recovered restart fixture must discover exactly one group"
    );
    let group = plan.groups_for_test()[0].clone();
    assert_eq!(
        group.files_for_test().len(),
        2,
        "recovered restart fixture must rewrite exactly two inputs"
    );
    let expected_rows = current_logical_rows(&fixture, &table).await;
    assert!(!expected_rows.is_empty());
    let mut expected_paths = current_live_paths(&table).await;
    for input in group.files_for_test() {
        assert!(expected_paths.remove(input.catalog_path_for_test()));
    }
    control.fail_after_next_commit();
    let error = context
        .replace_live_group_for_test(
            &mut lease,
            &fixture.binding,
            &table,
            base,
            &group,
            &CancellationToken::new(),
        )
        .await
        .expect_err("accepted commit response remains uncertain");
    assert!(
        error
            .to_string()
            .contains("injected post-commit uncertainty"),
        "unexpected uncertainty error: {error}"
    );
    assert!(
        lease
            .release(&fixture.operator_pool)
            .await
            .expect("uncertain replacement lease release")
    );
    drop(context);
    assert!(
        old_context.upgrade().is_none(),
        "the uncertain first-process Forge must be dropped before restart"
    );
    let outputs = assert_prepared_live_outputs_exist(&fixture).await;
    assert_eq!(
        outputs.len(),
        1,
        "recovered restart fixture must persist exactly one output"
    );
    expected_paths.extend(outputs);
    let prepared_rows = live_rewrite_state_rows(&fixture).await;
    assert_eq!(prepared_rows.len(), 1);
    assert_eq!(prepared_rows[0].0, "prepared");
    assert_eq!(prepared_rows[0].2, prepared_rows[0].3);

    let config = recovery_config_without_live_replacement(&fixture);
    advance_forge_clock_past_uncertainty(&server);
    let restarted = fixture.context_with_catalog(config, control);
    let restart_task = tokio::spawn({
        let restarted = Arc::clone(&restarted);
        async move { restarted.run_once().await }
    });
    let outcome = tokio::time::timeout(Duration::from_secs(30), restart_task)
        .await
        .expect("fresh recovered tick completion bound")
        .expect("fresh recovered tick task")
        .expect("fresh recovered tick");
    assert_eq!(outcome.tables_succeeded, 1, "recovery outcome: {outcome:?}");
    assert_eq!(outcome.live_recovered, 1);
    assert_eq!(outcome.live_reset, 0);
    assert_eq!(outcome.live_pending, 0);
    assert_eq!(outcome.live_unresolved, 0);
    assert_eq!(outcome.open_operation_overflows, 0);
    let rows = live_rewrite_state_rows(&fixture).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "recovered");
    assert_eq!(rows[0].1, prepared_rows[0].1);
    assert_eq!(rows[0].2, prepared_rows[0].2);
    assert_eq!(rows[0].3["operation_id"], prepared_rows[0].1.to_string());
    assert_eq!(rows[0].3["phase"], "recovered");
    assert_eq!(rows[0].3["input_paths"], prepared_rows[0].2["input_paths"]);
    assert_eq!(
        rows[0].3["output_paths"],
        prepared_rows[0].2["output_paths"]
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.recovered")
            .await,
        1
    );
    let replaced = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("recovered table");
    assert_single_snapshot_advance(&replaced, base);
    assert_eq!(
        rows[0].3["committed_snapshot_id"],
        replaced
            .metadata()
            .current_snapshot_id()
            .expect("recovered current snapshot")
    );
    assert_eq!(current_live_paths(&replaced).await, expected_paths);
    assert_eq!(
        current_logical_rows(&fixture, &replaced).await,
        expected_rows
    );
    let operation_id = rows[0].1.to_string();
    let summary = replaced
        .metadata()
        .current_snapshot()
        .expect("recovered current snapshot")
        .summary();
    assert_eq!(
        summary.additional_properties.get("forge.workflow"),
        Some(&"iceberg-rewrite".to_owned())
    );
    assert_eq!(
        summary.additional_properties.get("forge.operation_id"),
        Some(&operation_id)
    );
    let replay = restarted.run_once().await.expect("recovered replay tick");
    assert_eq!(replay.live_recovered, 0);
    assert_eq!(replay.live_reset, 0);
    assert_eq!(replay.live_pending, 0);
    assert_eq!(replay.live_unresolved, 0);
    assert_eq!(replay.open_operation_overflows, 0);
    assert_eq!(replay.tables_succeeded, 1);
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.recovered")
            .await,
        1
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Fence loss at the Reset delete boundary leaves Prepared and its output protected.
async fn fence_loss_before_reset_delete_keeps_prepared_and_protected() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_reset_fence").await;
    let (control, _) = prepare_cancelled_live_rewrite(&fixture).await;
    let output = control.last_output_path().expect("prepared output path");
    control.pause_next_delete();
    let config = recovery_config_without_live_replacement(&fixture);
    advance_forge_clock_past_uncertainty(&server);
    let restarted = fixture.context_with_object_store(config, Arc::clone(&control));
    let task = tokio::spawn(async move { restarted.run_once().await });
    tokio::time::timeout(Duration::from_secs(30), control.wait_for_delete())
        .await
        .expect("Reset reached delete boundary");
    let (owner, token) = steal_forge_lease(&fixture).await;
    control.reject_paused_delete();
    let outcome = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("stale Reset task bound")
        .expect("stale Reset task")
        .expect("stale Reset tick reports a table failure");
    assert_eq!(outcome.tables_failed, 1);
    assert_eq!(
        live_rewrite_state_phase(&fixture).await.as_deref(),
        Some("prepared")
    );
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        0
    );
    fixture
        .staging
        .stat(&output)
        .await
        .expect("Prepared output remains protected");
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &forge_lease_key(
                fixture.tenant,
                &fixture.binding.logical_namespace,
                &fixture.binding.table_name,
            ),
            owner,
            token,
        )
        .await
        .expect("successor reset lease release")
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Cancellation at the Reset delete boundary cannot append a false terminal.
async fn cancellation_at_reset_delete_keeps_prepared_without_terminal() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_reset_cancel").await;
    let (control, before_paths) = prepare_cancelled_live_rewrite(&fixture).await;
    let output = control.last_output_path().expect("prepared output path");
    control.pause_next_delete();
    let config = recovery_config_without_live_replacement(&fixture);
    advance_forge_clock_past_uncertainty(&server);
    let (restarted, publisher) =
        fixture.context_with_object_store_and_publisher(config, Arc::clone(&control));
    let old_context = Arc::downgrade(&restarted);
    let stop = CancellationToken::new();
    let task = tokio::spawn({
        let stop = stop.clone();
        async move { restarted.run(stop).await }
    });
    assert_eq!(
        publisher.try_publish(StagingFileCommitted::new(
            fixture.binding.clone(),
            chrono::NaiveDate::from_ymd_opt(2026, 7, 15).expect("partition day"),
        )),
        StagingPublishOutcome::Published
    );
    tokio::time::timeout(Duration::from_secs(30), control.wait_for_delete())
        .await
        .expect("Reset reached delete boundary");
    stop.cancel();
    control.release_paused_delete();
    tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("cancelled Reset scheduler bound")
        .expect("cancelled Reset scheduler task")
        .expect("cancelled Reset scheduler shutdown");
    assert!(
        old_context.upgrade().is_none(),
        "cancelled Reset Forge must be dropped after scheduler shutdown"
    );
    let rows = live_rewrite_state_rows(&fixture).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "prepared");
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        0
    );
    assert!(
        fixture.staging.stat(&output).await.is_err(),
        "delete may finish, but cancellation must prevent terminal state"
    );
    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("cancelled Reset table");
    assert_eq!(current_live_paths(&retained).await, before_paths);
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A failed terminal append cannot falsely close Prepared after Reset deletion.
async fn live_rewrite_terminal_append_failure_leaves_prepared() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_terminal_failure").await;
    let (control, before_paths) = prepare_cancelled_live_rewrite(&fixture).await;
    let output = control.last_output_path().expect("prepared output path");
    let config = recovery_config_without_live_replacement(&fixture);
    advance_forge_clock_past_uncertainty(&server);
    let restarted = fixture.context_with_object_store(config, control);
    restarted.fail_next_terminal_live_audit_for_test();
    let failed = restarted
        .run_once()
        .await
        .expect("terminal failure is isolated to the table");
    assert_eq!(failed.tables_failed, 1);
    assert_eq!(
        live_rewrite_state_phase(&fixture).await.as_deref(),
        Some("prepared")
    );
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        0
    );
    assert!(
        fixture.staging.stat(&output).await.is_err(),
        "the delete may complete, but terminal state must not be fabricated"
    );

    let replay = restarted.run_once().await.expect("terminal replay tick");
    assert_eq!(replay.live_reset, 1);
    assert_eq!(
        live_rewrite_state_phase(&fixture).await.as_deref(),
        Some("reset")
    );
    assert_eq!(
        fixture.operation_count("forge.iceberg_rewrite.reset").await,
        1
    );
    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("terminal replay table");
    assert_eq!(current_live_paths(&retained).await, before_paths);
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A rejected `Prepared` append returns its original error and reclaims only rewrite-owned outputs.
async fn live_replacement_prepared_audit_failure_reclaims_unreferenced_outputs() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_prepared_failure").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, table, plan, mut lease) = live_replacement_context(&fixture, control).await;
    let base = plan.base_snapshot_id_for_test();
    let group = plan
        .groups_for_test()
        .iter()
        .find(|group| group.files_for_test().len() >= 2)
        .expect("eligible group")
        .clone();
    let before_paths = current_live_paths(&table).await;
    let before_snapshots = table.metadata().snapshots().len();
    context.fail_next_prepared_live_audit_for_test();

    let error = context
        .replace_live_group_for_test(
            &mut lease,
            &fixture.binding,
            &table,
            base,
            &group,
            &CancellationToken::new(),
        )
        .await
        .expect_err("injected Prepared append failure");
    assert!(
        error
            .to_string()
            .contains("injected Prepared audit append failure"),
        "the audit error must remain authoritative: {error}"
    );

    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("unchanged table");
    assert_eq!(retained.metadata().snapshots().len(), before_snapshots);
    assert_eq!(current_live_paths(&retained).await, before_paths);
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        0
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.committed")
            .await,
        0
    );
    let entries = fixture
        .staging
        .list(&fixture.binding.object_prefix)
        .await
        .expect("rewrite output listing");
    assert!(
        !entries
            .iter()
            .any(|entry| entry.path().contains("/data/forge/")),
        "Prepared audit failure retained rewrite-owned output: {entries:?}"
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A successful replacement deletes exactly its planned inputs and records one typed phase pair.
async fn live_replacement_successfully_replaces_exact_live_set() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_success").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, table, plan, mut lease) = live_replacement_context(&fixture, control).await;
    let base = plan.base_snapshot_id_for_test();
    let group = plan
        .groups_for_test()
        .iter()
        .find(|group| group.files_for_test().len() >= 2)
        .expect("eligible group")
        .clone();
    let mut expected_paths = current_live_paths(&table).await;
    for input in group.files_for_test() {
        assert!(
            expected_paths.remove(input.catalog_path_for_test()),
            "every selected input must be live"
        );
    }

    let disposition = context
        .replace_live_group_for_test(
            &mut lease,
            &fixture.binding,
            &table,
            base,
            &group,
            &CancellationToken::new(),
        )
        .await
        .expect("live replacement commits");
    let IcebergRewriteDisposition::Committed {
        operation_id,
        snapshot_id,
        input_files,
        output_files,
        input_rows,
        output_rows,
        ..
    } = disposition
    else {
        panic!("live replacement must commit")
    };
    assert_eq!(input_files, group.files_for_test().len());
    assert!(output_files > 0, "replacement must retain rewritten rows");
    assert_eq!(input_rows, output_rows, "replacement must preserve rows");

    let replaced = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("replaced table");
    assert_single_snapshot_advance(&replaced, base);
    assert_eq!(
        replaced
            .metadata()
            .current_snapshot_id()
            .expect("replacement snapshot"),
        snapshot_id
    );
    let outputs = assert_prepared_live_outputs_exist(&fixture).await;
    expected_paths.extend(outputs);
    assert_eq!(current_live_paths(&replaced).await, expected_paths);
    let summary = replaced
        .metadata()
        .current_snapshot()
        .expect("replacement snapshot")
        .summary();
    assert_eq!(
        summary.additional_properties.get("forge.workflow"),
        Some(&"iceberg-rewrite".to_owned())
    );
    assert_eq!(
        summary.additional_properties.get("forge.operation_id"),
        Some(&operation_id.to_string())
    );

    let details = live_rewrite_details(&fixture).await;
    assert_eq!(details.len(), 2);
    for (detail, phase) in details.iter().zip(["prepared", "committed"]) {
        assert_eq!(detail["kind"], "forge_iceberg_rewrite");
        assert_eq!(detail["phase"], phase);
        assert_eq!(detail["operation_id"], operation_id.to_string());
        assert_eq!(detail["base_snapshot_id"], base);
    }
    assert_eq!(details[1]["committed_snapshot_id"], snapshot_id);
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A stale explicit plan base rejects replacement even when the current table has later files.
async fn live_replacement_uses_plan_base_not_file_addition_snapshot() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_plan_base").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, _table, plan, lease) = live_replacement_context(&fixture, control).await;
    let base = plan.base_snapshot_id_for_test();
    let group = plan
        .groups_for_test()
        .iter()
        .find(|group| group.files_for_test().len() >= 2)
        .expect("eligible group")
        .clone();

    assert!(
        lease
            .release(&fixture.operator_pool)
            .await
            .expect("plan lease release")
    );
    fixture.append_forge_file(4).await;
    fixture.append_forge_file(5).await;
    fold_staged_pair_without_live_replacement(&fixture).await;
    let candidate_table = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("candidate table");
    assert_ne!(
        candidate_table.metadata().current_snapshot_id(),
        Some(base),
        "candidate additions must not reuse the plan base"
    );
    let mut lease = ForgeLease::acquire(
        &fixture.operator_pool,
        forge_lease_key(
            fixture.tenant,
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        ),
        uuid::Uuid::now_v7(),
        fixture.config.lease_ttl,
    )
    .await
    .expect("stale-plan lease query")
    .expect("stale-plan lease");

    let disposition = context
        .replace_live_group_for_test(
            &mut lease,
            &fixture.binding,
            &candidate_table,
            base,
            &group,
            &CancellationToken::new(),
        )
        .await
        .expect("stale plan is a normal disposition");
    assert_eq!(disposition, IcebergRewriteDisposition::SnapshotChanged);
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        0
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.committed")
            .await,
        0
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Cancelling at the real catalog boundary leaves prepared live outputs and no terminal audit.
async fn live_replacement_cancellation_at_catalog_boundary_preserves_prepared() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_cancel").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, table, plan, mut lease) =
        live_replacement_context(&fixture, control.clone()).await;
    let base = plan.base_snapshot_id_for_test();
    let group = plan
        .groups_for_test()
        .iter()
        .find(|group| group.files_for_test().len() >= 2)
        .expect("eligible group")
        .clone();
    let before = table.metadata().snapshots().len();
    let before_paths = current_live_paths(&table).await;
    control.pause_before_commit();
    let stop = CancellationToken::new();
    let task = tokio::spawn({
        let stop = stop.clone();
        let binding = fixture.binding.clone();
        async move {
            context
                .replace_live_group_for_test(&mut lease, &binding, &table, base, &group, &stop)
                .await
        }
    });
    control.wait_for_before_commit().await;
    stop.cancel();
    control.reject_paused_before_commit();
    assert!(task.await.expect("cancelled replacement task").is_err());
    let retained = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("base table");
    assert_eq!(retained.metadata().snapshots().len(), before);
    assert_eq!(current_live_paths(&retained).await, before_paths);
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        1
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.committed")
            .await,
        0
    );
    assert_prepared_live_outputs_exist(&fixture).await;
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// A lease theft after catalog acceptance leaves exactly one replacement and no terminal audit.
async fn live_replacement_lease_theft_after_catalog_acceptance_claims_no_terminal() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_lease_theft").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, table, plan, mut lease) =
        live_replacement_context(&fixture, control.clone()).await;
    let base = plan.base_snapshot_id_for_test();
    let group = plan
        .groups_for_test()
        .iter()
        .find(|group| group.files_for_test().len() >= 2)
        .expect("eligible group")
        .clone();
    let mut expected_paths = current_live_paths(&table).await;
    for input in group.files_for_test() {
        assert!(
            expected_paths.remove(input.catalog_path_for_test()),
            "discovered replacement input must be live"
        );
    }
    control.pause_after_commit();
    let task = tokio::spawn({
        let binding = fixture.binding.clone();
        let stop = CancellationToken::new();
        async move {
            context
                .replace_live_group_for_test(&mut lease, &binding, &table, base, &group, &stop)
                .await
        }
    });
    control.wait_for_commit().await;
    let (owner, token) = steal_forge_lease(&fixture).await;
    control.reject_paused_commit();
    let error = task
        .await
        .expect("stale replacement task")
        .expect_err("post-acceptance lease theft must remain uncertain");
    assert!(
        error
            .to_string()
            .contains("injected stale Forge lease after catalog commit"),
        "unexpected post-acceptance error: {error}"
    );
    let replaced = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("replaced table");
    assert_single_snapshot_advance(&replaced, base);
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        1
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.committed")
            .await,
        0
    );
    expected_paths.extend(assert_prepared_live_outputs_exist(&fixture).await);
    assert_eq!(current_live_paths(&replaced).await, expected_paths);
    assert!(
        vala_sql::queries::maintenance_leases::release_lease_fenced(
            &fixture.operator_pool,
            &forge_lease_key(
                fixture.tenant,
                &fixture.binding.logical_namespace,
                &fixture.binding.table_name
            ),
            owner,
            token
        )
        .await
        .expect("successor release")
    );
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// An accepted but uncertain catalog response preserves prepared live-rewrite evidence.
async fn live_replacement_uncertain_catalog_response_preserves_prepared() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");
    let fixture = seed_forge_group(&server, "live_rewrite_uncertain").await;
    let control = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    let (context, table, plan, mut lease) =
        live_replacement_context(&fixture, control.clone()).await;
    let base = plan.base_snapshot_id_for_test();
    let group = plan
        .groups_for_test()
        .iter()
        .find(|group| group.files_for_test().len() >= 2)
        .expect("eligible group")
        .clone();
    let mut expected_paths = current_live_paths(&table).await;
    for input in group.files_for_test() {
        assert!(
            expected_paths.remove(input.catalog_path_for_test()),
            "discovered replacement input must be live"
        );
    }
    control.fail_after_next_commit();
    let stop = CancellationToken::new();
    let error = context
        .replace_live_group_for_test(&mut lease, &fixture.binding, &table, base, &group, &stop)
        .await
        .expect_err("uncertain catalog response");
    assert!(
        error
            .to_string()
            .contains("injected post-commit uncertainty"),
        "unexpected live replacement error: {error}"
    );
    let replaced = fixture
        .catalog
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("uncertain table");
    assert_single_snapshot_advance(&replaced, base);
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.prepared")
            .await,
        1
    );
    assert_eq!(
        fixture
            .operation_count("forge.iceberg_rewrite.committed")
            .await,
        0
    );
    expected_paths.extend(assert_prepared_live_outputs_exist(&fixture).await);
    assert_eq!(current_live_paths(&replaced).await, expected_paths);
    assert!(
        lease
            .release(&fixture.operator_pool)
            .await
            .expect("original lease release")
    );
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
    assert_eq!(
        live_rewrite_state_phase(&fixture).await,
        None,
        "pre-PUT cancellation must not fabricate an open live operation"
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
    advance_forge_clock_past_uncertainty(&server);
    fixture
        .context_with_config(recovery_config_without_live_replacement(&fixture))
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
    let config = fixture.config.clone();
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
            .any(|entry| entry.path().contains("/data/forge/")),
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
    assert!(
        live_rewrite_state_rows(&fixture).await.is_empty(),
        "pre-PUT lease loss must not create an iceberg-rewrite operation-state row"
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
async fn periodic_and_hints_serialize_without_false_terminal_audit() {
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
    tokio::time::timeout(
        Duration::from_secs(3),
        hint_control.wait_for_before_commit_drop(),
    )
    .await
    .expect("hint cancellation must drop the paused catalog call");
    tokio::time::timeout(Duration::from_secs(3), hint_task)
        .await
        .expect("hint scheduler exit bound")
        .expect("hint scheduler task")
        .expect("hint scheduler shutdown");
    assert_eq!(
        hinted.operation_count("forge.file_compact.committed").await,
        0
    );
    hint_forge.run_once().await.expect("periodic recovery tick");
    assert_eq!(
        hinted.operation_count("forge.file_compact.committed").await,
        0
    );
    assert_eq!(
        hinted.operation_count("forge.file_compact.recovered").await,
        0
    );
    assert_eq!(
        hinted
            .catalog
            .load_table(&hinted.binding.table_ident())
            .await
            .expect("hinted cancellation table")
            .metadata()
            .snapshots()
            .len(),
        0
    );

    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires the Postgres-backed Forge interleaving lane"]
/// Exercises cancellation on both sides of the Iceberg commit acceptance
/// boundary and proves the successor preserves the corresponding durable state.
///
/// The first fixture pauses immediately before catalog acceptance.  The stop
/// token is then cancelled while the catalog future is held, and the fixture
/// proves cancellation dropped that future before the supervised scheduler
/// returns within a bound.
/// The second fixture repeats the sequence after the real catalog has accepted
/// the commit but before its response reaches Forge. The pre-acceptance path
/// remains prepared during the uncertainty window; the post-acceptance path
/// reconciles the already-created snapshot. Both release the table lease and
/// never create a duplicate terminal event.
///
/// # Errors
///
/// The journey fails when the real Wyrd server, SQL lease, Iceberg catalog, or
/// audit transitions do not satisfy the cancellation and reconciliation
/// contract. The test proves cancellation wins at the controlled catalog
/// boundary before it bounds the scheduler join.
async fn forge_commit_cancellation_windows_reconcile_once() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_secs(3600))
        .start_bound()
        .await
        .expect("real Forge server");

    let before = seed_forge_group(&server, "cancel_before_acceptance").await;
    let before_control = CommitUncertaintyCatalog::new(Arc::clone(&before.catalog));
    before_control.pause_before_commit();
    let (before_forge, before_publisher) =
        before.context_with_catalog_and_publisher(before.config.clone(), before_control.clone());
    let day = chrono::NaiveDate::from_ymd_opt(2026, 7, 14).expect("partition day");
    assert_eq!(
        before_publisher.try_publish(StagingFileCommitted::new(before.binding.clone(), day)),
        StagingPublishOutcome::Published
    );
    let before_stop = CancellationToken::new();
    let before_task = tokio::spawn({
        let forge = before_forge.clone();
        let stop = before_stop.clone();
        async move { forge.run(stop).await }
    });
    before_control.wait_for_before_commit().await;
    before_stop.cancel();
    tokio::time::timeout(
        Duration::from_secs(3),
        before_control.wait_for_before_commit_drop(),
    )
    .await
    .expect("before-acceptance cancellation must drop the paused catalog call");
    tokio::time::timeout(Duration::from_secs(3), before_task)
        .await
        .expect("before-acceptance scheduler exit bound")
        .expect("before-acceptance scheduler task")
        .expect("before-acceptance scheduler shutdown");
    assert_eq!(
        before.operation_count("forge.file_compact.prepared").await,
        1
    );
    assert_eq!(
        before.operation_count("forge.file_compact.committed").await,
        0
    );
    before_forge
        .run_once()
        .await
        .expect("before-acceptance successor tick");
    assert_eq!(
        before.operation_count("forge.file_compact.committed").await,
        0
    );
    assert_eq!(
        before.operation_count("forge.file_compact.recovered").await,
        0
    );
    assert_eq!(
        before
            .catalog
            .load_table(&before.binding.table_ident())
            .await
            .expect("before-acceptance table")
            .metadata()
            .snapshots()
            .len(),
        0
    );

    let after = seed_forge_group(&server, "cancel_after_acceptance").await;
    let after_control = CommitUncertaintyCatalog::new(Arc::clone(&after.catalog));
    after_control.pause_after_commit();
    let (after_forge, after_publisher) =
        after.context_with_catalog_and_publisher(after.config.clone(), after_control.clone());
    assert_eq!(
        after_publisher.try_publish(StagingFileCommitted::new(after.binding.clone(), day)),
        StagingPublishOutcome::Published
    );
    let after_stop = CancellationToken::new();
    let after_task = tokio::spawn({
        let forge = after_forge.clone();
        let stop = after_stop.clone();
        async move { forge.run(stop).await }
    });
    after_control.wait_for_commit().await;
    after_stop.cancel();
    tokio::time::timeout(
        Duration::from_secs(3),
        after_control.wait_for_after_commit_drop(),
    )
    .await
    .expect("after-acceptance cancellation must drop the paused catalog call");
    tokio::time::timeout(Duration::from_secs(3), after_task)
        .await
        .expect("after-acceptance scheduler exit bound")
        .expect("after-acceptance scheduler task")
        .expect("after-acceptance scheduler shutdown");
    assert_eq!(
        after.operation_count("forge.file_compact.prepared").await,
        1
    );
    advance_forge_clock_past_uncertainty(&server);
    after
        .context_with_config(recovery_config_without_live_replacement(&after))
        .run_once()
        .await
        .expect("after-acceptance reconciliation");
    assert_eq!(
        after.operation_count("forge.file_compact.committed").await,
        0
    );
    assert_eq!(
        after.operation_count("forge.file_compact.recovered").await,
        1
    );
    assert_eq!(
        after
            .catalog
            .load_table(&after.binding.table_ident())
            .await
            .expect("after-acceptance table")
            .metadata()
            .snapshots()
            .len(),
        1
    );
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
    fold_staged_pair_without_live_replacement(&fixture).await;
    fixture.append_forge_file(2).await;
    fixture.append_forge_file(3).await;
    fold_staged_pair_without_live_replacement(&fixture).await;
    advance_forge_clock_past_uncertainty(&server);

    let mut config = fixture.config.clone();
    config.max_files_per_tick = 1;
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
