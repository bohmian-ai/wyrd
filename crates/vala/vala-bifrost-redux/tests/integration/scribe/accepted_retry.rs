//! A retry the current table definition refuses is answered by the version
//! that accepted its batch id.

mod pg_tests {
    use std::sync::Arc;

    use arrow::array::{RecordBatch, StringArray, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use vala_bifrost_redux::contracts::ScribeError;
    use vala_bifrost_redux::scribe::ScribeImpl;
    use vala_bifrost_redux::scribe::memory::MemoryCategory;

    use crate::forge::support::{
        PromotionIntegrationFixture, fixture_day, fixture_time_field, ingest_unclaimed, ingress_ipc,
    };

    /// Builds `scopes` rows in one fixture day, each with its own `scope`.
    ///
    /// A different `salt` is a different batch with its own content.
    ///
    /// # Panics
    ///
    /// Panics when the fixture day has no midnight or the batch does not build.
    fn scoped(scopes: usize, salt: &str) -> RecordBatch {
        let midnight = fixture_day()
            .and_hms_opt(0, 0, 0)
            .expect("the fixture day has a midnight")
            .and_utc()
            .timestamp_micros();
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("scope", DataType::Utf8, false),
                fixture_time_field(),
            ])),
            vec![
                Arc::new(StringArray::from_iter_values(
                    (0..scopes).map(|index| format!("{salt}.{index}")),
                )),
                Arc::new(
                    TimestampMicrosecondArray::from_iter_values((0..scopes).map(|index| {
                        midnight + i64::try_from(index).expect("row offset fits i64")
                    }))
                    .with_timezone("UTC"),
                ),
            ],
        )
        .expect("scoped batch")
    }

    /// An unclaimed retry the current layout refuses is acknowledged under the
    /// layout that accepted it, warm and from a cold Scribe after publication.
    ///
    /// Thirty-three rows in one day, each with its own `scope`, are accepted
    /// while the table partitions by day only. An identity partition on
    /// `scope` then makes current planning see thirty-three tuples, past the
    /// request-wide limit. The warm retry is answered from the committed
    /// index; a second Scribe with an empty index, after the first published
    /// and retired its WAL, must still acknowledge it as no first commit and
    /// publish nothing. A changed batch no fence accepted keeps the current
    /// layout's refusal.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start, a retry is refused or reported
    /// as a first commit, a retry publishes rows, or the changed batch is
    /// admitted or refused with another error.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_retry_the_current_layout_refuses_is_acknowledged_under_its_own() {
        let fixture = PromotionIntegrationFixture::start_with(
            "accepted_retry_grouping",
            vec![Field::new("scope", DataType::Utf8, false)],
            &[],
        )
        .await;
        let original = scoped(33, "original");
        let batch_id = uuid::Uuid::now_v7();
        let ingest = async |scribe, batch: &RecordBatch| {
            ingest_unclaimed(
                scribe,
                &fixture.catalog,
                fixture.tenant,
                &fixture.binding,
                batch_id,
                batch,
            )
            .await
        };

        let first = ingest(fixture.scribe(), &original)
            .await
            .expect("first acceptance");
        assert!(first.first_commit);
        fixture.commit_identity_partition("scope").await;
        let warm = ingest(fixture.scribe(), &original)
            .await
            .expect("warm retry");
        assert!(!warm.first_commit);
        assert_eq!(warm.rows_accepted, 33);
        fixture
            .scribe()
            .flush_staged()
            .await
            .expect("first Scribe publishes");
        let published = fixture.file_rows().await.len();
        assert_eq!(published, 1, "the batch is published once");

        let (cold, _wal, _scratch) = fixture.start_cold_scribe().await;
        let retried = ingest(&cold, &original).await.expect("cold retry");
        assert!(!retried.first_commit);
        assert_eq!(retried.rows_accepted, 33);
        let changed = ingest_unclaimed(
            &cold,
            &fixture.catalog,
            fixture.tenant,
            &fixture.binding,
            uuid::Uuid::now_v7(),
            &scoped(33, "changed"),
        )
        .await
        .expect_err("a changed batch keeps the current refusal");
        assert!(matches!(changed, ScribeError::InvalidFrame), "{changed:?}");
        cold.flush_staged().await.expect("cold Scribe flushes");
        assert_eq!(
            fixture.file_rows().await.len(),
            published,
            "no retry or refusal publishes rows"
        );
    }

    /// Builds one row per day in each of the two days before the fixture day
    /// ends, so day partitioning materializes two distinct slices.
    ///
    /// # Panics
    ///
    /// Panics when the fixture day has no midnight or the batch does not build.
    fn two_days(salt: &str) -> RecordBatch {
        let midnight = fixture_day()
            .and_hms_opt(0, 0, 0)
            .expect("the fixture day has a midnight")
            .and_utc()
            .timestamp_micros();
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("scope", DataType::Utf8, false),
                fixture_time_field(),
            ])),
            vec![
                Arc::new(StringArray::from_iter_values([
                    format!("{salt}.0"),
                    format!("{salt}.1"),
                ])),
                Arc::new(
                    TimestampMicrosecondArray::from_iter_values([
                        midnight - 86_400_000_000,
                        midnight,
                    ])
                    .with_timezone("UTC"),
                ),
            ],
        )
        .expect("two-day batch")
    }

    /// Bytes `scribe` charges to decoded memory.
    fn decoded(scribe: &ScribeImpl) -> usize {
        scribe.memory_snapshot().categories[MemoryCategory::Decode as usize]
    }

    /// The retained input of an unclaimed write stays charged while its
    /// completion is held, and its charge ends exactly once however the write
    /// settles.
    ///
    /// Each write spans two day partitions, so its prepared slices own new
    /// buffers and the retained IPC frame shares none of them. An open
    /// transaction holds the batch's fence key, which parks the shard group
    /// after its WAL sync. While parked, the retained frame alone is charged
    /// to decoded memory. Releasing the key commits the write, committing a
    /// contradicting fence refuses it, dropping the waiter cancels it, and a
    /// fence naming another version fails its recovery; each leaves no
    /// decoded charge. A cold Scribe then recovers the committed batch
    /// across a partition edit and also leaves none.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start, a write is not parked by its
    /// fence key, the retained charge differs from the frame, a write settles
    /// with another outcome, or any decoded charge outlives its write.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_held_unclaimed_write_keeps_its_retained_input_charged_once() {
        let fixture = PromotionIntegrationFixture::start_with(
            "accepted_retry_retained",
            vec![Field::new("scope", DataType::Utf8, false)],
            &[],
        )
        .await;
        let superuser = fixture.database.superuser_pool().expect("superuser pool");
        let scribe = fixture.scribe();
        let baseline = decoded(scribe);
        let hold = async |batch_id: uuid::Uuid, table_version: i64| {
            let mut blocker = superuser.begin().await.expect("fence blocker");
            sqlx::query(
                "INSERT INTO vala.scribe_batch_commits (data_tenant_id, logical_table_fqn, \
                 batch_id, table_version, slice_set_digest, slice_count, wal_node_id, \
                 wal_writer_epoch, wal_shard_id, wal_segment_sequence, wal_lsn_min, \
                 wal_lsn_max, request_id, ingested_at) \
                 VALUES ($1, $2, $3, $4, $5, 2, $6, 1, 0, 0, 0, 0, $7, statement_timestamp())",
            )
            .bind(fixture.tenant.as_uuid())
            .bind(fixture.binding.table_ref.fqn())
            .bind(batch_id)
            .bind(table_version)
            .bind([0xEE_u8; 32].as_slice())
            .bind(uuid::Uuid::now_v7())
            .bind(uuid::Uuid::now_v7())
            .execute(&mut *blocker)
            .await
            .expect("hold the fence key");
            blocker
        };
        let parked = async || loop {
            let waiting: i64 =
                sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE NOT granted")
                    .fetch_one(&superuser)
                    .await
                    .expect("lock wait probe");
            if waiting > 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };
        let write = |batch_id, batch: &RecordBatch| {
            let batch = batch.clone();
            let fixture = &fixture;
            Box::pin(async move {
                ingest_unclaimed(
                    scribe,
                    &fixture.catalog,
                    fixture.tenant,
                    &fixture.binding,
                    batch_id,
                    &batch,
                )
                .await
            })
        };
        let held = async |write: &mut std::pin::Pin<Box<_>>, batch: &RecordBatch| {
            tokio::select! {
                result = write => panic!("the write must park on its fence key: {result:?}"),
                waited = tokio::time::timeout(std::time::Duration::from_secs(30), parked()) => {
                    waited.expect("the group parks on the held fence key");
                }
            }
            assert_eq!(
                decoded(scribe),
                baseline + ingress_ipc(batch).len(),
                "the retained frame alone is charged while the completion is held"
            );
        };

        let committed = uuid::Uuid::now_v7();
        let original = two_days("committed");
        let blocker = hold(committed, 1).await;
        let mut pending = write(committed, &original);
        held(&mut pending, &original).await;
        blocker.rollback().await.expect("release the fence key");
        assert!(pending.await.expect("the held write commits").first_commit);
        assert_eq!(decoded(scribe), baseline, "success releases the charge");

        let refused = uuid::Uuid::now_v7();
        let batch = two_days("refused");
        let blocker = hold(refused, 1).await;
        let mut pending = write(refused, &batch);
        held(&mut pending, &batch).await;
        blocker
            .commit()
            .await
            .expect("commit a contradicting fence");
        pending
            .await
            .expect_err("a contradicting fence refuses the write");
        assert_eq!(decoded(scribe), baseline, "refusal releases the charge");

        let cancelled = uuid::Uuid::now_v7();
        let batch = two_days("cancelled");
        let blocker = hold(cancelled, 1).await;
        let mut pending = write(cancelled, &batch);
        held(&mut pending, &batch).await;
        drop(pending);
        assert_eq!(
            decoded(scribe),
            baseline,
            "cancellation releases the charge"
        );
        blocker.rollback().await.expect("release the fence key");

        let unrecovered = uuid::Uuid::now_v7();
        let batch = two_days("unrecovered");
        let blocker = hold(unrecovered, 2).await;
        let mut pending = write(unrecovered, &batch);
        held(&mut pending, &batch).await;
        blocker
            .commit()
            .await
            .expect("commit a fence under another version");
        pending
            .await
            .expect_err("a batch no version proves is refused");
        assert_eq!(
            decoded(scribe),
            baseline,
            "a failed recovery releases the charge"
        );

        fixture.commit_identity_partition("scope").await;
        let (cold, _wal, _scratch) = fixture.start_cold_scribe().await;
        let cold_baseline = decoded(&cold);
        let recovered = ingest_unclaimed(
            &cold,
            &fixture.catalog,
            fixture.tenant,
            &fixture.binding,
            committed,
            &original,
        )
        .await
        .expect("the committed batch recovers across the edit");
        assert!(!recovered.first_commit);
        assert_eq!(
            decoded(&cold),
            cold_baseline,
            "recovery releases the charge"
        );
    }
}
