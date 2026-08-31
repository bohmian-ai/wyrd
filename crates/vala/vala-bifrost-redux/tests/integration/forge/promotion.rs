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

/// Borrows the mutable `data_file` object of one persisted promotion record.
///
/// # Panics
///
/// Panics when the record does not carry a `data_file` object, which would mean
/// the fixture's own Scribe wrote evidence outside its published contract.
fn evidence_data_file(
    record: &mut serde_json::Value,
) -> &mut serde_json::Map<String, serde_json::Value> {
    record
        .get_mut("data_file")
        .and_then(serde_json::Value::as_object_mut)
        .expect("promotion evidence carries a data_file object")
}

/// Adds one to the first entry of a per-column count map inside `data_file`.
///
/// # Panics
///
/// Panics when the named map is absent or empty.
fn bump_first_count(record: &mut serde_json::Value, field: &str) {
    let map = evidence_data_file(record)
        .get_mut(field)
        .and_then(serde_json::Value::as_object_mut)
        .expect("a per-column count map");
    let key = map
        .keys()
        .next()
        .expect("the object has at least one column")
        .clone();
    let value = map[&key].as_u64().expect("a count is a number");
    map.insert(key, serde_json::Value::from(value + 1));
}

/// Flips the last hex digit of the first bound in a `data_file` bound map.
///
/// # Panics
///
/// Panics when the named map is absent, empty, or holds a non-hex bound.
fn flip_first_bound(record: &mut serde_json::Value, field: &str) {
    let map = evidence_data_file(record)
        .get_mut(field)
        .and_then(serde_json::Value::as_object_mut)
        .expect("a per-column bound map");
    let key = map
        .keys()
        .next()
        .expect("the object has at least one bound")
        .clone();
    let bound = map
        .get_mut(&key)
        .and_then(serde_json::Value::as_object_mut)
        .expect("a bound object");
    let hex = bound["value_hex"].as_str().expect("a hex bound").to_owned();
    let flipped = match hex.chars().next_back() {
        Some('0') => format!("{}1", &hex[..hex.len() - 1]),
        _ => format!("{}0", &hex[..hex.len() - 1]),
    };
    bound.insert("value_hex".to_owned(), serde_json::Value::from(flipped));
}

/// Every physical or policy contradiction is refused before any catalog effect.
///
/// The object is a real sealed Parquet file and it is never altered by a record
/// mutation, so a refusal can only come from Forge re-deriving the object's own
/// footer and disagreeing with the persisted evidence. Size and checksum alone
/// cannot see any of these mutations: each one leaves the bytes byte-identical.
///
/// Each mutation gets its own fixture, table, and sealed objects. A refused
/// promotion is durably terminal and its Prepared operation evidence is
/// append-only by design, so the same demand cannot legitimately be refused
/// twice — reusing one table would prove only that Forge refuses to re-run a
/// task it already closed.
#[tokio::test]
async fn scribe_promotion_integration_rejects_physical_datafile_mismatch_before_catalog_io() {
    type Mutation = (&'static str, fn(&mut serde_json::Value));
    const MUTATIONS: [Mutation; 11] = [
        ("record count", |record| {
            let file = evidence_data_file(record);
            let count = file["record_count"].as_u64().expect("a record count");
            file.insert(
                "record_count".to_owned(),
                serde_json::Value::from(count + 1),
            );
        }),
        ("column sizes", |record| {
            bump_first_count(record, "column_sizes");
        }),
        ("value counts", |record| {
            bump_first_count(record, "value_counts");
        }),
        ("null value counts", |record| {
            bump_first_count(record, "null_value_counts");
        }),
        ("lower bounds", |record| {
            flip_first_bound(record, "lower_bounds");
        }),
        ("upper bounds", |record| {
            flip_first_bound(record, "upper_bounds");
        }),
        ("split offsets", |record| {
            evidence_data_file(record).insert(
                "split_offsets".to_owned(),
                serde_json::Value::from(vec![0_i64, 4_i64]),
            );
        }),
        ("partition value", |record| {
            let object = record.as_object_mut().expect("a record object");
            let partition = object
                .get_mut("partition")
                .and_then(serde_json::Value::as_object_mut)
                .expect("a partition value");
            partition.insert(
                "start_utc".to_owned(),
                serde_json::Value::from("1999-01-01T00:00:00Z"),
            );
        }),
        ("partition spec identity", |record| {
            record
                .as_object_mut()
                .expect("a record object")
                .insert("partition_spec_id".to_owned(), serde_json::Value::from(97));
        }),
        ("schema identity", |record| {
            record.as_object_mut().expect("a record object").insert(
                "schema_fingerprint".to_owned(),
                serde_json::Value::from("0".repeat(64)),
            );
        }),
        ("sort order identity", |record| {
            record
                .as_object_mut()
                .expect("a record object")
                .insert("sort_order_id".to_owned(), serde_json::Value::from(93));
        }),
    ];

    for (index, (name, mutate)) in MUTATIONS.into_iter().enumerate() {
        assert_mutation_is_refused_before_catalog_io(index, name, mutate).await;
    }
}

/// Drives one mutated promotion record through the real scheduler and worker.
///
/// The fixture is per-mutation on purpose: a refused promotion is durably
/// terminal and its Prepared operation evidence is append-only, so a second
/// refusal over the same demand is not a thing production can produce.
///
/// # Panics
///
/// Panics when the mutation does not change the evidence, when the refusal does
/// not happen, or when the refusal left any catalog, object, or durable effect.
async fn assert_mutation_is_refused_before_catalog_io(
    index: usize,
    name: &str,
    mutate: fn(&mut serde_json::Value),
) {
    let fixture = PromotionIntegrationFixture::start(&format!("promotion_physical_{index}")).await;
    let records = fixture.promotion_records().await;
    let (row_id, original) = records
        .first()
        .expect("the fixture sealed promotion evidence")
        .clone();
    let mut altered = original.clone();
    mutate(&mut altered);
    assert_ne!(altered, original, "{name} must change the evidence");
    fixture.set_promotion_record(row_id, &altered).await;

    let object_store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    let mut forge = SupervisedPromotion::start(
        &fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    let error = forge.run_one_failure().await;
    forge.shutdown().await;

    assert!(!error.is_empty(), "{name} must report why it was refused");
    assert_eq!(
        catalog.attempts(),
        0,
        "{name} must be refused before any catalog commit"
    );
    assert_eq!(
        object_store.output_writers(),
        0,
        "{name} must not open a rewrite output"
    );
    assert_eq!(object_store.deletes(), 0, "{name} must delete no object");
    assert!(
        fixture.live_data_paths().await.is_empty(),
        "{name} must leave the table without a promoted snapshot"
    );
    assert!(
        fixture
            .file_rows()
            .await
            .iter()
            .all(|row| !row.compacted && row.committed_snapshot_id.is_none()),
        "{name} must leave every Scribe row eligible"
    );
}

/// An object whose bytes contradict its untouched evidence never reaches the catalog.
///
/// The record is left exactly as Scribe wrote it and only the sealed object is
/// corrupted, so the refusal can come from nothing but re-deriving the footer
/// of the bytes that are actually there.
#[tokio::test]
async fn scribe_promotion_integration_rejects_corrupted_object_before_catalog_io() {
    let fixture = PromotionIntegrationFixture::start("promotion_corrupt").await;
    let records = fixture.promotion_records().await;
    let (_, original) = records
        .first()
        .expect("the fixture sealed promotion evidence")
        .clone();
    let object_key = original["object_key"]
        .as_str()
        .expect("the record names its object")
        .to_owned();
    let intact = fixture
        .staging
        .read(&object_key)
        .await
        .expect("the sealed object is readable")
        .to_bytes();
    let mut corrupted = intact.to_vec();
    let last = corrupted.len() - 5;
    corrupted[last] ^= 0xFF;
    fixture
        .staging
        .write(&object_key, corrupted)
        .await
        .expect("fixture object corruption");
    {
        let object_store = CountingObjectStore::new(Arc::clone(&fixture.staging));
        let catalog = PromotionCatalogSeam::new(
            fixture.catalog.iceberg_catalog(),
            object_store.read_counter(),
        );
        let mut forge = SupervisedPromotion::start(
            &fixture,
            Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
            Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
            ForgeClock::system(),
        );
        forge.run_one_failure().await;
        forge.shutdown().await;
        assert_eq!(
            catalog.attempts(),
            0,
            "an object that contradicts its evidence never reaches the catalog"
        );
        assert!(fixture.live_data_paths().await.is_empty());
    }
    fixture
        .staging
        .write(&object_key, intact.to_vec())
        .await
        .expect("fixture object restoration");
}
