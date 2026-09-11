redacted
//!
//! One attempt plans every eligible group, admits them through the worker-local
//! queue, and publishes each admitted plan under its own operation against the
//! head the plan before it left. These scenarios drive that through the real
//! scheduler, worker, Postgres, catalog, and warehouse; nothing here calls the
//! managed core or the publication owner directly.

use std::collections::BTreeSet;
use std::sync::Arc;

use vala_bifrost_redux::forge::{ForgeClock, ForgeLifecycleEvent, ForgeObjectStore};

use super::rewrite_support::PromotedRewriteFixture;
use super::support::{CountingObjectStore, PromotionCatalogSeam, SupervisedPromotion};

/// Every admitted plan publishes its own operation onto the current head.
///
/// The attempt is the unit of planning, not of commit: one plan set is derived
/// from one table read, and each plan is then rewritten and published on its
/// own. What that has to produce is a chain, not a batch — one operation and
/// one snapshot per plan, each committed onto the head its predecessor left, so
/// a plan that fails costs only itself and a plan that succeeds is durable
/// immediately.
///
/// The table is seeded so the attempt has at least three plans to admit, which
/// is what separates "published independently" from "published once".
///
/// # Panics
///
/// Panics when the attempt publishes fewer operations than it admitted plans,
/// when two plans share an operation identity, when the published snapshots do
/// not form one linear chain over the base the attempt planned against, or when
/// an input the attempt rewrote is still live.
#[tokio::test]
async fn independent_plan_publications_compose_on_current_head() {
    let promoted = PromotedRewriteFixture::start_unpromoted("compaction_admission").await;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        promoted.fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    // Two files arrive with the fixture; two more make the promoted set large
    // enough that planning has at least three groups to offer.
    promoted.fixture.seal_more(2).await;
    supervisor.run_one_success().await;

    let inputs = promoted
        .live_data_files()
        .await
        .into_iter()
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();
    let base = promoted.load_table().await;
    let base_snapshot = base
        .metadata()
        .current_snapshot_id()
        .expect("the promoted table has a current snapshot");
    let snapshots_before = base.metadata().snapshots().count();

    supervisor.restart_worker();
    supervisor.run_one_success().await;
    supervisor.shutdown().await;

    let operations = promoted.fixture.rewrite_operations().await;
    let after = promoted.load_table().await;
    let published = after.metadata().snapshots().count() - snapshots_before;
    assert!(
        published >= 3,
        "the attempt admitted and published at least three plans: {operations:?}"
    );
    assert_eq!(
        operations.len(),
        published,
        "every published plan opened exactly one operation: {operations:?}"
    );
    assert!(
        operations.iter().all(|(_, phase)| phase == "committed"),
        "every published plan settled its own operation: {operations:?}"
    );
    assert_eq!(
        operations
            .iter()
            .map(|(id, _)| *id)
            .collect::<BTreeSet<_>>()
            .len(),
        operations.len(),
        "no two plans published under the same operation identity: {operations:?}"
    );

    // One linear chain over the base: a plan commits onto the head the plan
    // before it left, so following parents back from the current head reaches
    // the snapshot the attempt planned against in exactly `published` steps.
    let mut head = after
        .metadata()
        .current_snapshot()
        .expect("the rewritten table has a current snapshot")
        .clone();
    for step in 0..published {
        let parent = head
            .parent_snapshot_id()
            .unwrap_or_else(|| panic!("published snapshot {step} names the head it composed on"));
        if parent == base_snapshot {
            assert_eq!(
                step + 1,
                published,
                "each published plan added its own snapshot to the chain"
            );
            break;
        }
        head = after
            .metadata()
            .snapshot_by_id(parent)
            .unwrap_or_else(|| panic!("published snapshot {step} composed on a retained head"))
            .clone();
    }

    let live = promoted
        .live_data_files()
        .await
        .into_iter()
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();
    assert!(
        live.is_disjoint(&inputs),
        "the composed cut is made of replacements, never the inputs they replaced: {live:?}"
    );
}

redacted
///
/// An attempt is many independent publications, so its task result is a
redacted
/// rule
/// is that partial progress is progress: a single durable commit makes the
/// whole attempt successful and leaves its unfinished siblings as ordinary
/// planning debt, because the commit cannot be undone and re-running the debt
/// is exactly what the next attempt already does. Only when nothing at all
/// published does a failure decide the task, and only when planning selects
/// nothing does the attempt acknowledge an already-compact table without
/// spending an attempt against it.
///
/// The three phases below are the three outcomes that rule distinguishes, and
/// they are driven through the real catalog seam rather than a verdict: a
/// bounded refusal budget kills the head plan through its whole retry
/// schedule, an unbounded one kills every plan, and a compacted table plans
/// nothing.
///
/// # Panics
///
/// Panics when a partly published attempt is reported as a failure, when a
/// wholly refused attempt is reported as a success or misclassified, when a
/// refused plan leaves its operation open, or when an attempt that planned
/// nothing consumes the task's attempt budget or writes an operation.
#[tokio::test]
async fn admitted_batch_reports_partial_progress_semantics() {
    let promoted = PromotedRewriteFixture::start_unpromoted("compaction_reduction").await;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let mut supervisor = SupervisedPromotion::start_serial(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    promoted.fixture.seal_more(2).await;
    supervisor.run_one_success().await;

    // Phase one: the head plan exhausts its whole retry schedule against a
    // definite refusal while its siblings commit. One durable commit is
    // progress, so the attempt succeeds and spends nothing.
    let snapshots_before = promoted.load_table().await.metadata().snapshots().count();
    catalog.reject_next_commits(REFUSALS_PER_PLAN);
    supervisor.restart_worker();
    supervisor.run_one_success().await;

    let operations = promoted.fixture.rewrite_operations().await;
    let published = promoted.load_table().await.metadata().snapshots().count() - snapshots_before;
    assert!(
        published >= 1,
        "a sibling of the refused plan published its own commit: {operations:?}"
    );
    assert_eq!(
        operations
            .iter()
            .filter(|(_, phase)| phase == "reset")
            .count(),
        1,
        "the refused plan closed its own operation as never-published: {operations:?}"
    );
    assert_eq!(
        operations
            .iter()
            .filter(|(_, phase)| phase == "committed")
            .count(),
        published,
        "every other plan settled its own operation as committed: {operations:?}"
    );
    let partial = latest_small_files_task(&promoted.fixture).await;
    assert_eq!(
        partial.state, "succeeded",
        "a partly published attempt is a successful task: {partial:?}"
    );
    assert_eq!(
        partial.failure_class, None,
        "a successful task records no failure class: {partial:?}"
    );

    // Phase two: nothing publishes, so the reduction reports one plan's typed
    // failure, and that failure — not a sibling's — is what the task carries.
    let snapshots_before = promoted.load_table().await.metadata().snapshots().count();
    let operations_before = promoted.fixture.rewrite_operations().await.len();
    catalog.reject_remaining_commits();
    supervisor.restart_worker();
    let error = supervisor.run_one_failure().await;

    let operations = promoted.fixture.rewrite_operations().await;
    assert_eq!(
        promoted.load_table().await.metadata().snapshots().count(),
        snapshots_before,
        "an attempt whose every plan was refused publishes nothing: {error}"
    );
    assert!(
        operations.len() > operations_before,
        "every admitted plan still opened its own operation: {operations:?}"
    );
    assert!(
        operations[operations_before..]
            .iter()
            .all(|(_, phase)| phase == "reset"),
        "every refused plan closed its operation as never-published: {operations:?}"
    );
    let refused = latest_small_files_task(&promoted.fixture).await;
    assert_eq!(
        refused.state, "retryable",
        "a wholly refused attempt leaves the task retryable: {refused:?}"
    );
    assert_eq!(
        refused.failure_class.as_deref(),
        Some("transient_object_store"),
        "the deciding plan's own typed catalog failure classifies the task, \
         not a generic reduction verdict: {refused:?}"
    );
    assert_eq!(
        refused.attempt_count, 1,
        "exactly one attempt was spent: {refused:?}"
    );
    assert!(
        refused.next_eligible_at > chrono::Utc::now(),
        "a retryable task backs off before it is claimable again: {refused:?}"
    );

    assert_compact_table_is_acknowledged(&promoted, supervisor, &catalog).await;
}

/// Drives the fixture table to compaction and proves a zero-plan attempt.
///
/// Planning that selects nothing is the third outcome the reduction has to
/// distinguish: it is not a failure, so it must acknowledge the table rather
/// than spend the task's attempt budget and terminalize an idle table after
/// five passes. The table is first compacted through ordinary passes, and the
/// settled task is then re-offered, because that is the only state in which the
/// worker reaches its planner with nothing to select.
///
/// # Panics
///
/// Panics when the acknowledgement fails the task, spends an attempt, records a
/// failure class, opens an operation, or publishes a snapshot.
async fn assert_compact_table_is_acknowledged(
    promoted: &PromotedRewriteFixture,
    mut supervisor: SupervisedPromotion,
    catalog: &Arc<PromotionCatalogSeam>,
) {
    // Phase three: planning selects nothing on a compacted table. That is not a
    // failure and must not spend an attempt, open an operation, or publish.
    catalog.reject_next_commits(0);
    promoted.fixture.clear_task_backoff().await;
    for _ in 0..8 {
        supervisor.schedule_only().await;
        if latest_small_files_task(&promoted.fixture).await.state == "succeeded" {
            break;
        }
        promoted.fixture.clear_task_backoff().await;
        supervisor.restart_worker();
        supervisor.settle_one_success().await;
    }
    let snapshots_before = promoted.load_table().await.metadata().snapshots().count();
    let operations_before = promoted.fixture.rewrite_operations().await.len();
    promoted.fixture.reoffer_settled_small_files_task().await;
    // Maintenance work the earlier passes made ready shares the worker, so the
    // re-offered task is settled by whichever attempt reaches it rather than by
    // a fixed number of them.
    for _ in 0..4 {
        supervisor.restart_worker();
        supervisor.settle_one_success().await;
        if latest_small_files_task(&promoted.fixture).await.state != "ready" {
            break;
        }
    }
    supervisor.shutdown().await;

    let acknowledged = latest_small_files_task(&promoted.fixture).await;
    assert_eq!(
        acknowledged.state, "succeeded",
        "an already-compact table is acknowledged, not failed: {acknowledged:?}"
    );
    assert_eq!(
        acknowledged.attempt_count, 0,
        "acknowledging a compact table spends no attempt: {acknowledged:?}"
    );
    assert_eq!(
        acknowledged.failure_class, None,
        "acknowledging a compact table records no failure: {acknowledged:?}"
    );
    assert_eq!(
        promoted.fixture.rewrite_operations().await.len(),
        operations_before,
        "an attempt that planned nothing opens no operation"
    );
    assert_eq!(
        promoted.load_table().await.metadata().snapshots().count(),
        snapshots_before,
        "an attempt that planned nothing publishes no snapshot"
    );
}

/// One definite refusal costs a plan its initial submission plus three retries.
///
/// The publication protocol is pinned at three retries, so a scenario that
/// wants to exhaust exactly one plan has to spend exactly this many refusals.
const REFUSALS_PER_PLAN: usize = 4;

/// Reads the most recently created small-files task for the fixture table.
///
/// The reduction is observable only through the task row the attempt settled,
/// and each phase enqueues a fresh task, so every assertion is against the
/// newest one rather than a remembered id.
///
/// # Panics
///
/// Panics when the fixture has planned no small-files task at all.
async fn latest_small_files_task(
    fixture: &super::support::PromotionIntegrationFixture,
) -> super::support::ForgeTaskRow {
    fixture
        .forge_tasks()
        .await
        .into_iter()
        .rfind(|task| task.strategy == "small_files")
        .expect("the fixture planned at least one small-files task")
}

/// Reconciliation classifies every open operation, not one bounded page of them.
///
/// One attempt now opens an operation per published plan, so a table can hold
/// more open rows than the per-table page bound. A reader that stopped at the
/// first page would leave the rest of them unclassified and permanently
/// blocking, so the walk continues from the row it last classified — including
/// the rows it could not resolve — and closes with one recheck of the first
/// page. The page bound is set to one here so that a single ordinary attempt
/// already exceeds it.
///
/// Every commit's response is lost after the catalog accepted it, which is the
/// one failure that leaves an operation genuinely open: the replacement landed,
/// but only the retained evidence can prove it. The successor is then required
/// to settle all of them.
///
/// # Panics
///
/// Panics when the attempt leaves no more open operations than one page holds,
/// or when the takeover leaves any of them open.
#[tokio::test]
async fn reconciliation_walks_every_open_operation_a_page_cannot_hold() {
    let mut promoted = PromotedRewriteFixture::start_unpromoted("compaction_reconcile").await;
    promoted.fixture.config.max_open_operations_per_table = 1;
    // A stalled commit has to run out of publication budget while the scenario
    // is still watching, so the budget is the seconds a test can wait rather
    // than the production minutes.
    promoted.fixture.config.iceberg_total_retry_timeout = std::time::Duration::from_secs(2);
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let (clock, control) = super::support::manual_clock();
    let mut supervisor = SupervisedPromotion::start_serial(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    promoted.fixture.seal_more(2).await;
    supervisor.run_one_success().await;

    // Every plan lands and none of them is answered, so the attempt holds
    // several operations it cannot account for. Its owner is then dropped where
    // it stands rather than asked to stop, because a stop lets a plan whose
    // refusal is already definite settle its own operation on the way out. A
    // killed owner writes nothing, which is what leaves more open rows on this
    // table than one page holds.
    catalog.stall_next_commit_responses(8);
    supervisor.restart_worker();
    supervisor.schedule_only().await;
    supervisor.start_worker();
    tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            let open = operation_phases(&promoted.fixture).await;
            if open.values().filter(|phase| *phase == "prepared").count()
                > promoted.fixture.config.max_open_operations_per_table
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the stalled plans open more operations than one page holds");
    supervisor.kill_worker();
    catalog.stall_next_commit_responses(0);
    assert!(
        supervisor.returned_errors().is_empty(),
        "no attempt is failed or retried while its operations are still \
         Prepared: {:?}",
        supervisor.returned_errors()
    );

    let captured = operation_phases(&promoted.fixture)
        .await
        .into_iter()
        .filter(|(_, phase)| phase == "prepared")
        .map(|(id, _)| id)
        .collect::<BTreeSet<_>>();
    assert!(
        captured.len() > promoted.fixture.config.max_open_operations_per_table,
        "the attempt must leave more open operations than one page holds: {:?}",
        operation_phases(&promoted.fixture).await
    );

    // The whole-table takeover. Reconciliation refuses to call a young
    // operation absent, so it only starts once the uncertainty bound and the
    // reclaim backoff lapse. Measured from wall clock rather than from the
    // manual clock's own base: the operations were prepared with database
    // timestamps taken while the attempt ran, so only a bound taken after them
    // makes them old enough.
    let settled_at = chrono::Utc::now()
        + chrono::Duration::from_std(promoted.fixture.config.uncertainty_bound)
            .expect("the uncertainty bound is representable")
        + chrono::Duration::seconds(1);
    control
        .set(settled_at)
        .expect("manual Forge clock advances");
    promoted.fixture.expire_claims().await;
    // The killed owner released nothing, so its table fence lapses too.
    promoted.fixture.expire_table_lease().await;
    supervisor.reclaim_expired_claims().await;
    promoted.fixture.clear_task_backoff().await;
    supervisor.restart_worker();
    supervisor.schedule_only().await;
    supervisor.start_worker();

    // The proof is the durable rows: every captured UUID is classified, so a
    // reader bounded by one page — which would have settled one and silently
    // left the rest Prepared forever — cannot produce this state.
    await_operation_phase(&promoted.fixture, &captured, &["recovered", "reset"]).await;
    supervisor.shutdown().await;
    // Stated over the captured identities, not over the table: the successor
    // that took the table over keeps compacting it, and an operation it opened
    // for its own new work is open because it is in flight, not because the
    // walk missed it.
    let settled = operation_phases(&promoted.fixture).await;
    assert!(
        captured
            .iter()
            .all(|id| settled.get(id).is_some_and(|phase| phase != "prepared")),
        "the takeover leaves no operation it inherited behind: {settled:?}"
    );
}

/// Reads one Forge volume counter for the compaction route.
fn small_files_volume(telemetry: &super::support::ForgeTelemetryCheckpoint, family: &str) -> u64 {
    super::production_routes::counter_total(
        &telemetry.snapshot(),
        family,
        &[("task_type", "small_files")],
    )
}

/// Settles one compaction window and returns the live inputs it consumed.
///
/// The table's own live data files are the arithmetic ground truth: promotion
/// only adds files, so every path that stops being live across a window was
/// consumed by a committed rewrite plan. That makes the expected counter total
/// independent of any knowledge of how planning grouped the files.
///
/// # Panics
///
/// Panics when the awaited task count is not reached inside the bound.
async fn consumed_by_settled_compaction(
    promoted: &PromotedRewriteFixture,
    supervisor: &mut SupervisedPromotion,
    settled_tasks: usize,
) -> usize {
    let before = promoted.fixture.live_data_paths().await;
    promoted.fixture.clear_task_backoff().await;
    supervisor.restart_worker();
    supervisor.schedule_only().await;
    supervisor.start_worker();
    await_small_files_in_state(&promoted.fixture, &["succeeded"], settled_tasks).await;
    supervisor.stop_worker().await;
    let after = promoted.fixture.live_data_paths().await;
    before.difference(&after).count()
}

/// Drives one window whose every plan loses its answer, through the durable
/// recovery that settles it, and returns the live inputs it consumed.
///
/// An attempt that cannot account for any of its operations settles nothing:
/// it is released, leaving a Running task, Prepared operations, and a claim
/// lease it stopped renewing. Nothing local can close that, so this drives the
/// production sequence a lost process would get — the lease lapses, the
/// reclaim pass returns the task, and the next owner's table-wide
/// reconciliation classifies the exact operations before anything is counted.
///
/// # Panics
///
/// Panics when the attempt is not released, or its operations never leave
/// `prepared`, inside [`ADMISSION_BOUND`].
async fn consumed_by_recovered_compaction(
    promoted: &PromotedRewriteFixture,
    supervisor: &mut SupervisedPromotion,
    catalog: &PromotionCatalogSeam,
) -> usize {
    let before = promoted.fixture.live_data_paths().await;
    let released_before = supervisor.observer().released_attempts_for_test().len();
    promoted.fixture.clear_task_backoff().await;
    supervisor.restart_worker();
    supervisor.schedule_only().await;
    supervisor.start_worker();
    let released = tokio::time::timeout(ADMISSION_BOUND, async {
        while supervisor.observer().released_attempts_for_test().len() == released_before {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(
        released.is_ok(),
        "an attempt that learned nothing releases its task: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
    supervisor.stop_worker().await;

    // The catalog answers again, so recovery reads real evidence rather than
    // the fault that produced the ambiguity.
    catalog.stall_next_commit_responses(0);
    let open = operation_phases(&promoted.fixture)
        .await
        .into_iter()
        .filter(|(_, phase)| phase == "prepared")
        .map(|(id, _)| id)
        .collect::<BTreeSet<_>>();
    promoted.fixture.expire_claims().await;
    supervisor.reclaim_expired_claims().await;
    // No new planning pass: recovery is driven by the reclaimed task and the
    // table-wide reconciliation it carries, and a fresh rewrite planned into
    // the same window would consume files this window itself created.
    promoted.fixture.clear_task_backoff().await;
    supervisor.restart_worker();
    supervisor.start_worker();
    await_operation_phase(&promoted.fixture, &open, &["recovered", "reset"]).await;
    // The task itself is recovered too, not just its operations: a table whose
    // rewrite task is still owned admits no further maintenance of any kind.
    let returned = tokio::time::timeout(ADMISSION_BOUND, async {
        while small_files_in_state(&promoted.fixture, &["claimed", "running"]).await > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(
        returned.is_ok(),
        "recovery returns the task it reconciled: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
    supervisor.stop_worker().await;
    let after = promoted.fixture.live_data_paths().await;
    before.difference(&after).count()
}

/// Seals and promotes one more generation so the next window has work.
///
/// # Panics
///
/// Panics when the promotion attempt does not settle successfully.
async fn promote_more_inputs(
    promoted: &PromotedRewriteFixture,
    supervisor: &mut SupervisedPromotion,
) {
    let promoted_before = promotions_succeeded(&promoted.fixture).await;
    promoted.fixture.seal_more(4).await;
    promoted.fixture.clear_task_backoff().await;
    supervisor.restart_worker();
    supervisor.start_worker();
    supervisor.schedule_only().await;
    tokio::time::timeout(ADMISSION_BOUND, async {
        while promotions_succeeded(&promoted.fixture).await <= promoted_before {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the new hot objects are published before compaction plans them");
    supervisor.stop_worker().await;
}

/// A multi-plan attempt counts every committed plan's volume exactly once.
///
/// One attempt now commits one operation per plan, so the task's reported
/// throughput is a sum, not a sample. Three shapes have to agree with the
/// files the table actually lost: several ordinary successes, an attempt whose
/// plans are all unresolved until its own reconciliation proves one live, and
/// an ordinary success beside a still-Prepared sibling that only the later
/// table-wide owner can account for. In every shape the cumulative counter must
/// equal the arithmetic sum of the consumed inputs, and a further reconciliation
/// pass and replan must not move it again.
///
/// # Panics
///
/// Panics when a settled window reports volume other than the inputs it
/// consumed, when the unresolved sibling is counted before it is proved, or
/// when a later pass counts any plan a second time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn multi_plan_success_counts_all_committed_volume_once() {
    let telemetry = super::support::ForgeTelemetryCheckpoint::install();
    let mut promoted = PromotedRewriteFixture::start_unpromoted("volume_sum").await;
    // Every stalled plan has to run out of publication budget while the
    // scenario is still watching.
    promoted.fixture.config.iceberg_total_retry_timeout = std::time::Duration::from_secs(2);
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let (clock, control) = super::support::manual_clock();
    let mut supervisor = SupervisedPromotion::start_serial(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    promoted.fixture.seal_more(4).await;
    supervisor.run_one_success().await;

    // Several ordinary successes in one attempt: the counter is their sum, so
    // a reduction that kept only one plan's measurement is short here.
    let mut consumed = consumed_by_settled_compaction(&promoted, &mut supervisor, 1).await;
    assert_eq!(
        small_files_volume(&telemetry, "bifrost_forge_input_files_total"),
        consumed as u64,
        "an ordinary multi-plan success reports every committed plan's inputs"
    );
    assert!(
        small_files_volume(&telemetry, "bifrost_forge_input_bytes_total") > 0
            && small_files_volume(&telemetry, "bifrost_forge_output_bytes_total") > 0,
        "the committed plans reported the bytes they moved"
    );

    // No plan learns its acceptance, so the attempt settles nothing at all:
    // it is released, and the durable recovery that follows reports the volume
    // of whichever operation it proves live.
    promote_more_inputs(&promoted, &mut supervisor).await;
    catalog.stall_next_commit_responses(8);
    consumed += consumed_by_recovered_compaction(&promoted, &mut supervisor, &catalog).await;
    let after_recovery = small_files_volume(&telemetry, "bifrost_forge_input_files_total");
    assert!(
        after_recovery > 0 && after_recovery <= consumed as u64,
        "a durably recovered plan is counted, and no unproved sibling is: \
         {after_recovery} of {consumed}"
    );

    // One ordinary success beside one unresolved sibling: the task settles at
    // once on what it knows, and the sibling stays uncounted until the
    // table-wide owner proves it.
    promote_more_inputs(&promoted, &mut supervisor).await;
    catalog.stall_next_commit_responses(1);
    consumed += consumed_by_settled_compaction(&promoted, &mut supervisor, 3).await;
    catalog.stall_next_commit_responses(0);
    assert!(
        small_files_volume(&telemetry, "bifrost_forge_input_files_total") < consumed as u64,
        "the still-Prepared sibling is not counted before it is proved"
    );

    // The takeover only starts once the uncertainty bound and the reclaim
    // backoff lapse, measured from wall clock because the operations carry
    // database timestamps taken while their attempts ran.
    let settled_at = chrono::Utc::now()
        + chrono::Duration::from_std(promoted.fixture.config.uncertainty_bound)
            .expect("the uncertainty bound is representable")
        + chrono::Duration::seconds(1);
    control
        .set(settled_at)
        .expect("manual Forge clock advances");
    promoted.fixture.expire_claims().await;
    supervisor.reclaim_expired_claims().await;
    promoted.fixture.clear_task_backoff().await;
    supervisor.restart_worker();
    supervisor.schedule_only().await;
    supervisor.start_worker();
    await_operation_phase(
        &promoted.fixture,
        &operation_phases(&promoted.fixture)
            .await
            .into_iter()
            .filter(|(_, phase)| phase == "prepared")
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>(),
        &["recovered"],
    )
    .await;
    supervisor.stop_worker().await;
    let settled_total = small_files_volume(&telemetry, "bifrost_forge_input_files_total");
    assert_eq!(
        settled_total, consumed as u64,
        "recovery adds only the sibling volume nobody had counted yet"
    );

    // One further reconciliation pass and replan changes nothing beyond the
    // inputs it consumes itself: a second owner counting the same proof, or a
    // repeated reconciliation, would show up as a counter that outruns the
    // files the table lost.
    let idle = promoted.fixture.live_data_paths().await;
    supervisor.restart_worker();
    supervisor.schedule_only().await;
    supervisor.start_worker();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    supervisor.shutdown().await;
    let consumed_by_extra_pass = consumed_paths(&idle, &promoted.fixture.live_data_paths().await);
    assert_eq!(
        small_files_volume(&telemetry, "bifrost_forge_input_files_total"),
        settled_total + u64::try_from(consumed_by_extra_pass).expect("a consumed path count fits"),
        "no plan is counted twice by a later pass"
    );
}

/// Counts the live inputs one window removed.
fn consumed_paths(before: &BTreeSet<String>, after: &BTreeSet<String>) -> usize {
    before.difference(after).count()
}

/// A plan whose input left the current head is refused, never republished.
///
/// A retained planning snapshot is what lets a plan compose on a head that
/// moved for an unrelated reason. It is not permission to replace an input a
/// concurrent writer already replaced: doing that would publish the same source
/// rows a second time under a new snapshot. Two independent authorities have to
/// refuse it, and both are exercised here against the real catalog.
///
/// The first is explicit and pre-submit. The attempt is held between its
/// managed handoff and its publication while a competing writer replaces every
/// live data file of the table, so when the held plan reacquires authority its
/// own inputs are gone from the head. It must refuse before any `update_table`
/// call, close its operation as never-published, and carry its outputs out as
/// merely possible orphans.
///
/// The second closes the window the first cannot: a definite conflict makes an
/// attempt re-encode its plan against the head it lost to, and the inputs it
/// chose are gone from that head. The pinned transaction publication builds
/// refuses that request itself, before the catalog is asked anything.
///
/// # Panics
///
/// Panics when a stale plan submits or lands a catalog update, when a refused
/// plan leaves its operation open, or when any source row becomes readable more
/// than once.
#[tokio::test]
async fn stale_planned_input_cannot_be_republished() {
    let promoted = PromotedRewriteFixture::start_unpromoted("compaction_stale_input").await;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let mut supervisor = SupervisedPromotion::start_serial(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    promoted.fixture.seal_more(2).await;
    supervisor.run_one_success().await;

    let sources = live_row_values(&promoted).await;
    assert!(
        !sources.is_empty(),
        "the promoted table publishes the seeded rows"
    );

    // The explicit current-head check: the head moves while the attempt holds
    // its handoff, so every plan of it is stale by the time it asks to publish.
    let operations_before = promoted.fixture.rewrite_operations().await.len();
    let attempts_before = catalog.attempts();
    supervisor.restart_worker();
    let refusal = supervisor
        .run_one_failure_holding_handoff(replace_every_live_data_file(&promoted))
        .await;

    assert!(
        refusal.contains("InputsChanged"),
        "a plan whose input left the head is refused for exactly that reason: {refusal}"
    );
    assert_eq!(
        catalog.attempts(),
        attempts_before,
        "no stale plan reached the catalog at all: {refusal}"
    );
    let operations = promoted.fixture.rewrite_operations().await;
    assert_eq!(
        operations.len(),
        operations_before,
        "a refusal reached before the Prepared audit leaves no operation for a \
         successor to reconcile: {operations:?}"
    );
    assert!(
        supervisor
            .last_possible_rewrite_outputs()
            .is_some_and(|outputs| !outputs.is_empty()),
        "the refused outputs leave as possible orphan evidence, not as a commit"
    );
    assert_eq!(
        live_row_values(&promoted).await,
        sources,
        "the competing replacement is the only live copy of each source row"
    );

    // The commit-time backstop. A definite conflict makes an attempt reload
    // the table and re-encode its plan against the head it actually lost to.
    // That re-encoded request still names the inputs the losing plan chose, so
    // the pinned transaction — not the catalog — has to be the authority that
    // refuses them. Encoding one here the way publication does proves the
    // refusal is the pinned action's own.
    let stale_inputs = promoted.live_data_files().await;
    let snapshots_before = promoted.snapshot_count().await;
    replace_every_live_data_file(&promoted).await;

    let reloaded = promoted.load_table().await;
    let transaction = iceberg::transaction::Transaction::new(&reloaded);
    let action = transaction
        .rewrite_files()
        .set_enable_delete_filter_manager(false)
        .set_check_file_existence(true)
        .delete_files(stale_inputs);
    let republished = iceberg::transaction::ApplyTransactionAction::apply(action, transaction)
        .expect("a revalidated stale request still encodes")
        .commit(promoted.fixture.catalog.iceberg_catalog().as_ref())
        .await;
    assert!(
        republished.is_err(),
        "a revalidated plan whose inputs left the head is refused by the \
         transaction itself, before any catalog decision"
    );
    assert_eq!(
        promoted.snapshot_count().await,
        snapshots_before + 1,
        "only the competing writer's own replacement reached the table"
    );
    assert_eq!(
        live_row_values(&promoted).await,
        sources,
        "no source row is published twice by a raced stale commit"
    );

    supervisor.shutdown().await;
}

/// Reads every source row the table currently publishes, in ascending order.
///
/// Row identity rather than file identity is what a republished stale input
/// would corrupt, so the liveness assertions are made against values.
///
/// # Panics
///
/// Panics when a live object cannot be read as Parquet.
async fn live_row_values(promoted: &PromotedRewriteFixture) -> Vec<i64> {
    let paths = promoted
        .live_data_files()
        .await
        .into_iter()
        .map(|file| file.file_path().to_owned())
        .collect::<Vec<_>>();
    promoted.object_values(&paths).await
}

/// Replaces every live data file with a byte-identical copy at a new path.
///
/// This is a competing writer, not a Forge attempt: it commits directly through
/// the real catalog, so the head genuinely moves without touching the table
/// lease the held Forge attempt still owns. Copying rather than rewriting keeps
/// the published row set identical, which is what makes "each source row is
/// readable once" a statement about republication rather than about content.
///
/// # Panics
///
/// Panics when the table holds no live data file, when an object cannot be
/// copied, or when the competing replacement is refused.
async fn replace_every_live_data_file(promoted: &PromotedRewriteFixture) {
    let table = promoted.load_table().await;
    let live = promoted.live_data_files().await;
    assert!(
        !live.is_empty(),
        "a competing writer needs at least one live file to replace"
    );
    let mut added = Vec::with_capacity(live.len());
    for file in &live {
        let replacement = format!("{}.competing.parquet", file.file_path());
        let bytes = promoted
            .fixture
            .staging
            .read(&staging_key(promoted, file.file_path()))
            .await
            .expect("a live object is readable")
            .to_bytes();
        promoted
            .fixture
            .staging
            .write(&staging_key(promoted, &replacement), bytes)
            .await
            .expect("the competing copy is writable");
        added.push(
            iceberg::spec::DataFileBuilder::default()
                .content(iceberg::spec::DataContentType::Data)
                .file_path(replacement)
                .file_format(file.file_format())
                .partition(file.partition().clone())
                .record_count(file.record_count())
                .file_size_in_bytes(file.file_size_in_bytes())
                .partition_spec_id(file.partition_spec_id())
                .build()
                .expect("the competing descriptor is publishable"),
        );
    }
    let transaction = iceberg::transaction::Transaction::new(&table);
    let action = transaction
        .rewrite_files()
        .set_enable_delete_filter_manager(false)
        .add_data_files(added)
        .delete_files(live);
    iceberg::transaction::ApplyTransactionAction::apply(action, transaction)
        .expect("the competing replacement encodes")
        .commit(promoted.fixture.catalog.iceberg_catalog().as_ref())
        .await
        .expect("the competing replacement commits");
}

/// Maps one catalog file path onto the staging operator's own key.
///
/// The catalog stores table-qualified paths while the fixture operator is
/// rooted at the warehouse, so a scenario that reads or writes an object by its
/// catalog identity has to re-anchor it the same way the fixture's own readers
/// do.
fn staging_key(promoted: &PromotedRewriteFixture, path: &str) -> String {
    let prefix = &promoted.fixture.binding.object_prefix;
    path.split_once(&format!("{prefix}/")).map_or_else(
        || path.to_owned(),
        |(_, suffix)| format!("{prefix}/{suffix}"),
    )
}

/// Deterministic bound every wait in the worker-wide admission scenario shares.
const ADMISSION_BOUND: std::time::Duration = std::time::Duration::from_mins(1);

/// Reads every Forge task this tenant holds, across all of its tables.
///
/// [`super::support::PromotionIntegrationFixture::forge_tasks`] is scoped to the
/// fixture's own table, which is exactly what a scenario about two concurrently
/// owned attempts cannot use: the second attempt belongs to a sibling table.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn tenant_tasks(
    fixture: &super::support::PromotionIntegrationFixture,
) -> Vec<(uuid::Uuid, String, String, String)> {
    tasks_of(fixture, fixture.tenant).await
}

/// Reads one named tenant's Forge task rows in durable creation order.
///
/// A scenario that has to prove one tenant's ownership does not stop another's
/// reads a tenant it does not own, which [`tenant_tasks`] cannot express.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn tasks_of(
    fixture: &super::support::PromotionIntegrationFixture,
    tenant: wyrd_spec::ids::DataTenantId,
) -> Vec<(uuid::Uuid, String, String, String)> {
    sqlx::query_as(
        "SELECT task_id, strategy, state, table_name FROM vala.forge_tasks \
         WHERE data_tenant_id = $1 ORDER BY created_at, task_id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("Forge tasks are readable")
}

/// The durable task states only a claiming worker can hold.
///
/// Ownership is a Postgres row rather than an in-process number, so a scenario
/// about what one worker owns at an instant reads exactly these states.
const OWNED_TASK_STATES: &[&str] = &["claimed", "running", "prepared"];

/// Reports whether this tenant still owes an unsettled promotion.
///
/// A scheduling pass plans every strategy at once, so a caller that keeps
/// asking for passes while a promotion is in flight also plans the compaction
/// that promotion's own output owes — and a running worker consumes it. A
/// caller that needs that debt to still exist afterwards asks for a pass only
/// while no promotion is pending.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn promotion_pending(fixture: &super::support::PromotionIntegrationFixture) -> bool {
    tenant_tasks(fixture)
        .await
        .iter()
        .any(|(_, strategy, state, _)| {
            strategy == "scribe_promotion"
                && ["ready", "claimed", "running", "prepared"].contains(&state.as_str())
        })
}

/// Counts one tenant's small-files tasks whose table name starts with `prefix`.
///
/// Earlier phases of a long scenario leave settled rows behind on other tables,
/// so a count that is about *this* phase's tables has to name them.
async fn small_files_named(
    fixture: &super::support::PromotionIntegrationFixture,
    tenant: wyrd_spec::ids::DataTenantId,
    prefix: &str,
    states: &[&str],
) -> usize {
    tasks_of(fixture, tenant)
        .await
        .into_iter()
        .filter(|(_, strategy, state, table)| {
            strategy == "small_files"
                && states.contains(&state.as_str())
                && table.starts_with(prefix)
        })
        .count()
}

/// The durable tasks this supervisor's workers released unresolved.
///
/// A released attempt leaves its task Running with nobody publishing under it,
/// so a count of what a worker owns must subtract them.
fn released_tasks(supervisor: &SupervisedPromotion) -> BTreeSet<uuid::Uuid> {
    supervisor
        .observer()
        .released_attempts_for_test()
        .into_iter()
        .collect()
}

/// Counts the tasks these tenants currently owe to a worker, across strategies.
///
/// A pull turn's allowance is spent on tasks, not on tables or strategies, so
/// the durable evidence of what one turn took is every row in a state only a
/// claiming worker can hold — minus the rows of released attempts, which stay
/// Running with nobody publishing under them until their claim lease lapses and
/// are therefore no longer part of what this worker owns.
async fn owned_tasks(
    fixture: &super::support::PromotionIntegrationFixture,
    tenants: &[wyrd_spec::ids::DataTenantId],
    supervisor: &SupervisedPromotion,
) -> Vec<(uuid::Uuid, String, String, String)> {
    let mut rows = Vec::new();
    for tenant in tenants {
        rows.extend(tasks_of(fixture, *tenant).await);
    }
    // The release set is read after the rows, never before: a release frees the
    // worker's parallelism for the next claim, so a set sampled first can miss
    // the release whose successor the row snapshot already shows.
    let released = released_tasks(supervisor);
    rows.into_iter()
        .filter(|(task_id, _, state, _)| {
            matches!(state.as_str(), "claimed" | "running" | "prepared")
                && !released.contains(task_id)
        })
        .collect()
}

/// Counts this tenant's successfully settled Scribe promotions.
async fn promotions_succeeded(fixture: &super::support::PromotionIntegrationFixture) -> usize {
    tenant_tasks(fixture)
        .await
        .into_iter()
        .filter(|(_, strategy, state, _)| strategy == "scribe_promotion" && state == "succeeded")
        .count()
}

/// Counts this tenant's small-files tasks whose state is one of `states`.
async fn small_files_in_state(
    fixture: &super::support::PromotionIntegrationFixture,
    states: &[&str],
) -> usize {
    tenant_tasks(fixture)
        .await
        .into_iter()
        .filter(|(_, strategy, task_state, _)| {
            strategy == "small_files" && states.contains(&task_state.as_str())
        })
        .count()
}

/// Polls the durable task rows until `expected` small-files tasks hold `states`.
///
/// Returns the count actually observed, which is at least `expected` on
/// success. Polling the durable rows rather than a worker-local counter is
/// deliberate: the fact under test is that one worker genuinely *owns* two
/// attempts at once, and ownership is a Postgres row, not an in-process number.
///
/// # Panics
///
/// Panics when the count is not reached inside [`ADMISSION_BOUND`], reporting
/// the durable rows it did observe.
async fn await_small_files_in_state(
    fixture: &super::support::PromotionIntegrationFixture,
    states: &[&str],
    expected: usize,
) -> usize {
    let reached = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            let observed = small_files_in_state(fixture, states).await;
            if observed >= expected {
                return observed;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    if let Ok(observed) = reached {
        return observed;
    }
    panic!(
        "{expected} small-files tasks never reached {states:?}; \
         durable tasks are the authority on ownership: {:?}",
        tenant_tasks(fixture).await
    )
}

/// Publishes both fixture tables and leaves each owing one ready rewrite.
///
/// Settling as the tables are planned would compact one of them before the
/// other owed anything, which is the opposite of the state under test, so
/// promotion is settled first and compaction is only planned.
///
/// # Panics
///
/// Panics when the two tables do not reach two simultaneously ready
/// small-files tasks.
async fn plan_two_ready_rewrites(
    promoted: &PromotedRewriteFixture,
    supervisor: &mut SupervisedPromotion,
) {
    // Two tables, each with enough hot objects that its attempt plans more than
    // one group and the worker has siblings to interleave.
    promoted.fixture.seal_more(2).await;
    promoted
        .fixture
        .register_and_seal_table("wide_fifo_b", 4)
        .await;

    // Promote both tables first, and only then plan their compaction. Settling
    // as the tables are planned would compact one of them before the other owed
    // anything, which is the opposite of the state under test. The worker is
    // stopped by every settle helper, so it is rearmed between passes.
    let mut worker_running = true;
    for _ in 0..12 {
        if promotions_succeeded(&promoted.fixture).await >= 2 {
            break;
        }
        supervisor.schedule_only().await;
        if tenant_tasks(&promoted.fixture)
            .await
            .iter()
            .all(|(_, _, state, _)| state != "ready")
        {
            continue;
        }
        if !worker_running {
            supervisor.restart_worker();
        }
        supervisor.settle_some_success().await;
        worker_running = false;
    }
    assert!(
        promotions_succeeded(&promoted.fixture).await >= 2,
        "both tables must be published before either owes compaction: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
    // Plan only. Both small-files tasks have to be ready at the same time for
    // one worker to own both of them.
    for _ in 0..8 {
        if small_files_in_state(&promoted.fixture, &["ready"]).await >= 2 {
            break;
        }
        supervisor.schedule_only().await;
    }
    assert_eq!(
        small_files_in_state(&promoted.fixture, &["ready"]).await,
        2,
        "two tables must owe compaction at once: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
}

/// One worker's FIFO bounds every attempt it owns, not one attempt at a time.
///
/// The admission budgets belong to the *worker*, so the queue, the join set,
/// and the attempt map have to outlive any single claim. Two tables owe
/// compaction debt at once, and the per-tenant claim cap is raised to two so a
/// single worker genuinely holds both durable claims. One plan is then parked
/// inside its own runner, between its managed handoff and its publication.
///
/// That park is the whole proof. A worker that drained a queue inside one claim
/// frame would be blocked there: the parked plan would hold the only event loop
/// there is, the second task could never be claimed, and nothing else could
/// publish. Because the queue, the joins, and the attempts are worker-wide, the
/// loop instead claims the sibling task, admits its plans onto the same budgets,
/// and lets them publish out of order around the parked one.
///
/// A definite catalog refusal is injected across the same window so at least
redacted
/// any-success rule then has to hold across two concurrently owned attempts:
/// every task settles successfully, every plan keeps its own operation, and the
/// conflict costs the plan a retry rather than costing the task its outcome.
///
/// The queue's own invariants — strict FIFO order under capacity pressure,
/// head-of-line blocking, uncharged waiting memory, aggregate running
/// parallelism and memory, task-scoped cancellation, and exactly-once
/// reservation release — are proved directly against the production queue by
/// `forge::managed::queue::tests`, and the offer pass's stop-on-refusal rule by
/// `forge::worker::tests::pending_capacity_refusal_stops_offer_pass`. This
/// scenario proves the part only the real worker can show: that those budgets
/// are shared across concurrently owned attempts at all.
///
/// # Panics
///
/// Panics when the worker cannot own two attempts at once, when a parked plan
/// blocks its siblings, when a task fails despite a sibling publishing, when
/// two plans share an operation identity, when an operation is left open, or
/// when the worker does not drain its joins cleanly at shutdown.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_wide_fifo_bounds_concurrent_attempts() {
    let mut promoted = PromotedRewriteFixture::start_unpromoted("wide_fifo_a").await;
    // A withheld commit answer has to end inside a test's patience rather than
    // the production minutes, because the pull-turn phase below keeps its
    // running plans in flight exactly that long. It is set well above the time
    // one turn needs to claim its allowance so a loaded machine cannot end a
    // plan before the turn that admitted it is even observable.
    promoted.fixture.config.iceberg_total_retry_timeout = std::time::Duration::from_secs(20);
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let mut supervisor = SupervisedPromotion::start_with_worker_bounds(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
        vala_bifrost_redux::forge::ForgeWorkerConfig {
            // One worker, several concurrently owned claims. The always-due
            // orphan fallback competes for the same cap, so the bound is set
            // above the two compaction claims under test rather than exactly at
            // them. Every other bound stays at the production default so the
            // budgets under test are the real ones.
            per_tenant_active_cap: 8,
redacted
            // the running bound to it is what makes one turn's claims saturate
            // execution exactly: a later turn then has no allowance of its own
            // to spend, which is the state the pull-turn phase below measures.
            max_task_parallelism: 4,
            ..vala_bifrost_redux::forge::ForgeWorkerConfig::default()
        },
    );
    let own_tenant = promoted.fixture.tenant;
    let other_tenant = promoted.fixture.seed_tenant("wide-fifo-tenant-2").await;
    plan_two_ready_rewrites(&promoted, &mut supervisor).await;
    plan_second_tenant_rewrite(&promoted, &mut supervisor, other_tenant).await;

    let inputs = promoted
        .live_data_files()
        .await
        .into_iter()
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();
    let operations_before = promoted.fixture.rewrite_operations().await.len();

    // Park one plan between its managed handoff and its publication, and refuse
    // one commit outright so a sibling also spends a revalidated retry.
    supervisor
        .observer()
        .hold_after_next_rewrite_handoff_for_test();
    catalog.reject_next_commits(1);
    promoted.fixture.offer_tasks_of(own_tenant, 0).await;
    promoted.fixture.offer_tasks_of(other_tenant, 0).await;
    // Setup left the worker stopped at its own barrier; this generation runs
    // free so the scenario, not a fixture helper, decides when it has seen
    // enough.
    supervisor.restart_worker();
    supervisor.start_worker();
    tokio::time::timeout(
        ADMISSION_BOUND,
        supervisor
            .observer()
            .wait_for_held_rewrite_handoff_for_test(),
    )
    .await
    .expect("a production plan parks at its publication authority");

    // The parked plan holds a spawned runner, not the event loop: the same
    // worker must be able to claim and admit the sibling attempt meanwhile.
    let owned =
        await_small_files_in_state(&promoted.fixture, &["claimed", "running", "prepared"], 2).await;
    assert_eq!(
        owned,
        2,
        "one worker owns both attempts while a plan is parked: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
    assert_both_tenants_owned_at_once(&promoted, other_tenant).await;

    supervisor
        .observer()
        .release_held_rewrite_handoff_for_test();
    await_small_files_in_state(&promoted.fixture, &["succeeded"], 2).await;
    assert_second_tenant_settles(&promoted, other_tenant).await;
    assert_reservations_released_once(&supervisor);
    // The worker stops rather than shuts down, because the pull-turn phase at
    // the end of this scenario runs one more generation of the same supervisor.
    supervisor.stop_worker().await;

    assert_concurrent_attempts_published(&promoted, operations_before, &inputs).await;

    cancelling_one_waiting_task_keeps_the_other_tenant(&promoted, &mut supervisor, other_tenant)
        .await;

    pull_turn_stops_at_the_tenant_allowance(&promoted, &catalog, &mut supervisor, other_tenant)
        .await;

    // A clean shutdown drains every join before the worker returns; the helper
    // panics if the production loop exits any other way.
    supervisor.shutdown().await;
}

/// Unresolved durable authority keeps its owner unready and takes no new work.
///
/// A worker that released an attempt it could not account for is still the
/// durable owner of that task: it is `Running` under this worker's name, its
/// operation is `Prepared`, and its claim is owned until the lease lapses.
/// Nothing in memory records that ambiguity, so the only thing that can gate
/// the loop is the durable row itself. Until that authority is reconciled or
/// handed on, this worker must not advertise itself and must not take new task
/// or maintenance authority — otherwise an operator and a routing gateway are
/// told that a worker whose own publication is unreconciled is fit for more
/// work.
///
/// The gate is stated in exact identities on both sides. The task that goes
/// unclaimed is named before the window opens, so "took no new authority" is a
/// fact about that row rather than about a count; and the operations the
/// release left Prepared are compared before and after, so nothing inside the
/// window republished or closed them. The window then ends the only way it may:
/// the claim lapses, the ordinary reclaim hands the authority on, and both
/// readiness and the demand the gate held back resume.
///
/// # Panics
///
/// Panics when the released owner stays ready, when it claims the task offered
/// during the unready window, when the unresolved operations stop being
/// `Prepared`, or when readiness and queued demand do not resume once the
/// claim lapses.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn released_authority_gates_readiness_and_new_claims() {
    let mut promoted = PromotedRewriteFixture::start_unpromoted("gated_readiness").await;
    // The withheld answers have to run out of publication budget inside a
    // test's patience rather than the production minutes: the release under
    // test happens exactly when this budget expires with the commits still in
    // flight.
    promoted.fixture.config.iceberg_total_retry_timeout = std::time::Duration::from_secs(2);
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    let own_tenant = promoted.fixture.tenant;
    plan_two_ready_rewrites(&promoted, &mut supervisor).await;
    // Setup leaves both tables owing a rewrite and neither offered, so each of
    // the two moments below — the release, and the demand that arrives during
    // the unready window — is a moment this scenario chose.
    promoted.fixture.offer_tasks_of(own_tenant, 3_600).await;
    let ready = ready_small_files_tasks(&promoted, own_tenant).await;
    assert!(
        ready.len() >= 2,
        "the window needs one task to release and one to offer inside it: {ready:?}"
    );
    let released_task = ready[0];
    let blocked_task = ready[1];

    release_one_unresolvable_attempt(
        &promoted,
        &catalog,
        &mut supervisor,
        own_tenant,
        released_task,
    )
    .await;
    let unresolved = assert_unready_takes_no_new_authority(
        &promoted,
        &supervisor,
        own_tenant,
        released_task,
        blocked_task,
    )
    .await;
    assert_lapsed_claim_restores_readiness(
        &promoted,
        &supervisor,
        own_tenant,
        blocked_task,
        &unresolved,
    )
    .await;
    supervisor.stop_worker().await;
    supervisor.shutdown().await;
}

/// Drives one attempt into a release nothing durable can account for.
///
/// Every plan of the attempt has its commit answer withheld past the
/// publication budget. Every plan, because one that published ordinarily would
/// settle the task by the any-success rule, which is the opposite of the state
/// under test. The attempt therefore drains with nothing it can say about its
/// own operations and is released: `Running` task, `Prepared` operations, and a
/// claim owned until its lease lapses.
///
/// The release is held at its table-lease boundary before it is allowed to
/// finish, because that hold is the one place the unresolved interval is
/// observable from outside: the attempt is already classified unresolved and
/// the release is doing local work that can block. Readiness must already be
/// false there, with the exact task still `Running` and its operation still
/// `Prepared`.
///
/// # Panics
///
/// Panics when the release does not reach its lease boundary, when readiness
/// is still advertised there, when the durable identities are not the ones the
/// release left behind, or when the attempt settles instead of being released
/// inside [`ADMISSION_BOUND`].
async fn release_one_unresolvable_attempt(
    promoted: &PromotedRewriteFixture,
    catalog: &Arc<PromotionCatalogSeam>,
    supervisor: &mut SupervisedPromotion,
    own_tenant: wyrd_spec::ids::DataTenantId,
    released_task: uuid::Uuid,
) {
    let released_before = supervisor.observer().released_attempts_for_test().len();
    catalog.stall_next_commit_responses(8);
    supervisor.restart_worker();
    supervisor.start_worker();
    supervisor
        .observer()
        .hold_before_next_lease_release_for_test();
    promoted.fixture.offer_task(released_task, 0).await;
    let held = tokio::time::timeout(
        ADMISSION_BOUND,
        supervisor.observer().wait_for_held_lease_release_for_test(),
    )
    .await;
    assert!(
        held.is_ok(),
        "the unresolvable attempt reaches its lease release: {:?} / {:?}",
        tenant_tasks(&promoted.fixture).await,
        promoted.fixture.rewrite_operations().await
    );
    assert!(
        !supervisor.is_ready(),
        "readiness is retracted before the unresolved release does blocking work: {:?}",
        promoted.fixture.rewrite_operations().await
    );
    assert_eq!(
        task_state(&promoted.fixture, own_tenant, released_task).await,
        Some("running".to_owned()),
        "the exact task is still Running at that boundary: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
    assert!(
        !prepared_operations(promoted).await.is_empty(),
        "the exact operation is still Prepared at that boundary: {:?}",
        promoted.fixture.rewrite_operations().await
    );
    supervisor.observer().release_held_lease_release_for_test();
    let released = tokio::time::timeout(ADMISSION_BOUND, async {
        while supervisor.observer().released_attempts_for_test().len() == released_before {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(
        released.is_ok(),
        "the owner releases the attempt whose answers were lost: {:?} / {:?}",
        tenant_tasks(&promoted.fixture).await,
        promoted.fixture.rewrite_operations().await
    );
    catalog.stall_next_commit_responses(0);
}

/// Asserts the released owner is unready and claims nothing new.
///
/// Readiness is retracted from the durable row alone, and the exact identities
/// the release left behind are the ones it is retracted for. `blocked_task` is
/// then made eligible inside that window and must stay `ready`: three seconds
/// is many turns of the loop's idle interval, so an owner that was going to
/// take it has had every opportunity to.
///
/// Returns the operation identities the release left `Prepared`, so the caller
/// can prove the window closed on the same ones.
///
/// # Panics
///
/// Panics when readiness is not retracted, when the released task does not stay
/// `Running`, when it left no `Prepared` operation, or when the offered task is
/// claimed.
async fn assert_unready_takes_no_new_authority(
    promoted: &PromotedRewriteFixture,
    supervisor: &SupervisedPromotion,
    own_tenant: wyrd_spec::ids::DataTenantId,
    released_task: uuid::Uuid,
    blocked_task: uuid::Uuid,
) -> BTreeSet<uuid::Uuid> {
    await_readiness(
        supervisor,
        false,
        "an owner whose own operations are unreconciled retracts readiness",
    )
    .await;
    let unresolved = prepared_operations(promoted).await;
    assert!(
        !unresolved.is_empty(),
        "the released attempt left its operations Prepared: {:?}",
        promoted.fixture.rewrite_operations().await
    );
    assert_eq!(
        task_state(&promoted.fixture, own_tenant, released_task).await,
        Some("running".to_owned()),
        "the released task stays Running under its owner: {:?}",
        tenant_tasks(&promoted.fixture).await
    );

    promoted.fixture.offer_task(blocked_task, 0).await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    assert_eq!(
        task_state(&promoted.fixture, own_tenant, blocked_task).await,
        Some("ready".to_owned()),
        "an unready owner takes no new task authority: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
    assert!(
        !supervisor.is_ready(),
        "readiness stays retracted while the operations are unreconciled: {:?}",
        promoted.fixture.rewrite_operations().await
    );
    assert_eq!(
        prepared_operations(promoted).await,
        unresolved,
        "nothing inside the unready window republished or closed the exact operations"
    );
    unresolved
}

/// Asserts the gate reopens once the unresolved claim is handed on.
///
/// The claim lapses, the ordinary reclaim the loop already runs every turn
/// takes the task back, and the authority this owner could not explain is no
/// longer its own. Readiness returns without the worker being restarted —
/// which is also what shows the gated loop kept working rather than freezing —
/// and the demand it held back becomes claimable again.
///
/// # Panics
///
/// Panics when readiness does not return or the held-back task never leaves
/// `ready` inside [`ADMISSION_BOUND`].
async fn assert_lapsed_claim_restores_readiness(
    promoted: &PromotedRewriteFixture,
    supervisor: &SupervisedPromotion,
    own_tenant: wyrd_spec::ids::DataTenantId,
    blocked_task: uuid::Uuid,
    unresolved: &BTreeSet<uuid::Uuid>,
) {
    assert!(
        !unresolved.is_empty(),
        "the window this closes was opened by a real unresolved operation"
    );
    promoted.fixture.expire_claims_of(own_tenant).await;
    await_readiness(
        supervisor,
        true,
        "readiness returns once the unresolved authority is handed on",
    )
    .await;
    let progressed = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            promoted.fixture.clear_task_backoff().await;
            if task_state(&promoted.fixture, own_tenant, blocked_task).await
                != Some("ready".to_owned())
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(
        progressed.is_ok(),
        "the demand the gate held back is claimable again: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
}

/// Lists one tenant's ready small-files task ids, in durable creation order.
///
/// A scenario that must offer exactly one task at a time names them, because
/// offering by tenant releases every unrelated row that tenant owns.
async fn ready_small_files_tasks(
    promoted: &PromotedRewriteFixture,
    tenant: wyrd_spec::ids::DataTenantId,
) -> Vec<uuid::Uuid> {
    tasks_of(&promoted.fixture, tenant)
        .await
        .into_iter()
        .filter(|(_, strategy, state, _)| strategy == "small_files" && state == "ready")
        .map(|(task_id, _, _, _)| task_id)
        .collect()
}

/// Returns every operation identity this tenant currently holds Prepared.
async fn prepared_operations(promoted: &PromotedRewriteFixture) -> BTreeSet<uuid::Uuid> {
    promoted
        .fixture
        .rewrite_operations()
        .await
        .into_iter()
        .filter(|(_, phase)| phase == "prepared")
        .map(|(id, _)| id)
        .collect()
}

/// Polls the production readiness handle until it reports `expected`.
///
/// The loop republishes the bit every turn, so a scenario reads the settled
/// answer rather than whichever turn it happened to sample.
///
/// # Panics
///
/// Panics with `context` when the bit does not reach `expected` inside
/// [`ADMISSION_BOUND`].
async fn await_readiness(supervisor: &SupervisedPromotion, expected: bool, context: &str) {
    let reached = tokio::time::timeout(ADMISSION_BOUND, async {
        while supervisor.is_ready() != expected {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "{context}");
}

/// Leaves a second tenant owing one ready rewrite on a table only it owns.
///
/// Both tenants are held out of the claim window while it is planned, because
/// the scenario needs one claimable plan per tenant at the same instant rather
/// than whichever table the scheduler happened to reach first.
///
/// # Panics
///
/// Panics when the second tenant does not owe compaction inside
/// [`ADMISSION_BOUND`].
async fn plan_second_tenant_rewrite(
    promoted: &PromotedRewriteFixture,
    supervisor: &mut SupervisedPromotion,
    other_tenant: wyrd_spec::ids::DataTenantId,
) {
    promoted
        .fixture
        .offer_tasks_of(promoted.fixture.tenant, 3_600)
        .await;
    promote_tables(promoted, supervisor, &[(other_tenant, "wide_fifo_c")]).await;
    tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            supervisor.schedule_only().await;
            if small_files_named(&promoted.fixture, other_tenant, "wide_fifo_c", &["ready"]).await
                >= 1
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("the second tenant owes compaction");
    promoted.fixture.offer_tasks_of(other_tenant, 3_600).await;
}

/// Asserts one worker durably owns both tenants' work at the same instant.
///
/// The queue, the attempt map, and the admission budgets are worker-wide rather
/// than per tenant, so a held window that contains only one tenant's attempts
/// would prove nothing about that. Both halves are read from durable rows, and
/// from one snapshot each turn, so the claim is about a single moment.
///
/// # Panics
///
/// Panics when the two tenants are never owned together inside
/// [`ADMISSION_BOUND`].
async fn assert_both_tenants_owned_at_once(
    promoted: &PromotedRewriteFixture,
    other_tenant: wyrd_spec::ids::DataTenantId,
) {
    let both = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            let mine = small_files_in_state(&promoted.fixture, OWNED_TASK_STATES).await;
            let theirs = small_files_named(
                &promoted.fixture,
                other_tenant,
                "wide_fifo_c",
                OWNED_TASK_STATES,
            )
            .await;
            if mine >= 2 && theirs >= 1 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(
        both.is_ok(),
        "one worker FIFO owns work from both tenants at the same instant: {:?} / {:?}",
        tenant_tasks(&promoted.fixture).await,
        tasks_of(&promoted.fixture, other_tenant).await
    );
}

/// Asserts the second tenant's admitted plan published and settled.
///
/// One tenant's plan spent the whole window stuck at its publication authority
/// and a definite refusal cost another a revalidated retry. Neither is this
/// tenant's, so its already-admitted work has to reach `succeeded` regardless.
///
/// # Panics
///
/// Panics when that task does not settle inside [`ADMISSION_BOUND`].
async fn assert_second_tenant_settles(
    promoted: &PromotedRewriteFixture,
    other_tenant: wyrd_spec::ids::DataTenantId,
) {
    let settled = tokio::time::timeout(ADMISSION_BOUND, async {
        while small_files_named(
            &promoted.fixture,
            other_tenant,
            "wide_fifo_c",
            &["succeeded"],
        )
        .await
            == 0
        {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(
        settled.is_ok(),
        "another tenant's stuck plan never strands this tenant's admitted work: {:?}",
        tasks_of(&promoted.fixture, other_tenant).await
    );
}

/// Asserts both concurrently owned attempts published under their own identity.
///
/// # Panics
///
/// Panics when a task failed despite a sibling publishing, when fewer than two
/// operations were published, when an operation was left open, when two plans
/// shared an identity, when a rewritten input is still live, or when a fence
/// outlived the attempt.
async fn assert_concurrent_attempts_published(
    promoted: &PromotedRewriteFixture,
    operations_before: usize,
    inputs: &BTreeSet<String>,
) {
    let settled = tenant_tasks(&promoted.fixture)
        .await
        .into_iter()
        .filter(|(_, strategy, _, _)| strategy == "small_files")
        .collect::<Vec<_>>();
    assert!(
        settled
            .iter()
            .filter(|(_, _, state, _)| state == "succeeded")
            .count()
            >= 2,
        "a definite conflict costs its plan a retry, never its task's outcome: {settled:?}"
    );

    let operations = promoted.fixture.rewrite_operations().await;
    let published = operations.len() - operations_before;
    assert!(
        published >= 2,
        "both concurrently owned attempts published: {operations:?}"
    );
    assert!(
        operations.iter().all(|(_, phase)| phase == "committed"),
        "no concurrently admitted plan left its operation open: {operations:?}"
    );
    assert_eq!(
        operations
            .iter()
            .map(|(id, _)| *id)
            .collect::<BTreeSet<_>>()
            .len(),
        operations.len(),
        "plans that ran concurrently kept their own operation identities: {operations:?}"
    );
    assert!(
        promoted
            .live_data_files()
            .await
            .into_iter()
            .map(|file| file.file_path().to_owned())
            .collect::<BTreeSet<_>>()
            .is_disjoint(inputs),
        "every rewritten input left the live set"
    );
    assert_eq!(
        promoted.fixture.live_leases().await,
        0,
        "a drained worker holds no table fence"
    );
}

/// Reads the newest small-files task one tenant holds on a named table.
///
/// Returns its identity and durable state. A scenario about one exact task has
/// to name it, because a table accumulates a settled task per compaction it
/// has already been through and only the newest one is the work under test.
async fn newest_small_files_task(
    fixture: &super::support::PromotionIntegrationFixture,
    tenant: wyrd_spec::ids::DataTenantId,
    table: &str,
) -> Option<(uuid::Uuid, String)> {
    tasks_of(fixture, tenant)
        .await
        .into_iter()
        .rfind(|(_, strategy, _, name)| strategy == "small_files" && name == table)
        .map(|(id, _, state, _)| (id, state))
}

/// Reads one exact task's durable state.
///
/// Cross-tenant isolation is a claim about identities, not counts: a scenario
/// that cancelled one task has to follow the *other* task's own row to prove it
/// was neither removed nor stalled.
async fn task_state(
    fixture: &super::support::PromotionIntegrationFixture,
    tenant: wyrd_spec::ids::DataTenantId,
    task_id: uuid::Uuid,
) -> Option<String> {
    tasks_of(fixture, tenant)
        .await
        .into_iter()
        .find(|(id, _, _, _)| *id == task_id)
        .map(|(_, _, state, _)| state)
}

/// Seals two more hot objects on this fixture's table and publishes them.
///
/// Publication moves the table's current snapshot, which is what supersedes a
/// compaction task planned against the snapshot before it.
///
/// # Panics
///
/// Panics when the promotion does not settle inside [`ADMISSION_BOUND`].
async fn publish_more_hot_objects(
    promoted: &PromotedRewriteFixture,
    supervisor: &mut SupervisedPromotion,
) {
    let before = promotions_succeeded(&promoted.fixture).await;
    promoted.fixture.seal_more(2).await;
    // One active task per table is a production bound, so a claim a stopped
    // generation still holds would leave this table's new promotion
    // unclaimable. Returning those claims through the production reclaim
    // transaction frees the table without touching any deliberately deferred
    // task's eligibility.
    promoted.fixture.expire_claims().await;
    supervisor.reclaim_expired_claims().await;
    supervisor.restart_worker();
    supervisor.start_worker();
    // Re-planned on every turn: a promotion bound to a base that a concurrent
    // compaction then moved is cancelled, which is correct and is not this
    // helper's subject, so it simply asks again against the newer base.
    let published = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            if promotions_succeeded(&promoted.fixture).await > before {
                return;
            }
            if !promotion_pending(&promoted.fixture).await {
                supervisor.schedule_only().await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    })
    .await;
    supervisor.stop_worker().await;
    assert!(
        published.is_ok(),
        "the new hot objects are published: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
}

/// Cancelling one tenant's waiting task leaves the other tenant's work whole.
///
/// A task names the exact inputs one snapshot offered. When another writer
/// publishes over that snapshot before the task is claimed, the work it names
/// no longer exists, and the production claim path cancels it durably rather
/// than compacting a base nobody is on any more. That is this repository's
/// route to cancelling a task that is still *waiting* — never claimed, never
/// started — and it is the case a worker-wide FIFO must not generalize from:
/// the other tenant's waiting task is a different identity on a different
/// table, and it has to survive the cancellation and still run.
///
/// The pure queue-side bookkeeping — that cancelling one task's waiting entries
/// leaves every other key untouched — is proved directly against the production
/// queue by `forge::managed::queue::tests`. What only the real worker can show
/// is that the durable cancellation of one tenant's waiting task neither loses
/// nor blocks the other tenant's, and that is what this proves.
///
/// # Panics
///
/// Panics when the superseded task is not cancelled, or when the other
/// tenant's task is lost or does not run.
async fn cancelling_one_waiting_task_keeps_the_other_tenant(
    promoted: &PromotedRewriteFixture,
    supervisor: &mut SupervisedPromotion,
    other_tenant: wyrd_spec::ids::DataTenantId,
) {
    let own = promoted.fixture.tenant;
    let table = promoted.fixture.binding.table_name.clone();
    promoted.fixture.clear_task_backoff().await;
    // The other tenant owes compaction on a table only it owns, and it is held
    // out of every claim window until the cancellation below: work that ran
    // before the cancellation would prove nothing about surviving it.
    promote_tables(promoted, supervisor, &[(other_tenant, "fifo_cancel_b")]).await;
    tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            supervisor.schedule_only().await;
            if small_files_named(&promoted.fixture, other_tenant, "fifo_cancel_b", &["ready"]).await
                >= 1
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("the other tenant owes compaction");
    promoted.fixture.offer_tasks_of(other_tenant, 3_600).await;

    // This tenant owes compaction planned against the base it can see now.
    publish_more_hot_objects(promoted, supervisor).await;
    promoted.fixture.clear_task_backoff().await;
    let superseded = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            supervisor.schedule_only().await;
            if let Some((task_id, state)) =
                newest_small_files_task(&promoted.fixture, own, &table).await
                && state == "ready"
            {
                return task_id;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("this tenant owes compaction against the base it can see");

    // Held out of every claim window while its base moves under it, so the task
    // that is cancelled below is one that never ran.
    promoted.fixture.offer_tasks_of(own, 3_600).await;
    publish_more_hot_objects(promoted, supervisor).await;

    // Both tenants' compaction is claimable to one worker at the same moment.
    promoted.fixture.offer_tasks_of(own, 0).await;
    promoted.fixture.offer_tasks_of(other_tenant, 0).await;
    let (queued, queued_state) =
        newest_small_files_task(&promoted.fixture, other_tenant, "fifo_cancel_b")
            .await
            .expect("the other tenant owes compaction");
    assert_eq!(
        queued_state,
        "ready",
        "the other tenant's task is waiting when the cancellation happens: {:?}",
        tasks_of(&promoted.fixture, other_tenant).await
    );
    assert_eq!(
        task_state(&promoted.fixture, own, superseded).await,
        Some("ready".to_owned()),
        "the cancelled task is waiting, not running: {:?}",
        tasks_of(&promoted.fixture, own).await
    );

    supervisor.restart_worker();
    supervisor.start_worker();
    let settled = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            if task_state(&promoted.fixture, own, superseded)
                .await
                .as_deref()
                == Some("cancelled")
                && task_state(&promoted.fixture, other_tenant, queued)
                    .await
                    .as_deref()
                    == Some("succeeded")
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    supervisor.stop_worker().await;
    assert!(
        settled.is_ok(),
        "one tenant's waiting task is cancelled and the other tenant's queued \
         identity is still present and still runs: {:?} / {:?}",
        tasks_of(&promoted.fixture, own).await,
        tasks_of(&promoted.fixture, other_tenant).await
    );
}

/// Registers, seals, and publishes every named table before it owes compaction.
///
/// Passes are requested only while some table still has no promotion planned at
/// all. Every further pass would also plan compaction for the tables already
/// published, and the running worker would compact away the very debt the
/// caller needs them to owe.
///
/// # Panics
///
/// Panics when a table does not publish inside [`ADMISSION_BOUND`].
async fn promote_tables(
    promoted: &PromotedRewriteFixture,
    supervisor: &mut SupervisedPromotion,
    tables: &[(wyrd_spec::ids::DataTenantId, &str)],
) {
    for (tenant, name) in tables {
        promoted
            .fixture
            // Two inputs is one compaction group, so each of these tasks admits
            // exactly one plan and spends exactly one unit of running
            // parallelism. A table with more debt would let a single claim
            // saturate the worker and end the turn before its bound is reached.
            .register_and_seal_table_for(*tenant, name, 2)
            .await;
    }
    supervisor.restart_worker();
    supervisor.start_worker();
    let settled = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            let mut unplanned = false;
            let mut unsettled = false;
            for (tenant, name) in tables {
                let promotion = tasks_of(&promoted.fixture, *tenant).await.into_iter().find(
                    |(_, strategy, _, table)| strategy == "scribe_promotion" && table == name,
                );
                match promotion {
                    None => {
                        unplanned = true;
                        unsettled = true;
                    }
                    Some((_, _, state, _)) => unsettled |= state != "succeeded",
                }
            }
            if !unsettled {
                return;
            }
            if unplanned {
                supervisor.schedule_only().await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    })
    .await;
    supervisor.stop_worker().await;
    assert!(
        settled.is_ok(),
        "every registered table publishes before it owes compaction: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
}

redacted
///
/// The allowance is `(max_task_parallelism - running) .min(4)`, and a turn that
/// failed to spend a unit per claim would keep claiming past it. Proving that
/// needs more claimable tasks than one turn may take, across two tenants so the
/// per-tenant cap is not what stops it, and it needs every admitted plan to
/// stay running so no freed capacity can be mistaken for an overrun.
///
/// # Panics
///
/// Panics when the tables do not owe compaction together, when an earlier
/// phase's claim is still held as the turn begins, when a turn claims more than
/// four tasks, or when it leaves nothing claimable behind.
async fn pull_turn_stops_at_the_tenant_allowance(
    promoted: &PromotedRewriteFixture,
    catalog: &Arc<PromotionCatalogSeam>,
    supervisor: &mut SupervisedPromotion,
    other_tenant: wyrd_spec::ids::DataTenantId,
) {
    let own = promoted.fixture.tenant;
    promote_tables(
        promoted,
        supervisor,
        &[
            (own, "pull_bound_a"),
            (own, "pull_bound_b"),
            (own, "pull_bound_c"),
            (own, "pull_bound_d"),
            (own, "pull_bound_e"),
            (own, "pull_bound_f"),
            (own, "pull_bound_g"),
            (own, "pull_bound_h"),
            (other_tenant, "pull_bound_t2"),
        ],
    )
    .await;

    // Plan only: every table has to owe compaction at the same moment for one
    // pull turn to be able to overrun its allowance.
    promoted.fixture.clear_task_backoff().await;
    let ready = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            supervisor.schedule_only().await;
            let mine = small_files_named(&promoted.fixture, own, "pull_bound", &["ready"]).await;
            let theirs =
                small_files_named(&promoted.fixture, other_tenant, "pull_bound", &["ready"]).await;
            if mine >= 4 && theirs >= 1 {
                return (mine, theirs);
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("every table owes compaction at once");
    assert!(
        ready.0 + ready.1 >= 5 && ready.1 >= 1,
        "more tasks than one turn may claim are ready, across both tenants: {ready:?}"
    );

    // Withholding every commit answer keeps each admitted plan running, so the
    // worker's running parallelism stays saturated and no later turn has any
    // allowance of its own to spend.
    catalog.stall_next_commit_responses(64);
    let tenants = [own, other_tenant];
    // Rows an earlier phase left claimed belong to a worker that has already
    // stopped, for either tenant. Returning them through the production reclaim
    // transaction is what makes every claim counted below one this turn
    // actually took.
    promoted.fixture.expire_claims_of(own).await;
    promoted.fixture.expire_claims_of(other_tenant).await;
    supervisor.reclaim_expired_claims().await;
    promoted.fixture.clear_task_backoff().await;
    hold_out_earlier_phases(promoted, supervisor, own, other_tenant, &tenants).await;
    let claims_before = claims_recorded(supervisor);
    let ended_before = attempts_ended(supervisor);
    supervisor.restart_worker();
    supervisor.start_worker();
    let mut window_claims = claims_before;
    let mut window_ended = ended_before;
    let mut saturated = false;
    let mut still_ready = 0;
    for _ in 0..60 {
        let claims = claims_recorded(supervisor);
        // Read after the claims, never before: an end sampled first can miss
        // the release whose freed allowance the claim count already shows.
        let ended = attempts_ended(supervisor);
        assert!(
            claims - window_claims <= 4,
redacted
            claims - window_claims
        );
        if claims - claims_before >= 4 && !saturated {
            saturated = true;
            still_ready = small_files_named(&promoted.fixture, own, "pull_bound", &["ready"]).await;
        }
        if ended > window_ended {
            // An attempt gave its allowance back, so the next claims belong to
            // a turn with room of its own and the bound is measured from here.
            window_ended = ended;
            window_claims = claims;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        saturated,
        "the first pull turn claims its full allowance: {:?} / {:?}",
        tasks_of(&promoted.fixture, own).await,
        tasks_of(&promoted.fixture, other_tenant).await
    );
    assert!(
        still_ready >= 1,
        "the tasks the turn did not claim are still ready for the next one"
    );

    supervisor.stop_worker().await;
    catalog.stall_next_commit_responses(0);
}

/// Holds every earlier phase's table out of the turn under test.
///
/// A row an earlier phase left behind that becomes claimable again spends an
/// allowance this phase is counting, and once claimed it stands as owned across
/// every later turn, because an attempt that learns nothing leaves its task
/// Running until the claim lease lapses. Each is held out by name, and the
/// precondition that nothing is owned when the turn starts is asserted here.
///
/// # Panics
///
/// Panics when a claim from an earlier phase is still held.
async fn hold_out_earlier_phases(
    promoted: &PromotedRewriteFixture,
    supervisor: &SupervisedPromotion,
    own: wyrd_spec::ids::DataTenantId,
    other_tenant: wyrd_spec::ids::DataTenantId,
    tenants: &[wyrd_spec::ids::DataTenantId],
) {
    // The tables earlier phases used are not part of the turn under test, and a
    // row of theirs that becomes claimable again would spend an allowance this
    // phase is counting — and, once claimed, would stand as owned across every
    // later turn, because an attempt that learns nothing leaves its task
    // Running until the claim lease lapses. Each is held out by name.
    for (tenant, table) in [
        (other_tenant, "wide_fifo_c"),
        (other_tenant, "fifo_cancel_b"),
        (own, "wide_fifo_a"),
        (own, "wide_fifo_b"),
    ] {
        if let Some((task_id, _)) = newest_small_files_task(&promoted.fixture, tenant, table).await
        {
            promoted.fixture.offer_task(task_id, 3_600).await;
        }
    }
    assert_eq!(
        owned_tasks(&promoted.fixture, tenants, supervisor)
            .await
            .len(),
        0,
        "no earlier claim is still held when the turn under test starts: {:?} / {:?}",
        tasks_of(&promoted.fixture, own).await,
        tasks_of(&promoted.fixture, other_tenant).await
    );
    // What one turn claimed is read from the worker's own claim record, not
    // from durable rows: an attempt that ends without learning its outcome
    // frees its parallelism for the next turn while its task stays Running
    // until the claim lease lapses, so standing rows outnumber what any turn
    // took. Claims taken before the first attempt ends are exactly one turn's.
}

/// Counts the task claims this worker has recorded since it started.
///
/// The claim record is written by the production pull immediately after the
/// durable claim, which makes it the only account of what one turn took that a
/// released attempt cannot inflate.
fn claims_recorded(supervisor: &SupervisedPromotion) -> usize {
    supervisor
        .observer()
        .lifecycle_events()
        .into_iter()
        .filter(|event| matches!(event, ForgeLifecycleEvent::Claimed { .. }))
        .count()
}

/// Counts the attempts that have given their parallelism back, however they ended.
///
/// An attempt frees its allowance either by completing its durable terminal
/// transition or by being released with an operation it cannot account for.
/// Both are what lets a later turn claim again, so a pull-turn bound is only
/// measurable while this count has not moved.
fn attempts_ended(supervisor: &SupervisedPromotion) -> usize {
    let observer = supervisor.observer();
    observer
        .lifecycle_events()
        .into_iter()
        .filter(|event| matches!(event, ForgeLifecycleEvent::Terminal { .. }))
        .count()
        + observer.released_attempts_for_test().len()
}

/// Reads the commit-submission count once the attempt's own plans stop adding.
///
/// Readiness is retracted the moment the first unknown outcome is stored, so
/// the plans an attempt already admitted may still be reaching the seam: an
/// owner that cannot account for one operation keeps the work it started moving
/// and only stops taking more. Reading the count after it has stopped moving is
/// what makes a later quiet window describe reconciliation alone.
///
/// The lull has to outlast a definite refusal's revalidated retry, which is why
/// it is the publication budget rather than a few hundred milliseconds: a
/// shorter sample can land between a sibling's retries and read a count that is
/// still moving.
///
/// # Panics
///
/// Panics when submissions never stop inside [`ADMISSION_BOUND`].
async fn await_settled_submissions(catalog: &PromotionCatalogSeam) -> usize {
    tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            let before = catalog.attempts();
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if catalog.attempts() == before {
                return before;
            }
        }
    })
    .await
    .expect("the attempt's own admitted plans stop reaching the catalog")
}

/// Reads the phase of every rewrite operation this tenant holds, by id.
///
/// The retained-ambiguity contract is stated in operation identities: the same
/// UUIDs must still be there, still Prepared, after a reconciliation pass that
/// resubmitted nothing.
async fn operation_phases(
    fixture: &super::support::PromotionIntegrationFixture,
) -> std::collections::BTreeMap<uuid::Uuid, String> {
    fixture.rewrite_operations().await.into_iter().collect()
}

/// Polls the durable operation rows until every one of `ids` holds one of
/// `phases`.
///
/// A caller that proves classification rather than one exact outcome passes
/// every terminal phase it accepts: an operation whose commit never reached the
/// catalog is `reset`, one whose commit landed is `recovered`, and both prove
/// the row was read and settled.
///
/// # Panics
///
/// Panics when a phase is not reached inside [`ADMISSION_BOUND`].
async fn await_operation_phase(
    fixture: &super::support::PromotionIntegrationFixture,
    ids: &BTreeSet<uuid::Uuid>,
    phases: &[&str],
) {
    let reached = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            let held = operation_phases(fixture).await;
            if ids.iter().all(|id| {
                held.get(id)
                    .is_some_and(|phase| phases.contains(&phase.as_str()))
            }) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(
        reached.is_ok(),
        "operations {ids:?} never reached one of {phases:?}: {:?}",
        operation_phases(fixture).await
    );
}

/// Unknown acceptance leaves a Running task for durable recovery to settle.
///
/// A lost answer is not a failure. The operation is open, its outputs may be
/// live, and the only thing that can close it is proof about that exact UUID.
/// So the attempt is neither retried nor failed: it is released, leaving the
/// task Running with its operation Prepared and its claim lease unrenewed —
/// the durable trio the table-wide reconciliation owner classifies. There is
/// no in-memory copy of that ambiguity, so a graceful stop and a kill produce
/// the same residue and travel the same recovery path.
///
/// Three shapes are proved against one production worker:
///
/// 1. A commit that runs out of publication budget while it is still in flight
///    leaves nothing landed. The task stays Running and resubmits nothing, and
///    only once the uncertainty bound lapses does reconciliation prove the
///    operation absent and Reset it.
/// 2. A commit the catalog accepted whose answer was lost leaves the
///    replacement live. Reconciliation Recovers it rather than republishing.
/// 3. One ordinary success beside one ambiguous sibling settles the task on
///    the spot: the any-success reduction decides it and the Prepared sibling
///    is left to the table-wide owner.
///
/// # Panics
///
/// Panics when a released attempt fails or retries instead of staying Running,
/// when reconciliation resubmits a commit, when an operation identity changes,
/// or when a proven operation does not reach its exact terminal phase.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn acceptance_unknown_recovers_from_durable_state() {
    let mut promoted = PromotedRewriteFixture::start_unpromoted("unknown_acceptance").await;
    // A parked commit has to run out of publication budget while the scenario
    // is still watching, so the budget is the seconds a test can wait rather
    // than the production minutes.
    promoted.fixture.config.iceberg_total_retry_timeout = std::time::Duration::from_secs(2);
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let (clock, control) = super::support::manual_clock();
    // Serial, because every fault here is injected by commit count: a
    // concurrent sibling would make which plan received it unknowable.
    let mut supervisor = SupervisedPromotion::start_serial(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    supervisor.run_one_success().await;

    unresolved_commit_resets_once_absence_is_provable(
        &promoted,
        &catalog,
        &mut supervisor,
        &control,
    )
    .await;

    landed_replacement_recovers_into_success(&promoted, &catalog, &mut supervisor).await;
    known_success_leaves_its_ambiguous_sibling_open(&promoted, &catalog, &mut supervisor).await;
    crash_recovery_reconciles_the_exact_operation(
        &promoted,
        &catalog,
        &object_store,
        &mut supervisor,
        &control,
    )
    .await;
    supervisor.shutdown().await;
    assert_eq!(
        promoted.fixture.live_leases().await,
        0,
        "every settled attempt released its table fence"
    );
}

/// Proves a commit that never landed is Reset only once absence is provable.
///
/// The plan runs out of publication budget while its call is still in flight,
/// so nothing landed and nothing can say so. The attempt is released, and the
/// operation it opened is durable state nobody in this process owns: it stays
/// Prepared until the claim lease lapses, a successor reads it, and the
/// uncertainty bound has passed so absence is provable.
///
/// # Panics
///
/// Panics when the released attempt resubmits its operation, settles its task,
/// changes an identity, or when the operation never reaches `reset`.
async fn unresolved_commit_resets_once_absence_is_provable(
    promoted: &PromotedRewriteFixture,
    catalog: &Arc<PromotionCatalogSeam>,
    supervisor: &mut SupervisedPromotion,
    control: &vala_bifrost_redux::forge::ForgeClockControl,
) {
    // 1. Nothing landed, and nobody can say so yet.
    catalog.park_next_commit();
    supervisor.restart_worker();
    supervisor.start_worker();
    supervisor.schedule_only().await;
    tokio::time::timeout(ADMISSION_BOUND, catalog.wait_for_parked_commit())
        .await
        .expect("one plan reaches the catalog");
    // A sibling that published would settle the task on its own, which is the
    // *other* case. Refusing every later commit outright leaves this attempt
    // with exactly one thing it cannot explain.
    catalog.reject_remaining_commits();
    let unresolved = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            let open = operation_phases(&promoted.fixture).await;
            if open.values().any(|phase| phase == "prepared") {
                return open
                    .into_iter()
                    .filter(|(_, phase)| phase == "prepared")
                    .map(|(id, _)| id)
                    .collect::<BTreeSet<_>>();
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the parked plan opened its operation");
    await_small_files_in_state(&promoted.fixture, &["claimed", "running"], 1).await;

    // The budget lapses, the plan returns with acceptance unknown, and the
    // attempt is released rather than settled: the task stays Running, and the
    // operation it opened is now durable state nobody in this process owns.
    await_settled_submissions(catalog).await;
    assert_eq!(
        small_files_in_state(&promoted.fixture, &["claimed", "running"]).await,
        1,
        "an unresolved operation keeps its task Running: {:?}",
        tenant_tasks(&promoted.fixture).await
    );

    // Nothing changes over a window several reconciliation passes fall inside.
    // The window is stated in this operation's own durable identity, not in the
    // seam's commit count: the worker keeps taking other work, and a promotion
    // of its own commits across the same seam. What proves nothing resubmitted
    // *this* operation is that the exact ids are still there, still Prepared,
    // with their task still Running.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    assert_eq!(
        small_files_in_state(&promoted.fixture, &["claimed", "running"]).await,
        1,
        "repeated reconciliation of an unproven operation settles nothing: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
    assert_eq!(
        operation_phases(&promoted.fixture)
            .await
            .into_iter()
            .filter(|(_, phase)| phase == "prepared")
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>(),
        unresolved,
        "recovery classifies the exact identities the released attempt minted"
    );

    // Only age makes absence provable, so the terminal is reached exactly when
    // the uncertainty bound lapses and not before.
    control
        .set(
            chrono::Utc::now()
                + chrono::Duration::from_std(promoted.fixture.config.uncertainty_bound)
                    .expect("the uncertainty bound is representable")
                + chrono::Duration::seconds(1),
        )
        .expect("manual Forge clock advances");
    // The third leg of the durable trio: the released attempt renews nothing,
    // so its claim lease lapses on its own. Aging it here is that lapse, and it
    // is what lets a reconciliation owner read the operation at all.
    promoted
        .fixture
        .expire_claims_of(promoted.fixture.tenant)
        .await;
    supervisor.reclaim_expired_claims().await;
    promoted.fixture.clear_task_backoff().await;
    await_operation_phase(&promoted.fixture, &unresolved, &["reset"]).await;
    supervisor.stop_worker().await;
    // A release reports nothing, and the successor that recovered the operation
    // was still working when this worker was stopped, so the stop's own error is
    // the only one an attempt is allowed to have returned here.
    assert!(
        supervisor
            .returned_errors()
            .iter()
            .all(|error| error.contains("shut down")),
        "a released attempt reports no failure; its operation is settled from durable state: {:?}",
        supervisor.returned_errors()
    );
}

/// Proves that a landed replacement whose answer never arrived recovers.
///
/// The seam hands the commit to a task the caller cannot cancel, so the
/// replacement lands exactly as it would have while the publication budget ends
/// the call with nothing learned. The attempt then releases the task, and the
/// successor that reclaims the lapsed claim proves the operation live from
/// retained evidence and settles it Succeeded — and the first proof is enough,
/// so a sibling that is still unresolved is left open for the table-wide owner
/// rather than failed or reset.
///
/// # Panics
///
/// Panics when an exact recovery is retried instead of settled, or when a
/// proven operation is not closed as recovered.
async fn landed_replacement_recovers_into_success(
    promoted: &PromotedRewriteFixture,
    catalog: &Arc<PromotionCatalogSeam>,
    supervisor: &mut SupervisedPromotion,
) {
    // 2. The replacement landed and only the answer was lost.
    catalog.reject_next_commits(0);
    // The previous shape's task was retired by the successor that recovered it,
    // and nothing it planned ever committed, so the same small files are still
    // there to compact. Re-offering that task is what gives this shape claimable
    // work without sealing new objects, whose promotions would race the stalled
    // seam armed below for the same commit.
    promoted.fixture.reoffer_settled_small_files_task().await;
    promoted.fixture.clear_task_backoff().await;
    let landed_before = promoted.fixture.rewrite_operations().await.len();
    catalog.stall_next_commit_responses(8);
    let released_before = supervisor.observer().released_attempts_for_test().len();
    supervisor.restart_worker();
    supervisor.start_worker();
    // The attempt learns nothing and lets its task go. Only then is the durable
    // trio complete: Running task, Prepared operation, and a claim lease nobody
    // renews — which is what a successor recovers from, whether this owner
    // stopped gracefully or was killed.
    let released = tokio::time::timeout(ADMISSION_BOUND, async {
        while supervisor.observer().released_attempts_for_test().len() == released_before {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(
        released.is_ok(),
        "the owner releases the attempt whose answer was lost: {:?} / {:?}",
        tenant_tasks(&promoted.fixture).await,
        promoted.fixture.rewrite_operations().await
    );
    catalog.stall_next_commit_responses(0);
    promoted
        .fixture
        .expire_claims_of(promoted.fixture.tenant)
        .await;
    supervisor.reclaim_expired_claims().await;
    promoted.fixture.clear_task_backoff().await;
    let recovered_to_success = tokio::time::timeout(ADMISSION_BOUND, async {
        while small_files_in_state(&promoted.fixture, &["succeeded"]).await == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(
        recovered_to_success.is_ok(),
        "an exact recovery is a success, not a retry: {:?} / {:?} / {:?}",
        tenant_tasks(&promoted.fixture).await,
        supervisor.returned_errors(),
        promoted.fixture.rewrite_operations().await
    );
    supervisor.stop_worker().await;
    let recovered = promoted
        .fixture
        .rewrite_operations()
        .await
        .into_iter()
        .skip(landed_before)
        .collect::<Vec<_>>();
    assert!(
        recovered.iter().any(|(_, phase)| phase == "recovered"),
        "an exact recovery closes the operation it proved: {recovered:?}"
    );
    assert!(
        recovered
            .iter()
            .all(|(_, phase)| phase == "recovered" || phase == "prepared"),
        "the first proven operation settles the task and the rest stay open for \
         the table-wide owner, which never fails or resets them: {recovered:?}"
    );
}

/// Proves that a known success settles the task without local reconciliation.
///
/// One plan publishes ordinarily while a sibling's answer never arrives. The
/// any-success reduction decides the task on the spot, so no local
/// reconciliation runs at all and the ambiguous sibling's Prepared row stays
/// open for the existing table-wide owner.
///
/// # Panics
///
/// Panics when the ordinary success does not settle the task, or when its
/// ambiguous sibling is closed by this attempt.
async fn known_success_leaves_its_ambiguous_sibling_open(
    promoted: &PromotedRewriteFixture,
    catalog: &Arc<PromotionCatalogSeam>,
    supervisor: &mut SupervisedPromotion,
) {
    // 3. One known success beside one ambiguous sibling.
    promoted.fixture.seal_more(4).await;
    promoted.fixture.clear_task_backoff().await;
    supervisor.restart_worker();
    supervisor.start_worker();
    supervisor.schedule_only().await;
    tokio::time::timeout(ADMISSION_BOUND, async {
        while promotions_succeeded(&promoted.fixture).await < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the new hot objects are published before compaction plans them");
    supervisor.stop_worker().await;

    let succeeded_before = small_files_in_state(&promoted.fixture, &["succeeded"]).await;
    let open_before = operation_phases(&promoted.fixture).await.len();
    promoted.fixture.clear_task_backoff().await;
    catalog.stall_next_commit_responses(1);
    supervisor.restart_worker();
    supervisor.start_worker();
    supervisor.schedule_only().await;
    await_small_files_in_state(
        &promoted.fixture,
        &["succeeded"],
        succeeded_before.saturating_add(1),
    )
    .await;
    let known_success = operation_phases(&promoted.fixture)
        .await
        .into_iter()
        .skip(open_before)
        .collect::<Vec<_>>();
    assert!(
        known_success.iter().any(|(_, phase)| phase == "committed"),
        "a plan that published ordinarily settles the task on its own: {known_success:?}"
    );
    assert!(
        known_success.iter().any(|(_, phase)| phase == "prepared"),
        "its ambiguous sibling is left open for the table-wide owner, not failed: \
         {known_success:?}"
    );
}

/// Drives one attempt into an unresolved release and reports its shape.
///
/// Two plans reach the catalog and never learn what happened, and every later
/// sibling is refused outright so no success can settle the task instead. Two
/// is the smallest number that shows the release is per-attempt rather than
/// per-plan: both operations must be left open together.
///
/// The attempt is not held anywhere after this returns. The worker released it
/// the moment it drained without proof, leaving the durable trio a killed
/// process leaves — a `Running` task, its `Prepared` operations, and a claim
/// lease nobody is renewing.
///
/// Returns the operation identities the attempt left Prepared and the
/// settlement count its supervisor had already returned.
///
/// # Panics
///
/// Panics when the parked plans never open two operations.
async fn release_one_unresolved_attempt(
    promoted: &PromotedRewriteFixture,
    catalog: &Arc<PromotionCatalogSeam>,
    supervisor: &mut SupervisedPromotion,
) -> (BTreeSet<uuid::Uuid>, usize) {
    supervisor.stop_worker().await;
    // The new hot objects are published first, so the seam armed below meets
    // the rewrite's own commit rather than the promotion's.
    let promoted_before = promotions_succeeded(&promoted.fixture).await;
    promoted.fixture.seal_more(4).await;
    promoted.fixture.clear_task_backoff().await;
    supervisor.restart_worker();
    supervisor.start_worker();
    supervisor.schedule_only().await;
    tokio::time::timeout(ADMISSION_BOUND, async {
        while promotions_succeeded(&promoted.fixture).await <= promoted_before {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the new hot objects are published before compaction plans them");
    supervisor.stop_worker().await;

    let open_before = operation_phases(&promoted.fixture).await.len();
    let errors_before = supervisor.returned_errors().len();

    // One plan reaches the catalog and never learns what happened, and every
    // sibling is refused outright so no success can settle the task instead.
    catalog.park_next_commit();
    promoted.fixture.clear_task_backoff().await;
    supervisor.restart_worker();
    supervisor.start_worker();
    supervisor.schedule_only().await;
    tokio::time::timeout(ADMISSION_BOUND, catalog.wait_for_parked_commit())
        .await
        .expect("one plan reaches the catalog");
    // A second plan has to reach the same dead end. One unresolved UUID cannot
    // show an ordered walk, and the reduction that runs once after a complete
    // pass is only distinguishable from a per-operation reduction when a pass
    // has more than one operation to visit. Re-arming the park while the first
    // call is still held is what removes the race: the arm is in place before
    // this plan's publication budget releases it and its sibling starts.
    catalog.park_next_commit();
    tokio::time::timeout(ADMISSION_BOUND, catalog.wait_for_parked_commit())
        .await
        .expect("a sibling plan reaches the catalog");
    catalog.reject_remaining_commits();
    let retained = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            let open = operation_phases(&promoted.fixture).await;
            let prepared = open
                .into_iter()
                .skip(open_before)
                .filter(|(_, phase)| phase == "prepared")
                .map(|(id, _)| id)
                .collect::<BTreeSet<_>>();
            if prepared.len() >= 2 {
                return prepared;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("both parked plans opened their own operations");
    (retained, errors_before)
}

/// Proves that a killed owner's unresolved operation recovers from durable state.
///
/// A worker that cannot say what its own operation did has nothing durable it
/// may write about it: failing the task would be a guess and retrying it would
/// republish an effect that may already be live. So it closes only what is
/// local — the heartbeat and the table fence — and leaves the task Running with
/// its operations Prepared. Nothing else records the ambiguity, which is what
/// makes the recovery path one path rather than two: this scenario never stops
/// the worker gracefully at all. It replaces the process outright, the way a
/// `kill -9` does, and the successor is handed exactly the durable trio a
/// crash leaves behind — a Running task, its Prepared operations, and a claim
/// lease nobody is renewing.
///
/// Orphan deletion is the thing that must not run first. The outputs those
/// Prepared rows name may be live in a snapshot nobody has read back yet, so
/// deleting them before the operation is classified would destroy committed
/// data. The object store's delete counter is therefore asserted unmoved
/// across the whole ambiguous window, and only then is the successor allowed
/// to reconcile.
///
/// # Panics
///
/// Panics when the killed owner settled, failed, retried, or reset its
/// unresolved attempt, when it appended a terminal audit row for one of its own
/// operations, when it changed an operation identity, when any object was
/// deleted while an operation was still Prepared, or when the fence it left
/// behind is not the one a killed process leaves.
async fn crash_recovery_reconciles_the_exact_operation(
    promoted: &PromotedRewriteFixture,
    catalog: &Arc<PromotionCatalogSeam>,
    object_store: &Arc<CountingObjectStore>,
    supervisor: &mut SupervisedPromotion,
    control: &vala_bifrost_redux::forge::ForgeClockControl,
) {
    let deletes_before = object_store.deletes();
    let (unresolved, errors_before) =
        release_one_unresolved_attempt(promoted, catalog, supervisor).await;
    let unresolved_ids = unresolved.iter().copied().collect::<Vec<_>>();

    // The process is replaced, not asked to stop. The worker task is dropped
    // where it stands, so no shutdown branch runs and whatever it had in memory
    // about these operations is gone with it: everything asserted below was
    // read back out of Postgres.
    supervisor.kill_worker();

    assert_eq!(
        supervisor.returned_errors().len(),
        errors_before,
        "a killed owner settles nothing for an attempt it cannot account for: {:?}",
        supervisor.returned_errors()
    );
    assert!(
        small_files_in_state(&promoted.fixture, &["claimed", "running"]).await > 0,
        "the task stays Running for whoever recovers it: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
    let after = operation_phases(&promoted.fixture).await;
    assert!(
        unresolved
            .iter()
            .all(|id| after.get(id).is_some_and(|phase| phase == "prepared")),
        "every unresolved operation is left exactly as it was: {after:?}"
    );
    // A killed owner runs no shutdown branch, so the fence it held is still
    // there. That is the point: the fence lapses on its own, exactly like the
    // claim lease, and a successor waits it out rather than being handed it.
    assert_eq!(
        promoted.fixture.live_leases().await,
        1,
        "a killed owner leaves its table fence to lapse"
    );
    assert_eq!(
        object_store.deletes(),
        deletes_before,
        "nothing an open operation may have written is deleted while its phase \
         is still Prepared: {after:?}"
    );
    assert!(
        unresolved.len() >= 2,
        "the released attempt leaves both exercised operations open: {after:?}"
    );

    successor_reconciles_before_publishing(promoted, catalog, supervisor, control, &unresolved)
        .await;
    assert_eq!(
        object_store.deletes(),
        deletes_before,
        "the successor classifies the exact operation it inherited; it never \
         reaches deletion first"
    );
}

/// Proves a successor closes the inherited operation before it publishes again.
///
/// The parked commit is released as the definite conflict it always was, so the
/// inherited operation is provably absent from the catalog and a successor can
/// decide it. New debt is what gives the takeover something to publish — and
/// what makes "reconciled before it published" a question with an answer.
///
/// The ordering is proven by a barrier rather than by polling. The successor's
/// own rewrite is parked at the catalog *before* delegation, so while the test
/// reads the inherited operations that publication provably cannot have
/// entered the catalog — the snapshot count is asserted unchanged at the same
/// instant. Polling for a phase change and then checking snapshots reads two
/// states at two times, and a reconciliation and a publication that both land
/// between polls would leave the ordering unobserved.
///
/// # Panics
///
/// Panics when the successor's rewrite never reaches the parked boundary,
/// when the parked commit's publication has already entered the catalog, when
/// no inherited operation is reconciled by the time the successor is at that
/// boundary, or when the released rewrite never settles its own publication
/// inside [`ADMISSION_BOUND`].
async fn successor_reconciles_before_publishing(
    promoted: &PromotedRewriteFixture,
    catalog: &Arc<PromotionCatalogSeam>,
    supervisor: &mut SupervisedPromotion,
    control: &vala_bifrost_redux::forge::ForgeClockControl,
    inherited: &BTreeSet<uuid::Uuid>,
) {
    // The successor takes over the exact operation identity its predecessor
    // minted, and it closes that operation before it publishes anything new on
    // the table: a takeover that compacted first would be publishing over an
    // effect nobody has accounted for yet.
    //
    // The parked commit is released as the definite conflict it always was, so
    // the inherited operation is now provably absent from the catalog and a
    // successor can decide it. New debt is what gives the takeover something to
    // publish - and what makes "reconciled before it published" a question with
    // an answer.
    catalog.reject_parked_commit();
    catalog.reject_next_commits(0);
    // Absence is only provable once the uncertainty bound has lapsed, and this
    // fixture's clock is manual, so the successor's "now" is moved past the
    // bound for the operation it just inherited.
    control
        .set(
            chrono::Utc::now()
                + chrono::Duration::from_std(promoted.fixture.config.uncertainty_bound)
                    .expect("the uncertainty bound is representable")
                + chrono::Duration::seconds(1),
        )
        .expect("manual Forge clock advances");
    let promoted_before = promotions_succeeded(&promoted.fixture).await;
    promoted.fixture.expire_claims().await;
    // The killed owner's fence is the third thing that has to lapse before a
    // successor can take the table at all.
    promoted.fixture.expire_table_lease().await;
    promoted.fixture.seal_more(4).await;
    promoted.fixture.clear_task_backoff().await;
    supervisor.restart_worker();
    supervisor.start_worker();
    supervisor.reclaim_expired_claims().await;
    supervisor.schedule_only().await;
    // The new hot objects publish first, so a later snapshot is the rewrite the
    // ordering assertion below is about rather than a promotion.
    tokio::time::timeout(ADMISSION_BOUND, async {
        while promotions_succeeded(&promoted.fixture).await <= promoted_before {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the successor publishes the new hot objects");
    let snapshots_before = promoted.load_table().await.metadata().snapshots().count();
    // Ordering is proved at a barrier, not inferred from two polled reads that
    // a reconciliation and a publication can both slip between. The successor's
    // compaction is planned with no worker running, the existing commit park is
    // armed while nothing can yet reach the catalog, and only then is a worker
    // allowed to start: the next commit on this table therefore stops at the
    // seam *before* it publishes, and everything the operation rows say while
    // it is held is something that was already durable beforehand.
    supervisor.stop_worker().await;
    promoted.fixture.clear_task_backoff().await;
    supervisor.schedule_only().await;
    catalog.park_next_commit();
    supervisor.restart_worker();
    supervisor.start_worker();
    tokio::time::timeout(ADMISSION_BOUND, catalog.wait_for_parked_commit())
        .await
        .expect("the successor's new rewrite reaches its publication authority");

    let phases = operation_phases(&promoted.fixture).await;
    assert_eq!(
        promoted.load_table().await.metadata().snapshots().count(),
        snapshots_before,
        "the parked commit is held before delegation, so nothing it carries has \
         entered the catalog: {phases:?}"
    );
    // The first operation a takeover decides settles the task, and any other
    // unresolved row is left to the table-wide owner, so the evidence of a
    // successor-first takeover is that one of the exact inherited identities is
    // closed - not that all of them are.
    assert!(
        inherited
            .iter()
            .any(|id| phases.get(id).is_some_and(|phase| phase != "prepared")),
        "a successor reconciles the exact operation it inherited before its own \
         publication is allowed to enter the catalog: {inherited:?} / {phases:?}"
    );

    // Released as the definite conflict it always was; production re-derives
    // and publishes, which is what proves the barrier held a real publication
    // rather than a call that was never going to land.
    catalog.reject_parked_commit();
    let published = tokio::time::timeout(ADMISSION_BOUND, async {
        while promoted.load_table().await.metadata().snapshots().count() == snapshots_before {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(
        published.is_ok(),
        "the released rewrite settles its own publication: {:?} / {:?}",
        tenant_tasks(&promoted.fixture).await,
        operation_phases(&promoted.fixture).await
    );
    supervisor.stop_worker().await;
}

/// Asserts no plan completion released a reservation the queue had already freed.
///
/// The worker treats a second release of one `(task_id, plan_index)` as an
/// invariant failure and returns it, so an exactly-once violation surfaces as a
/// returned attempt error rather than as silently corrupted budgets.
///
/// # Panics
///
/// Panics when any returned attempt error reports a double release.
fn assert_reservations_released_once(supervisor: &SupervisedPromotion) {
    assert!(
        supervisor
            .returned_errors()
            .iter()
            .all(|error| !error.contains("more than once")),
        "each plan released its own reservation exactly once: {:?}",
        supervisor.returned_errors()
    );
}
