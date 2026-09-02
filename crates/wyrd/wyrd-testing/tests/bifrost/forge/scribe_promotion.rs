//! Production-harness proofs for the Scribe promotion route.
//!
//! Every scenario drives the real scheduler and the real worker over hot
//! objects a real Scribe sealed. Nothing here fabricates a `vala.file_list`
//! row, plans a task by hand, or calls a promotion owner directly: the route
//! is only proven if the production supervisors reach it on their own.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use wyrd_testing::bifrost::{
    CommitUncertaintyCatalog, ForgeFixture, ForgeObjectStoreControl, seed_forge_group,
};

use crate::support::{
    SupervisedForge, live_data_paths, reclaim_stopped_claim, start_engine_fixture_server,
};

/// One durable Forge task row as the promotion proofs read it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TaskRow {
    /// Durable strategy discriminator.
    strategy: String,
    /// Durable lifecycle state.
    state: String,
    /// Persisted plan document, including promotion parameters.
    plan: serde_json::Value,
    /// Snapshot the plan was derived against.
    base_snapshot_id: i64,
}

/// One `vala.file_list` row projected to its promotion settlement columns.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileRow {
    /// Durable row identity, which is also the promoted-file identity.
    id: uuid::Uuid,
    /// Canonical object path Scribe published.
    file_path: String,
    /// Whether the row has left the hot source.
    compacted: bool,
    /// Snapshot that represents the row, once settled.
    committed_snapshot_id: Option<i64>,
    /// Forge operation that placed it there, once settled.
    forge_publication_operation_id: Option<uuid::Uuid>,
}

/// Reads every durable Forge task for one tenant in creation order.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn task_rows(fixture: &ForgeFixture) -> Vec<TaskRow> {
    sqlx::query_as::<_, (String, String, serde_json::Value, i64)>(
        "SELECT strategy, state, plan, base_snapshot_id FROM vala.forge_tasks \
         WHERE data_tenant_id = $1 ORDER BY created_at, task_id",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("Forge task inspection")
    .into_iter()
    .map(|(strategy, state, plan, base_snapshot_id)| TaskRow {
        strategy,
        state,
        plan,
        base_snapshot_id,
    })
    .collect()
}

/// Reads the promotion settlement columns of one fixture table, in durable order.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn file_rows(fixture: &ForgeFixture) -> Vec<FileRow> {
    sqlx::query_as::<_, (uuid::Uuid, String, bool, Option<i64>, Option<uuid::Uuid>)>(
        "SELECT id, file_path, compacted, committed_snapshot_id, \
         forge_publication_operation_id FROM vala.file_list \
         WHERE data_tenant_id = $1 AND namespace = $2 AND table_name = $3 \
         ORDER BY created_at, file_ordinal, id",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("Forge file-list inspection")
    .into_iter()
    .map(
        |(id, file_path, compacted, committed_snapshot_id, forge_publication_operation_id)| {
            FileRow {
                id,
                file_path,
                compacted,
                committed_snapshot_id,
                forge_publication_operation_id,
            }
        },
    )
    .collect()
}

/// Borrows the ordered promoted file identities from one persisted plan.
///
/// # Panics
///
/// Panics when the plan does not carry the locked promotion parameter shape.
fn plan_file_ids(plan: &serde_json::Value) -> Vec<uuid::Uuid> {
    let parameters = plan
        .get("parameters")
        .expect("promotion plan carries parameters");
    assert_eq!(
        parameters.get("kind").and_then(serde_json::Value::as_str),
        Some("scribe_promotion"),
        "promotion plan must carry the locked parameter kind"
    );
    parameters
        .get("file_ids")
        .and_then(serde_json::Value::as_array)
        .expect("promotion parameters carry ordered file ids")
        .iter()
        .map(|value| {
            uuid::Uuid::parse_str(value.as_str().expect("file id is a string"))
                .expect("file id is a uuid")
        })
        .collect()
}

/// The scheduler plans exactly the never-published hot rows, once.
///
/// Three properties are proven together because they are the same rule: the
/// plan names every eligible row in durable order, it names nothing that has
/// already been settled, and a table with an active promotion does not receive
/// a second one.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_scheduler_claims_only_exact_eligible_hot_rows() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_eligibility").await;

    let expected = file_rows(&fixture).await;
    assert_eq!(expected.len(), 2, "two sealed hot objects");
    assert!(
        expected
            .iter()
            .all(|row| !row.compacted && row.committed_snapshot_id.is_none()),
        "sealed hot objects start unpublished"
    );

    let mut forge = SupervisedForge::start_default(&fixture, fixture.config.clone());
    forge.run_one_success().await;

    let tasks = task_rows(&fixture).await;
    let promotions = tasks
        .iter()
        .filter(|task| task.strategy == "scribe_promotion")
        .collect::<Vec<_>>();
    assert_eq!(
        promotions.len(),
        1,
        "exactly one promotion per table, got {tasks:?}"
    );
    assert_eq!(
        plan_file_ids(&promotions[0].plan),
        expected.iter().map(|row| row.id).collect::<Vec<_>>(),
        "the plan names every eligible row in durable order"
    );

    let settled = file_rows(&fixture).await;
    assert!(
        settled.iter().all(|row| row.compacted
            && row.committed_snapshot_id.is_some()
            && row.forge_publication_operation_id.is_some()),
        "a successful promotion settles every planned row: {settled:?}"
    );

    forge.shutdown().await;

    // A second supervised pass over already-settled rows plans nothing: the
    // settled columns are exactly what makes a row ineligible.
    let forge = SupervisedForge::start_default(&fixture, fixture.config.clone());
    forge.schedule_once().await;
    forge.shutdown().await;
    let after = task_rows(&fixture).await;
    assert_eq!(
        after
            .iter()
            .filter(|task| task.strategy == "scribe_promotion")
            .count(),
        1,
        "settled rows create no second promotion: {after:?}"
    );
}

/// One sealed hot object is promoted unchanged and stays exactly visible.
///
/// This is the tier-1 shape of the route: the object Scribe published is the
/// object the snapshot references, at the same path, and no row is ever
/// visible twice or not at all across the transition.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn forge_hot_object_promotes_unchanged_and_remains_exact() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_journey").await;

    let published = server
        .published_hot_files_for_test(
            fixture.tenant,
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .await
        .expect("published hot files");
    assert!(!published.is_empty(), "the fixture sealed hot objects");
    let sealed_paths = published
        .iter()
        .map(|file| file.object_key.clone())
        .collect::<BTreeSet<_>>();
    assert!(
        live_data_paths(&fixture, &fixture.binding).await.is_empty(),
        "a hot object is not yet referenced by the table"
    );

    let mut forge = SupervisedForge::start_default(&fixture, fixture.config.clone());
    forge.run_one_success().await;
    forge.shutdown().await;

    let live = live_data_paths(&fixture, &fixture.binding).await;
    assert_eq!(
        live, sealed_paths,
        "promotion references the writer's own objects at their own paths"
    );

    let settled = file_rows(&fixture).await;
    assert_eq!(
        settled.len(),
        published.len(),
        "promotion adds no durable row"
    );
    assert!(
        settled
            .iter()
            .all(|row| row.compacted && row.committed_snapshot_id.is_some()),
        "every promoted row left the hot source: {settled:?}"
    );

    let republished = server
        .published_hot_files_for_test(
            fixture.tenant,
            &fixture.binding.logical_namespace,
            &fixture.binding.table_name,
        )
        .await
        .expect("published hot files after promotion");
    assert_eq!(
        republished
            .iter()
            .map(|file| (
                file.object_key.clone(),
                file.file_size,
                file.file_checksum.clone()
            ))
            .collect::<Vec<_>>(),
        published
            .iter()
            .map(|file| (
                file.object_key.clone(),
                file.file_size,
                file.file_checksum.clone()
            ))
            .collect::<Vec<_>>(),
        "promotion rewrites no object"
    );
}

/// Snapshots every object under one table's prefix with its exact content hash.
///
/// Comparing this map across a promotion is the direct proof that no data
/// object was created, rewritten, or deleted: a PUT anywhere under the prefix
/// changes either the key set or one object's digest.
///
/// # Panics
///
/// Panics when the staging operator cannot be listed or read.
async fn object_digests(fixture: &ForgeFixture) -> BTreeMap<String, String> {
    let prefix = format!("{}/", fixture.binding.object_prefix);
    let entries = fixture
        .staging
        .list_with(&prefix)
        .recursive(true)
        .await
        .expect("promotion object listing");
    let mut digests = BTreeMap::new();
    for entry in entries {
        if entry.metadata().is_dir() {
            continue;
        }
        let bytes = fixture
            .staging
            .read(entry.path())
            .await
            .expect("promotion object read");
        digests.insert(entry.path().to_owned(), {
            use sha2::Digest as _;
            hex::encode(sha2::Sha256::digest(bytes.to_bytes()))
        });
    }
    digests
}

/// Promotion reads every hot footer and writes no data object at all.
///
/// The proof is bidirectional. The object store is byte-identical across the
/// whole route, so nothing was written; and the production object-store seam
/// records reads but no output notification and no delete, so the reads that
/// did happen are revalidation rather than a rewrite in disguise.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_revalidates_footer_and_appends_without_data_put() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_no_put").await;
    let before = object_digests(&fixture).await;
    assert!(
        before.keys().any(|path| !path.contains("/metadata/")),
        "the fixture sealed real data objects"
    );

    let object_store = ForgeObjectStoreControl::new(Arc::clone(&fixture.staging));
    let mut forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&fixture.catalog),
        Arc::clone(&object_store) as Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>,
    );
    forge.run_one_success().await;
    forge.shutdown().await;

    let sealed = file_rows(&fixture).await;
    assert!(
        sealed.iter().all(|row| row.committed_snapshot_id.is_some()),
        "the promotion committed: {sealed:?}"
    );
    let after = object_digests(&fixture).await;
    let data_object = |path: &str| !path.contains("/metadata/");
    assert_eq!(
        after
            .iter()
            .filter(|(path, _)| data_object(path))
            .collect::<BTreeMap<_, _>>(),
        before
            .iter()
            .filter(|(path, _)| data_object(path))
            .collect::<BTreeMap<_, _>>(),
        "promotion left every data object byte-identical"
    );
    assert!(
        after
            .keys()
            .filter(|path| !before.contains_key(path.as_str()))
            .all(|path| !data_object(path)),
        "promotion added only catalog metadata, never a data object: {:?}",
        after
            .keys()
            .filter(|path| !before.contains_key(path.as_str()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        object_store.output_put_calls(),
        0,
        "promotion produced no rewrite output"
    );
    assert_eq!(
        object_store.delete_calls(),
        0,
        "promotion deleted no object"
    );
    assert!(
        object_store.object_io_calls() >= before.len(),
        "promotion revalidated every hot object it appended"
    );
}

/// Every sealed row stays visible exactly once across the catalog-to-SQL window.
///
/// The catalog commit is held open after acceptance, which is the only interval
/// in which a row could be counted twice — referenced by the new snapshot and
/// still unsettled in SQL. The production `pin_sealed_table` cut is taken at
/// that instant, and before and after it, and each cut must partition the
/// sealed set exactly.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_catalog_sql_window_preserves_exact_visibility() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_window").await;
    let catalog_owner = server.bifrost_catalog();
    let table_ref = fixture.binding.table_ref.clone();
    let tenant = fixture.tenant;

    let sealed = file_rows(&fixture)
        .await
        .into_iter()
        .map(|row| row.file_path)
        .collect::<BTreeSet<_>>();
    assert!(!sealed.is_empty(), "the fixture sealed real objects");

    /// Asserts one production cut sees every sealed path exactly once.
    async fn assert_exact_cut(
        catalog: &vala_bifrost_redux::catalog::BifrostCatalog,
        table: &vala_bifrost_redux::catalog::TableRef,
        tenant: wyrd_spec::DataTenantId,
        sealed: &BTreeSet<String>,
        label: &str,
    ) {
        let permit =
            vala_bifrost_redux::oracle::reader_pins::ReaderIoPermit::unfenced_for_test();
        let prepared = catalog
            .prepare_reader_identity(table, tenant)
            .await
            .expect("the registered table prepares its reader identity");
        let pinned = catalog
            .materialize_reader_cut(prepared, &permit)
            .await
            .unwrap_or_else(|error| panic!("{label} cut: {error}"));
        let hot = pinned
            .hot_files
            .iter()
            .map(|row| row.file_path.clone())
            .collect::<BTreeSet<_>>();
        let promoted = pinned.iceberg_file_paths;
        assert!(
            hot.is_disjoint(&promoted),
            "{label}: a row is visible from both sources: hot={hot:?} promoted={promoted:?}"
        );
        assert_eq!(
            hot.union(&promoted).cloned().collect::<BTreeSet<_>>(),
            *sealed,
            "{label}: the cut does not cover the sealed set exactly"
        );
    }

    assert_exact_cut(&catalog_owner, &table_ref, tenant, &sealed, "before").await;

    let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    catalog.pause_after_commit();
    let mut forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&fixture.object_store),
    );
    let held = tokio::spawn(async move {
        forge.run_one_success().await;
        forge
    });

    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        catalog.wait_for_commit(),
    )
    .await
    .expect("promotion commit reached the catalog boundary");
    assert_exact_cut(&catalog_owner, &table_ref, tenant, &sealed, "mid-window").await;
    catalog.release_paused_commit();

    let forge = tokio::time::timeout(std::time::Duration::from_secs(60), held)
        .await
        .expect("held promotion completes")
        .expect("held promotion task");
    forge.shutdown().await;

    assert_exact_cut(&catalog_owner, &table_ref, tenant, &sealed, "after").await;
    assert_eq!(
        file_rows(&fixture)
            .await
            .into_iter()
            .filter(|row| row.committed_snapshot_id.is_some())
            .count(),
        sealed.len(),
        "every sealed row settled exactly once"
    );
}

/// Reads the durable Forge operation phases for one tenant's promotions.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn promotion_phases(fixture: &ForgeFixture) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT phase FROM vala.forge_operation_state \
         WHERE data_tenant_id = $1 AND family = 'scribe_promotion' \
         ORDER BY prepared_at, operation_id",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("Forge operation-state inspection")
}

/// One definite conflict is revalidated and retried; a second one resets.
///
/// A refusal before delegation is certain knowledge that nothing landed, so
/// promotion is allowed exactly one revalidated retry against it. The retry
/// must re-read the durable demand rather than replay a stale plan, and a
/// conflict that survives the retry must leave the operation Reset with every
/// row still hot — never half-settled.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_conflict_revalidates_once_and_second_conflict_resets() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_conflict_once").await;
    let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    catalog.reject_next_commits(1);

    let mut forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&fixture.object_store),
    );
    forge.run_one_success().await;
    forge.shutdown().await;

    assert_eq!(
        catalog.update_attempts(),
        2,
        "one definite conflict is followed by exactly one retry"
    );
    let settled = file_rows(&fixture).await;
    assert!(
        settled
            .iter()
            .all(|row| row.committed_snapshot_id.is_some()),
        "the revalidated retry promoted every row: {settled:?}"
    );
    assert_eq!(
        promotion_phases(&fixture).await,
        vec!["committed".to_owned()],
        "the retried promotion settles once"
    );

    // A conflict that survives the single retry must not settle anything.
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_conflict_twice").await;
    let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    catalog.reject_next_commits(2);

    let forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&fixture.object_store),
    );
    let forge = forge.run_one_failure().await;
    forge.shutdown().await;

    assert_eq!(
        catalog.update_attempts(),
        2,
        "a second conflict is not retried again"
    );
    let unsettled = file_rows(&fixture).await;
    assert!(
        unsettled
            .iter()
            .all(|row| !row.compacted && row.committed_snapshot_id.is_none()),
        "a reset promotion leaves every row hot: {unsettled:?}"
    );
    assert_eq!(
        promotion_phases(&fixture).await,
        vec!["reset".to_owned()],
        "the failed promotion resets its operation"
    );
}

/// A lost commit response is reconciled from evidence, never re-committed.
///
/// The catalog accepts the append and then loses the response, and refuses
/// every later commit. The next attempt must therefore recognise its own
/// committed snapshot by the task identity written into the snapshot summary
/// and settle from that evidence. If it tried to append again the refused
/// commit would fail the attempt, so a green run is itself the proof that no
/// second append was made.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_ambiguity_reconciles_without_recommit() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_ambiguity").await;
    let sealed = file_rows(&fixture)
        .await
        .into_iter()
        .map(|row| row.file_path)
        .collect::<BTreeSet<_>>();
    let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    catalog.fail_after_next_commit();

    let forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&fixture.object_store),
    );
    let forge = forge.run_one_failure().await;

    assert_eq!(
        promotion_phases(&fixture).await,
        vec!["prepared".to_owned()],
        "an uncertain commit leaves the operation open for recovery ({} catalog attempts, errors {:?})",
        catalog.update_attempts(),
        forge.returned_errors()
    );
    assert_eq!(
        live_data_paths(&fixture, &fixture.binding).await,
        sealed,
        "the lost response still committed the append"
    );
    forge.shutdown().await;
    release_retry_backoff(&fixture).await;

    let mut forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&fixture.object_store),
    );
    forge.run_one_success().await;
    forge.shutdown().await;

    assert_eq!(
        promotion_phases(&fixture).await,
        vec!["recovered".to_owned()],
        "the successor settles the operation from evidence"
    );
    assert_eq!(
        live_data_paths(&fixture, &fixture.binding).await,
        sealed,
        "recovery appended nothing a second time"
    );
    let settled = file_rows(&fixture).await;
    assert!(
        settled
            .iter()
            .all(|row| row.compacted && row.committed_snapshot_id.is_some()),
        "recovery settled every promoted row: {settled:?}"
    );
}

/// The operation deadline bounds the conflict retry to zero second attempts.
///
/// The commit is parked at the real catalog seam, the Forge clock is moved
/// past the operation's own retry budget, and only then is the parked commit
/// refused. The deadline captured before the first attempt must therefore
/// already be spent, so the definite conflict closes the operation instead of
/// buying a second catalog call. One delegated update — against two in the
/// retry scenario — is what proves the barrier held.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_deadline_expires_before_conflict_retry() {
    let server = start_engine_fixture_server().await;
    let clock = server.forge_clock();
    let fixture = seed_forge_group(&server, "promotion_deadline").await;
    let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    catalog.pause_before_commit();

    let forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&fixture.object_store),
    );
    let forge = forge
        .run_one_failure_while(async {
            catalog.wait_for_before_commit().await;
            let expired = clock.now().expect("manual Forge clock")
                + chrono::Duration::from_std(fixture.config.iceberg_total_retry_timeout)
                    .expect("retry budget is representable")
                + chrono::Duration::seconds(1);
            clock.set(expired).expect("manual Forge clock advances");
            catalog.reject_paused_before_commit();
        })
        .await;

    assert_eq!(
        catalog.update_attempts(),
        1,
        "the expired deadline permitted no second catalog call: {:?}",
        forge.returned_errors()
    );
    assert_eq!(
        promotion_phases(&fixture).await,
        vec!["reset".to_owned()],
        "a conflict past the deadline closes the operation"
    );
    let unsettled = file_rows(&fixture).await;
    assert!(
        unsettled
            .iter()
            .all(|row| !row.compacted && row.committed_snapshot_id.is_none()),
        "nothing was settled by a refused promotion: {unsettled:?}"
    );
    forge.shutdown().await;
}

/// Cancellation drains a parked promotion without settling anything.
///
/// The worker is cancelled while its commit is parked before delegation, so
/// acceptance is unknown by construction. Draining must therefore release the
/// attempt and its lease while leaving the operation Prepared: settling it
/// either way would claim knowledge the worker does not have.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_cancellation_drains_without_settlement() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_cancellation").await;
    let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    catalog.pause_before_commit();

    let forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&fixture.object_store),
    );
    let worker_stop = forge.worker_stop();
    let forge = forge
        .run_one_failure_while(async {
            catalog.wait_for_before_commit().await;
            worker_stop.cancel();
            catalog.wait_for_before_commit_drop().await;
        })
        .await;

    assert_eq!(
        promotion_phases(&fixture).await,
        vec!["prepared".to_owned()],
        "a drained promotion stays open rather than claiming an outcome: {:?}",
        forge.returned_errors()
    );
    let unsettled = file_rows(&fixture).await;
    assert!(
        unsettled
            .iter()
            .all(|row| !row.compacted && row.committed_snapshot_id.is_none()),
        "a drained promotion settled nothing: {unsettled:?}"
    );
    assert_eq!(
        live_forge_leases(&fixture).await,
        0,
        "the drained attempt released its lease"
    );
    forge.shutdown().await;
}

/// Counts unexpired Forge leases still held over the fixture's own table.
///
/// The lease key is derived through the production key builder, so a renamed
/// or re-scoped lease identity fails this read rather than silently counting
/// zero.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn live_forge_leases(fixture: &ForgeFixture) -> i64 {
    let lease_key = vala_bifrost_redux::forge::forge_lease_key(
        fixture.tenant,
        &fixture.binding.logical_namespace,
        &fixture.binding.table_name,
    );
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM vala.maintenance_leases \
         WHERE lease_key = $1 AND expires_at > statement_timestamp()",
    )
    .bind(&lease_key)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("Forge lease inspection")
}

/// Brings a retryable task's production backoff forward to now.
///
/// The durable retry delay is a fixed exponential interval computed in SQL, so
/// there is no configuration or clock seam a journey can turn down. Only the
/// two scheduling timestamps move; the task's identity, state, plan, base
/// snapshot, and attempt count are exactly what the production failure path
/// left behind, so the successor attempt this releases is the real retry.
///
/// # Panics
///
/// Panics when the update fails or moves no retryable task.
async fn release_retry_backoff(fixture: &ForgeFixture) {
    let moved = sqlx::query(
        "UPDATE vala.forge_tasks SET ready_at = statement_timestamp(), \
         next_eligible_at = statement_timestamp() \
         WHERE data_tenant_id = $1 AND state = 'retryable'",
    )
    .bind(fixture.tenant.as_uuid())
    .execute(fixture.operator_pool.pool())
    .await
    .expect("Forge retry backoff release")
    .rows_affected();
    assert_eq!(
        moved, 1,
        "exactly one retryable task was waiting on backoff"
    );
}

/// A promotion whose fence is stolen settles exactly once, by the successor.
///
/// The commit is held open after acceptance while the durable lease is taken
/// over. The original worker therefore returns without settling — its fence is
/// gone — and the successor reconciles the same operation identity. What must
/// never happen is two settlements, or a settled row with no snapshot behind
/// it.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_lease_loss_and_takeover_settle_once() {
    let server = start_engine_fixture_server().await;
    let fixture = seed_forge_group(&server, "promotion_takeover").await;
    let sealed = file_rows(&fixture)
        .await
        .into_iter()
        .map(|row| row.file_path)
        .collect::<BTreeSet<_>>();
    let catalog = CommitUncertaintyCatalog::new(Arc::clone(&fixture.catalog));
    catalog.pause_after_commit();

    let forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&fixture.object_store),
    );
    let held = tokio::spawn(async move { forge.run_one_failure().await });

    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        catalog.wait_for_commit(),
    )
    .await
    .expect("promotion commit reached the catalog boundary");
    let stolen = steal_forge_lease(&fixture).await;
    catalog.reject_paused_commit();

    let forge = tokio::time::timeout(std::time::Duration::from_secs(60), held)
        .await
        .expect("stolen promotion returns")
        .expect("stolen promotion task");
    forge.shutdown().await;

    reclaim_stopped_claim(&fixture).await;
    release_forge_lease(&fixture, stolen).await;
    let mut forge = SupervisedForge::start_with_seams(
        &fixture,
        fixture.config.clone(),
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&fixture.object_store),
    );
    forge.run_one_success().await;
    forge.shutdown().await;

    assert_eq!(
        promotion_phases(&fixture).await.len(),
        1,
        "takeover settles the one operation identity, not a second"
    );
    assert_eq!(
        live_data_paths(&fixture, &fixture.binding).await,
        sealed,
        "takeover appended nothing a second time"
    );
    let settled = file_rows(&fixture).await;
    assert!(
        settled
            .iter()
            .all(|row| row.compacted && row.committed_snapshot_id.is_some()),
        "the successor settled every promoted row exactly once: {settled:?}"
    );
}

/// Takes the fixture table's durable Forge lease away from its current holder.
///
/// Expiring the row and re-acquiring it through the production lease query is
/// what makes the takeover real: the successor's fencing token is issued by
/// the same transaction a second worker process would use, so the displaced
/// worker's fence assertions fail for the production reason.
///
/// # Panics
///
/// Panics when the lease cannot be expired or re-acquired.
async fn steal_forge_lease(fixture: &ForgeFixture) -> uuid::Uuid {
    let lease_key = vala_bifrost_redux::forge::forge_lease_key(
        fixture.tenant,
        &fixture.binding.logical_namespace,
        &fixture.binding.table_name,
    );
    sqlx::query(
        "UPDATE vala.maintenance_leases SET expires_at = now() - interval '1 second' \
         WHERE lease_key = $1",
    )
    .bind(&lease_key)
    .execute(fixture.operator_pool.pool())
    .await
    .expect("expire the Forge lease for a deterministic takeover");
    let successor = uuid::Uuid::now_v7();
    vala_sql::queries::maintenance_leases::try_acquire_lease(
        &fixture.operator_pool,
        &lease_key,
        successor,
        i64::try_from(fixture.config.lease_ttl.as_secs()).expect("lease seconds"),
    )
    .await
    .expect("successor lease acquisition")
    .expect("successor owns the expired Forge lease");
    successor
}

/// Releases a lease this test took, so a real worker can acquire it again.
///
/// The theft stands in for a second Forge process; once its point is made the
/// lease has to go back, or the reconciling worker fails for the artificial
/// reason instead of proving anything.
///
/// # Panics
///
/// Panics when the lease release query fails.
async fn release_forge_lease(fixture: &ForgeFixture, owner: uuid::Uuid) {
    let lease_key = vala_bifrost_redux::forge::forge_lease_key(
        fixture.tenant,
        &fixture.binding.logical_namespace,
        &fixture.binding.table_name,
    );
    sqlx::query(
        "UPDATE vala.maintenance_leases SET expires_at = now() - interval '1 second' \
         WHERE lease_key = $1 AND owner = $2",
    )
    .bind(&lease_key)
    .bind(owner)
    .execute(fixture.operator_pool.pool())
    .await
    .expect("release the stolen Forge lease");
}
