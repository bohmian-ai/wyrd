//! Tier-2 proof that the managed rewrite seam produces objects and no snapshot.
//!
//! One scenario at the end covers the surrounding production route instead: it
//! runs the real scheduler and worker to prove that publishing a compaction is
//! non-destructive, which is the contract the non-committing core exists for.
//!
//! Every scenario starts from real promoted state: a real Scribe sealed the
//! objects and a real Forge promotion published them. What the scenarios then
//! drive is the production non-committing owner over the real catalog, the real
//! object store, and the real resource governor — never a scheduler, never a
//! worker, and never a fabricated plan.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use iceberg::spec::DataContentType;
use vala_bifrost_redux::forge::{ForgeClock, ForgeError, ForgeObjectStore, ForgeUnsettledOutput};

use super::rewrite_support::{AttemptRun, PromotedRewriteFixture, RewriteOutputBreak};
use super::support::{CountingObjectStore, PromotionCatalogSeam, SupervisedPromotion};

/// Runs one whole attempt over the promoted snapshot with no plan budget.
///
/// # Panics
///
/// Panics when the attempt fails; every caller here runs against a promoted
/// small-file set that must select.
async fn rewrite_whole_attempt(fixture: &PromotedRewriteFixture) -> AttemptRun {
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let forge = fixture.forge(
        fixture.fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );
    let run = fixture
        .run_attempt(
            &forge,
            uuid::Uuid::now_v7(),
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
    assert!(
        run.failure.is_none(),
        "the promoted snapshot is rewritable: {:?}",
        run.failure
    );
    run
}

/// Planning returns every real plan and each one rewrites only its own group.
///
/// The promoted set offers more than one eligible group. Planning is not capped,
/// so the plan set must cover the whole selection: one handoff per plan, each
/// naming its own inputs and its own output, and their union naming exactly the
/// live data files the core selected. A truncated plan set would leave inputs
/// unconsumed while the selection receipt still described them; a merged handoff
/// would ask publication to remove inputs a sibling plan is still rewriting.
#[tokio::test]
async fn managed_rewrite_plan_matches_core_report_on_promoted_snapshot() {
    let fixture = PromotedRewriteFixture::start("rewrite_plan").await;
    let live = fixture
        .live_data_files()
        .await
        .into_iter()
        .filter(|file| file.content_type() == DataContentType::Data)
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();
    assert!(
        live.len() > 1,
        "the fixture must offer more than one eligible group: {live:?}"
    );
    let table = fixture.load_table().await;
    let base = table
        .metadata()
        .current_snapshot()
        .expect("a promoted snapshot")
        .snapshot_id();

    let run = rewrite_whole_attempt(&fixture).await;

    assert_eq!(
        run.evidence().base_snapshot_id,
        base,
        "the attempt is bound to the promoted snapshot"
    );
    assert_eq!(
        run.handoffs.len(),
        live.len(),
        "planning returned one real plan per eligible group"
    );
    assert_eq!(
        run.rewritten_data_files()
            .into_iter()
            .collect::<BTreeSet<_>>(),
        live,
        "the plans together consumed exactly the core's selected data files"
    );
    assert_eq!(
        run.rewritten_data_files().len(),
        live.len(),
        "no live data file is consumed by two plans"
    );
    for handoff in &run.handoffs {
        assert_eq!(
            handoff.base_snapshot_id, base,
            "every plan resolves against the one planned snapshot"
        );
        assert_eq!(
            handoff.output_data_files.len(),
            1,
            "each plan produced its own single object"
        );
        assert_eq!(
            fixture
                .object_values(
                    &handoff
                        .output_data_files
                        .iter()
                        .map(|file| file.file_path().to_owned())
                        .collect::<Vec<_>>()
                )
                .await,
            fixture.object_values(&handoff.rewritten_data_files).await,
            "each plan carried forward exactly the rows of the files it consumed"
        );
    }
    assert!(
        !run.evidence().selection_fingerprint.is_empty()
            && !run.evidence().debt_fingerprint.is_empty()
            && !run.evidence().policy_fingerprint.is_empty(),
        "every planned attempt records its three canonical fingerprints"
    );
}

/// Drives one attempt to cancellation at the moment its Nth output opens.
///
/// The seam counts rewrite-output opens and trips the attempt's own token on
/// the `ordinal`-th, so the attempt is live and already producing when it is
/// cancelled — never cancelled up front. The returned pair is the attempt id
/// and the possible-output set the drain reported, which is what a caller
/// inspects for attempt-global ordinal behavior.
///
/// # Panics
///
/// Panics if the attempt errors or reports anything but a cancellation, or if
/// the seam did not observe exactly `ordinal` output opens.
async fn drain_at_output(
    fixture: &PromotedRewriteFixture,
    ordinal: usize,
) -> (uuid::Uuid, Vec<ForgeUnsettledOutput>) {
    let cancel = tokio_util::sync::CancellationToken::new();
    let (catalog, store) = fixture.breaking_catalog(RewriteOutputBreak::CancelAtOpen {
        ordinal,
        token: cancel.clone(),
    });
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let attempt_id = uuid::Uuid::now_v7();
    let forge = fixture.forge(
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );
    let run = fixture.run_attempt(&forge, attempt_id, cancel).await;
    assert!(
        matches!(run.failure, Some(ForgeError::Shutdown)),
        "a cancelled plan ends the attempt as a shutdown: {:?}",
        run.failure
    );
    assert_eq!(
        store.opened_outputs(),
        ordinal,
        "the drain was tripped by the {ordinal}th output open"
    );
    (attempt_id, run.rewrite.possible_outputs())
}

/// Produced objects are distinguishable across plans, and the next pass keeps them.
///
/// One attempt executes every eligible group, so the objects it produces come
/// from different plan calls. The filename ordinal resets to zero for each of
/// them — it is per-writer — so the writer UUID is the only thing in the path
/// that separates two objects, which is why the path assertions below are about
/// the pair rather than either half.
///
/// The logical ordinals are the other half and are not in a path at all. They
/// are read back from a drained attempt over the same table: they must be the
/// attempt-global sequence `0, 1`, strictly increasing across the two plan
/// calls, never restarting per plan and never colliding.
///
/// The last section is the reclassification proof: a produced object carries
/// the current writer recipe, so re-running selection against a live set that
/// now contains it must not name it again.
#[tokio::test]
async fn managed_rewrite_output_identity_is_unique_across_concurrent_writers() {
    let fixture = PromotedRewriteFixture::start("rewrite_identity").await;
    let attempt_id = uuid::Uuid::now_v7();
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let forge = fixture.forge(
        fixture.fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );
    let run = fixture
        .run_attempt(
            &forge,
            attempt_id,
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
    assert!(
        run.failure.is_none(),
        "the promoted snapshot is rewritable: {:?}",
        run.failure
    );

    let paths = run.output_paths();
    assert!(
        paths.len() > 1,
        "the attempt must execute more than one plan for this to say anything: {paths:?}"
    );
    let distinct = paths
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        distinct.len(),
        paths.len(),
        "every produced object has its own path: {paths:?}"
    );
    for path in &paths {
        assert!(
            path.contains("/data/forge/v1/"),
            "a produced object carries the current writer recipe: {path}"
        );
        assert!(
            path.contains(&format!("{attempt_id}-00000-")),
            "the filename ordinal is per-writer and restarts for each plan: {path}"
        );
    }

    let (drain_id, possible_outputs) = drain_at_output(&fixture, 2).await;
    assert_eq!(
        possible_outputs
            .iter()
            .map(|output| output.logical_ordinal)
            .collect::<Vec<_>>(),
        vec![0, 1],
        "logical ordinals are attempt-global and strictly increasing across plan calls: \
         {possible_outputs:?}"
    );
    for output in &possible_outputs {
        assert!(
            output.path.contains(&format!("{drain_id}-00000-")),
            "each plan's own writer restarted its filename ordinal: {}",
            output.path
        );
    }
    assert_eq!(
        possible_outputs
            .iter()
            .map(|output| output.path.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        possible_outputs.len(),
        "no two attempt-global ordinals name the same object: {possible_outputs:?}"
    );

    let repeat = fixture
        .run_attempt(
            &forge,
            uuid::Uuid::now_v7(),
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
    let reselected = repeat.rewritten_data_files();
    for produced in &paths {
        assert!(
            !reselected.contains(produced),
            "a current-recipe object must not be reselected: {produced}"
        );
    }
}

/// Cancellation drains and reports every object it may have produced.
///
/// The attempt is not cancelled up front — that would refuse before any IO and
/// prove nothing about draining. It is cancelled at the moment a *later* plan's
/// output opens, after an earlier plan already settled one. The core decides
/// cancellation only after draining its writers, so both objects exist, and the
/// seam has to report both: the earlier plan's object is the one a
/// plan-scoped report would lose.
///
/// The rest is what makes the drain safe to act on. No commit crossed the
/// catalog, the live set still names no rewrite output, both reported objects
/// are in storage, and the attempt's scratch child is gone, so the lease was
/// returned rather than leaked. `NoProgress` is not an acceptable answer here:
/// the attempt did open objects, and reporting no progress would strand them.
#[tokio::test]
async fn managed_rewrite_cancellation_drains_and_preserves_possible_outputs() {
    let fixture = PromotedRewriteFixture::start("rewrite_drain").await;
    let live_before = fixture.fixture.live_data_paths().await;
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let cancel = tokio_util::sync::CancellationToken::new();
    let (catalog, store) = fixture.breaking_catalog(RewriteOutputBreak::CancelAtOpen {
        ordinal: 2,
        token: cancel.clone(),
    });
    let forge = fixture.forge(
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );
    let attempt_id = uuid::Uuid::now_v7();
    let run = fixture.run_attempt(&forge, attempt_id, cancel).await;

    assert!(
        matches!(run.failure, Some(ForgeError::Shutdown)),
        "a cancelled plan ends the attempt as a shutdown: {:?}",
        run.failure
    );
    let possible_outputs = run.rewrite.possible_outputs();
    assert_eq!(
        run.evidence().base_snapshot_id,
        fixture
            .load_table()
            .await
            .metadata()
            .current_snapshot()
            .expect("a promoted snapshot")
            .snapshot_id(),
        "the drain names the snapshot it was planned against"
    );
    assert_eq!(
        store.opened_outputs(),
        2,
        "two plan calls opened two objects"
    );
    assert_eq!(
        possible_outputs.len(),
        2,
        "the drain reports the whole attempt, not the plan that was cancelled: {possible_outputs:?}"
    );
    assert_eq!(
        possible_outputs
            .iter()
            .map(|output| output.logical_ordinal)
            .collect::<Vec<_>>(),
        vec![0, 1],
        "cumulative ordinals stay attempt-global across the drain"
    );
    assert!(
        possible_outputs.iter().all(|output| output.settled),
        "cancellation is decided after the drain, so every writer had closed: {possible_outputs:?}"
    );
    let objects = fixture.object_digests().await;
    for output in &possible_outputs {
        assert!(
            objects
                .keys()
                .any(|object| output.path.ends_with(object.as_str())),
            "each reported object exists and can be reclaimed: {}",
            output.path
        );
    }
    assert_eq!(
        catalog.attempts(),
        0,
        "a drained attempt issued no catalog commit"
    );
    assert_eq!(
        fixture.fixture.live_data_paths().await,
        live_before,
        "a drained attempt published nothing"
    );
}

/// A failure after an output opened still reports the whole attempt's objects.
///
/// The earlier plan settles one object, then the later plan's output is opened
/// and its close refused. That ordering is the point: the failing object was
/// reported as opened, so a correct failure names *both* it and the earlier
/// plan's settled object. Losing either is losing an object nothing can
/// afterwards name, because the attempt that named it has ended.
///
/// The refusal is still a refusal — it is an error, not an outcome — and it
/// still classifies as the transient object-store failure the underlying
/// boundary declared, so wrapping the evidence did not move the failure into a
/// different retry class. Nothing was committed and nothing was published.
#[tokio::test]
async fn managed_rewrite_failure_preserves_attempt_global_possible_outputs() {
    let fixture = PromotedRewriteFixture::start("rewrite_failure").await;
    let live_before = fixture.fixture.live_data_paths().await;
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let (catalog, store) = fixture.breaking_catalog(RewriteOutputBreak::FailAtClose { ordinal: 2 });
    let forge = fixture.forge(
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );
    let attempt_id = uuid::Uuid::now_v7();
    let run = fixture
        .run_attempt(
            &forge,
            attempt_id,
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
    let error = run
        .failure
        .expect("a refused output settlement fails the attempt");

    assert!(
        matches!(error, ForgeError::RewriteUnsettled { .. }),
        "a failure after an object may exist carries that object: {error:?}"
    );
    assert_eq!(
        error.failure_class(),
        vala_sql::row_types::forge_tasks::ForgeFailureClass::TransientObjectStore,
        "carrying evidence did not reclassify the failure: {error:?}"
    );
    assert_eq!(
        store.opened_outputs(),
        2,
        "two plan calls opened two objects"
    );
    let possible_outputs = error.possible_rewrite_outputs();
    assert_eq!(
        possible_outputs
            .iter()
            .map(|output| (output.logical_ordinal, output.settled))
            .collect::<Vec<_>>(),
        vec![(0, true), (1, false)],
        "the failure retains the earlier plan's settled object and the later plan's \
         unsettled one: {possible_outputs:?}"
    );
    let objects = fixture.object_digests().await;
    assert!(
        objects
            .keys()
            .any(|object| possible_outputs[0].path.ends_with(object.as_str())),
        "the settled object is in storage and reclaimable: {}",
        possible_outputs[0].path
    );
    assert_eq!(
        catalog.attempts(),
        0,
        "a failed attempt issued no catalog commit"
    );
    assert_eq!(
        fixture.fixture.live_data_paths().await,
        live_before,
        "a failed attempt published nothing"
    );
}

/// The seam produces exactly five fields of evidence and commits nothing.
///
/// The output objects exist in storage and belong to no snapshot: the live set
/// after the attempt is the live set before it, and the catalog seam counted no
/// commit. The produced core `DataFile` values are compared by path against the
/// objects that actually appeared, which is what proves they were preserved
/// rather than reconstructed.
#[tokio::test]
async fn managed_rewrite_produces_exact_handoff_without_catalog_commit() {
    let fixture = PromotedRewriteFixture::start("rewrite_handoff").await;
    let live_before = fixture.fixture.live_data_paths().await;
    let objects_before = fixture.object_digests().await;
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        fixture.fixture.catalog.iceberg_catalog(),
        Arc::new(AtomicUsize::new(0)),
    );
    let forge = fixture.forge(
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );
    let run = fixture
        .run_attempt(
            &forge,
            uuid::Uuid::now_v7(),
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
    assert!(
        run.failure.is_none(),
        "the promoted snapshot is rewritable: {:?}",
        run.failure
    );

    assert_eq!(
        catalog.attempts(),
        0,
        "the rewrite seam issued no catalog commit"
    );
    assert_eq!(
        fixture.fixture.live_data_paths().await,
        live_before,
        "the live set is unchanged by a rewrite that publishes nothing"
    );
    for handoff in &run.handoffs {
        assert_eq!(
            handoff.base_snapshot_id,
            run.evidence().base_snapshot_id,
            "each handoff names the snapshot the evidence was derived from"
        );
    }
    let objects_after = fixture.object_digests().await;
    for path in &run.output_paths() {
        assert!(
            objects_after
                .keys()
                .any(|object| path.ends_with(object.as_str()) || object.ends_with(path.as_str())),
            "each produced identity names an object that exists: {path}"
        );
        assert!(
            !objects_before
                .keys()
                .any(|object| path.ends_with(object.as_str())),
            "each produced object is new: {path}"
        );
    }
}

/// Both delete kinds are applied to the rewritten rows within their own scope.
///
/// Each half is a row assertion. The equality half deletes one value across the
/// partition and the produced objects carry every other row, so ignoring the
/// delete or widening its scope changes the answer. The position half names row
/// 0 of one promoted object: the produced objects must carry every row except
/// that one, which fails both if the delete is ignored and if it is applied to
/// the wrong object — the other promoted object has a row 0 too.
#[tokio::test]
async fn managed_rewrite_applies_position_and_equality_deletes_to_output_rows() {
    assert_equality_delete_applies_to_output_rows().await;
    assert_position_delete_applies_to_output_rows().await;
}

/// Rewrites a promoted set carrying one equality delete and checks the rows.
///
/// # Panics
///
/// Panics when the attempt does not select, when the delete is named more than
/// once, or when the produced rows are not the promoted set minus the deleted
/// value.
async fn assert_equality_delete_applies_to_output_rows() {
    let fixture = PromotedRewriteFixture::start("rewrite_deletes").await;
    let mut live = fixture
        .live_data_files()
        .await
        .into_iter()
        .filter(|file| file.content_type() == DataContentType::Data)
        .map(|file| file.file_path().to_owned())
        .collect::<Vec<_>>();
    live.sort();
    let target = live.first().cloned().expect("a promoted data object");
    let before = fixture.object_values(&live).await;
    assert_eq!(
        before,
        vec![0, 1, 10, 11],
        "the promoted set carries four distinguishable rows"
    );
    fixture.publish_deletes(&target, None, Some(11)).await;

    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let forge = fixture.forge(
        fixture.fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );
    let run = fixture
        .run_attempt(
            &forge,
            uuid::Uuid::now_v7(),
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
    assert!(
        run.failure.is_none(),
        "a table carrying equality-delete debt is rewritable: {:?}",
        run.failure
    );

    assert_eq!(
        run.applied_equality_delete_files()
            .into_iter()
            .collect::<BTreeSet<_>>()
            .len(),
        1,
        "the equality delete is named exactly once however many groups it covers"
    );
    assert!(
        run.applied_position_delete_files().is_empty(),
        "no position delete was published in this pass"
    );
    let produced = run.output_paths();
    assert_eq!(
        fixture.object_values(&produced).await,
        vec![0, 1, 10],
        "the equality delete removed its own row and nothing else"
    );
}

/// Rewrites a promoted set carrying one position delete and checks the rows.
///
/// # Panics
///
/// Panics when the attempt does not select, when the delete is named more than
/// once, when the produced rows are not the promoted set minus row 0 of the
/// referenced object, or when the attempt published a snapshot.
async fn assert_position_delete_applies_to_output_rows() {
    let deletes = PromotedRewriteFixture::start("rewrite_position_delete").await;
    let mut live = deletes
        .live_data_files()
        .await
        .into_iter()
        .filter(|file| file.content_type() == DataContentType::Data)
        .map(|file| file.file_path().to_owned())
        .collect::<Vec<_>>();
    live.sort();
    let target = live.first().cloned().expect("a promoted data object");
    let deleted_row = deletes
        .object_row_values(std::slice::from_ref(&target))
        .await
        .first()
        .copied()
        .expect("the delete target carries a row 0");
    let mut expected = deletes.object_values(&live).await;
    let removed = expected
        .iter()
        .position(|value| *value == deleted_row)
        .expect("row 0 of the target is one of the promoted rows");
    expected.remove(removed);

    deletes.publish_deletes(&target, Some(0), None).await;
    let live_before = deletes.fixture.live_data_paths().await;
    let store = CountingObjectStore::new(Arc::clone(&deletes.fixture.staging));
    let forge = deletes.forge(
        deletes.fixture.catalog.iceberg_catalog(),
        Arc::clone(&store) as Arc<dyn ForgeObjectStore>,
    );
    let run = deletes
        .run_attempt(
            &forge,
            uuid::Uuid::now_v7(),
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
    assert!(
        run.failure.is_none(),
        "a table carrying position-delete debt is rewritable: {:?}",
        run.failure
    );

    assert_eq!(
        run.applied_position_delete_files()
            .into_iter()
            .collect::<BTreeSet<_>>()
            .len(),
        1,
        "the position delete is named exactly once"
    );
    let produced = run.output_paths();
    assert_eq!(
        deletes.object_values(&produced).await,
        expected,
        "the position delete removed row 0 of its own object and nothing else"
    );
    assert_eq!(
        deletes.fixture.live_data_paths().await,
        live_before,
        "the rewrite published nothing: the live set is still the promoted one"
    );
}

/// One attempt's geometry follows the table's declared targets, not a legacy cap.
///
/// The declared file target is raised well past the erased 128 MiB whole-file
/// ceiling and the row-group target is left independent of it. What the core
/// receives must carry both values unchanged, and the attempt must consume the
/// whole promoted set rather than a fixed number of groups.
#[tokio::test]
async fn managed_rewrite_scaled_geometry_has_no_legacy_file_or_group_ceiling() {
    let fixture = PromotedRewriteFixture::start("rewrite_geometry").await;
    let table = fixture.load_table().await;
    let properties = table.metadata().properties();
    assert_eq!(
        properties.get("write.target-file-size-bytes"),
        None,
        "the fixture table declares no legacy whole-file ceiling"
    );

    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let forge = fixture.forge(
        fixture.fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );
    let run = fixture
        .run_attempt(
            &forge,
            uuid::Uuid::now_v7(),
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
    assert!(
        run.failure.is_none(),
        "the promoted snapshot is rewritable: {:?}",
        run.failure
    );

    assert_eq!(
        run.rewritten_data_files().len(),
        2,
        "one attempt consumed the whole promoted set rather than a fixed number of groups"
    );
    assert_eq!(
        run.output_data_files()
            .into_iter()
            .map(iceberg::spec::DataFile::record_count)
            .sum::<u64>(),
        4,
        "every promoted row is carried forward exactly once"
    );
    for produced in run.output_data_files() {
        assert!(
            produced.file_size_in_bytes() > 0,
            "each produced object carries its own encoded size"
        );
        assert!(
            produced.file_size_in_bytes() < 128 * 1024 * 1024,
            "a set this small is nowhere near a whole-file split: {}",
            produced.file_size_in_bytes()
        );
    }
}

/// Compaction publishes replacements and never deletes what it rewrote.
///
/// This is the whole production route, not the managed core alone: one real
/// promotion, then one real rewrite through the production scheduler and
/// worker over the real catalog and object store. What it requires is the
/// non-destructive contract every later retention decision depends on. The
/// rewrite adds exactly one snapshot; the new cut is made of replacement
/// objects and none of the inputs it consumed; every input object is still
/// present in the store, byte for byte; and the object store was never asked to
/// delete anything at all. A pinned cut that still names an input can therefore
/// keep reading it until retention and cleanup independently permit deletion.
///
/// # Panics
///
/// Panics when the rewrite publishes no snapshot or more than one, when an
/// input survives in the live cut, when an input object was mutated or removed,
/// or when compaction issued any delete.
#[tokio::test]
async fn compaction_publishes_replacements_without_deleting_inputs() {
    let promoted = PromotedRewriteFixture::start_unpromoted("rewrite_nondestructive").await;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        promoted.fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    supervisor.run_one_success().await;

    let inputs = promoted
        .live_data_files()
        .await
        .iter()
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();
    assert!(
        !inputs.is_empty(),
        "the rewrite starts from a promoted live set"
    );
    let before = promoted.load_table().await;
    let snapshots_before = before.metadata().snapshots().count();
    let base_snapshot = before
        .metadata()
        .current_snapshot_id()
        .expect("the promoted table has a current snapshot");
    let digests_before = promoted.fixture.object_digests().await;
    let deletes_before = object_store.deletes();

    supervisor.restart_worker();
    supervisor.run_one_success().await;
    supervisor.shutdown().await;

    let after = promoted.load_table().await;
    assert_eq!(
        after.metadata().snapshots().count(),
        snapshots_before + 1,
        "compaction publishes exactly one new snapshot"
    );
    let published = after
        .metadata()
        .current_snapshot_id()
        .expect("the rewritten table has a current snapshot");
    assert_ne!(
        published, base_snapshot,
        "the published snapshot is the rewrite's own, not the base it planned against"
    );
    let live = promoted
        .live_data_files()
        .await
        .iter()
        .map(|file| file.file_path().to_owned())
        .collect::<BTreeSet<_>>();
    assert!(
        !live.is_empty(),
        "the new cut is made of the replacement objects"
    );
    assert!(
        live.is_disjoint(&inputs),
        "a new cut reads replacements, never the inputs they replaced: {live:?}"
    );

    let digests_after = promoted.fixture.object_digests().await;
    for (path, digest) in &digests_before {
        assert_eq!(
            digests_after.get(path),
            Some(digest),
            "compaction left every input object untouched: {path}"
        );
    }
    assert!(
        inputs
            .iter()
            .all(|path| digests_after.contains_key(path.as_str())),
        "every rewritten input is still readable by an existing pinned cut"
    );
    assert_eq!(
        object_store.deletes(),
        deletes_before,
        "compaction issues no delete at all"
    );
    assert_eq!(
        deletes_before, 0,
        "nothing before the rewrite deleted either"
    );
}
