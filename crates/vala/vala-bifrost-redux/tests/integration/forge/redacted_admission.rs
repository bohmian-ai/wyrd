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

    catalog.lose_commit_responses(true);
    supervisor.restart_worker();
    supervisor.run_one_failure().await;
    catalog.lose_commit_responses(false);

    let open = promoted.fixture.rewrite_operations().await;
    assert!(
        open.len() > promoted.fixture.config.max_open_operations_per_table,
        "the attempt must leave more open operations than one page holds: {open:?}"
    );
    assert!(
        open.iter().all(|(_, phase)| phase == "prepared"),
        "a lost commit response leaves its operation open: {open:?}"
    );

    // Reconciliation refuses to call a young operation absent, so the takeover
    // only starts once the uncertainty bound and the reclaim backoff lapse.
    // Measured from wall clock rather than from the manual clock's own base:
    // the operations were prepared with database timestamps taken while this
    // attempt ran, so only a bound taken after them makes them old enough.
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
    let reconciliation = supervisor.run_one_failure().await;
    supervisor.shutdown().await;

    // The successor refuses to plan while any open operation is unaccounted
    // for, and it names how many it accounted for. That count is the proof: a
    // reader bounded by one page would have reported one open operation and
    // silently left the rest of them unclassified.
    assert!(
        reconciliation.contains(&format!("{} unresolved live operations", open.len())),
        "reconciliation classified every open operation, not one page of them: \
         {reconciliation} over {open:?}"
    );
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
