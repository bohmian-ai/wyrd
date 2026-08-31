//! Tier-2 seam proofs for the Scribe promotion owner.
//!
//! Every scenario drives the real Forge scheduler and worker over hot objects a
//! real Scribe sealed into the fixture's own database and warehouse. Nothing
//! here inserts a `vala.file_list` row, plans a task by hand, calls the
//! promotion owner directly, or fabricates a committed result: the owner is
//! only proven if the production loops reach it on their own.

use std::collections::BTreeMap;
use std::sync::Arc;

use vala_bifrost_redux::forge::{ForgeClock, ForgeObjectStore};

use super::support::{
    CountingObjectStore, PromotionCatalogSeam, PromotionIntegrationFixture, SupervisedPromotion,
    manual_clock,
};

/// Promotion appends the writer's own objects and never writes a data object.
///
/// The proof is bidirectional. Every data object under the table prefix is
/// byte-identical across the whole route, so nothing was written; and the
/// production object-store seam records reads but opens no output writer and
/// issues no delete, so the reads that did happen are footer revalidation
/// rather than a rewrite in disguise. The table then references exactly the
/// paths Scribe published.
#[tokio::test]
async fn scribe_promotion_integration_appends_existing_datafile_without_put() {
    let fixture = PromotionIntegrationFixture::start("promotion_append").await;
    let sealed = fixture
        .file_rows()
        .await
        .into_iter()
        .map(|row| row.file_path)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        fixture.live_data_paths().await.is_empty(),
        "a hot object is not yet referenced by the table"
    );
    let before = fixture.object_digests().await;
    assert!(
        before.keys().any(|path| !path.contains("/metadata/")),
        "the fixture sealed real data objects"
    );

    let object_store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let mut forge = SupervisedPromotion::start(
        &fixture,
        fixture.catalog.iceberg_catalog(),
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    forge.run_one_success().await;
    forge.shutdown().await;

    assert_eq!(
        fixture.live_data_paths().await,
        sealed,
        "promotion references the writer's own objects at their own paths"
    );
    let after = fixture.object_digests().await;
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
    assert_eq!(
        object_store.output_writers(),
        0,
        "promotion opened no rewrite output"
    );
    assert_eq!(object_store.deletes(), 0, "promotion deleted no object");
    assert!(
        object_store.reads() >= sealed.len(),
        "promotion revalidated every hot object it appended"
    );

    let settled = fixture.file_rows().await;
    assert_eq!(
        settled.len(),
        sealed.len(),
        "promotion added no durable row"
    );
    assert!(
        settled
            .iter()
            .all(|row| row.compacted && row.committed_snapshot_id.is_some()),
        "a successful promotion settles every planned row: {settled:?}"
    );
    assert_eq!(
        fixture.promotion_phases().await,
        vec!["committed".to_owned()],
        "the promotion settles its operation once"
    );
}

/// A definite conflict is revalidated against fresh state before the retry.
///
/// A refusal issued before delegation is certain knowledge that nothing
/// landed, so promotion is allowed exactly one retry against it — but only
/// after re-reading the hot objects. Comparing the object-store read count
/// sampled at the refusal with the count after the attempt is what proves the
/// retry revalidated rather than replaying a stale plan; the second delegated
/// commit is what proves the retry happened at all.
#[tokio::test]
async fn scribe_promotion_integration_conflict_revalidates_before_retry() {
    let fixture = PromotionIntegrationFixture::start("promotion_conflict").await;
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    catalog.reject_next_commits(1);

    let mut forge = SupervisedPromotion::start(
        &fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    forge.run_one_success().await;
    forge.shutdown().await;

    assert_eq!(
        catalog.attempts(),
        2,
        "one definite conflict is followed by exactly one retry"
    );
    assert!(
        object_store.reads() > catalog.reads_at_conflict(),
        "the retry revalidated the hot objects: {} reads at the conflict, {} after",
        catalog.reads_at_conflict(),
        object_store.reads()
    );
    let settled = fixture.file_rows().await;
    assert!(
        settled
            .iter()
            .all(|row| row.compacted && row.committed_snapshot_id.is_some()),
        "the revalidated retry promoted every row: {settled:?}"
    );
    assert_eq!(
        fixture.promotion_phases().await,
        vec!["committed".to_owned()],
        "the retried promotion settles once"
    );
}

/// The operation deadline bounds the conflict retry to zero second attempts.
///
/// The commit is parked at the real catalog seam, the Forge clock is moved past
/// the operation's own retry budget, and only then is the parked commit
/// refused. The deadline captured before the first attempt is therefore already
/// spent, so the definite conflict closes the operation instead of buying a
/// second catalog call. One delegated update — against two in the retry
/// scenario — is what proves the barrier held.
#[tokio::test]
async fn scribe_promotion_integration_deadline_expires_before_conflict_retry() {
    let fixture = PromotionIntegrationFixture::start("promotion_deadline").await;
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    catalog.park_next_commit();
    let (clock, control) = manual_clock();

    let forge = SupervisedPromotion::start(
        &fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        clock,
    );
    let forge = forge
        .run_one_failure_while(async {
            catalog.wait_for_parked_commit().await;
            let expired = control.now().expect("manual Forge clock")
                + chrono::Duration::from_std(fixture.config.iceberg_total_retry_timeout)
                    .expect("retry budget is representable")
                + chrono::Duration::seconds(1);
            control.set(expired).expect("manual Forge clock advances");
            catalog.reject_parked_commit();
        })
        .await;

    assert_eq!(
        catalog.attempts(),
        1,
        "the expired deadline permitted no second catalog call: {:?}",
        forge.returned_errors()
    );
    assert_eq!(
        fixture.promotion_phases().await,
        vec!["reset".to_owned()],
        "a conflict past the deadline closes the operation"
    );
    let unsettled = fixture.file_rows().await;
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
/// either way would claim knowledge the worker does not have, and holding the
/// lease would leak the owner past its own shutdown.
#[tokio::test]
async fn scribe_promotion_integration_cancellation_drains_without_settlement() {
    let fixture = PromotionIntegrationFixture::start("promotion_drain").await;
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    catalog.park_next_commit();

    let forge = SupervisedPromotion::start(
        &fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    let worker_stop = forge.worker_stop();
    let forge = forge
        .run_one_failure_while(async {
            catalog.wait_for_parked_commit().await;
            worker_stop.cancel();
            catalog.wait_for_parked_commit_drop().await;
        })
        .await;

    assert_eq!(
        fixture.promotion_phases().await,
        vec!["prepared".to_owned()],
        "a drained promotion stays open rather than claiming an outcome: {:?}",
        forge.returned_errors()
    );
    let unsettled = fixture.file_rows().await;
    assert!(
        unsettled
            .iter()
            .all(|row| !row.compacted && row.committed_snapshot_id.is_none()),
        "a drained promotion settled nothing: {unsettled:?}"
    );
    assert_eq!(
        fixture.live_leases().await,
        0,
        "the drained attempt released its lease"
    );
    forge.shutdown().await;
}
