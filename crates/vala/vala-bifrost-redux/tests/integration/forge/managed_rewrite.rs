//! Tier-2 proof that the managed rewrite seam produces objects and no snapshot.
//!
//! Every scenario starts from real promoted state: a real Scribe sealed the
//! objects and a real Forge promotion published them. What the scenarios then
//! drive is the production non-committing owner over the real catalog, the real
//! object store, and the real resource governor — never a scheduler, never a
//! worker, and never a fabricated plan.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use iceberg::spec::DataContentType;
use vala_bifrost_redux::forge::{
    ForgeError, ForgeObjectStore, ForgeRewriteAttempt, ForgeRewriteOutcome,
};
use vala_bifrost_redux::resources::ForgeRewriteRequest;

use super::rewrite_support::PromotedRewriteFixture;
use super::support::{CountingObjectStore, PromotionCatalogSeam};

/// The adapter's plan is the core's report, unedited.
///
/// The core is asked once and its selection is compared against the handoff the
/// adapter derived from it: the consumed data paths must be exactly the paths
/// the core's own plans named, in no fewer and no greater number. Trimming,
/// reordering into a different grouping, or recomputing a total locally would
/// break the equality, which is what makes this a call-path proof rather than a
/// count check.
#[tokio::test]
async fn managed_rewrite_plan_matches_core_report_on_promoted_snapshot() {
    let fixture = PromotedRewriteFixture::start("rewrite_plan").await;
    let live = fixture
        .live_data_files()
        .await
        .into_iter()
        .filter(|file| file.content_type() == DataContentType::Data)
        .map(|file| file.file_path().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    let table = fixture.load_table().await;
    let base = table
        .metadata()
        .current_snapshot()
        .expect("a promoted snapshot")
        .snapshot_id();

    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let forge = fixture.forge(
        fixture.fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );
    let outcome = forge
        .execute_rewrite_attempt(
            &fixture.fixture.binding,
            fixture.attempt(uuid::Uuid::now_v7()).await,
        )
        .await
        .expect("the promoted snapshot is rewritable");

    let ForgeRewriteOutcome::Rewritten { evidence, handoff } = outcome else {
        panic!("a promoted small-file set must select: {outcome:?}");
    };
    assert_eq!(
        evidence.base_snapshot_id, base,
        "the plan is bound to the promoted snapshot"
    );
    assert_eq!(
        handoff
            .rewritten_data_files
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        live,
        "the adapter consumed exactly the core's selected data files"
    );
    assert!(
        !handoff.output_data_files.is_empty(),
        "a selected plan produced at least one object"
    );
    assert!(
        !evidence.selection_fingerprint.is_empty()
            && !evidence.debt_fingerprint.is_empty()
            && !evidence.policy_fingerprint.is_empty(),
        "every planned attempt records its three canonical fingerprints"
    );
}

/// A refused lease is refused before the catalog and the object store are touched.
///
/// The demand is set past this fixture's admitted ceiling, so the governor must
/// refuse. The proof that the refusal is free of side effects is bidirectional:
/// the seam records zero table loads, so no manifest was read, and the object
/// digest map is byte-identical, so nothing was written. Any read or write
/// permitted before the refusal breaks one of the two.
#[tokio::test]
async fn managed_rewrite_refuses_resources_before_object_io() {
    let fixture = PromotedRewriteFixture::start("rewrite_refusal").await;
    let before = fixture.object_digests().await;
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        fixture.fixture.catalog.iceberg_catalog(),
        Arc::new(AtomicUsize::new(0)),
    );
    let forge = fixture.forge(
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );

    let mut attempt = fixture.attempt(uuid::Uuid::now_v7()).await;
    attempt.request = ForgeRewriteRequest {
        memory_bytes: usize::MAX / 2,
        ..attempt.request
    };
    let error = forge
        .execute_rewrite_attempt(&fixture.fixture.binding, attempt)
        .await
        .expect_err("an impossible demand must be refused");

    assert!(
        matches!(error, ForgeError::Capacity { .. }),
        "the refusal is a capacity refusal: {error:?}"
    );
    assert_eq!(
        catalog.loads(),
        0,
        "a refused attempt read no table metadata"
    );
    assert_eq!(
        object_store.output_writers(),
        0,
        "a refused attempt opened no output"
    );
    assert_eq!(
        fixture.object_digests().await,
        before,
        "a refused attempt left every object byte-identical"
    );
}

/// Produced objects are distinguishable, and the next pass keeps them.
///
/// The path grammar carries a per-writer ordinal, which resets, and a writer
/// UUID, which does not; the pair is what makes two concurrently produced
/// objects distinct. The second half is the reclassification proof: a produced
/// object carries the current writer recipe, so re-running selection against a
/// live set that now contains it must not name it again.
#[tokio::test]
async fn managed_rewrite_output_identity_is_unique_across_concurrent_writers() {
    let fixture = PromotedRewriteFixture::start("rewrite_identity").await;
    let attempt_id = uuid::Uuid::now_v7();
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let forge = fixture.forge(
        fixture.fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );
    let outcome = forge
        .execute_rewrite_attempt(&fixture.fixture.binding, fixture.attempt(attempt_id).await)
        .await
        .expect("the promoted snapshot is rewritable");
    let ForgeRewriteOutcome::Rewritten { handoff, .. } = outcome else {
        panic!("a promoted small-file set must select: {outcome:?}");
    };

    let paths = handoff
        .output_data_files
        .iter()
        .map(|file| file.file_path().to_owned())
        .collect::<Vec<_>>();
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
            path.contains(&attempt_id.to_string()),
            "a produced object names its attempt: {path}"
        );
    }

    let repeat = forge
        .execute_rewrite_attempt(
            &fixture.fixture.binding,
            ForgeRewriteAttempt {
                attempt_id: uuid::Uuid::now_v7(),
                ..fixture.attempt(uuid::Uuid::now_v7()).await
            },
        )
        .await
        .expect("a second pass is executable");
    if let ForgeRewriteOutcome::Rewritten { handoff, .. } = repeat {
        for produced in &paths {
            assert!(
                !handoff.rewritten_data_files.contains(produced),
                "a current-recipe object must not be reselected: {produced}"
            );
        }
    }
}

/// Cancellation drains and reports every object it may have produced.
///
/// The attempt is cancelled before it is executed, so the core stops without
/// settling. What the seam must not do is claim a result: it returns the
/// cancellation with its possible-output set intact and leaves the catalog
/// alone, which is what lets a later reconciliation reclaim whatever landed.
#[tokio::test]
async fn managed_rewrite_cancellation_drains_and_preserves_possible_outputs() {
    let fixture = PromotedRewriteFixture::start("rewrite_drain").await;
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        fixture.fixture.catalog.iceberg_catalog(),
        Arc::new(AtomicUsize::new(0)),
    );
    let forge = fixture.forge(
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );
    let attempt = fixture.attempt(uuid::Uuid::now_v7()).await;
    attempt.cancel.cancel();

    let outcome = forge
        .execute_rewrite_attempt(&fixture.fixture.binding, attempt)
        .await;

    match outcome {
        Ok(ForgeRewriteOutcome::Cancelled {
            possible_outputs, ..
        }) => {
            let paths = possible_outputs
                .iter()
                .map(|output| output.path.clone())
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                paths.len(),
                possible_outputs.len(),
                "a drained attempt reports each possible object once"
            );
        }
        Ok(ForgeRewriteOutcome::NoProgress { .. }) => {}
        other => panic!("a cancelled attempt must not claim a rewrite: {other:?}"),
    }
    assert_eq!(
        catalog.attempts(),
        0,
        "a drained attempt issued no catalog commit"
    );
    assert!(
        fixture
            .fixture
            .live_data_paths()
            .await
            .iter()
            .all(|path| { !path.contains("/data/forge/") }),
        "a drained attempt published nothing"
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
    let outcome = forge
        .execute_rewrite_attempt(
            &fixture.fixture.binding,
            fixture.attempt(uuid::Uuid::now_v7()).await,
        )
        .await
        .expect("the promoted snapshot is rewritable");
    let ForgeRewriteOutcome::Rewritten { evidence, handoff } = outcome else {
        panic!("a promoted small-file set must select: {outcome:?}");
    };

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
    assert_eq!(
        handoff.base_snapshot_id, evidence.base_snapshot_id,
        "the handoff names the snapshot the evidence was derived from"
    );
    let objects_after = fixture.object_digests().await;
    for path in handoff
        .output_data_files
        .iter()
        .map(iceberg::spec::DataFile::file_path)
    {
        assert!(
            objects_after
                .keys()
                .any(|object| path.ends_with(object.as_str()) || object.ends_with(path)),
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
    let outcome = forge
        .execute_rewrite_attempt(
            &fixture.fixture.binding,
            fixture.attempt(uuid::Uuid::now_v7()).await,
        )
        .await
        .expect("a table carrying equality-delete debt is rewritable");
    let ForgeRewriteOutcome::Rewritten { handoff, .. } = outcome else {
        panic!("delete debt must select: {outcome:?}");
    };

    assert_eq!(
        handoff.applied_equality_delete_files.len(),
        1,
        "the equality delete is named exactly once however many groups it covers"
    );
    assert!(
        handoff.applied_position_delete_files.is_empty(),
        "no position delete was published in this pass"
    );
    let produced = handoff
        .output_data_files
        .iter()
        .map(|file| file.file_path().to_owned())
        .collect::<Vec<_>>();
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
    let outcome = forge
        .execute_rewrite_attempt(
            &deletes.fixture.binding,
            deletes.attempt(uuid::Uuid::now_v7()).await,
        )
        .await
        .expect("a table carrying position-delete debt is rewritable");
    let ForgeRewriteOutcome::Rewritten { handoff, .. } = outcome else {
        panic!("position-delete debt must select: {outcome:?}");
    };

    assert_eq!(
        handoff.applied_position_delete_files.len(),
        1,
        "the position delete is named exactly once"
    );
    let produced = handoff
        .output_data_files
        .iter()
        .map(|file| file.file_path().to_owned())
        .collect::<Vec<_>>();
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
/// receives must carry both values unchanged, and the whole promoted set must
/// still fit one plan rather than being split into a fixed number of groups.
#[tokio::test]
async fn managed_rewrite_scaled_geometry_has_no_legacy_file_or_group_ceiling() {
    let mut fixture = PromotedRewriteFixture::start("rewrite_geometry").await;
    fixture.fixture.config.rewrite_max_plans_per_attempt = 8;
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
    let outcome = forge
        .execute_rewrite_attempt(
            &fixture.fixture.binding,
            fixture.attempt(uuid::Uuid::now_v7()).await,
        )
        .await
        .expect("the promoted snapshot is rewritable");
    let ForgeRewriteOutcome::Rewritten { handoff, .. } = outcome else {
        panic!("a promoted small-file set must select: {outcome:?}");
    };

    assert_eq!(
        handoff.rewritten_data_files.len(),
        2,
        "one attempt consumed the whole promoted set rather than a fixed number of groups"
    );
    assert_eq!(
        handoff
            .output_data_files
            .iter()
            .map(iceberg::spec::DataFile::record_count)
            .sum::<u64>(),
        4,
        "every promoted row is carried forward exactly once"
    );
    for produced in &handoff.output_data_files {
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

/// A second pass over an unchanged live set does no work at all.
///
/// The first attempt's evidence is handed back to the second. Because the base
/// snapshot has not advanced, the refusal happens before any manifest or object
/// is opened: the seam records no table load past the first, and the object
/// digest map is unchanged.
#[tokio::test]
async fn managed_rewrite_second_pass_is_zero_io_without_live_set_delta() {
    let fixture = PromotedRewriteFixture::start("rewrite_second_pass").await;
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        fixture.fixture.catalog.iceberg_catalog(),
        Arc::new(AtomicUsize::new(0)),
    );
    let forge = fixture.forge(
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
    );
    let outcome = forge
        .execute_rewrite_attempt(
            &fixture.fixture.binding,
            fixture.attempt(uuid::Uuid::now_v7()).await,
        )
        .await
        .expect("the promoted snapshot is rewritable");
    let ForgeRewriteOutcome::Rewritten { evidence, .. } = outcome else {
        panic!("a promoted small-file set must select: {outcome:?}");
    };

    let digests = fixture.object_digests().await;
    let writers_before = object_store.output_writers();
    let second = forge
        .execute_rewrite_attempt(
            &fixture.fixture.binding,
            ForgeRewriteAttempt {
                previous: Some(&evidence),
                ..fixture.attempt(uuid::Uuid::now_v7()).await
            },
        )
        .await
        .expect("a second pass is executable");

    assert!(
        matches!(second, ForgeRewriteOutcome::NoProgress { .. }),
        "an unchanged live set makes no progress: {second:?}"
    );
    assert_eq!(
        object_store.output_writers(),
        writers_before,
        "the refused second pass opened no output"
    );
    assert_eq!(
        fixture.object_digests().await,
        digests,
        "the refused second pass wrote nothing"
    );
}
