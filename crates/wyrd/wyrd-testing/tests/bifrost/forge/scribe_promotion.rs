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
/// Panics when the Bifrost store cannot be listed or read.
async fn object_digests(fixture: &ForgeFixture) -> BTreeMap<String, String> {
    use futures_util::TryStreamExt as _;
    use object_store::ObjectStoreExt as _;
    use sha2::Digest as _;
    let prefix = format!("{}/", fixture.binding.object_prefix);
    let objects = fixture.storage.objects();
    let entries: Vec<_> = objects
        .list(Some(&object_store::path::Path::from(prefix)))
        .try_collect()
        .await
        .expect("promotion object listing");
    let mut digests = BTreeMap::new();
    for entry in entries {
        let bytes = objects
            .get(&entry.location)
            .await
            .expect("promotion object read")
            .bytes()
            .await
            .expect("promotion object body");
        digests.insert(
            entry.location.to_string(),
            hex::encode(sha2::Sha256::digest(&bytes)),
        );
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

    let object_store = ForgeObjectStoreControl::new(&fixture.storage);
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
///
/// # Panics
///
/// Panics when the fixture seals nothing, the held promotion does not reach
/// or leave the catalog boundary in time, or any cut double-counts or misses
/// a sealed row.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_catalog_sql_window_preserves_exact_visibility() {
    /// Asserts one production cut sees every sealed path exactly once.
    ///
    /// # Panics
    ///
    /// Panics when the cut cannot be read or materialized, when
    /// a path is visible from both hot and promoted sources, or when the cut
    /// does not cover the sealed set exactly.
    async fn assert_exact_cut(
        catalog: &vala_bifrost_redux::catalog::BifrostCatalog,
        table: &vala_bifrost_redux::catalog::TableRef,
        tenant: wyrd_spec::DataTenantId,
        sealed: &BTreeSet<String>,
        label: &str,
    ) {
        let cut = catalog
            .read_cut(tenant, std::slice::from_ref(table))
            .await
            .expect("the registered table reads its cut")
            .pop()
            .expect("one table cut");
        let pinned = catalog
            .materialize_cut(tenant, table, cut)
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

    let forge = tokio::time::timeout(std::time::Duration::from_mins(1), held)
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
///
/// # Panics
///
/// Panics when the manual Forge clock cannot advance past the retry budget,
/// the catalog sees more than one update attempt, the operation does not
/// close as `reset`, or any sealed row is settled.
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
/// The coordinator is cancelled while its inline promotion commit is parked
/// before delegation, so acceptance is unknown by construction. Draining must therefore release the
/// attempt and its claim while leaving the operation Prepared: settling it
/// either way would claim knowledge the worker does not have.
///
/// # Panics
///
/// Panics when the drained operation leaves `prepared`, any sealed row is
/// settled, or the drained attempt still holds its claim.
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
    let coordinator_stop = forge.coordinator_stop();
    let forge = forge
        .run_one_failure_while(async {
            catalog.wait_for_before_commit().await;
            coordinator_stop.cancel();
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
        live_forge_claims(&fixture).await,
        0,
        "the drained attempt released its claim"
    );
    forge.shutdown().await;
}

/// Counts unexpired Forge task claims still held over the fixture's own table.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn live_forge_claims(fixture: &ForgeFixture) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM vala.forge_tasks \
         WHERE data_tenant_id = $1 AND namespace_name = $2 AND table_name = $3 \
         AND state IN ('claimed', 'running', 'prepared') \
         AND claim_expires_at > statement_timestamp()",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(&fixture.binding.logical_namespace)
    .bind(&fixture.binding.table_name)
    .fetch_one(fixture.operator_pool.pool())
    .await
    .expect("Forge claim inspection")
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
/// The commit is held open after acceptance while the task's claim lapses. The
/// original worker therefore returns without settling — its fence is gone —
/// and the successor reconciles the same operation identity. What must
/// never happen is two settlements, or a settled row with no snapshot behind
/// it.
#[tokio::test]
#[ignore = "requires Postgres"]
async fn scribe_promotion_claim_loss_and_takeover_settle_once() {
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
    lapse_forge_claim(&fixture).await;
    catalog.reject_paused_commit();

    let forge = tokio::time::timeout(std::time::Duration::from_mins(1), held)
        .await
        .expect("stolen promotion returns")
        .expect("stolen promotion task");
    forge.shutdown().await;

    reclaim_stopped_claim(&fixture).await;
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

/// Lapses the claim of the fixture table's one active Forge task.
///
/// The active task's claim is the table's only exclusive authority, so aging
/// it is exactly what a lost fence looks like to the holder: its next renewal
/// or fenced commit matches no live claim and refuses for the production
/// reason. The task's state, attempt, and owner are untouched.
///
/// # Panics
///
/// Panics when the update fails or the table does not hold exactly one claim.
async fn lapse_forge_claim(fixture: &ForgeFixture) {
    let lapsed = sqlx::query(
        "UPDATE vala.forge_tasks \
         SET claim_expires_at = statement_timestamp() - interval '1 second' \
         WHERE data_tenant_id = $1 AND state IN ('claimed', 'running', 'prepared')",
    )
    .bind(fixture.tenant.as_uuid())
    .execute(fixture.operator_pool.pool())
    .await
    .expect("lapse the Forge claim for a deterministic takeover")
    .rows_affected();
    assert_eq!(lapsed, 1, "the promotion holds exactly one table claim");
}

/// Asserts every live data file of `binding`'s current snapshot tells the
/// truth about itself and returns the rows each `(spec, sort order)` holds.
///
/// Each file's rows are read back from its object: every row lies in the
/// hour and customer its recorded partition tuple names, the tuple has one
/// value per field of the spec the file records, and rows are ordered by the
/// identity-key prefix of the sort order the file records.
///
/// # Panics
///
/// Panics when the table, a manifest, or an object cannot be read, or when a
/// file's rows contradict its recorded partition, spec, or sort order.
async fn assert_live_layout_truth(
    server: &wyrd_testing::WyrdTestServer,
    binding: &vala_bifrost_redux::catalog::TenantTableBinding,
    stage: &str,
) -> BTreeMap<(i32, Option<i32>), u64> {
    use arrow::array::{Array as _, AsArray as _};
    use iceberg::spec::{Literal, PrimitiveLiteral, Transform};
    use object_store::ObjectStoreExt as _;
    let table = server
        .bifrost_catalog()
        .iceberg_catalog()
        .load_table(&binding.table_ident())
        .await
        .expect("the journey table loads");
    let metadata = table.metadata();
    let snapshot = metadata
        .current_snapshot()
        .expect("a promoted table has a snapshot")
        .clone();
    let storage = server
        .state()
        .bifrost_storage()
        .expect("the server owns Bifrost storage");
    let mut rows_by_layout = BTreeMap::new();
    for (path, file) in crate::live_rewrite::snapshot_files(&table, &snapshot, binding)
        .await
        .data
    {
        *rows_by_layout
            .entry((file.partition_spec_id, file.sort_order_id))
            .or_insert(0) += file.record_count;
        let bytes = storage
            .objects()
            .get(&object_store::path::Path::from(path.as_str()))
            .await
            .expect("live object reads")
            .bytes()
            .await
            .expect("live object body");
        let batches = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(bytes)
            .expect("live object is Parquet")
            .build()
            .expect("live object reader")
            .collect::<Result<Vec<_>, _>>()
            .expect("live object rows");
        let batch = arrow::compute::concat_batches(&batches[0].schema(), &batches)
            .expect("live object rows concatenate");
        let spec = metadata
            .partition_spec_by_id(file.partition_spec_id)
            .expect("the recorded spec is retained");
        let tuple = file.partition.iter().collect::<Vec<_>>();
        assert_eq!(
            tuple.len(),
            spec.fields().len(),
            "{stage}: {path} records one partition value per field of spec {}",
            file.partition_spec_id
        );
        let schema = metadata.current_schema();
        for (field, value) in spec.fields().iter().zip(tuple) {
            let source = &schema
                .field_by_id(field.source_id)
                .expect("partition source is a column")
                .name;
            let column = batch
                .column_by_name(source)
                .expect("partition source column");
            match (field.transform, value) {
                (Transform::Hour, Some(Literal::Primitive(PrimitiveLiteral::Int(hour)))) => {
                    let times = column.as_primitive::<arrow::datatypes::TimestampMicrosecondType>();
                    assert!(
                        times.values().iter().all(|micros| micros
                            .div_euclid(wyrd_testing::bifrost::revisions::COMPOSITE_HOUR_MICROS)
                            == i64::from(*hour)),
                        "{stage}: every row of {path} lies in its recorded hour"
                    );
                }
                (Transform::Identity, value) => {
                    let names = column.as_string::<i32>();
                    let expected = match value {
                        Some(Literal::Primitive(PrimitiveLiteral::String(name))) => {
                            Some(name.as_str())
                        }
                        None => None,
                        other => panic!("{stage}: {path} records a non-string customer {other:?}"),
                    };
                    assert!(
                        (0..names.len())
                            .all(|row| names.is_valid(row).then(|| names.value(row)) == expected),
                        "{stage}: every row of {path} carries its recorded customer {expected:?}"
                    );
                }
                (Transform::Bucket(_), Some(_)) => {}
                (transform, value) => {
                    panic!("{stage}: {path} records {value:?} for a {transform} field")
                }
            }
        }
        let order = metadata
            .sort_order_by_id(i64::from(
                file.sort_order_id
                    .expect("every Bifrost object records its sort order"),
            ))
            .expect("the recorded sort order is retained");
        let (fields, columns): (Vec<_>, Vec<_>) = order
            .fields
            .iter()
            .take_while(|field| field.transform == Transform::Identity)
            .map(|field| {
                let column = std::sync::Arc::clone(
                    batch
                        .column_by_name(
                            &schema
                                .field_by_id(field.source_id)
                                .expect("sort source is a column")
                                .name,
                        )
                        .expect("sort source column"),
                );
                let options = arrow::compute::SortOptions {
                    descending: field.direction == iceberg::spec::SortDirection::Descending,
                    nulls_first: field.null_order == iceberg::spec::NullOrder::First,
                };
                (
                    arrow::row::SortField::new_with_options(column.data_type().clone(), options),
                    column,
                )
            })
            .unzip();
        if columns.is_empty() {
            continue;
        }
        let keys = arrow::row::RowConverter::new(fields)
            .expect("sort key converter")
            .convert_columns(&columns)
            .expect("sort keys convert");
        assert!(
            (1..keys.num_rows()).all(|row| keys.row(row - 1) <= keys.row(row)),
            "{stage}: {path} is ordered by its recorded sort order {}",
            order.order_id
        );
    }
    rows_by_layout
}

/// Rows written under two partition specs and two sort orders, two batches
/// each, plus a Bloom-only version sharing the second's ids, promote and
/// compact without changing a public row: every live file — promoted and
/// rewritten — holds only rows of the partition tuple it records under the
/// spec it records, ordered by the sort order it records, and promotion keeps
/// each object under the spec and order it was written with. The specs bucket
/// `shard` four and eight ways, so equal tuples, null customers included,
/// exist under both; every rewrite still reads one spec's partition. Each
/// object's published evidence names the version that accepted it, so the
/// Bloom-only version stays distinguishable, and promotion revalidates every
/// object against that version's retained definition.
///
/// # Panics
///
/// Panics when the pod cannot start, a public call fails, published evidence
/// names another accepting version, Forge leaves work owed or commits no rewrite, a public read differs, a live file
/// contradicts its recorded layout, or a rewrite's inputs span two specs or
/// partitions.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn mixed_specifications_preserve_partition_and_sort_truth() {
    use wyrd_testing::bifrost::revisions::{
        COMPOSITE_ROWS, composite_batch, register_composite_table, shifted,
    };
    let cluster = wyrd_testing::bifrost::WyrdTestCluster::start_with_embedded_forge_observer()
        .await
        .expect("one bound embedded Bifrost pod starts");
    cluster.lead_forge_for_test().await;
    let observer = cluster
        .forge_completion_observer()
        .expect("the journey pod carries a Forge completion observer");
    let server = cluster.server(0).expect("the embedded pod is running");
    let tenant = cluster.data_tenant_id();
    let client = crate::public_support::tenant_client(server, tenant).await;
    let name = crate::public_support::unique_table("mixed_forge");
    let qualified = register_composite_table(server, tenant, &name).await;
    let table_ref = vala_bifrost_redux::catalog::TableRef::new(
        vala_bifrost_redux::namespaces::BifrostNamespace::Datasets,
        &name,
    );
    let table = crate::public_support::JourneyTable {
        qualified: qualified.clone(),
        name: name.clone(),
        binding: vala_bifrost_redux::catalog::TenantTableBinding::resolve((
            tenant,
            table_ref.clone(),
        ))
        .expect("the journey table resolves to its physical binding"),
    };

    // Two batches per layout, each flushed to its own objects, so every
    // physical partition holds a real rewrite candidate under both specs.
    let mut rows = Vec::new();
    let mut append_round = async |shifts: [i64; 2]| {
        for shift in shifts {
            rows.extend(
                crate::public_support::append_batch(
                    &client,
                    &qualified,
                    uuid::Uuid::now_v7(),
                    &shifted(&composite_batch(), shift * COMPOSITE_ROWS),
                )
                .await,
            );
            server.flush_bifrost().await.expect("the batch publishes");
        }
    };
    append_round([0, 1]).await;
    let first_spec = server
        .bifrost_catalog()
        .iceberg_catalog()
        .load_table(&table.binding.table_ident())
        .await
        .expect("the journey table loads")
        .metadata()
        .default_partition_spec_id();
    server.commit_bucket_revision(tenant, &table_ref, 8).await;
    let revised_order = server.commit_sort_revision(tenant, &table_ref).await;
    append_round([2, 3]).await;
    server
        .commit_bloom_revision(tenant, &table_ref, &["customer"])
        .await;
    append_round([4, 5]).await;
    let expected = crate::public_support::canonical_order(rows);
    let rows_per_layout = 2 * u64::try_from(COMPOSITE_ROWS).expect("row count");
    let mut accepted = BTreeMap::new();
    for file in server
        .published_hot_files_for_test(tenant, "bifrost.datasets", &name)
        .await
        .expect("published evidence reads")
    {
        *accepted
            .entry(file.promotion_record.table_version.get())
            .or_insert(0) += file.row_count;
    }
    assert_eq!(
        accepted,
        BTreeMap::from([
            (1, rows_per_layout),
            (3, rows_per_layout),
            (4, rows_per_layout)
        ]),
        "published evidence names each object's accepting version, Bloom-only ones included"
    );
    crate::live_rewrite::assert_public_rows(&client, &table, &expected, "hot").await;

    server
        .forge_clock()
        .advance(chrono::Duration::days(1))
        .expect("the written partitions close");
    crate::live_rewrite::drain_forge_backlog(&cluster, &observer, &[tenant]).await;
    crate::live_rewrite::assert_public_rows(&client, &table, &expected, "promoted").await;
    let revised_spec = server
        .bifrost_catalog()
        .iceberg_catalog()
        .load_table(&table.binding.table_ident())
        .await
        .expect("the journey table loads")
        .metadata()
        .default_partition_spec_id();
    let promoted_files = {
        let promoted = server
            .bifrost_catalog()
            .iceberg_catalog()
            .load_table(&table.binding.table_ident())
            .await
            .expect("the journey table loads");
        let snapshot = promoted
            .metadata()
            .current_snapshot()
            .expect("a promoted table has a snapshot")
            .clone();
        crate::live_rewrite::snapshot_files(&promoted, &snapshot, &table.binding)
            .await
            .data
    };
    let promoted = assert_live_layout_truth(server, &table.binding, "promoted").await;
    assert_eq!(
        promoted,
        BTreeMap::from([
            (
                (
                    first_spec,
                    Some(vala_bifrost_redux::catalog::layout::BIFROST_SORT_ORDER_ID)
                ),
                rows_per_layout
            ),
            ((revised_spec, Some(revised_order)), 2 * rows_per_layout),
        ]),
        "promotion keeps each object under the spec and sort order it was written with"
    );

    server
        .forge_clock()
        .advance(chrono::Duration::hours(1))
        .expect("the default compaction interval passes");
    crate::live_rewrite::await_committed_rewrites(&cluster, &observer, &[&table.binding]).await;
    crate::live_rewrite::assert_public_rows(&client, &table, &expected, "compacted").await;
    let compacted = assert_live_layout_truth(server, &table.binding, "compacted").await;
    assert_eq!(
        compacted.values().sum::<u64>(),
        3 * rows_per_layout,
        "compaction keeps every row exactly once: {compacted:?}"
    );

    // Equal tuples exist under both specs, so a spec-blind grouping would
    // read both specs' files in one rewrite.
    let layouts = |spec: i32| {
        promoted_files
            .values()
            .filter(|file| file.partition_spec_id == spec)
            .map(|file| format!("{:?}", file.partition))
            .collect::<BTreeSet<_>>()
    };
    assert!(
        layouts(first_spec)
            .intersection(&layouts(revised_spec))
            .next()
            .is_some(),
        "the two specs share at least one partition tuple"
    );
    let rewrites = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT current_detail FROM vala.forge_operation_state \
         WHERE data_tenant_id = $1 AND family = 'iceberg_rewrite' \
           AND phase IN ('committed', 'recovered') AND resource LIKE $2 \
         ORDER BY prepared_at, operation_id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("%/{name}"))
    .fetch_all(cluster.pg_fixture().operator_pool().pool())
    .await
    .expect("Forge rewrite inspection");
    // Each committed rewrite's promoted inputs; a rewrite of rewritten
    // outputs reads none.
    let groups = rewrites
        .into_iter()
        .map(|detail| {
            let wyrd_spec::vala::audit_detail::AuditDetail::ForgeIcebergRewrite {
                input_paths, ..
            } = serde_json::from_value(detail).expect("rewrite detail decodes")
            else {
                panic!("an iceberg_rewrite operation records rewrite detail");
            };
            input_paths
                .iter()
                .filter_map(|path| promoted_files.get(path.as_str()))
                .map(|file| (file.partition_spec_id, file.partition.clone()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert!(
        groups.iter().any(|group| !group.is_empty()),
        "a committed rewrite read promoted files"
    );
    for group in groups.iter().filter(|group| !group.is_empty()) {
        let layouts = group
            .iter()
            .map(|(spec, partition)| (*spec, format!("{partition:?}")))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            layouts.len(),
            1,
            "one rewrite reads one spec's partition: {layouts:?}"
        );
    }
}
