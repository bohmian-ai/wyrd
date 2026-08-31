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

/// A rewrite is planned only behind promotion, once, and fully bound.
///
/// Three rules share one scenario because they are one decision made in one
/// place. A table that still owes Scribe a publication must not start a
/// rewrite, or the rewrite would reason about a live set that is about to
/// change underneath it — so the pass that sees both demands plans the
/// promotion. Once nothing is owed, the pass binds exactly one rewrite task,
/// and it binds it completely: the base snapshot, the exact input identities,
/// and the canonical plan hash are all durable before any worker can claim it.
/// A second pass over the same unchanged table must add nothing, because the
/// table may carry only one active publication attempt at a time.
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

    // Settle that promotion through the production worker.
    supervisor.restart_worker();
    supervisor.run_one_success().await;
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
    telemetry.require_metrics(&[
        "bifrost_forge_scheduler_duration_seconds",
        "bifrost_forge_demand_transitions_total",
    ]);
    assert!(
        !telemetry
            .spans_named("bifrost.forge.scheduler.pass")
            .is_empty(),
        "the production scheduler reported the passes this scenario drove"
    );
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

    supervisor.restart_worker();
    supervisor.run_one_success().await;
    supervisor.shutdown().await;

    assert_eq!(
        catalog.attempts() - attempts_before,
        1,
        "one rewrite publishes through exactly one catalog commit"
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
    assert!(
        published
            .iter()
            .all(|file| file.content_type() == iceberg::spec::DataContentType::Data),
        "both applied deletes lost their whole scope and were removed: {published:?}"
    );
    assert!(
        !published.is_empty(),
        "the rewrite published its replacement data"
    );
    assert_eq!(
        promoted
            .object_values(&live_paths.iter().cloned().collect::<Vec<_>>())
            .await,
        expected,
        "the published cut reads exactly the rows the deletes left behind"
    );

    // The replacements are published at the base snapshot's sequence, not the
    // new snapshot's. That is what keeps an already-applied delete from
    // reaching them again while a concurrent equality delete written after
    // planning — which sits at a higher sequence — still does.
    let sequences = promoted.live_file_sequences().await;
    assert!(
        live_paths
            .iter()
            .all(|path| sequences.get(path) == Some(&base_sequence)),
        "the replacements carry the base sequence {base_sequence}: {sequences:?}"
    );

    assert_eq!(
        promoted.fixture.rewrite_phases().await,
        vec!["committed".to_owned()],
        "the rewrite settles its operation exactly once"
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

    telemetry.require_metrics(&[
        "bifrost_forge_operations",
        "bifrost_forge_rewrite_output_files_total",
    ]);
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
    supervisor.restart_worker();
    supervisor.run_one_success().await;
    assert_eq!(
        catalog.attempts(),
        2,
        "one refusal buys exactly one more commit attempt"
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
        vec!["committed".to_owned()],
        "a retried publication still settles its operation exactly once"
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

    // Two refusals: the retry is spent, so the operation closes as uncommitted.
    catalog.reject_next_commits(2);
    supervisor.restart_worker();
    let error = supervisor.run_one_failure().await;
    supervisor.shutdown().await;
    assert_eq!(
        catalog.attempts(),
        2,
        "a spent retry makes no third catalog call: {error}"
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
        promoted.fixture.rewrite_phases().await,
        vec!["committed".to_owned(), "reset".to_owned()],
        "the refused operation is recorded as definitely uncommitted"
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
        4,
        "every rewrite commit attempt was reported by production telemetry: {commits:?}"
    );
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
    let telemetry = ForgeTelemetryCheckpoint::install();
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
    assert_eq!(
        settled,
        vec!["reset".to_owned(), "committed".to_owned()],
        "the drained operation settles from retained evidence before the \
         takeover opens its own: {settled:?}"
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

    telemetry.require_metrics(&["bifrost_forge_operations"]);
}
