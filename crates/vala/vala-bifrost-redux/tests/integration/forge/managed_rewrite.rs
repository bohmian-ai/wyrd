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

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use arrow::array::{Array as _, AsArray as _, Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow::compute::cast;
use arrow::datatypes::{DataType, Field, Int64Type, Schema as ArrowSchema, TimeUnit};
use arrow::util::display::array_value_to_string;
use futures_util::TryStreamExt as _;
use iceberg::metadata_columns::{
    RESERVED_COL_NAME_FILE, RESERVED_COL_NAME_LAST_UPDATED_SEQUENCE_NUMBER,
    RESERVED_COL_NAME_ROW_ID,
};
use iceberg::spec::{DataContentType, DataFile, FormatVersion, ManifestContentType, Operation};
use iceberg::table::Table;
use parquet_variant_compute::{VariantArray, unshred_variant, variant_to_json};
use vala_bifrost_redux::catalog::layout::FORGE_WRITER_RECIPE;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::forge::{
    ForgeClock, ForgeError, ForgeObjectStore, ForgeTableKey, ForgeUnsettledOutput,
};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::tables::builtin_tables;
use vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity;
use wyrd_queue::variant::{EncodedVariant, VariantColumnBuilder};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;
use wyrd_types::variant::variant_field;

use super::rewrite_support::{AttemptRun, PromotedRewriteFixture, RewriteOutputBreak};
use super::support::{
    CountingObjectStore, PromotionCatalogSeam, PromotionIntegrationFixture, SupervisedPromotion,
    fixture_day, remove_table_properties, set_table_properties,
};

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
        matches!(&run.failure, Some(ForgeError::Shutdown))
            || matches!(
                &run.failure,
                Some(ForgeError::RewriteUnsettled { source, .. })
                    if matches!(**source, ForgeError::Shutdown)
            ),
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
            path.contains(&format!("/data/forge/{FORGE_WRITER_RECIPE}/")),
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
        matches!(&run.failure, Some(ForgeError::Shutdown))
            || matches!(
                &run.failure,
                Some(ForgeError::RewriteUnsettled { source, .. })
                    if matches!(**source, ForgeError::Shutdown)
            ),
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
///
/// # Panics
///
/// Panics when the fixture table declares a whole-file size property, the
/// attempt fails, it rewrites fewer than both promoted files, the output does
/// not carry every row exactly once, or a produced object is empty or reaches
/// the erased 128 MiB ceiling.
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
            .map(DataFile::record_count)
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
    // One snapshot per admitted plan: each plan publishes independently, so
    // the attempt adds as many snapshots as it published plans and never
    // rewrites one of them into another.
    assert!(
        after.metadata().snapshots().count() > snapshots_before,
        "compaction publishes a snapshot for every plan it admitted"
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

/// Rows in every staged object the small-files scenario seals.
///
/// Large enough that the incompressible `value` column dominates each object's
/// size, so two staged objects merged are close to the sum of their sizes.
const STAGED_ROWS: usize = 8192;

/// Day, counted back from the fixture day, the small-files scenario seals into.
///
/// Clear of the fixture's own lone objects at days zero and one.
const STAGED_DAY: i64 = 5;

/// Returns the live data files whose row count is exactly `rows`.
///
/// The scenario's staged objects all carry [`STAGED_ROWS`] rows and a merged
/// pair carries twice that, while the fixture's own lone objects carry two, so
/// row count alone identifies each kind of file in the live cut.
///
/// # Panics
///
/// Panics when the live cut cannot be read.
async fn live_files_with_rows(promoted: &PromotedRewriteFixture, rows: usize) -> Vec<DataFile> {
    let rows = u64::try_from(rows).expect("fixture row counts fit u64");
    promoted
        .live_data_files()
        .await
        .into_iter()
        .filter(|file| file.record_count() == rows)
        .collect()
}

/// Returns the object path of every file, in order.
fn paths(files: &[DataFile]) -> BTreeSet<String> {
    files
        .iter()
        .map(|file| file.file_path().to_owned())
        .collect()
}

/// Returns the sorted `value` column of every file.
///
/// # Panics
///
/// Panics when an object cannot be read.
async fn values(promoted: &PromotedRewriteFixture, files: &[DataFile]) -> Vec<i64> {
    promoted
        .object_values(&paths(files).into_iter().collect::<Vec<_>>())
        .await
}

/// Runs one production promotion and then the compaction it makes due.
///
/// The fixture table is due on every commit, so each promotion is followed by
/// a leader dispatch planned with the table's default compaction type.
///
/// # Panics
///
/// Panics when either attempt misses its bound or fails.
async fn promote_then_compact(supervisor: &mut SupervisedPromotion) {
    supervisor.restart_worker();
    supervisor.run_one_success().await;
    supervisor.restart_worker();
    supervisor.run_one_success().await;
}

/// Declares a file target a quarter above two staged objects and returns its
/// small-file threshold.
///
/// This is the scaled form of production's geometry: the 75% threshold sits
/// above one staged object and below a merged pair. The quarter of headroom is
/// needed because the writer rolls on written bytes plus its open row group's
/// uncompressed estimate, so a target equal to the pair would roll the merge
/// into two files. The row group keeps production's one-eighth of the target.
///
/// # Panics
///
/// Panics when a staged object is not below the threshold or the property
/// commit fails.
async fn scale_target_to_pair(promoted: &PromotedRewriteFixture, staged: &[DataFile]) -> u64 {
    let staged_bytes: u64 = staged.iter().map(DataFile::file_size_in_bytes).sum();
    let target = staged_bytes * 5 / 4;
    let row_group = target / 8;
    let threshold = target / 100 * 75 + target % 100 * 75 / 100;
    for file in staged {
        assert!(
            file.file_size_in_bytes() < threshold,
            "a staged object is a small file: {} of {threshold}",
            file.file_size_in_bytes()
        );
    }
    set_table_properties(
        &promoted.fixture.catalog,
        &promoted.fixture.binding,
        &[
            ("write.target-file-size-bytes", &target.to_string()),
            ("write.parquet.row-group-size-bytes", &row_group.to_string()),
        ],
    )
    .await;
    threshold
}

/// Staged files merge once, a finished file is never revisited, and a lone
/// staged file waits for a partner.
///
/// Scaled geometry over real files through the production scheduler and
/// worker, with the fixture's compaction type removed so the table plans with
/// the default. The table target is set a quarter above two staged objects,
/// so its 75% small-file threshold sits above one staged object and below a
/// merged pair, as 768 MiB sits between a 512 MiB staged file and a 1 GiB
/// output. Two staged objects
/// in one day merge into one output that reaches the threshold, while the
/// fixture's single-file days stay as they are. A third staged object alone
/// beside that output is not rewritten. When a fourth arrives, the third and
/// fourth merge and the first output stays live, untouched.
///
/// # Panics
///
/// Panics when a staged pair is not merged into one threshold-sized output,
/// when a lone file or a finished output is rewritten, or when rows change.
#[tokio::test]
async fn small_files_merges_staged_pairs_once_and_lone_files_wait() {
    let promoted = PromotedRewriteFixture::start_unpromoted("rewrite_small_files_once").await;
    remove_table_properties(
        &promoted.fixture.catalog,
        &promoted.fixture.binding,
        &["wyrd.forge.compaction.type"],
    )
    .await;
    promoted
        .fixture
        .seal_partition_files(STAGED_DAY, STAGED_ROWS, 2, 0)
        .await;
    let object_store = CountingObjectStore::new(Arc::clone(&promoted.fixture.staging));
    let mut supervisor = SupervisedPromotion::start(
        &promoted.fixture,
        promoted.fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    supervisor.run_one_success().await;

    let staged = live_files_with_rows(&promoted, STAGED_ROWS).await;
    assert_eq!(staged.len(), 2, "both staged objects are promoted as-is");
    let lone = paths(&live_files_with_rows(&promoted, 2).await);
    assert_eq!(
        lone.len(),
        2,
        "the fixture keeps one object in each of two days"
    );
    let threshold = scale_target_to_pair(&promoted, &staged).await;
    let staged_values = values(&promoted, &staged).await;

    supervisor.restart_worker();
    supervisor.run_one_success().await;
    let merged = live_files_with_rows(&promoted, 2 * STAGED_ROWS).await;
    assert_eq!(merged.len(), 1, "the staged pair merges into one output");
    let first_output = merged[0].file_path().to_owned();
    assert!(
        merged[0].file_size_in_bytes() >= threshold,
        "the merged output reaches the small-file threshold: {} of {threshold}",
        merged[0].file_size_in_bytes()
    );
    assert!(
        live_files_with_rows(&promoted, STAGED_ROWS)
            .await
            .is_empty(),
        "both staged inputs left the live cut"
    );
    assert_eq!(
        paths(&live_files_with_rows(&promoted, 2).await),
        lone,
        "a file alone in its day is not rewritten"
    );
    assert_eq!(
        values(&promoted, &merged).await,
        staged_values,
        "the merge carries every staged row exactly once"
    );

    let rows = u64::try_from(STAGED_ROWS).expect("fixture row counts fit u64");
    promoted
        .fixture
        .seal_partition_files(STAGED_DAY, STAGED_ROWS, 1, 2 * rows)
        .await;
    promote_then_compact(&mut supervisor).await;
    let waiting = live_files_with_rows(&promoted, STAGED_ROWS).await;
    assert_eq!(
        waiting.len(),
        1,
        "a staged object alone beside a finished output waits"
    );
    assert_eq!(
        paths(&live_files_with_rows(&promoted, 2 * STAGED_ROWS).await),
        BTreeSet::from([first_output.clone()]),
        "a finished output is not rewritten with a lone staged object"
    );

    promoted
        .fixture
        .seal_partition_files(STAGED_DAY, STAGED_ROWS, 1, 3 * rows)
        .await;
    promote_then_compact(&mut supervisor).await;
    supervisor.shutdown().await;
    let outputs = live_files_with_rows(&promoted, 2 * STAGED_ROWS).await;
    assert_eq!(
        outputs.len(),
        2,
        "the waiting object merged with its partner"
    );
    assert!(
        paths(&outputs).contains(&first_output),
        "the first output is never selected again"
    );
    assert!(
        live_files_with_rows(&promoted, STAGED_ROWS)
            .await
            .is_empty(),
        "the waiting object and its partner left the live cut"
    );
    assert_eq!(
        paths(&live_files_with_rows(&promoted, 2).await),
        lone,
        "single-file days still wait"
    );
}

/// Upper bound on production steps any one lineage phase may take.
const LINEAGE_STEPS: usize = 24;

/// Distinct files every original row must have lived in: its promoted object
/// and two successive rewrite outputs.
const REWRITTEN_TWICE: usize = 3;

/// One live row: its logical identity and the hidden v3 lineage the reader
/// resolves for it.
///
/// Ordered by logical identity first. The fixtures may repeat a logical value
/// once Forge has cleaned up the files that carried it, so a row is the whole
/// tuple rather than the logical value alone.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LiveRow {
    /// Logical identity of the row, rendered from its key column.
    key: String,
    /// `_row_id` the reader resolves for the row.
    row_id: i64,
    /// `_last_updated_sequence_number` the reader resolves for the row.
    last_updated_sequence_number: i64,
}

/// One table whose lineage the scenario follows across every rewrite.
struct LineageTable {
    /// Bound tenant table the rows live in.
    binding: TenantTableBinding,
    /// Logical column that identifies each generated row.
    key_column: &'static str,
    /// Every row seen so far, as first observed, with every data file it has
    /// lived in.
    rows: BTreeMap<LiveRow, BTreeSet<String>>,
    /// Rows promoted before the first rewrite, which must be rewritten twice.
    original: BTreeSet<LiveRow>,
}

impl LineageTable {
    /// Starts following one table that has not been observed yet.
    fn new(binding: TenantTableBinding, key_column: &'static str) -> Self {
        Self {
            binding,
            key_column,
            rows: BTreeMap::new(),
            original: BTreeSet::new(),
        }
    }

    /// Loads the table's current metadata through the real catalog.
    ///
    /// # Panics
    ///
    /// Panics when the table cannot be loaded.
    async fn load(&self, fixture: &PromotionIntegrationFixture) -> Table {
        fixture
            .catalog
            .iceberg_catalog()
            .load_table(&self.binding.table_ident())
            .await
            .expect("lineage table loads")
    }

    /// Scans every live row with its hidden lineage and current file.
    ///
    /// The scan asks the fork reader for both reserved columns and `_file`
    /// beside the key column, which is the resolution any reader of the table
    /// gets: a physical value when a rewrite wrote one, otherwise the value
    /// inherited from the file's `first_row_id` and data sequence number.
    ///
    /// # Panics
    ///
    /// Panics when the scan fails or a lineage value is null.
    async fn scan(&self, fixture: &PromotionIntegrationFixture) -> Vec<(LiveRow, String)> {
        let table = self.load(fixture).await;
        let mut rows = Vec::new();
        if table.metadata().current_snapshot().is_none() {
            return rows;
        }
        let batches: Vec<RecordBatch> = table
            .scan()
            .select([
                self.key_column,
                RESERVED_COL_NAME_ROW_ID,
                RESERVED_COL_NAME_LAST_UPDATED_SEQUENCE_NUMBER,
                RESERVED_COL_NAME_FILE,
            ])
            .build()
            .expect("lineage scan builds")
            .to_arrow()
            .await
            .expect("lineage scan starts")
            .try_collect()
            .await
            .expect("lineage scan reads");
        for batch in &batches {
            let lineage_column = |name: &str, to: &DataType| {
                let column = batch
                    .column_by_name(name)
                    .unwrap_or_else(|| panic!("the scan returns {name}"));
                let values = cast(column, to).expect("lineage values cast");
                assert_eq!(values.null_count(), 0, "every live row carries {name}");
                values
            };
            let row_ids = lineage_column(RESERVED_COL_NAME_ROW_ID, &DataType::Int64);
            let sequences = lineage_column(
                RESERVED_COL_NAME_LAST_UPDATED_SEQUENCE_NUMBER,
                &DataType::Int64,
            );
            let files = lineage_column(RESERVED_COL_NAME_FILE, &DataType::Utf8);
            let row_ids = row_ids.as_primitive::<Int64Type>();
            let sequences = sequences.as_primitive::<Int64Type>();
            let files = files.as_string::<i32>();
            let keys = batch
                .column_by_name(self.key_column)
                .expect("the scan returns the key column");
            for row in 0..batch.num_rows() {
                let live = LiveRow {
                    key: array_value_to_string(keys, row).expect("key renders"),
                    row_id: row_ids.value(row),
                    last_updated_sequence_number: sequences.value(row),
                };
                rows.push((live, files.value(row).to_owned()));
            }
        }
        rows
    }

    /// Proves every row seen before is still live with the lineage it was
    /// first seen with, then records new rows and each row's current file.
    ///
    /// # Panics
    ///
    /// Panics when a known row is no longer live with the same logical value,
    /// `_row_id`, and `_last_updated_sequence_number`.
    async fn observe(&mut self, fixture: &PromotionIntegrationFixture) {
        let current = self.scan(fixture).await;
        let name = &self.binding.table_ref.name;
        let live: BTreeSet<&LiveRow> = current.iter().map(|(row, _)| row).collect();
        for row in self.rows.keys() {
            assert!(
                live.contains(row),
                "{name} row {row:?} keeps its _row_id and _last_updated_sequence_number"
            );
        }
        for (row, file) in current {
            self.rows.entry(row).or_default().insert(file);
        }
    }

    /// Marks every row seen so far as one that must be rewritten twice.
    ///
    /// # Panics
    ///
    /// Panics when a row already lived in more than its promoted object, which
    /// would mean a rewrite ran before the baseline was taken.
    fn freeze_original(&mut self) {
        for (row, homes) in &self.rows {
            assert_eq!(homes.len(), 1, "{row:?} is recorded before any rewrite");
        }
        self.original = self.rows.keys().cloned().collect();
    }

    /// Reports whether every original row has lived in two rewrite outputs.
    fn rewritten_twice(&self) -> bool {
        !self.original.is_empty()
            && self.original.iter().all(|row| {
                self.rows
                    .get(row)
                    .is_some_and(|homes| homes.len() >= REWRITTEN_TWICE)
            })
    }

    /// Reports whether the held leader term currently owes this table a rewrite.
    ///
    /// # Panics
    ///
    /// Panics when the table identity is invalid.
    fn owes_compaction(
        &self,
        fixture: &PromotionIntegrationFixture,
        supervisor: &SupervisedPromotion,
    ) -> bool {
        let key = ForgeTableKey {
            tenant: fixture.tenant,
            table: ForgeTaskTableIdentity::new(
                "wyrd-redux",
                self.binding.table_ref.namespace.as_str(),
                &self.binding.table_ref.name,
            )
            .expect("lineage table identity"),
        };
        supervisor
            .forge()
            .held_leader_term()
            .is_some_and(|term| term.schedule().owes_compaction(&key))
    }
}

/// Requests one production pass and settles the attempts it makes runnable.
///
/// Two tables share the worker, so one pass can make more than one attempt
/// runnable; this requires success and progress rather than exactly one.
///
/// # Panics
///
/// Panics when the pass or the attempts miss their bound or any attempt fails.
async fn advance(supervisor: &mut SupervisedPromotion) {
    supervisor.restart_worker();
    let pass = supervisor.request_pass();
    supervisor.settle_some_success().await;
    supervisor.await_pass(pass).await;
}

/// Runs one production step and re-observes every table.
///
/// # Panics
///
/// Panics when the step fails or any observation fails.
async fn advance_and_observe(
    supervisor: &mut SupervisedPromotion,
    fixture: &PromotionIntegrationFixture,
    tables: &mut [LineageTable],
) {
    advance(supervisor).await;
    for table in tables.iter_mut() {
        table.observe(fixture).await;
    }
}

/// Counts the data manifests of a table's current snapshot.
///
/// # Panics
///
/// Panics when the table has no head or its manifest list cannot be read.
async fn head_data_manifests(table: &Table) -> usize {
    let head = table.metadata().current_snapshot().expect("a head");
    table
        .manifest_list_reader(head)
        .load()
        .await
        .expect("manifest list loads")
        .entries()
        .iter()
        .filter(|manifest| manifest.content == ManifestContentType::Data)
        .count()
}

/// Provisions every built-in and asserts each is created as format v3.
///
/// # Panics
///
/// Panics when a built-in cannot be provisioned or loaded, or is not v3.
async fn assert_builtins_are_v3(fixture: &PromotionIntegrationFixture) {
    for definition in builtin_tables() {
        fixture
            .catalog
            .ensure_builtin(fixture.tenant, definition)
            .await
            .expect("every built-in provisions");
        let namespace = BifrostNamespace::from_domain_namespace(definition.namespace)
            .expect("built-in namespace");
        let binding = TenantTableBinding::resolve((
            fixture.tenant,
            TableRef::new(namespace, definition.name),
        ))
        .expect("built-in binding");
        let table = fixture
            .catalog
            .iceberg_catalog()
            .load_table(&binding.table_ident())
            .await
            .expect("built-in loads");
        assert_eq!(
            table.metadata().format_version(),
            FormatVersion::V3,
            "vala.{}.{} is v3",
            definition.namespace,
            definition.name
        );
    }
}

/// Fragments the user table's head and lets one leader pass collect it.
///
/// Compaction is disabled and drained first so the fragmenting promotion is
/// the head; the maintenance pass must then rewrite the v3 manifests, expire
/// replaced snapshots, and leave every observed lineage intact. Returns the
/// user row count before the fragmenting promotion.
///
/// # Panics
///
/// Panics when the head is not fragmented, the pass does not rewrite and
/// expire, or any lineage observation fails.
async fn collect_v3_garbage(
    fixture: &PromotionIntegrationFixture,
    supervisor: &mut SupervisedPromotion,
    tables: &mut [LineageTable],
) -> usize {
    set_table_properties(
        &fixture.catalog,
        &fixture.binding,
        &[("wyrd.forge.enable-compaction", "false")],
    )
    .await;
    for _ in 0..LINEAGE_STEPS {
        if !tables[0].owes_compaction(fixture, supervisor) {
            break;
        }
        advance_and_observe(supervisor, fixture, tables).await;
    }
    let rows_before = tables[0].rows.len();
    fixture.seal_more(1).await;
    advance_and_observe(supervisor, fixture, tables).await;
    assert_eq!(
        tables[0].rows.len(),
        rows_before + 2,
        "the fragmenting rows are promoted"
    );
    let before = tables[0].load(fixture).await;
    let before_manifests = head_data_manifests(&before).await;
    let before_snapshots = before.metadata().snapshots().count();
    assert!(
        before_manifests >= 2,
        "the head is fragmented: {before_manifests}"
    );
    supervisor.maintain_only().await;
    let after = tables[0].load(fixture).await;
    assert_eq!(
        after
            .metadata()
            .current_snapshot()
            .expect("a head")
            .summary()
            .operation,
        Operation::Replace,
        "the leader pass rewrote the v3 manifests"
    );
    assert!(
        head_data_manifests(&after).await < before_manifests,
        "the leader pass merged the v3 manifests"
    );
    assert!(
        after.metadata().snapshots().count() < before_snapshots,
        "the leader pass expired replaced snapshots"
    );
    tables[0].observe(fixture).await;
    rows_before
}

/// Hidden v3 row lineage survives repeated Forge rewrites and v3 GC.
///
/// Every built-in and the user table are created as format v3. A user table
/// and a built-in table are written through the real Scribe and promoted;
/// the production scheduler and worker then rewrite them until every
/// originally promoted row has lived in two successive rewrite outputs, with
/// more rows promoted between rounds because only Forge's own commits make a
/// table due. After every step each known row, found by its logical value,
/// must still carry the `_row_id` and
/// `_last_updated_sequence_number` it was first seen with. Finally a leader
/// maintenance pass rewrites the user table's fragmented manifests and expires
/// its replaced snapshots, existing lineage survives it, and rows promoted
/// after it are read with lineage too.
///
/// # Panics
///
/// Panics when a table is not v3, when any row's lineage changes, or when GC
/// does not rewrite and expire.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn v3_row_lineage_survives_repeated_rewrite() {
    let promoted = PromotedRewriteFixture::start_unpromoted("v3_lineage").await;
    let fixture = &promoted.fixture;
    assert_builtins_are_v3(fixture).await;
    let mut builtin_files = 2;
    let builtin = fixture
        .seal_builtin_table(
            BifrostNamespace::Verification,
            "results",
            0,
            builtin_files,
            3,
        )
        .await;
    set_table_properties(
        &fixture.catalog,
        &fixture.binding,
        &[
            ("wyrd.forge.enable-manifest-rewrite", "true"),
            ("commit.manifest.min-count-to-merge", "2"),
        ],
    )
    .await;
    let mut tables = [
        LineageTable::new(fixture.binding.clone(), "value"),
        LineageTable::new(builtin, "result_id"),
    ];
    assert_eq!(
        tables[0].load(fixture).await.metadata().format_version(),
        FormatVersion::V3,
        "the user table is v3"
    );

    let object_store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let mut supervisor = SupervisedPromotion::start_serial(
        fixture,
        fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    supervisor.join_worker().await;

    // Promotion publishes the sealed rows; their lineage is the baseline.
    advance_and_observe(&mut supervisor, fixture, &mut tables).await;
    assert_eq!(tables[0].rows.len(), 4, "the user rows are promoted");
    assert_eq!(tables[1].rows.len(), 6, "the built-in rows are promoted");
    for table in &mut tables {
        table.freeze_original();
    }

    // Rewrite until every original row has moved through two outputs.
    for _ in 0..LINEAGE_STEPS {
        if tables.iter().all(LineageTable::rewritten_twice) {
            break;
        }
        if !tables
            .iter()
            .any(|table| table.owes_compaction(fixture, &supervisor))
        {
            fixture.seal_more(1).await;
            fixture
                .seal_builtin_table(
                    BifrostNamespace::Verification,
                    "results",
                    builtin_files,
                    1,
                    3,
                )
                .await;
            builtin_files += 1;
        }
        advance_and_observe(&mut supervisor, fixture, &mut tables).await;
    }
    for table in &tables {
        assert!(
            table.rewritten_twice(),
            "every original row of {} was rewritten twice",
            table.binding.table_ref.name
        );
    }

    let rows_before = collect_v3_garbage(fixture, &mut supervisor, &mut tables).await;

    // Rows promoted after the manifest rewrite join the lineage baseline.
    fixture.seal_more(1).await;
    advance_and_observe(&mut supervisor, fixture, &mut tables).await;
    assert_eq!(
        tables[0].rows.len(),
        rows_before + 4,
        "the fresh rows are promoted"
    );
    supervisor.shutdown().await;
}

/// One logical row of the Variant scenario: its key and its JSON document.
type VariantRow = (i64, serde_json::Value);

/// Builds one ingress batch of `docs` whose event times land `day` days before
/// the fixture day, so each batch is its own closed partition and Forge plan.
///
/// # Panics
///
/// Panics when a document cannot be encoded or the batch cannot be assembled.
fn variant_batch(day: i64, docs: &[VariantRow]) -> RecordBatch {
    let noon = (fixture_day() - chrono::Duration::days(day))
        .and_hms_opt(12, 0, 0)
        .expect("fixture timestamp")
        .and_utc()
        .timestamp_micros();
    let mut variants = VariantColumnBuilder::with_capacity(docs.len());
    for (_, doc) in docs {
        variants.append(&EncodedVariant::from_json(doc).expect("fixture Variant encodes"));
    }
    RecordBatch::try_new(
        Arc::new(ArrowSchema::new(vec![
            Field::new("value", DataType::Int64, false),
            variant_field("v", true),
            Field::new(
                WYRD_EVENT_TIME,
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
        ])),
        vec![
            Arc::new(Int64Array::from_iter_values(
                docs.iter().map(|(key, _)| *key),
            )),
            variants.finish(),
            Arc::new(
                TimestampMicrosecondArray::from_iter_values(
                    (0..i64::try_from(docs.len()).expect("bounded fixture rows"))
                        .map(|row| noon + row),
                )
                .with_timezone("UTC"),
            ),
        ],
    )
    .expect("fixture Variant batch")
}

/// Reads every row of `batches` as its key and its logical JSON document.
///
/// The Variant column is unshredded first, so a shredded object and an
/// unshredded scan of the same rows read back identically.
///
/// # Panics
///
/// Panics when a batch lacks either column or a value cannot be rendered.
fn logical_rows(batches: &[RecordBatch]) -> BTreeSet<(i64, String)> {
    let mut rows = BTreeSet::new();
    for batch in batches {
        let keys = batch
            .column_by_name("value")
            .expect("the key column")
            .as_primitive::<Int64Type>();
        let variant = VariantArray::try_new(batch.column_by_name("v").expect("the Variant column"))
            .expect("Variant storage");
        let logical: arrow::array::ArrayRef = unshred_variant(&variant).expect("unshred").into();
        let json = variant_to_json(&logical).expect("Variant JSON");
        for row in 0..batch.num_rows() {
            let doc: serde_json::Value =
                serde_json::from_str(json.value(row)).expect("Variant JSON parses");
            rows.insert((keys.value(row), doc.to_string()));
        }
    }
    rows
}

/// Renders the scenario's input rows the way [`logical_rows`] reads them.
fn expected_rows(docs: &[VariantRow]) -> BTreeSet<(i64, String)> {
    docs.iter()
        .map(|(key, doc)| (*key, doc.to_string()))
        .collect()
}

/// Returns the shredded field names of column `v` in one stored object.
///
/// An unshredded object returns no names.
///
/// # Panics
///
/// Panics when the object has no batch or no Variant column.
fn shredded_fields(batches: &[RecordBatch]) -> Vec<String> {
    let column = batches[0].column_by_name("v").expect("the Variant column");
    match VariantArray::try_new(column)
        .expect("Variant storage")
        .typed_value_column()
        .map(arrow::array::Array::data_type)
    {
        Some(DataType::Struct(fields)) => fields.iter().map(|field| field.name().clone()).collect(),
        _ => Vec::new(),
    }
}

/// Returns the top-level keys of every document an object holds, sorted.
fn document_keys(rows: &BTreeSet<(i64, String)>) -> Vec<String> {
    let keys: BTreeSet<String> = rows
        .iter()
        .flat_map(|(_, doc)| {
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(doc)
                .expect("an object document")
                .into_iter()
                .map(|(key, _)| key)
        })
        .collect();
    keys.into_iter().collect()
}

/// Asserts every object shreds exactly its own documents' fields and keeps its rows.
///
/// Each object's shredded fields must equal the keys of the documents it
/// holds: a table-wide union would add keys another object's documents carry.
/// The union of every object's logical rows must equal `expected`.
///
/// # Panics
///
/// Panics when an object's layout or the combined rows differ.
async fn assert_per_file_layouts(
    promoted: &PromotedRewriteFixture,
    paths: &[String],
    expected: &BTreeSet<(i64, String)>,
    read: &str,
) {
    let mut rows = BTreeSet::new();
    for path in paths {
        let batches = promoted.object_batches(path).await;
        let object_rows = logical_rows(&batches);
        assert_eq!(
            shredded_fields(&batches),
            document_keys(&object_rows),
            "{read} object {path} shreds exactly its own documents' fields"
        );
        rows.extend(object_rows);
    }
    assert_eq!(&rows, expected, "{read} objects keep every logical row");
}

/// Reads every live row of the table through the Iceberg reader.
///
/// # Panics
///
/// Panics when the scan fails.
async fn published_rows(promoted: &PromotedRewriteFixture) -> BTreeSet<(i64, String)> {
    let batches: Vec<RecordBatch> = promoted
        .load_table()
        .await
        .scan()
        .select(["value", "v"])
        .build()
        .expect("published scan builds")
        .to_arrow()
        .await
        .expect("published scan starts")
        .try_collect()
        .await
        .expect("published scan reads");
    logical_rows(&batches)
}

/// Returns the `typed_value` type of shredded field `field` of column `v`.
///
/// # Panics
///
/// Panics when `field` is not shredded in the first batch.
fn shredded_type(batches: &[RecordBatch], field: &str) -> DataType {
    let column = batches[0].column_by_name("v").expect("the Variant column");
    let Some(DataType::Struct(fields)) = VariantArray::try_new(column)
        .expect("Variant storage")
        .typed_value_column()
        .map(arrow::array::Array::data_type)
        .cloned()
    else {
        panic!("column v is not shredded");
    };
    let (_, child) = fields.find(field).expect("shredded field");
    let DataType::Struct(slots) = child.data_type() else {
        panic!("shredded field {field} is not a value/typed_value pair");
    };
    slots
        .find("typed_value")
        .expect("typed_value slot")
        .1
        .data_type()
        .clone()
}

/// A rewrite's outputs take the layout combined from its sources' footers.
///
/// Two hot objects share one closed day. `A` holds `{"t": int, "a": int}` in
/// 10 rows. `B` holds `t` as a string in 3 rows, one of which also has `z`,
/// so `B` alone shreds `t` as a string and `z`. Combined, `a` covers 10 of
/// 13 rows and is kept; `t` keeps the integer type that covers more rows;
/// `z` covers 1 of 13 rows, under the 10% threshold, and is dropped even
/// though a source shredded it. Every output reads back the logical rows and
/// records one Forge residual-share sample.
///
/// # Panics
///
/// Panics when an output's layout differs from the combined layout or a
/// logical row is lost.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn rewrite_outputs_share_the_combined_source_layout() {
    let telemetry = super::support::ForgeTelemetryCheckpoint::install();
    let mut docs: Vec<VariantRow> = (1..=10)
        .map(|key| (key, serde_json::json!({"a": key, "t": key})))
        .collect();
    docs.push((11, serde_json::json!({"t": "x", "z": 1})));
    docs.push((12, serde_json::json!({"t": "y"})));
    docs.push((13, serde_json::json!({"t": "w"})));
    let fixture = PromotionIntegrationFixture::start_with(
        "variant_combined",
        vec![
            Field::new("value", DataType::Int64, false),
            variant_field("v", true),
        ],
        &[variant_batch(0, &docs[..10]), variant_batch(0, &docs[10..])],
    )
    .await;
    let promoted = PromotedRewriteFixture { fixture };
    let fixture = &promoted.fixture;
    let hot: Vec<String> = fixture
        .file_rows()
        .await
        .into_iter()
        .map(|row| row.file_path)
        .collect();
    assert_per_file_layouts(&promoted, &hot, &expected_rows(&docs), "hot").await;

    let object_store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let mut supervisor = SupervisedPromotion::start_serial(
        fixture,
        fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    supervisor.join_worker().await;
    let mut tables = [LineageTable::new(fixture.binding.clone(), "value")];
    advance_and_observe(&mut supervisor, fixture, &mut tables).await;
    assert_eq!(
        tables[0].rows.len(),
        docs.len(),
        "the Variant rows are promoted"
    );
    supervisor.shutdown().await;

    let run = rewrite_whole_attempt(&promoted).await;
    let outputs = run.output_paths();
    assert!(!outputs.is_empty(), "the shared day is rewritten");
    let mut rows = BTreeSet::new();
    for path in &outputs {
        let batches = promoted.object_batches(path).await;
        assert_eq!(
            shredded_fields(&batches),
            ["a", "t"],
            "output {path} keeps the fields covering 10% of the rewrite's rows"
        );
        assert!(
            shredded_type(&batches, "t").is_integer(),
            "output {path} keeps the type covering more rows"
        );
        rows.extend(logical_rows(&batches));
    }
    assert_eq!(
        rows,
        expected_rows(&docs),
        "rewrite outputs keep every logical row"
    );
    let forge_samples: u64 = telemetry
        .snapshot()
        .histograms
        .iter()
        .filter(|(series, _)| {
            series.starts_with("bifrost_variant_residual_share{")
                && series.contains("writer=\"forge\"")
        })
        .map(|(_, histogram)| histogram.count)
        .sum();
    assert_eq!(
        forge_samples,
        u64::try_from(outputs.len()).expect("bounded outputs"),
        "each output records its residual share"
    );
}

/// Standard Variant layouts round-trip through Scribe, promotion, and Forge
/// with each file's layout drawn from its own day's documents.
///
/// Two closed days hold documents of different shapes (`a` integers, `b`
/// strings). Each sealed object takes its claim's sampled layout and each
/// rewrite plan combines only its own day's source footers, so no file
/// shreds another day's fields. Logical rows are compared on the hot objects, the
/// published table, one non-committing rewrite's outputs, and after every
/// original row has been rewritten twice by the production scheduler. Each
/// handoff's outputs must describe the objects written (row count, size) and
/// name the evidence's base snapshot, and no object may shred another day's
/// fields. Rolled outputs may close in any order, so outputs are compared as
/// a set.
///
/// # Panics
///
/// Panics when any read differs from the input rows, a layout is a union, or
/// a handoff does not describe its outputs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Postgres, Iceberg, and object storage"]
async fn standard_variant_layouts_round_trip_per_file() {
    let mut docs: Vec<VariantRow> = vec![
        (1, serde_json::json!({"a": 1})),
        (2, serde_json::json!({"a": 2})),
        (3, serde_json::json!({"b": "x"})),
        (4, serde_json::json!({"b": "y"})),
    ];
    let fixture = PromotionIntegrationFixture::start_with(
        "variant_layouts",
        vec![
            Field::new("value", DataType::Int64, false),
            variant_field("v", true),
        ],
        &[variant_batch(0, &docs[..2]), variant_batch(1, &docs[2..])],
    )
    .await;
    let promoted = PromotedRewriteFixture { fixture };
    let fixture = &promoted.fixture;

    let hot: Vec<String> = fixture
        .file_rows()
        .await
        .into_iter()
        .map(|row| row.file_path)
        .collect();
    assert_per_file_layouts(&promoted, &hot, &expected_rows(&docs), "hot").await;

    let object_store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let mut supervisor = SupervisedPromotion::start_serial(
        fixture,
        fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    supervisor.join_worker().await;
    let mut tables = [LineageTable::new(fixture.binding.clone(), "value")];
    advance_and_observe(&mut supervisor, fixture, &mut tables).await;
    assert_eq!(tables[0].rows.len(), 4, "the Variant rows are promoted");
    tables[0].freeze_original();
    assert_eq!(
        published_rows(&promoted).await,
        expected_rows(&docs),
        "the published read returns the logical rows"
    );

    let run = rewrite_whole_attempt(&promoted).await;
    assert_eq!(run.handoffs.len(), 2, "one plan per closed day");
    for handoff in &run.handoffs {
        assert_eq!(
            handoff.base_snapshot_id,
            run.evidence().base_snapshot_id,
            "each handoff names the snapshot the evidence was derived from"
        );
        for output in &handoff.output_data_files {
            let batches = promoted.object_batches(output.file_path()).await;
            let rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
            assert_eq!(
                output.record_count(),
                u64::try_from(rows).expect("bounded fixture rows"),
                "the output DataFile counts the rows its object holds"
            );
        }
    }
    assert_per_file_layouts(
        &promoted,
        &run.output_paths(),
        &expected_rows(&docs),
        "rewrite",
    )
    .await;

    let mut day = 2;
    for _ in 0..LINEAGE_STEPS {
        if tables[0].rewritten_twice() {
            break;
        }
        if !tables[0].owes_compaction(fixture, &supervisor) {
            let first = docs.len();
            let key = i64::try_from(first).expect("bounded fixture rows") + 1;
            docs.push((key, serde_json::json!({ format!("d{day}"): key })));
            fixture.seal(&[variant_batch(day, &docs[first..])]).await;
            day += 1;
        }
        advance_and_observe(&mut supervisor, fixture, &mut tables).await;
    }
    assert!(
        tables[0].rewritten_twice(),
        "every original Variant row was rewritten twice"
    );
    supervisor.shutdown().await;

    let live: Vec<String> = promoted
        .live_data_files()
        .await
        .iter()
        .map(|file| file.file_path().to_owned())
        .collect();
    assert_per_file_layouts(&promoted, &live, &expected_rows(&docs), "twice-compacted").await;
    assert_eq!(
        published_rows(&promoted).await,
        expected_rows(&docs),
        "the twice-compacted read returns the logical rows"
    );
}
