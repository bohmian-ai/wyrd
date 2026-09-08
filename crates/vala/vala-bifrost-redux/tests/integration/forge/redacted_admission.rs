redacted
//!
//! One attempt plans every eligible group, admits them through the worker-local
//! queue, and publishes each admitted plan under its own operation against the
//! head the plan before it left. These scenarios drive that through the real
//! scheduler, worker, Postgres, catalog, and warehouse; nothing here calls the
//! managed core or the publication owner directly.

use std::collections::BTreeSet;
use std::sync::Arc;

use vala_bifrost_redux::forge::{ForgeClock, ForgeObjectStore};

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
redacted
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
redacted
redacted
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
redacted
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
    // several operations it cannot account for. Its owner is stopped while they
    // are still open, and a stopping owner settles none of them: it hands the
    // Prepared authority on, which is what puts more open rows on this table
    // than one page holds.
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
    supervisor.stop_worker().await;
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
    assert!(
        operation_phases(&promoted.fixture)
            .await
            .into_values()
            .all(|phase| phase != "prepared"),
        "the takeover leaves no open operation behind: {:?}",
        operation_phases(&promoted.fixture).await
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

    // No plan learns its acceptance, so the attempt settles on the one
    // operation its own reconciliation proves live and reports that plan's
    // retained request volume.
    promote_more_inputs(&promoted, &mut supervisor).await;
    catalog.stall_next_commit_responses(8);
    consumed += consumed_by_settled_compaction(&promoted, &mut supervisor, 2).await;
    catalog.stall_next_commit_responses(0);
    let after_local_recovery = small_files_volume(&telemetry, "bifrost_forge_input_files_total");
    assert!(
        after_local_recovery > 0 && after_local_recovery <= consumed as u64,
        "a locally recovered plan is counted, and no unproved sibling is: \
         {after_local_recovery} of {consumed}"
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
redacted
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

/// Counts the tasks these tenants currently owe to a worker, across strategies.
///
/// A pull turn's allowance is spent on tasks, not on tables or strategies, so
/// the durable evidence of what one turn took is every row in a state only a
/// claiming worker can hold.
async fn owned_tasks(
    fixture: &super::support::PromotionIntegrationFixture,
    tenants: &[wyrd_spec::ids::DataTenantId],
) -> usize {
    let mut owned = 0;
    for tenant in tenants {
        owned += tasks_of(fixture, *tenant)
            .await
            .into_iter()
            .filter(|(_, _, state, _)| matches!(state.as_str(), "claimed" | "running" | "prepared"))
            .count();
    }
    owned
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

redacted
        .await;

    admitted_work_finishes_while_another_tenant_is_ambiguous(
        &promoted,
        &catalog,
        &mut supervisor,
        other_tenant,
    )
    .await;
    // A clean shutdown drains every join before the worker returns; the helper
    // panics if the production loop exits any other way.
    supervisor.shutdown().await;
}

/// One tenant's admitted plan finishes while another's outcome is unknown.
///
/// Retention is a property of one attempt, not of the worker: an owner that
/// cannot account for one tenant's operation stops taking *new* authority, and
/// that is all it stops. Work it has already admitted for every other tenant
/// has to run to completion, because freezing it would strand durable claims
/// behind an ambiguity those tenants have nothing to do with and that only a
/// table-wide owner can ever resolve.
///
/// Both halves are made exact rather than sampled. The progressing tenant's
/// plan is admitted and held immediately before its own publication, so it is
/// already inside the worker when the ambiguity appears. The commit park is
/// armed only then, and only the ambiguous tenant's task is offered afterwards,
/// so the commit that runs out of publication budget with nothing learned is
/// provably that tenant's. Nothing releases or resolves it: its exact operation
/// stays `Prepared` and readiness stays retracted for the whole of the other
/// tenant's remaining work.
///
/// This phase runs last because it leaves one attempt retained on purpose. The
/// scenario's closing shutdown hands that attempt off, which is the production
/// path a stopping owner takes for an operation it cannot explain.
///
/// # Panics
///
/// Panics when the ambiguous tenant does not store an unknown acceptance, when
/// its exact operation is resolved, when readiness is republished, or when the
/// progressing tenant's exact task does not settle while all of that holds.
async fn admitted_work_finishes_while_another_tenant_is_ambiguous(
    promoted: &PromotedRewriteFixture,
    catalog: &Arc<PromotionCatalogSeam>,
    supervisor: &mut SupervisedPromotion,
    other_tenant: wyrd_spec::ids::DataTenantId,
) {
    let own = promoted.fixture.tenant;
    // Earlier phases left claims held by workers that have since stopped and
    // rows that are claimable again. Returning those claims through the
    // production reclaim transaction and holding every existing row out of the
    // window is what makes the only claimable work in this phase its own.
    promoted.fixture.expire_claims_of(own).await;
    promoted.fixture.expire_claims_of(other_tenant).await;
    supervisor.reclaim_expired_claims().await;
    promoted.fixture.offer_tasks_of(own, 3_600).await;
    promoted.fixture.offer_tasks_of(other_tenant, 3_600).await;

    // One fresh table per tenant, published before either owes compaction. Two
    // inputs is one compaction group, so each attempt admits exactly one plan
    // and the fault each of them receives is unambiguous.
    promote_tables(
        promoted,
        supervisor,
        &[(own, "ambiguous_a"), (other_tenant, "progress_b")],
    )
    .await;

    // Plan only, and then hold every row out again: the order in which the two
    // tenants enter the worker is this scenario's to choose, not the
    // scheduler's.
    let (ambiguous_task, progress_task) = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            supervisor.schedule_only().await;
            if let (Some((ambiguous, ambiguous_state)), Some((progress, progress_state))) = (
                newest_small_files_task(&promoted.fixture, own, "ambiguous_a").await,
                newest_small_files_task(&promoted.fixture, other_tenant, "progress_b").await,
            ) && ambiguous_state == "ready"
                && progress_state == "ready"
            {
                return (ambiguous, progress);
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("both tenants owe compaction on a table only this phase uses");
    promoted.fixture.offer_tasks_of(own, 3_600).await;
    promoted.fixture.offer_tasks_of(other_tenant, 3_600).await;
    let operations_before = operation_phases(&promoted.fixture).await;

    // The progressing tenant is admitted first and held immediately before it
    // publishes, so its plan is already inside this worker when the ambiguity
    // below appears.
    supervisor
        .observer()
        .hold_after_next_rewrite_handoff_for_test();
    supervisor.restart_worker();
    supervisor.start_worker();
    promoted.fixture.offer_task(progress_task, 0).await;
    tokio::time::timeout(
        ADMISSION_BOUND,
        supervisor
            .observer()
            .wait_for_held_rewrite_handoff_for_test(),
    )
    .await
    .expect("the progressing tenant's plan is held before its publication");
    tokio::time::timeout(ADMISSION_BOUND, async {
        while !supervisor.worker_ready() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("a worker that holds no unknown outcome advertises itself");

    // The next commit to reach the catalog is therefore the ambiguous tenant's,
    // and nothing ever answers it: the publication budget ends the call with
    // nothing learned, which is the one outcome this owner cannot account for.
    catalog.park_next_commit();
    promoted.fixture.offer_task(ambiguous_task, 0).await;
    tokio::time::timeout(ADMISSION_BOUND, catalog.wait_for_parked_commit())
        .await
        .expect("the ambiguous tenant's plan reaches the catalog");
    tokio::time::timeout(ADMISSION_BOUND, async {
        while supervisor.worker_ready() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("an unknown acceptance retracts this worker's readiness");

    let phases = operation_phases(&promoted.fixture).await;
    let opened = phases
        .iter()
        .filter(|(id, phase)| phase.as_str() == "prepared" && !operations_before.contains_key(*id))
        .map(|(id, _)| *id)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        opened.len(),
        1,
        "the ambiguous tenant's single plan opened one operation: {phases:?}"
    );
    let ambiguous_operation = opened
        .into_iter()
        .next()
        .expect("the ambiguous tenant opened one operation");
    assert!(
        task_state(&promoted.fixture, own, ambiguous_task)
            .await
            .is_some_and(|state| OWNED_TASK_STATES.contains(&state.as_str())),
        "the ambiguous tenant's exact task is retained by this owner: {:?}",
        tasks_of(&promoted.fixture, own).await
    );
    assert!(
        task_state(&promoted.fixture, other_tenant, progress_task)
            .await
            .is_some_and(|state| OWNED_TASK_STATES.contains(&state.as_str())),
        "the progressing tenant's exact task is admitted and unfinished: {:?}",
        tasks_of(&promoted.fixture, other_tenant).await
    );

    // Only the progressing tenant's hold is released. The ambiguous operation
    // is neither resolved nor handed off, so this owner is holding an outcome
    // it cannot explain for the whole of the other tenant's remaining work.
    supervisor
        .observer()
        .release_held_rewrite_handoff_for_test();
    let settled = tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            assert_eq!(
                operation_phases(&promoted.fixture)
                    .await
                    .get(&ambiguous_operation)
                    .map(String::as_str),
                Some("prepared"),
                "nothing resolves the ambiguous tenant's operation while the \
                 other tenant is running"
            );
            if task_state(&promoted.fixture, other_tenant, progress_task)
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
    assert!(
        settled.is_ok(),
        "an owner holding one tenant's unknown outcome still finishes the work \
         it admitted for another: {:?} / {:?}",
        tasks_of(&promoted.fixture, other_tenant).await,
        operation_phases(&promoted.fixture).await
    );
    assert_eq!(
        operation_phases(&promoted.fixture)
            .await
            .get(&ambiguous_operation)
            .map(String::as_str),
        Some("prepared"),
        "the other tenant's task settled while this operation was still \
         unaccounted for"
    );
    assert!(
        !supervisor.worker_ready(),
        "readiness stays retracted across the other tenant's whole progress"
    );
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
redacted
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
    // The tables earlier phases used are not part of the turn under test, and a
    // row of theirs that becomes claimable again would spend an allowance this
    // phase is counting. Each is held out by name.
    for table in ["wide_fifo_c", "fifo_cancel_b"] {
        if let Some((task_id, _)) =
            newest_small_files_task(&promoted.fixture, other_tenant, table).await
        {
            promoted.fixture.offer_task(task_id, 3_600).await;
        }
    }
    assert_eq!(
        owned_tasks(&promoted.fixture, &tenants).await,
        0,
        "no earlier claim is still held when the turn under test starts: {:?} / {:?}",
        tasks_of(&promoted.fixture, own).await,
        tasks_of(&promoted.fixture, other_tenant).await
    );
    supervisor.restart_worker();
    supervisor.start_worker();
    let sample = first_turn_sample(promoted, &tenants).await;
    let Ok((held, still_ready, rows)) = sample else {
        let mine = tasks_of(&promoted.fixture, own).await;
        let theirs = tasks_of(&promoted.fixture, other_tenant).await;
        panic!("the first pull turn claims its full allowance: {mine:?} / {theirs:?}");
    };
    assert_eq!(
        held, 4,
redacted
    );
    assert!(
        still_ready >= 1,
        "the tasks the turn did not claim are still ready for the next one: {rows:?}"
    );

    // A task whose plans all settle frees its parallelism and lets the next turn
    // spend it, so the bound is a ceiling on concurrent ownership rather than a
    // total. Sampling it across the window is what proves no turn ever overran
    // the allowance, not just the one sampled above.
    for _ in 0..30 {
        let held = owned_tasks(&promoted.fixture, &tenants).await;
        assert!(
            held <= 4,
redacted
            tasks_of(&promoted.fixture, own).await,
            tasks_of(&promoted.fixture, other_tenant).await
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    supervisor.stop_worker().await;
    catalog.stall_next_commit_responses(0);
}

/// Samples one turn's ownership and its leftovers from a single snapshot.
///
/// Reading both halves of the bound from one snapshot is what makes "no more
/// than four" and "the rest stay claimable" describe the same turn rather than
/// two moments a later turn could sit between.
///
/// Returns the number of tasks a worker owned, the number of `pull_bound`
/// small-files tasks still ready beside them, and the rows both came from, or
/// the elapsed error when no turn ever reached its full allowance.
async fn first_turn_sample(
    promoted: &PromotedRewriteFixture,
    tenants: &[wyrd_spec::ids::DataTenantId],
) -> Result<(usize, usize, Vec<(uuid::Uuid, String, String, String)>), tokio::time::error::Elapsed>
{
    tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            let mut rows = Vec::new();
            for tenant in tenants {
                rows.extend(tasks_of(&promoted.fixture, *tenant).await);
            }
            let held = rows
                .iter()
                .filter(|(_, _, state, _)| {
                    matches!(state.as_str(), "claimed" | "running" | "prepared")
                })
                .count();
            if held >= 4 {
                let ready = rows
                    .iter()
                    .filter(|(_, strategy, state, table)| {
                        strategy == "small_files"
                            && state == "ready"
                            && table.starts_with("pull_bound")
                    })
                    .count();
                return (held, ready, rows);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
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

/// Unknown acceptance retains a Running task until its exact UUID is settled.
///
/// A lost answer is not a failure. The operation is open, its outputs may be
/// live, and the only thing that can close it is proof about that exact UUID.
/// So the attempt is neither retried nor failed: the task stays Running, the
/// worker stops taking new work and goes unready, and the retained attempt
/// reconciles its own operation identities until each is proven.
///
/// Three shapes are proved against one production worker:
///
/// 1. A commit that runs out of publication budget while it is still in flight
///    leaves nothing landed. The task is retained Running and resubmits
///    nothing, and only once the uncertainty bound lapses does the exact
///    reconciliation prove the operation absent and Reset it.
/// 2. A commit the catalog accepted whose answer was lost leaves the
///    replacement live. The exact reconciliation Recovers it, and that
///    recovery is a success: the task settles Succeeded, not retried.
/// 3. One ordinary success beside one ambiguous sibling never reaches local
///    reconciliation at all: the any-success reduction settles the task
///    immediately and the Prepared sibling is left to the table-wide owner.
///
/// # Panics
///
/// Panics when a retained attempt fails or retries instead of staying Running,
/// when reconciliation resubmits a commit, when an operation identity changes,
/// or when a proven operation does not reach its exact terminal phase.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn acceptance_unknown_retains_running_until_exact_reconciliation() {
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
    // attempt is retained rather than settled. Retraction of readiness is the
    // worker saying so: it has stopped claiming because it cannot account for
    // an operation it opened.
    tokio::time::timeout(ADMISSION_BOUND, async {
        while supervisor.worker_ready() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("a worker holding an unresolved operation stops advertising itself");
    let submissions = await_settled_submissions(&catalog).await;
    assert_eq!(
        small_files_in_state(&promoted.fixture, &["claimed", "running"]).await,
        1,
        "an unresolved operation keeps its task Running: {:?}",
        tenant_tasks(&promoted.fixture).await
    );

    // Nothing changes over a window several reconciliation passes fall inside.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    assert_eq!(
        catalog.attempts(),
        submissions,
        "reconciliation asks about an operation; it never resubmits one"
    );
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
        "a retained attempt keeps the operation identities it minted"
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
    await_operation_phase(&promoted.fixture, &unresolved, &["reset"]).await;
    supervisor.stop_worker().await;
    assert_eq!(
        supervisor.returned_errors().len(),
        1,
        "a retained attempt settles once, with the failure its own plan returned: {:?}",
        supervisor.returned_errors()
    );

    landed_replacement_recovers_into_success(&promoted, &catalog, &mut supervisor).await;
    known_success_leaves_its_ambiguous_sibling_open(&promoted, &catalog, &mut supervisor).await;
    shutdown_hands_off_retained_authority(&promoted, &catalog, &mut supervisor, &control).await;
    supervisor.shutdown().await;
    assert_eq!(
        promoted.fixture.live_leases().await,
        0,
        "every settled attempt released its table fence"
    );
}

/// Proves that a landed replacement whose answer never arrived recovers.
///
/// The seam hands the commit to a task the caller cannot cancel, so the
/// replacement lands exactly as it would have while the publication budget ends
/// the call with nothing learned. The retained attempt then proves its own
/// operation live and settles Succeeded — and the first proof is enough, so a
/// sibling that is still unresolved is left open for the table-wide owner
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
    promoted.fixture.clear_task_backoff().await;
    let landed_before = promoted.fixture.rewrite_operations().await.len();
    catalog.stall_next_commit_responses(8);
    supervisor.restart_worker();
    supervisor.start_worker();
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
    catalog.stall_next_commit_responses(0);
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

/// Publishes a second tenant's table and leaves this one owing more debt.
///
/// The other tenant's table is published now and its compaction offered later,
/// on purpose: work that was claimable here would be claimable before the
/// ambiguity under test exists. The extra inputs are what make the attempt
/// under test plan more than one rewrite — one to park at the catalog and one
/// to hold before it publishes.
///
/// Returns the second tenant's isolation key.
///
/// # Panics
///
/// Panics when either tenant does not publish inside [`ADMISSION_BOUND`].
async fn publish_second_tenant_and_new_debt(
    promoted: &PromotedRewriteFixture,
    supervisor: &mut SupervisedPromotion,
) -> wyrd_spec::ids::DataTenantId {
    let other = promoted.fixture.seed_tenant("gate-tenant-2").await;
    promoted
        .fixture
        .register_and_seal_table_for(other, "gate_probe", 4)
        .await;

    // The other tenant's table is published now and its compaction is offered
    // later, on purpose: work that was claimable here would be claimable before
    // the ambiguity under test exists.
    // Four more inputs, so the attempt under test plans more than one rewrite:
    // one to park at the catalog and one to hold before it publishes.
    promoted.fixture.seal_more(4).await;
    promoted.fixture.clear_task_backoff().await;
    let promoted_before = promotions_succeeded(&promoted.fixture).await;
    supervisor.restart_worker();
    supervisor.start_worker();
    supervisor.schedule_only().await;
    tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            let published = tasks_of(&promoted.fixture, other).await.into_iter().any(
                |(_, strategy, state, _)| strategy == "scribe_promotion" && state == "succeeded",
            );
            if published && promotions_succeeded(&promoted.fixture).await > promoted_before {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("both tenants publish before either owes compaction");
    supervisor.stop_worker().await;

    other
}

/// An unknown acceptance stops new authority before its siblings have drained.
///
/// Retention is a property of an attempt's own indexed outcomes, and the moment
/// one of them is an unknown acceptance this owner can no longer say what it
/// did. Waiting for the whole attempt to drain first would leave a reachable
/// window in which the worker republishes readiness and claims more work while
/// it already holds an operation it cannot account for.
///
/// One plan is parked at the catalog until its publication budget ends it with
/// nothing learned, while a sibling of the same attempt is held before its own
/// publication so the attempt cannot drain. A second tenant's task is then
/// offered: it is claimable — the per-tenant cap this owner is already at does
/// not apply to it — so the only thing that may keep it waiting is the
/// ambiguity gate itself.
///
/// # Panics
///
/// Panics when the worker stays ready while it holds an unknown outcome, or
/// when it claims the other tenant's waiting task before that outcome is
/// decided.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ambiguity_stops_new_authority_before_siblings_drain() {
    let mut promoted = PromotedRewriteFixture::start_unpromoted("ambiguity_gate").await;
    // A parked commit has to run out of publication budget while the scenario
    // is still watching, so the budget is the seconds a test can wait rather
    // than the production minutes.
    promoted.fixture.config.iceberg_total_retry_timeout = std::time::Duration::from_secs(2);
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    // Two plans of one attempt must be in flight at once, and the other
    // tenant's task must still fit the pull allowance afterwards, or capacity
    // rather than the ambiguity gate would be what holds it.
    let mut supervisor = SupervisedPromotion::start_with_worker_bounds(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
        vala_bifrost_redux::forge::ForgeWorkerConfig {
            max_task_parallelism: 4,
            ..vala_bifrost_redux::forge::ForgeWorkerConfig::default()
        },
    );
    promoted.fixture.seal_more(4).await;
    supervisor.run_one_success().await;

    let other = publish_second_tenant_and_new_debt(&promoted, &mut supervisor).await;

    // One plan is held before it publishes and another is parked at the
    // catalog, so this attempt stores an unknown outcome while a sibling of the
    // same attempt is still running.
    supervisor
        .observer()
        .hold_after_next_rewrite_handoff_for_test();
    catalog.park_next_commit();
    promoted.fixture.clear_task_backoff().await;
    // One pass plans both tenants' compaction, so the other tenant's task is
    // held out of the window before any worker runs. It is released only once
    // the ambiguity exists, which leaves the gate as the single thing that can
    // still keep it waiting.
    supervisor.schedule_only().await;
    promoted.fixture.offer_tasks_of(other, 3_600).await;
    supervisor.restart_worker();
    supervisor.start_worker();
    tokio::time::timeout(
        ADMISSION_BOUND,
        supervisor
            .observer()
            .wait_for_held_rewrite_handoff_for_test(),
    )
    .await
    .expect("one plan is held before its publication");
    tokio::time::timeout(ADMISSION_BOUND, catalog.wait_for_parked_commit())
        .await
        .expect("a sibling plan reaches the catalog");
    // No later commit may succeed: a known success would settle the attempt on
    // the spot and there would be nothing unknown left to gate on.
    catalog.reject_remaining_commits();

    // The parked plan's publication budget ends it with nothing learned. Its
    // held sibling is still running, so the attempt has not drained - and the
    // worker must already have stopped advertising itself.
    tokio::time::timeout(ADMISSION_BOUND, async {
        while supervisor.worker_ready() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("an unknown outcome retracts readiness before its siblings drain");

    // Now offer the other tenant's work. Nothing but the ambiguity gate stands
    // between this owner and that claim.
    promoted.fixture.offer_tasks_of(other, 0).await;
    let planted = tasks_of(&promoted.fixture, other)
        .await
        .into_iter()
        .find(|(_, strategy, _, _)| strategy == "small_files")
        .map(|(_, _, state, _)| state);
    assert_eq!(
        planted,
        Some("ready".to_owned()),
        "the offered task starts out claimable: {:?}",
        tasks_of(&promoted.fixture, other).await
    );

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(
        !supervisor.worker_ready(),
        "readiness stays retracted while an unknown outcome is unresolved"
    );
    assert_eq!(
        tasks_of(&promoted.fixture, other)
            .await
            .into_iter()
            .find(|(_, strategy, _, _)| strategy == "small_files")
            .map(|(_, _, state, _)| state),
        Some("ready".to_owned()),
        "an owner holding an unknown outcome takes no further authority, from \
         any tenant: {:?}",
        tasks_of(&promoted.fixture, other).await
    );

    supervisor
        .observer()
        .release_held_rewrite_handoff_for_test();
    supervisor.shutdown().await;
}

/// Drives one attempt into a retained, unresolved state and reports its shape.
///
/// Two plans reach the catalog and never learn what happened, and every later
/// sibling is refused outright so no success can settle the task instead. Two
/// is the smallest number that makes a pass a *walk*: it is what an ordered
/// visit and a once-per-pass reduction can be told apart from a per-operation
/// one on.
///
/// Returns the operation identities the attempt left Prepared and the
/// settlement count its supervisor had already returned.
///
/// # Panics
///
/// Panics when the parked plans never open two operations, or when the worker
/// keeps advertising itself while holding them.
async fn retain_one_unresolved_attempt(
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
    tokio::time::timeout(ADMISSION_BOUND, async {
        while supervisor.worker_ready() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("a worker holding an unresolved operation stops advertising itself");
    (retained, errors_before)
}

/// Asserts a retained shutdown captured the attempt's possible-output snapshot.
///
/// # Panics
///
/// Panics when no snapshot was captured, when it is empty, when an entry has no
/// path, or when the entries are not in the attempt's own output order.
fn assert_handoff_snapshot(supervisor: &SupervisedPromotion, handoffs_before: usize) {
    // The runbook's ambiguous-commit procedure starts from the object names an
    // unresolved attempt may have written, so the handoff has to carry the
    // exact ordered set - including the objects a sibling produced before its
    // own commit was refused, which nothing else records.
    let handoffs = supervisor.observer().retained_handoff_outputs_for_test();
    assert!(
        handoffs.len() > handoffs_before,
        "a retained shutdown captures one attempt-wide possible-output snapshot"
    );
    let handed_off = &handoffs[handoffs_before];
    assert!(
        !handed_off.is_empty(),
        "the captured snapshot names the objects the attempt may have written"
    );
    assert!(
        handed_off.iter().all(|output| !output.path.is_empty()),
        "every captured entry carries its exact path: {handed_off:?}"
    );
    assert!(
        handed_off
            .windows(2)
            .all(|pair| pair[0].logical_ordinal <= pair[1].logical_ordinal),
        "the captured snapshot keeps the attempt's own output order: {handed_off:?}"
    );
}

/// Reads the durable accounting a retained attempt must not change, once still.
///
/// Returns this tenant's small-files retry accounting, the terminal rewrite
/// audit count for the retained operations, the catalog submission count, and
/// the planning demand. Callers read it with the worker held inside one exact
/// reconciliation await, and every field is still sampled twice and accepted
/// only when nothing moved between the samples: a plan or sibling settlement
/// that was already in flight when the hold took effect would otherwise be
/// blamed on the stop that follows.
///
/// # Panics
///
/// Panics when durable state never stops moving inside [`ADMISSION_BOUND`].
async fn settled_retention_baseline(
    promoted: &PromotedRewriteFixture,
    catalog: &Arc<PromotionCatalogSeam>,
    retained_ids: &[uuid::Uuid],
) -> (
    Vec<(uuid::Uuid, i32, Option<String>, String)>,
    i64,
    usize,
    Vec<(String, i64, String)>,
) {
    tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            let accounting = small_files_retry_accounting(&promoted.fixture).await;
            let audit = promoted
                .fixture
                .forge_terminal_audit_count_for(retained_ids)
                .await;
            let submissions = catalog.attempts();
            let demand = planning_demand(&promoted.fixture).await;
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if accounting == small_files_retry_accounting(&promoted.fixture).await
                && audit
                    == promoted
                        .fixture
                        .forge_terminal_audit_count_for(retained_ids)
                        .await
                && submissions == catalog.attempts()
                && demand == planning_demand(&promoted.fixture).await
            {
                return (accounting, audit, submissions, demand);
            }
        }
    })
    .await
    .expect("the refused siblings stop writing before shutdown is measured")
}

/// Quiet window that separates two exact-operation reconciliation passes.
///
/// A pass issues every load it needs back to back and the worker then returns
/// to its one-second maintenance cadence, so a window this long in which the
/// seam's load count does not move is the gap between two passes rather than a
/// lull inside one.
const RECONCILIATION_PASS_GAP: std::time::Duration = std::time::Duration::from_millis(350);

/// Waits for the quiet window that follows a reconciliation pass.
///
/// Returns the seam's load count at that window, requiring it to be past
/// `after` so a caller measuring a pass cannot be handed the same gap twice.
///
/// # Panics
///
/// Panics when no gap appears inside [`ADMISSION_BOUND`].
async fn await_reconciliation_pass_gap(catalog: &PromotionCatalogSeam, after: usize) -> usize {
    tokio::time::timeout(ADMISSION_BOUND, async {
        loop {
            let observed = catalog.loads();
            tokio::time::sleep(RECONCILIATION_PASS_GAP).await;
            if observed > after && catalog.loads() == observed {
                return observed;
            }
        }
    })
    .await
    .expect("a retained owner's reconciliation passes are separated by a quiet window")
}

/// Measures reconciliation passes until two consecutive ones agree.
///
/// Returns the seam's load count at the gap that closes the last measured pass
/// and the number of loads that pass issued. Agreement is what the caller
/// needs and what it cannot assume: the attempt's plans reach their unknown
/// acceptances one publication budget at a time, so the earliest passes walk
/// fewer operations than the retained set will finally hold, and a scenario
/// that counted its way into one of those would be pausing on the wrong
/// operation of a shorter pass.
///
/// # Panics
///
/// Panics when the pass shape does not settle inside [`ADMISSION_BOUND`].
async fn stable_reconciliation_pass(catalog: &PromotionCatalogSeam) -> (usize, usize) {
    tokio::time::timeout(ADMISSION_BOUND, async {
        let mut previous = measure_reconciliation_pass(catalog).await;
        loop {
            let observed = measure_reconciliation_pass(catalog).await;
            if observed.1 == previous.1 {
                return observed;
            }
            previous = observed;
        }
    })
    .await
    .expect("a retained attempt's reconciliation pass settles on one load shape")
}

/// Measures one complete exact-operation reconciliation pass.
///
/// Returns the seam's load count at the gap that closes the measured pass and
/// the number of loads that pass issued. Both halves are what a scenario needs
/// to stand inside one exact await: the count is where the next pass starts
/// counting from, and the size is how far into it the final operation's own
/// await begins. Measuring rather than assuming is deliberate — the pass reads
/// the retained table once per observation and twice per unresolved operation,
/// and a scenario that hard-coded either would silently pause on the wrong
/// operation if the production walk ever changed shape.
///
/// # Panics
///
/// Panics when no pass completes inside [`ADMISSION_BOUND`].
async fn measure_reconciliation_pass(catalog: &PromotionCatalogSeam) -> (usize, usize) {
    let opened = await_reconciliation_pass_gap(catalog, 0).await;
    let closed = await_reconciliation_pass_gap(catalog, opened).await;
    (closed, closed - opened)
}

/// Proves that a stopping worker leaves an unresolved attempt for its successor.
///
/// A worker that cannot say what its own operation did has nothing durable it
/// may write about it: failing the task would be a guess and retrying it would
/// republish an effect that may already be live. So shutdown closes only what
/// is local — the heartbeat and the table fence — and leaves the task Running
/// with its operation Prepared, which is exactly the state a lost process
/// leaves and the one the table-wide reconciliation owner takes over from.
///
/// # Panics
///
/// Panics when shutdown settles, fails, retries, or resets a retained
/// attempt, when it changes an operation identity, when it does not return
/// inside the fixture's shutdown bound, or when it leaves a table fence held.
async fn shutdown_hands_off_retained_authority(
    promoted: &PromotedRewriteFixture,
    catalog: &Arc<PromotionCatalogSeam>,
    supervisor: &mut SupervisedPromotion,
    control: &vala_bifrost_redux::forge::ForgeClockControl,
) {
    let (retained, errors_before) =
        retain_one_unresolved_attempt(promoted, catalog, supervisor).await;
    // The stop lands while the attempt is still unresolved. A worker that
    // waited for proof it cannot obtain would hang here; the helper's bound is
    // what proves it does not.
    let retained_ids = retained.iter().copied().collect::<Vec<_>>();
    let pass = stable_reconciliation_pass(catalog).await.1;

    // The stop lands *inside the last* reconciliation await of a pass, not
    // between two of them and not on the pass's first operation. A retained
    // owner claims nothing and publishes nothing, so the only table loads it
    // can be making are the ones this pass is asking about; counting them is
    // what turns "some await" into the exact one that follows every earlier
    // operation's completed visit.
    assert!(
        pass >= 4 && pass.is_multiple_of(2),
        "the measured pass walks more than one unresolved operation, two \
         retained observations each: {pass} loads"
    );
    let loads_before = await_reconciliation_pass_gap(catalog, 0).await;
    catalog.pause_load_after(pass - 2);
    tokio::time::timeout(ADMISSION_BOUND, catalog.wait_for_paused_load())
        .await
        .expect("a retained owner reconciles its operations on its own cadence");
    assert_eq!(
        catalog.loads() - loads_before,
        pass - 1,
        "the held load opens the final operation's await, after every earlier \
         operation of the same pass was visited to completion"
    );
    let held = operation_phases(&promoted.fixture).await;
    assert!(
        retained
            .iter()
            .all(|id| held.get(id).is_some_and(|phase| phase == "prepared")),
        "the held load belongs to a pass over operations nothing has decided: {held:?}"
    );

    // Everything shutdown must not change is read here, with the worker held
    // inside that final await. The siblings refused before retention keep
    // settling and resubmitting on their own task backoff for as long as this
    // owner exists, so a baseline read before the freeze would credit shutdown
    // with writes the scenario merely waited through; a baseline read at the
    // freeze describes exactly the window the stop below opens.
    let owned = small_files_in_state(&promoted.fixture, &["claimed", "running"]).await;
    let (accounting_before, audit_before, submissions, demand_before) =
        settled_retention_baseline(promoted, catalog, &retained_ids).await;
    let handoffs_before = supervisor
        .observer()
        .retained_handoff_outputs_for_test()
        .len();
    supervisor.worker_stop().cancel();
    assert!(
        !supervisor.worker_ready(),
        "a worker stopped inside its reconciliation await never advertises itself"
    );
    catalog.release_paused_load();
    supervisor.stop_worker().await;

    assert_handoff_snapshot(supervisor, handoffs_before);
    assert!(
        !supervisor.worker_ready(),
        "readiness is never republished after a stop taken inside the final \
         exact reconciliation await"
    );
    assert_eq!(
        planning_demand(&promoted.fixture).await,
        demand_before,
        "shutdown records no planning demand for an attempt it cannot account for"
    );

    assert_eq!(
        supervisor.returned_errors().len(),
        errors_before,
        "shutdown settles nothing for an attempt it cannot account for: {:?}",
        supervisor.returned_errors()
    );
    assert_eq!(
        small_files_retry_accounting(&promoted.fixture).await,
        accounting_before,
        "shutdown writes no attempt count, failure class, or state for a \
         retained attempt"
    );
    assert_eq!(
        promoted
            .fixture
            .forge_terminal_audit_count_for(&retained_ids)
            .await,
        audit_before,
        "a handed-off attempt appends no terminal rewrite audit row for its \
         own operation"
    );
    assert_eq!(
        catalog.attempts(),
        submissions,
        "shutdown resubmits nothing for a retained attempt"
    );
    assert_eq!(
        small_files_in_state(&promoted.fixture, &["claimed", "running"]).await,
        owned,
        "a handed-off task stays Running for its successor: {:?}",
        tenant_tasks(&promoted.fixture).await
    );
    let after = operation_phases(&promoted.fixture).await;
    assert!(
        retained
            .iter()
            .all(|id| after.get(id).is_some_and(|phase| phase == "prepared")),
        "shutdown leaves every unresolved operation exactly as it was: {after:?}"
    );
    assert_eq!(
        promoted.fixture.live_leases().await,
        0,
        "a handed-off attempt still releases its table fence"
    );
    assert!(
        !supervisor.worker_ready(),
        "a worker stopped while still holding an unresolved operation never \
         advertises itself again"
    );

    // What a successor inherits is the exact set this phase exercised, not
    // whatever else happens to be open on the table: binding the takeover to
    // `retained` is what makes the ordering assertion below about the
    // operations whose ambiguity was actually produced here.
    assert!(
        retained.len() >= 2,
        "the handed-off attempt leaves both exercised operations open: {after:?}"
    );

    successor_reconciles_before_publishing(promoted, catalog, supervisor, control, &retained).await;
}

/// Proves a successor closes the inherited operation before it publishes again.
///
/// The parked commit is released as the definite conflict it always was, so the
/// inherited operation is provably absent from the catalog and a successor can
/// decide it. New debt is what gives the takeover something to publish — and
/// what makes "reconciled before it published" a question with an answer.
///
/// # Panics
///
/// Panics when the successor publishes a snapshot while an inherited operation
/// is still Prepared, or when it never reconciles one inside
/// [`ADMISSION_BOUND`].
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

/// Reads this tenant's durable planning demand, table by table.
///
/// Demand is the durable request for another planning pass, so it is the one
/// field a shutdown could plausibly write for an attempt it is giving up on —
/// asking someone to replan work whose effect nobody has accounted for yet.
/// The generation and source are read beside the table name because an
/// unchanged row count would hide a bumped generation.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn planning_demand(
    fixture: &super::support::PromotionIntegrationFixture,
) -> Vec<(String, i64, String)> {
    sqlx::query_as(
        "SELECT table_name, generation, last_source FROM vala.forge_planning_demands \
         WHERE data_tenant_id = $1 ORDER BY table_name",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("Forge planning demand is readable")
}

/// Reads the durable retry-accounting fields of every small-files task.
///
/// Shutdown of a retained attempt must write none of them: an attempt whose
/// operation cannot be accounted for has no retry, no failure class, and no
/// re-scheduled state to record, so the exact tuple is what proves the handoff
/// wrote nothing rather than writing something that happens to look unchanged
/// in the state column alone.
///
/// # Panics
///
/// Panics when the read-only diagnostic query fails.
async fn small_files_retry_accounting(
    fixture: &super::support::PromotionIntegrationFixture,
) -> Vec<(uuid::Uuid, i32, Option<String>, String)> {
    sqlx::query_as(
        "SELECT task_id, attempt_count, failure_class, state FROM vala.forge_tasks \
         WHERE data_tenant_id = $1 AND strategy = 'small_files' \
         ORDER BY created_at, task_id",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(fixture.operator_pool.pool())
    .await
    .expect("Forge task retry accounting is readable")
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
