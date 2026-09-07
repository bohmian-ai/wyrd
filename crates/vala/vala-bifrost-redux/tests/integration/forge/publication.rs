//! Tier-2 seam proofs for the Forge rewrite publication and recovery route.
//!
//! Every scenario drives the real Forge scheduler and worker over a table a
//! real Scribe sealed and a real promotion published, against the fixture's own
//! Postgres, catalog, and warehouse. Nothing here enqueues a task by hand,
//! calls the publication owner directly, or writes a durable row Forge did not
//! write: a rule is only proven if the production loops reach it themselves.
//!
//! Each scenario runs one supervisor for its whole lifetime. Forge's planning
//! fence is a TTL-bound singleton that shutdown does not release, so a second
//! supervisor inside one test would stand by and plan nothing.

use std::collections::BTreeSet;
use std::sync::Arc;

use iceberg::Catalog;
use vala_bifrost_redux::forge::{ForgeClock, ForgeObjectStore};

use super::rewrite_support::PromotedRewriteFixture;
use super::support::{
    CountingObjectStore, ForgeTelemetryCheckpoint, PromotionCatalogSeam,
    PromotionIntegrationFixture, SupervisedPromotion, manual_clock,
};

/// Asserts the settled table carries exactly one fully bound rewrite task.
///
/// The binding is the whole point of the enqueue: a worker may only claim a
/// task whose base snapshot, input identities, and canonical plan hash were
/// durable before the claim, so all of that is checked here against the table's
/// live cut. Returns the bound task's identity so the caller can prove a second
/// scheduling pass reuses it.
async fn assert_one_bound_rewrite(fixture: &PromotionIntegrationFixture) -> uuid::Uuid {
    let live = fixture.live_data_paths().await;
    let current = fixture
        .catalog
        .iceberg_catalog()
        .load_table(&fixture.binding.table_ident())
        .await
        .expect("the promoted fixture table")
        .metadata()
        .current_snapshot_id()
        .expect("the promoted table has a current snapshot");
    let rewrites = fixture
        .forge_tasks()
        .await
        .into_iter()
        .filter(|task| task.strategy == "small_files")
        .collect::<Vec<_>>();
    assert_eq!(
        rewrites.len(),
        1,
        "a settled table plans exactly one rewrite: {rewrites:?}"
    );
    let rewrite = &rewrites[0];
    assert_eq!(
        rewrite.state, "ready",
        "the bound rewrite is schedulable and unclaimed"
    );
    assert_eq!(
        rewrite.base_snapshot_id, current,
        "the rewrite is bound to the snapshot it was planned against"
    );
    assert_eq!(
        rewrite.plan_hash.len(),
        32,
        "the durable plan carries its canonical hash"
    );
    let inputs = rewrite
        .plan
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .expect("the durable plan binds its inputs")
        .iter()
        .map(|input| {
            input
                .as_str()
                .expect("a plan input is an object path")
                .to_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        inputs, live,
        "the rewrite binds exactly the live data files it was planned over"
    );
    rewrite.task_id
}

/// Dispatch is authorized twice: at planning, and again before any effect.
///
/// The scheduling half and the publication half are one rule — a rewrite acts
/// only while it still holds the authority it was planned under — so they share
/// one scenario.
///
/// Planning first. A table that still owes Scribe a publication must not start
/// a rewrite, or the rewrite would reason about a live set that is about to
/// change underneath it, so the pass that sees both demands plans the
/// promotion. Once nothing is owed, the pass binds exactly one rewrite task,
/// and it binds it completely: the base snapshot, the exact input identities,
/// and the canonical plan hash are all durable before any worker can claim it.
/// A second pass over the same unchanged table must add nothing, because the
/// table may carry only one active publication attempt at a time.
///
/// Then publication. A bound task's plan is evidence, not standing permission:
/// the table stays open to every other writer while managed execution runs. So
/// each held-authority phase mutates the real owner of one authority dimension
/// at the only point where the distinction is observable — outputs exist,
/// nothing has been derived, audited, or submitted — and requires the attempt
/// to refuse with no effect at all.
#[tokio::test]
async fn rewrite_scheduler_dispatches_only_after_promotion_and_authority() {
    let telemetry = ForgeTelemetryCheckpoint::install();
    let fixture = PromotionIntegrationFixture::start("rewrite_scheduler").await;
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let mut supervisor = SupervisedPromotion::start(
        &fixture,
        fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );

    // Publish the hot objects so the table carries a live set worth rewriting.
    supervisor.run_one_success().await;
    let promoted = fixture.live_data_paths().await;
    assert!(
        promoted.len() >= 2,
        "the rewrite route starts from a promoted live set: {promoted:?}"
    );

    // Promotion outranks the rewrite debt that live set already owes.
    fixture.seal_more(2).await;
    supervisor.schedule_only().await;
    let planned = fixture.forge_tasks().await;
    assert!(
        planned
            .iter()
            .all(|task| task.strategy == "scribe_promotion"),
        "a table that still owes a promotion cannot start a rewrite: {planned:?}"
    );
    assert!(
        planned.iter().any(|task| task.state == "ready"),
        "the outstanding promotion demand planned a schedulable task: {planned:?}"
    );

    // Settle that promotion through the production worker, without a planning
    // pass: a pass taken while the promotion is still in flight can bind a
    // rewrite against the pre-promotion snapshot, and the next statement is
    // about exactly one rewrite bound to the settled cut.
    supervisor.restart_worker();
    supervisor.settle_one_success().await;
    assert!(
        fixture.file_rows().await.iter().all(|row| row.compacted),
        "the table owes no further promotion"
    );

    // With nothing owed, one pass binds exactly one fully bound rewrite task.
    supervisor.schedule_only().await;
    let bound = assert_one_bound_rewrite(&fixture).await;

    // The same table cannot acquire a second active publication attempt.
    supervisor.schedule_only().await;
    let after = fixture
        .forge_tasks()
        .await
        .into_iter()
        .filter(|task| task.strategy == "small_files")
        .collect::<Vec<_>>();
    assert_eq!(
        after.len(),
        1,
        "a second pass over an unchanged table binds no second rewrite: {after:?}"
    );
    assert_eq!(
        after[0].task_id, bound,
        "the idempotent enqueue keeps the identity the first pass bound"
    );
    supervisor.shutdown().await;

    assert_eq!(
        object_store.output_writers(),
        0,
        "planning a rewrite writes no object"
    );
    assert!(
        !telemetry
            .spans_named("bifrost.forge.scheduler.pass")
            .is_empty(),
        "the production scheduler reported the passes this scenario drove"
    );

    for mutation in [
        HeldAuthorityMutation::LeaseExpired,
        HeldAuthorityMutation::AttemptCancelled,
        HeldAuthorityMutation::DeadlineElapsed,
    ] {
        assert_held_authority_change_refuses(&telemetry, mutation).await;
    }
}

/// One promoted table carrying both delete kinds over the data being rewritten.
///
/// The seeded state a delete-disposition scenario reasons about: the exact
/// input paths the rewrite must consume, the row values the published cut must
/// still read once the deletes have been applied, and the base sequence the
/// replacements have to inherit.
struct SharedDeleteState {
    /// Live data paths the rewrite is planned over.
    inputs: BTreeSet<String>,
    /// Sorted row values that survive both deletes.
    expected: Vec<i64>,
    /// Data sequence number of the snapshot the deletes left current.
    base_sequence: i64,
}

/// Publishes one position delete and one equality delete over the promoted cut.
///
/// Both deletes are committed through the real catalog so the rewrite meets
/// them as ordinary table state. The position delete removes a row of the
/// chosen target object; the equality delete removes a row value that lives in
/// the other object, which is what makes the two disposition rules separable.
async fn seed_shared_deletes(promoted: &PromotedRewriteFixture) -> SharedDeleteState {
    let inputs = promoted
        .live_data_files()
        .await
        .into_iter()
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(inputs.len(), 2, "the promotion published two data objects");
    let target = inputs.iter().next().expect("a delete target").clone();
    let target_rows = promoted
        .object_row_values(std::slice::from_ref(&target))
        .await;
    let deleted_by_position = target_rows.first().copied().expect("the target has rows");
    let other = inputs.iter().nth(1).expect("a second object").clone();
    let deleted_by_equality = promoted
        .object_row_values(std::slice::from_ref(&other))
        .await
        .first()
        .copied()
        .expect("the second object has rows");
    promoted
        .publish_deletes(&target, Some(0), Some(deleted_by_equality))
        .await;
    let mut expected = promoted
        .object_values(&inputs.iter().cloned().collect::<Vec<_>>())
        .await;
    expected.retain(|value| *value != deleted_by_position && *value != deleted_by_equality);
    let base_sequence = promoted
        .load_table()
        .await
        .metadata()
        .current_snapshot()
        .expect("the delete commit left a current snapshot")
        .sequence_number();
    SharedDeleteState {
        inputs,
        expected,
        base_sequence,
    }
}

/// Publication commits exactly the handoff, and only provably dead deletes.
///
/// The managed core's applied-delete lists are evidence about what its readers
/// materialized, not permission to drop a file — so the interesting case is a
/// table that carries both kinds of delete over the exact data being rewritten.
/// A position delete whose referenced object is rewritten has no scope left and
/// must go; an equality delete with no surviving live data below its sequence
/// likewise; and the rows they removed must stay removed in the published cut,
/// which is what the value read proves. One catalog attempt does all of it.
#[tokio::test]
async fn rewrite_publication_commits_exact_handoff_and_delete_disposition() {
    let telemetry = ForgeTelemetryCheckpoint::install();
    let promoted = PromotedRewriteFixture::start_unpromoted("rewrite_commit").await;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    supervisor.run_one_success().await;

    let seeded = seed_shared_deletes(&promoted).await;
    let inputs = seeded.inputs;
    let expected = seeded.expected;
    let base_sequence = seeded.base_sequence;
    let attempts_before = catalog.attempts();
    let snapshots_before = promoted.snapshot_count().await;

    supervisor.restart_worker();
    supervisor.run_one_success().await;
    supervisor.shutdown().await;

    // Each admitted plan publishes independently, so the attempt count is not
    // one -- it is one per published plan. Comparing it to the snapshots the
    // run actually added is the property that matters and the one a combined
    // commit or a silent retry would both break.
    let published_plans = promoted.snapshot_count().await - snapshots_before;
    assert!(
        published_plans > 0,
        "the rewrite published at least one plan"
    );
    assert_eq!(
        catalog.attempts() - attempts_before,
        published_plans,
        "every published plan commits exactly once, and nothing commits twice"
    );
    let published = promoted.live_data_files().await;
    let live_paths = published
        .iter()
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();
    assert!(
        live_paths.is_disjoint(&inputs),
        "every rewritten input left the live set: {live_paths:?}"
    );
    // A position delete names the exact object it applies to, so rewriting that
    // object always ends its scope inside the one plan that rewrote it. An
    // equality delete's scope is every live file below its sequence, which can
    // span sibling plans -- and a plan may only drop a delete whose whole scope
    // its own inputs covered, so an equality delete that outlives one plan's
    // publication is retained rather than dropped on a sibling's evidence.
    assert!(
        published
            .iter()
            .all(|file| file.content_type() != iceberg::spec::DataContentType::PositionDeletes),
        "every applied position delete lost its whole scope and was removed: {published:?}"
    );
    assert!(
        !published.is_empty(),
        "the rewrite published its replacement data"
    );
    let data_paths = published
        .iter()
        .filter(|file| file.content_type() == iceberg::spec::DataContentType::Data)
        .map(|file| file.file_path().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        promoted.object_values(&data_paths).await,
        expected,
        "the published cut reads exactly the rows the deletes left behind"
    );

    // The replacements are published at the base snapshot's sequence, not the
    // new snapshot's. That is what keeps an already-applied delete from
    // reaching them again while a concurrent equality delete written after
    // planning — which sits at a higher sequence — still does.
    let sequences = promoted.live_file_sequences().await;
    assert!(
        data_paths
            .iter()
            .all(|path| sequences.get(path) == Some(&base_sequence)),
        "the replacements carry the base sequence {base_sequence}: {sequences:?}"
    );

    // One operation per published plan, each settled exactly once. A combined
    // commit would leave one phase for two snapshots, and a plan that retried
    // or was reset would leave a phase that is not "committed".
    assert_eq!(
        promoted.fixture.rewrite_phases().await,
        vec!["committed".to_owned(); published_plans],
        "every published plan settles its own operation exactly once"
    );
    let tasks = promoted
        .fixture
        .forge_tasks()
        .await
        .into_iter()
        .filter(|task| task.strategy == "small_files")
        .collect::<Vec<_>>();
    assert_eq!(tasks.len(), 1, "one rewrite task ran: {tasks:?}");
    assert_eq!(
        tasks[0].state, "succeeded",
        "the durable task records its committed outcome"
    );
    assert_eq!(
        promoted.fixture.live_leases().await,
        0,
        "the publication released its table lease"
    );

    telemetry.require_metrics(&["bifrost_forge_output_files_total"]);
    let commits = telemetry.spans_named("bifrost.forge.catalog.commit");
    assert!(
        commits.iter().any(|span| span
            .attributes
            .get("strategy")
            .is_some_and(|value| value.contains("iceberg_rewrite"))),
        "the production catalog commit reported the rewrite strategy: {commits:?}"
    );
}

/// A definite conflict buys exactly one revalidated retry, and then resets.
///
/// Both halves of that rule are one decision, so they share one table. The
/// first rewrite meets a single refusal: the catalog *answered*, so nothing
/// landed, and re-deriving the replacement against the reloaded table and
/// committing once more is the only safe follow-up — it publishes on the second
/// attempt and settles once. The second rewrite meets a refusal it cannot
/// out-wait: the retry is already spent, so the operation is recorded as
/// definitely uncommitted rather than left open, no third catalog call is made,
/// and the table keeps the cut it already had.
#[tokio::test]
async fn rewrite_publication_conflict_revalidates_once_or_resets() {
    let telemetry = ForgeTelemetryCheckpoint::install();
    let promoted = PromotedRewriteFixture::start_unpromoted("rewrite_conflict").await;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    supervisor.run_one_success().await;
    let first_inputs = promoted
        .live_data_files()
        .await
        .into_iter()
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();

    // One refusal: revalidate against the reloaded table and commit once more.
    // Arming the seam also zeroes its counter, so the counts below are absolute.
    catalog.reject_next_commits(1);
    let snapshots_before = promoted.snapshot_count().await;
    supervisor.restart_worker();
    supervisor.run_one_success().await;
    let published_plans = promoted.snapshot_count().await - snapshots_before;
    assert_eq!(
        catalog.attempts(),
        published_plans + 1,
        "one refusal buys exactly one more commit attempt, and no plan retries twice"
    );
    let published = promoted
        .live_data_files()
        .await
        .into_iter()
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();
    assert!(
        published.is_disjoint(&first_inputs),
        "the revalidated retry published the replacement: {published:?}"
    );
    assert_eq!(
        promoted.fixture.rewrite_phases().await,
        vec!["committed".to_owned(); published_plans],
        "a retried publication still settles each plan's operation exactly once"
    );

    // Give the table a fresh rewrite demand through the production route.
    promoted.fixture.seal_more(2).await;
    supervisor.restart_worker();
    supervisor.run_one_success().await;
    assert!(
        promoted
            .fixture
            .file_rows()
            .await
            .iter()
            .all(|row| row.compacted),
        "the second promotion settled before the second rewrite is planned"
    );
    let standing = promoted
        .live_data_files()
        .await
        .into_iter()
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();

    assert_spent_retry_closes_every_operation(SpentRetryPhase {
        promoted: &promoted,
        catalog: &catalog,
        supervisor,
        standing,
        published_plans,
        telemetry: &telemetry,
    })
    .await;

    // The retry the rule grants is bounded by the same absolute deadline the
    // initial call started under, so the two phases below take that budget away
    // in the only two ways it can end: entirely, and almost entirely.
    assert_expired_deadline_makes_no_second_call().await;
    assert_retry_inherits_only_the_remaining_budget().await;
}

/// Everything the spent-retry phase needs from the run that preceded it.
struct SpentRetryPhase<'a> {
    /// Promoted table the refused attempt runs over.
    promoted: &'a PromotedRewriteFixture,
    /// Catalog seam that counts and refuses this phase's commits.
    catalog: &'a Arc<PromotionCatalogSeam>,
    /// Supervisor the earlier phases left, consumed by this final one.
    supervisor: SupervisedPromotion,
    /// Live cut the earlier phases published, which must survive untouched.
    standing: BTreeSet<String>,
    /// Plans the earlier successful rewrite published, one operation each.
    published_plans: usize,
    /// Scenario-wide production telemetry checkpoint.
    telemetry: &'a ForgeTelemetryCheckpoint,
}

/// Refuses every commit of one attempt and proves each plan closed exactly once.
///
/// Split out of the scenario because it is the third and last phase of one
/// story, not a separate one: it inherits the supervisor, the published cut,
/// and the operation history the first two phases created, and its assertions
/// are stated relative to them.
///
/// # Panics
///
/// Panics when a plan made a call past its retry budget, when the refused
/// attempt changed the published cut, when an operation was left open, or when
/// production telemetry did not report every commit attempt.
async fn assert_spent_retry_closes_every_operation(phase: SpentRetryPhase<'_>) {
    let SpentRetryPhase {
        promoted,
        catalog,
        mut supervisor,
        standing,
        published_plans,
        telemetry,
    } = phase;
    // Every plan refused twice: each spends its one retry, so no plan commits
    // and the attempt has no partial progress to report as success.
    catalog.reject_next_commits(usize::MAX);
    supervisor.restart_worker();
    let error = supervisor.run_one_failure().await;
    supervisor.shutdown().await;
    let phases = promoted.fixture.rewrite_phases().await;
    let refused_plans = phases.iter().filter(|phase| *phase == "reset").count();
    assert!(
        refused_plans > 0,
        "the refused attempt planned work: {error}"
    );
    // Four calls per plan: the first submission plus the bounded conflict
    // retries the publication schedule grants, and not one call past them.
    assert_eq!(
        catalog.attempts(),
        4 * refused_plans,
        "each refused plan spends exactly its bounded retry budget: {error}"
    );
    assert_eq!(
        promoted
            .live_data_files()
            .await
            .into_iter()
            .map(|file| file.file_path().to_owned())
            .collect::<BTreeSet<_>>(),
        standing,
        "a refused publication leaves the published cut exactly as it was"
    );
    assert_eq!(
        phases,
        [
            vec!["committed".to_owned(); published_plans],
            vec!["reset".to_owned(); refused_plans],
        ]
        .concat(),
        "every refused operation is recorded as definitely uncommitted"
    );

    let commits = telemetry.spans_named("bifrost.forge.catalog.commit");
    assert_eq!(
        commits
            .iter()
            .filter(|span| span
                .attributes
                .get("strategy")
                .is_some_and(|value| value.contains("iceberg_rewrite")))
            .count(),
        published_plans + 1 + 4 * refused_plans,
        "every rewrite commit attempt was reported by production telemetry: {commits:?}"
    );
}

/// One publication whose deadline elapses while its first call is in flight.
///
/// The retry a definite conflict buys is not unconditional: it is permitted
/// only while the publication's one absolute deadline still has budget left.
/// Advancing the manual clock past that deadline while the first call is parked
/// makes this exact: the catalog then answers with a definite refusal — proof
/// nothing landed — and the follow-up must still refuse to submit again,
/// recording the operation as definitely uncommitted rather than spending a
/// retry the deadline no longer covers.
///
/// # Panics
///
/// Panics when the fixture cannot start, a deterministic bound is missed, or a
/// second catalog call was made.
async fn assert_expired_deadline_makes_no_second_call() {
    let promoted = PromotedRewriteFixture::start_unpromoted("rewrite_deadline_spent").await;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let (clock, control) = manual_clock();
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    supervisor.run_one_success().await;
    let standing = live_data_paths(&promoted).await;

    let attempts_before = catalog.attempts();
    catalog.park_next_commit();
    supervisor.restart_worker();
    let supervisor = supervisor
        .run_one_failure_while(async {
            catalog.wait_for_parked_commit().await;
            control
                .set(expired_publication_deadline(&promoted, &control))
                .expect("manual Forge clock advances");
            catalog.reject_parked_commit();
            // The plans this one does not represent must not publish either,
            // or the attempt would report their success instead of this
            // plan's refusal.
            catalog.reject_remaining_commits();
        })
        .await;
    supervisor.shutdown().await;

    let phases = promoted.fixture.rewrite_phases().await;
    assert_eq!(
        catalog.attempts() - attempts_before,
        1,
        "a conflict answered past the publication deadline buys no second call, \
         and a sibling plan under the same spent budget never submits at all: \
         {phases:?}"
    );
    assert_eq!(
        phases,
        vec!["reset".to_owned(); phases.len()],
        "the refused publication is recorded as definitely uncommitted"
    );
    assert_eq!(
        live_data_paths(&promoted).await,
        standing,
        "no replacement was published past the deadline"
    );
}

/// The permitted retry inherits only what the first call left of the deadline.
///
/// A retry that restarted the configured budget would be a second publication
/// wearing the first one's identity: it could still be in flight long after the
/// window its Prepared record promised. So the clock is advanced to just short
/// of the deadline while the first call is parked. The retry that follows then
/// has about a second of budget, and the parked second call is never released —
/// only the production deadline can end it. It does, leaving acceptance
/// unknown, which is the honest answer for a call that was submitted: the
/// operation stays Prepared for evidence-based recovery, and no third call is
/// made.
///
/// # Panics
///
/// Panics when the fixture cannot start, a deterministic bound is missed — a
/// renewed budget would miss it by minutes — or the publication claimed an
/// outcome it could not know.
async fn assert_retry_inherits_only_the_remaining_budget() {
    let promoted = PromotedRewriteFixture::start_unpromoted("rewrite_deadline_remainder").await;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let (clock, control) = manual_clock();
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    supervisor.run_one_success().await;
    let standing = live_data_paths(&promoted).await;

    let attempts_before = catalog.attempts();
    catalog.park_next_commit();
    supervisor.restart_worker();
    let supervisor = supervisor
        .run_one_failure_while(async {
            catalog.wait_for_parked_commit().await;
            // Three seconds short of the deadline, not one: the retry the
            // conflict buys is owed a one-second backoff first, and a margin
            // that only just covers it would leave whether the retry is
            // submitted or truncated to rounding.
            let nearly_spent =
                expired_publication_deadline(&promoted, &control) - chrono::Duration::seconds(3);
            control
                .set(nearly_spent)
                .expect("manual Forge clock advances");
            catalog.reject_parked_commit_and_park_next();
            catalog.wait_for_parked_commit().await;
            catalog.wait_for_parked_commit_drop().await;
            catalog.reject_remaining_commits();
        })
        .await;
    supervisor.shutdown().await;

    let phases = promoted.fixture.rewrite_phases().await;
    assert_eq!(
        catalog.attempts() - attempts_before,
        2 * phases.len(),
        "the publication made its initial call and exactly one retry, and every \
         sibling plan got the same two calls out of what the attempt's one \
         shared budget still had left: {phases:?}"
    );
    assert_eq!(
        phases.first().map(String::as_str),
        Some("prepared"),
        "a submitted call that ran out of budget claims no outcome: {phases:?}"
    );
    assert_eq!(
        live_data_paths(&promoted).await,
        standing,
        "the abandoned retry published nothing"
    );
}

/// The first instant at which one publication's deadline has certainly passed.
///
/// Derived from the configured Iceberg budget rather than a literal, so the
/// scenario stays correct if that budget is retuned.
fn expired_publication_deadline(
    promoted: &PromotedRewriteFixture,
    control: &vala_bifrost_redux::forge::ForgeClockControl,
) -> chrono::DateTime<chrono::Utc> {
    control.now().expect("manual Forge clock")
        + chrono::Duration::from_std(promoted.fixture.config.iceberg_total_retry_timeout)
            .expect("the Iceberg retry budget is representable")
        + chrono::Duration::seconds(1)
}

/// Asserts a cancelled in-flight commit left no claim on any outcome.
///
/// A drained attempt is the ambiguous case: the catalog may or may not have
/// accepted it, so the only honest durable state is the prepared record it
/// wrote before the call. The live cut must be untouched and the table lease
/// released, or a successor could neither read the table nor take over.
async fn assert_drained_attempt_is_unsettled(
    promoted: &PromotedRewriteFixture,
    inputs: &BTreeSet<String>,
    errors: &[String],
) {
    assert_eq!(
        promoted.fixture.rewrite_phases().await,
        vec!["prepared".to_owned()],
        "a drained rewrite claims no outcome: {errors:?}"
    );
    assert_eq!(
        promoted
            .live_data_files()
            .await
            .into_iter()
            .map(|file| file.file_path().to_owned())
            .collect::<BTreeSet<_>>(),
        inputs.clone(),
        "the drained attempt published nothing"
    );
    assert_eq!(
        promoted.fixture.live_leases().await,
        0,
        "the drained attempt released its table lease"
    );
}

/// An ambiguous publication stays open, blocks fresh work, and settles once.
///
/// Cancelling a submitted commit is the one outcome that is not an answer: the
/// replacement may have landed, and the worker that submitted it cannot tell.
/// So it claims nothing — the operation stays open, the outputs stay
/// unreferenced, and the lease is released. The successor that takes the table
/// over settles that operation from retained evidence *before* it plans any new
/// rewrite, and settles it exactly once: the second run adds no second terminal
/// transition and no second replacement of the same inputs.
#[tokio::test]
async fn rewrite_publication_ambiguity_restart_settles_once() {
    let promoted = PromotedRewriteFixture::start_unpromoted("rewrite_ambiguity").await;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let (clock, control) = manual_clock();
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    supervisor.run_one_success().await;
    let inputs = promoted
        .live_data_files()
        .await
        .into_iter()
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();

    // Cancel the rewrite while its commit is in flight: acceptance is unknown.
    catalog.park_next_commit();
    supervisor.restart_worker();
    let worker_stop = supervisor.worker_stop();
    let supervisor = supervisor
        .run_one_failure_while(async {
            catalog.wait_for_parked_commit().await;
            worker_stop.cancel();
            catalog.wait_for_parked_commit_drop().await;
        })
        .await;

    assert_drained_attempt_is_unsettled(&promoted, &inputs, &supervisor.returned_errors()).await;

    // The successor settles that operation before it does anything new. The
    // drained claim is still held by its dead attempt, so the takeover only
    // starts once that deadline and the reclaim backoff have both lapsed.
    let mut supervisor = supervisor;
    // Reconciliation deliberately refuses to call a young operation absent:
    // the answer may still be in flight. Advancing past the uncertainty bound
    // is what turns "unknown" into evidence a successor may act on.
    let settled_at = control.now().expect("manual Forge clock")
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
    supervisor.run_one_success().await;
    let settled = promoted.fixture.rewrite_phases().await;
    // The drained operation is reset first; every operation after it belongs to
    // the takeover, which opens one per plan it publishes.
    assert_eq!(
        settled.first().map(String::as_str),
        Some("reset"),
        "the drained operation settles from retained evidence before the \
         takeover opens its own: {settled:?}"
    );
    assert!(
        settled.len() > 1 && settled[1..].iter().all(|phase| phase == "committed"),
        "every operation the takeover opened committed exactly once: {settled:?}"
    );
    let published = promoted
        .live_data_files()
        .await
        .into_iter()
        .filter(|file| file.content_type() == iceberg::spec::DataContentType::Data)
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();
    assert!(
        published.is_disjoint(&inputs),
        "the takeover published exactly one replacement of the inputs: {published:?}"
    );

    // Settling is idempotent: a further pass adds no second transition and no
    // second replacement of a live set that is already rewritten.
    promoted.fixture.clear_task_backoff().await;
    supervisor.schedule_only().await;
    supervisor.shutdown().await;
    assert_eq!(
        promoted.fixture.rewrite_phases().await,
        settled,
        "a settled operation is never settled twice"
    );
    assert_eq!(
        promoted
            .live_data_files()
            .await
            .into_iter()
            .filter(|file| file.content_type() == iceberg::spec::DataContentType::Data)
            .map(|file| file.file_path().to_owned())
            .collect::<BTreeSet<_>>(),
        published,
        "the rewritten cut is not rewritten again"
    );
}

/// One real owner of publication authority, mutated while a rewrite is held.
///
/// Each variant names the durable thing production code consults, not the
/// refusal it produces: the point of the scenario is that changing the real
/// owner is enough, so no test-only verdict, branch, or injected decision is
/// involved anywhere.
#[derive(Debug, Clone, Copy)]
enum HeldAuthorityMutation {
    /// The durable lease row stops being renewable.
    LeaseExpired,
    /// The worker's own cancellation token is cancelled.
    AttemptCancelled,
    /// The manual Forge clock passes the publication's absolute deadline.
    DeadlineElapsed,
}

impl HeldAuthorityMutation {
    /// Names the table this phase owns.
    ///
    /// Each phase gets a fresh table because a refusal is durable: an expired
    /// lease stays refused, so phases sharing one table would prove only the
    /// first mutation.
    const fn table_name(self) -> &'static str {
        match self {
            Self::LeaseExpired => "rewrite_hold_lease",
            Self::AttemptCancelled => "rewrite_hold_cancel",
            Self::DeadlineElapsed => "rewrite_hold_deadline",
        }
    }

    /// The exact `RewriteRefusal` this mutation must produce.
    ///
    /// Asserted as the refusal's own name rather than as "some refusal": the
    /// decision order is fail-closed, so a phase that refused for a *different*
    /// reason than the owner it mutated would still leave no effect behind and
    /// would otherwise pass every other assertion here.
    const fn expected_refusal(self) -> &'static str {
        match self {
            Self::LeaseExpired => "LeaseLost",
            Self::AttemptCancelled => "Cancelled",
            Self::DeadlineElapsed => "Deadline",
        }
    }
}

/// The complete live cut of a promoted table, split by Iceberg content type.
///
/// Data and delete attachments are held apart because a refusal has to leave
/// *both* exactly as the concurrent owner left them: comparing only data would
/// accept a publication that dropped a delete file, which changes the rows a
/// reader sees without changing a single data path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LiveCut {
    /// Live data object paths of the current snapshot.
    data: BTreeSet<String>,
    /// Live position- and equality-delete object paths of the current snapshot.
    deletes: BTreeSet<String>,
}

/// Projects the promoted table's current snapshot into a [`LiveCut`].
async fn live_cut(promoted: &PromotedRewriteFixture) -> LiveCut {
    let files = promoted.live_data_files().await;
    let (data, deletes): (Vec<_>, Vec<_>) = files
        .iter()
        .partition(|file| file.content_type() == iceberg::spec::DataContentType::Data);
    LiveCut {
        data: data
            .into_iter()
            .map(|file| file.file_path().to_owned())
            .collect(),
        deletes: deletes
            .into_iter()
            .map(|file| file.file_path().to_owned())
            .collect(),
    }
}

/// Drives one held-authority phase end to end over its own promoted table.
///
/// Returns nothing — every observation is asserted here, next to the mutation
/// that has to explain it. `telemetry` is the scenario's one process-wide
/// checkpoint; the phase takes its own mark from it so an earlier phase's
/// spans cannot answer this phase's questions.
///
/// # Panics
///
/// Panics when the fixture cannot start, a deterministic bound is missed, or
/// the held attempt produced any durable effect.
async fn assert_held_authority_change_refuses(
    telemetry: &ForgeTelemetryCheckpoint,
    mutation: HeldAuthorityMutation,
) {
    let promoted = PromotedRewriteFixture::start_unpromoted(mutation.table_name()).await;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        promoted.fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let (clock, control) = manual_clock();
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        Arc::clone(&catalog) as Arc<dyn Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    supervisor.run_one_success().await;
    let planned = live_cut(&promoted).await;
    assert!(
        planned.data.len() >= 2,
        "the held rewrite starts from a promoted live set: {planned:?}"
    );

    let mutations_before = catalog.attempts();
    let loads_before = catalog.loads();
    let audit_before = promoted.fixture.forge_audit_count().await;
    let files_before = promoted.fixture.file_rows().await;
    let objects_before = promoted.fixture.object_digests().await;
    let span_mark = telemetry.mark();
    supervisor.restart_worker();
    let worker_stop = supervisor.worker_stop();
    let error = supervisor
        .run_one_failure_holding_handoff(async {
            apply_held_authority_mutation(mutation, &promoted, &control, &worker_stop).await;
        })
        .await;
    let possible_outputs = supervisor.last_possible_rewrite_outputs();
    supervisor.shutdown().await;

    // 1. The refusal was decided against metadata this attempt reloaded after
    //    its handoff, and it mutated nothing.
    assert!(
        catalog.loads() > loads_before,
        "publication reacquired authoritative metadata after the handoff ({mutation:?})"
    );
    assert_eq!(
        catalog.attempts(),
        mutations_before,
        "a refused publication mutates no catalog state ({mutation:?}): {error}"
    );
    assert!(
        error.contains(mutation.expected_refusal()),
        "the mutated owner produced its own refusal ({mutation:?}): {error}"
    );

    // 2. No durable operation row and no durable Forge audit transition.
    assert_eq!(
        promoted.fixture.rewrite_phases().await,
        Vec::<String>::new(),
        "the refusal arrived before any Prepared operation row ({mutation:?})"
    );
    assert_eq!(
        promoted.fixture.forge_audit_count().await,
        audit_before,
        "a refused publication appends no Forge audit transition ({mutation:?})"
    );

    // 3. No rewrite settlement was applied to the durable file ledger.
    assert_eq!(
        promoted.fixture.file_rows().await,
        files_before,
        "a refused publication settles no promoted file row ({mutation:?})"
    );

    assert_refused_publication_left_no_trace(HeldAuthorityAftermath {
        promoted: &promoted,
        telemetry,
        mutation,
        span_mark,
        planned: &planned,
        objects_before: &objects_before,
        possible_outputs,
        error: &error,
    })
    .await;
}

/// Applies one held-authority phase's mutation while publication is held.
///
/// Each arm changes the durable owner production code consults and nothing
/// else; no verdict, branch, or decision is injected. Every phase expects the
/// pre-mutation cut unchanged afterwards.
///
/// # Panics
///
/// Panics when the fixture cannot apply the mutation or the manual clock
/// cannot represent the phase's deadline.
async fn apply_held_authority_mutation(
    mutation: HeldAuthorityMutation,
    promoted: &PromotedRewriteFixture,
    control: &vala_bifrost_redux::forge::ForgeClockControl,
    worker_stop: &tokio_util::sync::CancellationToken,
) {
    match mutation {
        HeldAuthorityMutation::LeaseExpired => {
            promoted.fixture.expire_table_lease().await;
        }
        HeldAuthorityMutation::AttemptCancelled => {
            worker_stop.cancel();
        }
        HeldAuthorityMutation::DeadlineElapsed => {
            let elapsed = control.now().expect("manual Forge clock")
                + chrono::Duration::from_std(promoted.fixture.config.iceberg_total_retry_timeout)
                    .expect("the Iceberg retry budget is representable")
                + chrono::Duration::seconds(1);
            control.set(elapsed).expect("manual Forge clock advances");
        }
    }
}

/// Everything one held-authority phase captured before it started asserting.
///
/// Grouping these keeps the phase's two halves — drive-and-refuse, then
/// prove-nothing-happened — separately readable while still binding every
/// later assertion to the exact state the mutation was applied against.
struct HeldAuthorityAftermath<'a> {
    /// Promoted table the held attempt ran over.
    promoted: &'a PromotedRewriteFixture,
    /// Scenario-wide production telemetry checkpoint.
    telemetry: &'a ForgeTelemetryCheckpoint,
    /// Held-authority change this phase applied.
    mutation: HeldAuthorityMutation,
    /// Span position taken immediately before the held attempt began.
    span_mark: usize,
    /// Live cut the promotion left, before the mutation was applied.
    planned: &'a LiveCut,
    /// Object digests taken immediately before the held attempt began.
    objects_before: &'a std::collections::BTreeMap<String, String>,
    /// Typed unsettled-output evidence the refusal carried, if it was typed.
    possible_outputs: Option<Vec<vala_bifrost_redux::forge::ForgeUnsettledOutput>>,
    /// Rendered refusal, retained for failure messages.
    error: &'a str,
}

/// Proves a refused publication left no cut, object, task, or telemetry trace.
///
/// # Panics
///
/// Panics when the refused attempt changed the published cut, wrote or
/// rewrote an object it cannot name, left its task or lease in the wrong
/// durable state, or failed to report itself exactly once.
async fn assert_refused_publication_left_no_trace(aftermath: HeldAuthorityAftermath<'_>) {
    let HeldAuthorityAftermath {
        promoted,
        telemetry,
        mutation,
        span_mark,
        planned,
        objects_before,
        possible_outputs,
        error,
    } = aftermath;

    // 4. The live cut is exactly what the concurrent owner left, deletes
    //    included, and no managed output became live.
    let after = live_cut(promoted).await;
    assert_eq!(
        after,
        planned.clone(),
        "a refused publication leaves the published cut exactly as it was ({mutation:?})"
    );
    assert_eq!(
        after.data, planned.data,
        "no managed output was made live and no input left the cut ({mutation:?})"
    );

    // 5. The typed failure carries the exact objects managed execution wrote,
    //    named by identity rather than by the wrapper's rendered text. The
    //    objects that appeared under the table prefix during the held attempt
    //    are the independent answer it has to match: nothing may be left behind
    //    that no attempt can name, and nothing may be named that never existed.
    let objects_after = promoted.fixture.object_digests().await;
    let prefix = format!("{}/", promoted.fixture.binding.object_prefix);
    // Scoped to the managed core's own output namespace, so fixture objects
    // and Iceberg metadata that no rewrite attempt produced stay out of it.
    let managed_prefix = format!("{prefix}data/forge/");
    let appeared = objects_after
        .keys()
        .filter(|path| {
            path.starts_with(managed_prefix.as_str()) && !objects_before.contains_key(*path)
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    assert!(
        !appeared.is_empty(),
        "the held attempt wrote the managed outputs it refused to publish ({mutation:?})"
    );
    let possible_outputs = possible_outputs
        .unwrap_or_else(|| panic!("the refusal is typed as unsettled ({mutation:?}): {error}"));
    assert_eq!(
        possible_outputs
            .iter()
            .map(|output| {
                let start = output.path.find(prefix.as_str()).unwrap_or_else(|| {
                    panic!(
                        "a possible output lives under the table's object prefix \
                         ({mutation:?}): {}",
                        output.path
                    )
                });
                output.path[start..].to_owned()
            })
            .collect::<BTreeSet<_>>(),
        appeared,
        "the refusal names exactly the objects managed execution produced ({mutation:?})"
    );
    assert!(
        objects_before
            .iter()
            .all(|(path, digest)| objects_after.get(path) == Some(digest)),
        "a refused publication rewrote no existing object ({mutation:?})"
    );

    assert_refused_attempt_settled_and_reported(promoted, telemetry, mutation, span_mark).await;
}

/// Proves the held rewrite's durable settlement and production telemetry.
///
/// Separated from the cut-and-object proof because it answers a different
/// question: not "did the table change" but "did the owner classify and report
/// its own refusal exactly once".
///
/// # Panics
///
/// Panics when the held rewrite is not left retryable, its lease is not
/// released, it did not report exactly one task span, or a catalog commit span
/// was reported for an attempt that never committed.
async fn assert_refused_attempt_settled_and_reported(
    promoted: &PromotedRewriteFixture,
    telemetry: &ForgeTelemetryCheckpoint,
    mutation: HeldAuthorityMutation,
    span_mark: usize,
) {
    // 6. Durable task classification and lease release match the phase.
    let tasks = promoted
        .fixture
        .forge_tasks()
        .await
        .into_iter()
        .filter(|task| task.strategy == "small_files")
        .collect::<Vec<_>>();
    assert_eq!(tasks.len(), 1, "one rewrite task was held: {tasks:?}");
    assert_eq!(
        tasks[0].state, "retryable",
        "a coordination refusal leaves the held rewrite retryable ({mutation:?}): {tasks:?}"
    );
    assert_eq!(
        promoted.fixture.live_leases().await,
        0,
        "the refused attempt released its table lease ({mutation:?})"
    );

    // 7. Production telemetry reported the refused attempt and no commit.
    let executed = telemetry.spans_named_since(span_mark, "bifrost.forge.task.execute");
    let held = executed
        .iter()
        .filter(|span| {
            span.attributes
                .get("task_id")
                .is_some_and(|value| value.contains(&tasks[0].task_id.to_string()))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        held.len(),
        1,
        "the held attempt reported exactly one production task span ({mutation:?}): {executed:?}"
    );
    assert!(
        held[0]
            .attributes
            .get("strategy")
            .is_some_and(|value| value.contains("small_files")),
        "the held span is the rewrite's own ({mutation:?}): {held:?}"
    );
    assert!(
        telemetry
            .spans_named_since(span_mark, "bifrost.forge.catalog.commit")
            .is_empty(),
        "a refused publication reports no catalog commit ({mutation:?})"
    );
}

/// Collects the live *data* object paths of the promoted table's current cut.
///
/// Delete attachments are excluded on purpose: a concurrent writer that adds
/// one moves the branch without replacing any data, so comparing data paths is
/// what makes "the refused publication changed nothing" a statement about the
/// rows a reader would see.
async fn live_data_paths(promoted: &PromotedRewriteFixture) -> BTreeSet<String> {
    promoted
        .live_data_files()
        .await
        .into_iter()
        .filter(|file| file.content_type() == iceberg::spec::DataContentType::Data)
        .map(|file| file.file_path().to_owned())
        .collect()
}
