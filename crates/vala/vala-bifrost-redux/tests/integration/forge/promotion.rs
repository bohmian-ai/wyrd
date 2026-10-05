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

/// A promotion whose operation was reset is retried under a fresh operation.
///
/// Two definite conflicts exhaust the single revalidated retry, so the attempt
/// closes its operation as `Reset` and the task waits as `retryable`. A Reset
/// operation can never be reopened, so the task's next attempt must prepare
/// and commit the next operation generation. Reusing the reset identity would
/// instead fail every remaining attempt on the reopen refusal while the
/// retryable task holds the table's promotion behind it. The production retry
/// backoff is brought forward rather than slept through.
///
/// # Panics
///
/// Panics when the surviving conflict does not reset the operation, a later
/// attempt fails on the reopen refusal, or the retry does not commit a
/// second operation generation beside the closed reset one.
#[tokio::test]
async fn scribe_promotion_integration_reset_operation_retries_under_fresh_operation() {
    let fixture = PromotionIntegrationFixture::start("promotion_reset_retry").await;
    let object_store = CountingObjectStore::new(Arc::clone(&fixture.staging));
    let catalog = PromotionCatalogSeam::new(
        fixture.catalog.iceberg_catalog(),
        object_store.read_counter(),
    );
    catalog.reject_next_commits(2);

    let forge = SupervisedPromotion::start(
        &fixture,
        Arc::clone(&catalog) as Arc<dyn iceberg::Catalog>,
        Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
        ForgeClock::system(),
    );
    let mut forge = forge.run_one_failure_while(async {}).await;
    assert_eq!(
        fixture.promotion_phases().await,
        vec!["reset".to_owned()],
        "a conflict that survives the retry resets the operation"
    );

    fixture.clear_task_backoff().await;
    forge.restart_worker();
    forge.run_one_success().await;
    let errors = forge.returned_errors();
    forge.shutdown().await;

    assert!(
        !errors
            .iter()
            .any(|error| error.contains("cannot be reopened")),
        "no attempt tried to reopen the reset operation: {errors:?}"
    );
    assert_eq!(
        fixture.promotion_phases().await,
        vec!["reset".to_owned(), "committed".to_owned()],
        "the retry commits a second operation generation and leaves the reset one closed"
    );
    let settled = fixture.file_rows().await;
    assert!(
        settled
            .iter()
            .all(|row| row.compacted && row.committed_snapshot_id.is_some()),
        "the fresh operation promoted every row: {settled:?}"
    );
    let tasks = fixture.forge_tasks().await;
    assert_eq!(
        tasks
            .iter()
            .map(|task| (task.state.as_str(), task.attempt_count))
            .collect::<Vec<_>>(),
        vec![("succeeded", 1)],
        "the one task succeeded on its first retry: {tasks:?}"
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
/// The coordinator is cancelled while its inline promotion commit is parked, so
/// acceptance is unknown by construction. Draining must therefore release the
/// attempt and its lease while leaving the operation Prepared: settling it
/// either way would claim knowledge the worker does not have, and holding the
/// lease would leak the owner past its own shutdown.
///
/// # Panics
///
/// Panics when the drained operation is not left `prepared`, the drained
/// attempt keeps its lease, or any settlement is recorded.
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
    let coordinator_stop = forge.coordinator_stop();
    let forge = forge
        .run_one_failure_while(async {
            catalog.wait_for_parked_commit().await;
            coordinator_stop.cancel();
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

/// Forge operation family and phase vocabulary shared by the barrier proof.
mod promotion_barrier {
    use uuid::Uuid;
    use vala_sql::SqlError;
    use vala_sql::queries::forge_operations::ForgeOperations;
    use vala_sql::row_types::forge_operations::{
        ForgeClaimTable, ForgeExpirationAuthority, ForgeExpirationPreparation, ForgeOperationFamily,
    };
    use vala_sql::row_types::forge_tasks::ForgeTaskEvidence;
    use vala_sql::row_types::oracle_reader_authority::TableAuthorityIdentity;
    use wyrd_spec::vala::api::{
        AuditDetail, ForgeIcebergRewritePhase, ForgePromotedFile, ForgePromotedFileSetDigest,
        ForgeScribePromotionPhase, ForgeSnapshotExpirePhase, StoragePath, TimeGranularityWire,
        TimePartitionWire,
    };

    use super::super::support::PromotionIntegrationFixture;

    /// Canonical Forge resource every operation family uses for one table.
    fn resource_for(identity: &TableAuthorityIdentity) -> String {
        format!(
            "bifrost://{}/{}/{}",
            identity.tenant, identity.namespace_name, identity.table_name
        )
    }

    /// Builds one Scribe promotion transition detail for the fixture table.
    ///
    /// # Panics
    /// Panics when the fixed object identity is invalid.
    fn promotion_detail(
        resource: &str,
        operation_id: Uuid,
        phase: ForgeScribePromotionPhase,
        committed_snapshot_id: Option<i64>,
    ) -> AuditDetail {
        let path = StoragePath::new("table/hot-a.parquet").expect("valid path");
        let promoted = [
            ForgePromotedFile::new(Uuid::from_u128(5), path.clone(), "checksum")
                .expect("promoted file"),
        ];
        AuditDetail::ForgeScribePromotion {
            operation_id,
            phase,
            group: resource.to_owned(),
            base_snapshot_id: 200,
            committed_snapshot_id,
            input_file_ids: vec![Uuid::from_u128(5)],
            input_paths: vec![path],
            promoted_file_set_digest: ForgePromotedFileSetDigest::compute(&promoted),
        }
    }

    /// Appends one Scribe promotion transition through the operation owner.
    ///
    /// # Panics
    /// Panics when the transition or its commit fails.
    async fn append_promotion(
        fixture: &PromotionIntegrationFixture,
        resource: &str,
        detail: &AuditDetail,
        operation: &str,
    ) {
        let operations = ForgeOperations::new(resource, ForgeOperationFamily::ScribePromotion)
            .expect("valid Forge resource");
        let mut conn = fixture
            .vala
            .tenant_conn(fixture.tenant)
            .await
            .expect("tenant connection");
        if operation.ends_with(".prepared") {
            operations
                .append_prepared(&mut conn, operation, detail)
                .await
        } else {
            operations
                .append_terminal(&mut conn, operation, detail)
                .await
        }
        .expect("the promotion transition appends");
        conn.commit()
            .await
            .expect("the promotion transition commits");
    }

    /// Prepares one new rewrite operation through the operation owner.
    ///
    /// # Errors
    /// Returns the owner's refusal, including an unsettled promotion, and
    /// [`SqlError::Conflict`] when an active read leaves no exclusive authority.
    ///
    /// # Panics
    /// Panics when the fixed detail is invalid or the commit fails.
    async fn prepare_rewrite(
        fixture: &PromotionIntegrationFixture,
        resource: &str,
    ) -> Result<(), SqlError> {
        let detail = AuditDetail::ForgeIcebergRewrite {
            operation_id: Uuid::now_v7(),
            phase: ForgeIcebergRewritePhase::Prepared,
            group: resource.to_owned(),
            base_snapshot_id: 100,
            committed_snapshot_id: None,
            partition_spec_id: 3,
            time_partition: TimePartitionWire::new(
                TimeGranularityWire::Day,
                chrono::DateTime::from_timestamp(1_756_684_800, 0).expect("fixture instant"),
            )
            .expect("fixture instant is an exact day boundary"),
            target_file_size_bytes: 1024,
            input_paths: vec![StoragePath::new("table/live-a.parquet").expect("valid path")],
            output_paths: vec![StoragePath::new("table/live-b.parquet").expect("valid path")],
        };
        let mut conn = fixture
            .vala
            .tenant_conn(fixture.tenant)
            .await
            .expect("tenant connection");
        ForgeOperations::new(resource, ForgeOperationFamily::IcebergRewrite)
            .expect("valid Forge resource")
            .append_prepared(&mut conn, "forge.iceberg_rewrite.prepared", &detail)
            .await?;
        conn.commit()
            .await
            .expect("the rewrite preparation commits");
        Ok(())
    }

    /// Seeds one live Forge lease and one running snapshot-expiry task.
    ///
    /// # Panics
    /// Panics when either seeding statement fails.
    async fn seed_running_expiry_task(
        fixture: &PromotionIntegrationFixture,
        identity: &TableAuthorityIdentity,
    ) -> ForgeExpirationAuthority {
        let authority = ForgeExpirationAuthority {
            task_id: Uuid::now_v7(),
            attempt_id: Uuid::now_v7(),
            worker_id: Uuid::now_v7(),
            lease_key: format!("forge:{}:barrier", identity.table_name),
            lease_fencing_token: 20,
        };
        let pool = fixture.database.operator_pool().pool();
        sqlx::query("INSERT INTO vala.maintenance_leases (lease_key,owner,fencing_token,expires_at,heartbeat_at) VALUES ($1,$2,$3,now()+interval '10 minutes',now())")
            .bind(&authority.lease_key)
            .bind(authority.worker_id)
            .bind(authority.lease_fencing_token)
            .execute(pool)
            .await
            .expect("lease seeds");
        sqlx::query("INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,state,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,ready_at) VALUES ($1,$2,$3,$4,$5,'snapshot_expiry',20,'{}'::jsonb,decode(repeat('00',32),'hex'),1,1,'running',$6,$7,now()+interval '10 minutes',20,1,now())")
            .bind(authority.task_id)
            .bind(identity.tenant.as_uuid())
            .bind(&identity.catalog_name)
            .bind(&identity.namespace_name)
            .bind(&identity.table_name)
            .bind(authority.attempt_id)
            .bind(authority.worker_id)
            .execute(pool)
            .await
            .expect("running task seeds");
        authority
    }

    /// Prepares one snapshot expiration for snapshot 30 through the SQL owner.
    ///
    /// # Errors
    /// Returns the owner's refusal, including an unsettled promotion, and
    /// [`SqlError::Conflict`] when an active read leaves no exclusive authority.
    ///
    /// # Panics
    /// Panics when the fixed resource or path is invalid.
    async fn prepare_expiration(
        fixture: &PromotionIntegrationFixture,
        identity: &TableAuthorityIdentity,
        authority: &ForgeExpirationAuthority,
    ) -> Result<(), SqlError> {
        let resource = resource_for(identity);
        let detail = AuditDetail::ForgeSnapshotExpire {
            operation_id: Uuid::now_v7(),
            phase: ForgeSnapshotExpirePhase::Prepared,
            group: resource.clone(),
            base_metadata_location: StoragePath::new("table/iceberg/metadata/00001-base.json")
                .expect("valid path"),
            current_snapshot_id: Some(90),
            retained_ref_heads: vec![90],
            selected_snapshot_ids: vec![30],
        };
        let table = ForgeClaimTable {
            table_uid: identity.table_uid,
            catalog_name: identity.catalog_name.clone(),
            namespace_name: identity.namespace_name.clone(),
            table_name: identity.table_name.clone(),
            table_uuid: Uuid::now_v7(),
        };
        let evidence = ForgeTaskEvidence {
            prepared_candidate_index: None,
            version: 1,
            committed_snapshot_id: None,
            committed_metadata_location: None,
            committed_metadata_digest: None,
            cleanup_candidates: Vec::new(),
            deleted_candidate_count: 0,
        };
        // Preparation runs the way Forge runs it: under the table's live
        // exclusive maintenance authority, surrendered afterwards.
        let mut conn =
            vala_sql::TenantConn::acquire(fixture.database.app_pool(), identity.tenant).await?;
        let exclusive =
            vala_sql::queries::oracle_reader_authority::BifrostTableMaintenanceAuthority::new(
                &mut conn,
            )
            .exclusive(identity.clone())
            .await?
            .ok_or_else(|| SqlError::Conflict {
                detail: "an Oracle query is still reading this table".to_owned(),
            })?;
        let prepared = ForgeOperations::new(&resource, ForgeOperationFamily::SnapshotExpire)
            .expect("valid Forge resource")
            .prepare_snapshot_expiration(
                fixture.database.operator_pool(),
                identity.tenant,
                &exclusive,
                &ForgeExpirationPreparation {
                    authority,
                    table: &table,
                    evidence: &evidence,
                    operation: "forge.snapshot_expire.prepared",
                    detail: &detail,
                },
            )
            .await
            .map(|_| ());
        drop(exclusive);
        conn.commit().await?;
        prepared
    }

    /// Proves a Prepared promotion blocks rewrite and expiration until settled.
    ///
    /// The promotion's Prepared row is the state a promotion holds between its
    /// catalog append and its `file_list` settlement. While it stands, a new
    /// rewrite preparation and a snapshot-expiration preparation on the same
    /// table both refuse through the operation owner. Once the promotion's
    /// Committed transition lands, both proceed through the same owner.
    ///
    /// # Panics
    /// Panics when either operation is admitted while the promotion is open
    /// or refused after it settles.
    #[tokio::test]
    async fn unsettled_promotion_blocks_rewrite_and_expiration() {
        let fixture = PromotionIntegrationFixture::start("promotion_barrier").await;
        let identity = fixture.table_identity().await;
        let resource = resource_for(&identity);
        let promotion = Uuid::now_v7();
        append_promotion(
            &fixture,
            &resource,
            &promotion_detail(
                &resource,
                promotion,
                ForgeScribePromotionPhase::Prepared,
                None,
            ),
            "forge.scribe_promotion.prepared",
        )
        .await;
        let expiring = seed_running_expiry_task(&fixture, &identity).await;

        assert!(
            matches!(
                prepare_rewrite(&fixture, &resource).await,
                Err(SqlError::Conflict { .. })
            ),
            "an unsettled promotion refuses a new rewrite"
        );
        assert!(
            matches!(
                prepare_expiration(&fixture, &identity, &expiring).await,
                Err(SqlError::Conflict { .. })
            ),
            "an unsettled promotion refuses snapshot expiration"
        );

        append_promotion(
            &fixture,
            &resource,
            &promotion_detail(
                &resource,
                promotion,
                ForgeScribePromotionPhase::Committed,
                Some(201),
            ),
            "forge.scribe_promotion.committed",
        )
        .await;
        prepare_rewrite(&fixture, &resource)
            .await
            .expect("a settled promotion admits the rewrite");
        prepare_expiration(&fixture, &identity, &expiring)
            .await
            .expect("a settled promotion admits snapshot expiration");
    }
}
