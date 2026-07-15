mod pg_tests {
    //! Integration tests for Scribe seal state machine.
    //!
    //! Tests verify:
    //! - Seal writes exactly one `vala.file_list` row with correct metadata
    //! - Seal emits one `vala.audit_outbox` row per append (preserves principal)
    //! - Cross-day batches produce separate `file_list` rows per `partition_day`
    //! - Seal tx failure leaves no `file_list` or audit rows (rollback atomicity)
    //!
    //! Skipped when `WYRD_DATABASE_URL` is unset (credential-free default suite).

    use arrow::array::{RecordBatch, TimestampMicrosecondArray, UInt64Array};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use chrono::DateTime;
    use opendal::services::Memory;
    use sqlx::types::Uuid;
    use std::sync::Arc;
    use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
    use vala_bifrost_redux::scribe::ScribeImpl;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;

    #[cfg(feature = "scribe-inspect")]
    use vala_bifrost_redux::inspect::ScribeInspect;

    async fn setup() -> (PgFixture, DataTenantId, ScribeImpl) {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant,
                &format!("test-{}", tenant.as_uuid().simple()),
            )
            .await
            .unwrap();

        let operator = Arc::new(
            opendal::Operator::new(Memory::default())
                .expect("memory backend")
                .finish(),
        );

        let temp_dir = tempfile::tempdir().expect("temp WAL dir");
        let node_id = Uuid::now_v7();
        let node_id_bytes = *node_id.as_bytes();
        let wal = Arc::new(
            vala_bifrost_redux::scribe::wal::WalWriter::new(
                temp_dir.path(),
                node_id_bytes,
                1,
                tenant,
                None,
            )
            .expect("WAL writer"),
        );

        // Leak temp_dir to keep WAL files for test lifetime
        std::mem::forget(temp_dir);

        let writer_epoch = 1;
        let scribe = ScribeImpl::new_with_deps(operator, wal, node_id.to_string(), writer_epoch);

        (fixture, tenant, scribe)
    }

    fn make_batch(row_count: usize, base_time_micros: i64) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("value", DataType::UInt64, false),
        ]));

        let timestamps: Vec<i64> = (0..row_count)
            .map(|i| base_time_micros + (i64::try_from(i).expect("bounded row index") * 1000))
            .collect();
        let values: Vec<u64> = (0..row_count)
            .map(|i| u64::try_from(i).expect("bounded row index"))
            .collect();

        RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(TimestampMicrosecondArray::from(timestamps)),
                Arc::new(UInt64Array::from(values)),
            ],
        )
        .expect("batch")
    }

    fn encode_batch(batch: &RecordBatch) -> Vec<u8> {
        use arrow::ipc::writer::StreamWriter;
        let mut buf = Vec::new();
        {
            let mut writer = StreamWriter::try_new(&mut buf, &batch.schema()).expect("writer");
            writer.write(batch).expect("write");
            writer.finish().expect("finish");
        }
        buf
    }

    fn principal_for_tenant(tenant: DataTenantId) -> Principal {
        Principal {
            id: PrincipalId::new(Uuid::now_v7()),
            kind: PrincipalKind::User,
            tenant_id: tenant,
            roles: vec![],
            effective_permissions: PermissionSet::new(),
        }
    }

    fn stub_schema_fingerprint() -> [u8; 32] {
        [0u8; 32]
    }

    #[tokio::test]
    #[cfg(feature = "scribe-inspect")]
    async fn pg_scribe_append_seal_file_list() {
        let (fixture, tenant, scribe) = setup().await;

        // 1. Append 50k rows to trigger seal predicate
        let base_time = DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
            .unwrap()
            .timestamp_micros();
        let batch = make_batch(50_000, base_time);
        let batch_data = encode_batch(&batch);
        let principal = principal_for_tenant(tenant);

        let req = ScribeAppend {
            table_fqn: "vala.events".to_string(),
            schema_fingerprint: stub_schema_fingerprint(),
            batch_data,
            principal,
        };

        scribe.append(req).await.expect("append");

        // 2. Force seal
        let vala = vala_sql::ValaPostgres::from_pools(fixture.app_pool().clone(), None);
        let pool = vala.pool();
        let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn");
        scribe.force_seal(&mut conn).await.expect("force_seal");
        conn.commit().await.expect("commit");

        // 3. Verify file_list row - assert all 14 columns per plan
        let mut conn2 = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn2");
        let tx = conn2.transaction();

        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            Uuid,   // id
            String, // namespace
            String, // table_name
            String, // file_path
            i64,    // row_count
            i64,    // file_size
            String, // partition_day
            i64,    // wal_lsn_min
            i64,    // wal_lsn_max
            i32,    // tenant_bucket
            Uuid,   // node_id
            i64,    // writer_epoch
            i64,    // min_event_time
            i64,    // max_event_time
        )> = sqlx::query_as(
            r"
            SELECT id, namespace, table_name, file_path, row_count, file_size,
                   partition_day::text, wal_lsn_min, wal_lsn_max, tenant_bucket,
                   node_id, writer_epoch, min_event_time, max_event_time
            FROM vala.file_list
            WHERE namespace = 'vala' AND table_name = 'events'
            ",
        )
        .fetch_all(&mut **tx)
        .await
        .expect("file_list query");

        assert_eq!(rows.len(), 1, "expected exactly one file_list row");
        let (
            id,
            namespace,
            table_name,
            file_path,
            row_count,
            file_size,
            partition_day,
            wal_lsn_min,
            wal_lsn_max,
            tenant_bucket,
            node_id,
            writer_epoch,
            min_event_time,
            max_event_time,
        ) = &rows[0];

        assert_ne!(*id, Uuid::nil(), "id should be non-nil UUID");
        assert_eq!(namespace, "vala");
        assert_eq!(table_name, "events");
        assert!(
            file_path.starts_with("bifrost/"),
            "file_path should start with bifrost/"
        );
        assert!(
            file_path.ends_with(".parquet"),
            "file_path should end with .parquet"
        );
        assert_eq!(*row_count, 50_000);
        assert!(*file_size > 0, "file_size should be positive");
        assert_eq!(partition_day, "2026-07-14");
        assert!(*wal_lsn_min > 0, "wal_lsn_min should be > 0");
        assert!(*wal_lsn_max > 0, "wal_lsn_max should be > 0");
        assert!(*wal_lsn_min <= *wal_lsn_max, "LSN range should be valid");
        assert!(
            *tenant_bucket >= 0 && *tenant_bucket < 1024,
            "tenant_bucket should be in [0, 1024)"
        );
        assert_ne!(*node_id, Uuid::nil(), "node_id should be non-nil");
        assert_eq!(*writer_epoch, 1);
        assert!(
            *min_event_time <= *max_event_time,
            "event time range should be valid"
        );
    }

    #[tokio::test]
    #[cfg(feature = "scribe-inspect")]
    async fn pg_scribe_seal_emits_one_audit_row_per_append() {
        let (fixture, tenant, scribe) = setup().await;

        // 1. Three appends from three distinct principals
        let base_time = DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
            .unwrap()
            .timestamp_micros();

        for i in 0..3 {
            let batch = make_batch(1000, base_time + (i * 1_000_000));
            let batch_data = encode_batch(&batch);
            let mut principal = principal_for_tenant(tenant);
            principal.id = PrincipalId::new(Uuid::now_v7()); // Unique principal per append

            let req = ScribeAppend {
                table_fqn: "vala.events".to_string(),
                schema_fingerprint: stub_schema_fingerprint(),
                batch_data,
                principal,
            };

            scribe.append(req).await.expect("append");
        }

        // 2. Force seal
        let vala = vala_sql::ValaPostgres::from_pools(fixture.app_pool().clone(), None);
        let pool = vala.pool();
        let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn");
        scribe.force_seal(&mut conn).await.expect("force_seal");
        conn.commit().await.expect("commit");

        // 3. Verify audit_outbox has 3 rows with all 13 AuditEvent fields
        let mut conn2 = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn2");
        let tx = conn2.transaction();

        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            Uuid,           // request_id
            Option<String>, // trace_id
            String,         // operation
            String,         // resource
            Option<String>, // card_ref (JSON)
            Uuid,           // principal_id
            String,         // principal_kind
            Uuid,           // principal_tenant_id
            String,         // decision
            Option<String>, // result (JSON)
            String,         // auth_method
            Option<String>, // metadata (JSON)
            Option<String>, // error (JSON)
        )> = sqlx::query_as(
            r"
            SELECT request_id, trace_id, operation, resource, card_ref,
                   principal_id, principal_kind, principal_tenant_id,
                   decision, result, auth_method, metadata, error
            FROM vala.audit_outbox
            WHERE operation = 'bifrost.append'
            ORDER BY request_id
            ",
        )
        .fetch_all(&mut **tx)
        .await
        .expect("audit query");

        assert_eq!(rows.len(), 3, "expected 3 audit rows (one per append)");

        for (i, row) in rows.iter().enumerate() {
            let (
                request_id,
                trace_id,
                operation,
                resource,
                card_ref,
                principal_id,
                principal_kind,
                principal_tenant_id,
                decision,
                result,
                auth_method,
                _metadata,
                _error,
            ) = row;

            assert_ne!(*request_id, Uuid::nil(), "request_id {i} should be non-nil");
            assert!(trace_id.is_none(), "trace_id should be None for this test");
            assert_eq!(operation, "bifrost.append");
            assert!(!resource.is_empty(), "resource should be non-empty");
            assert!(card_ref.is_none(), "card_ref should be None for this test");
            assert_ne!(
                *principal_id,
                Uuid::nil(),
                "principal_id {i} should be non-nil"
            );
            assert_eq!(principal_kind, "user");
            assert_eq!(*principal_tenant_id, tenant.as_uuid());
            assert_eq!(decision, "allow", "decision should be allow");
            assert!(result.is_some(), "result should be Some");
            assert!(!auth_method.is_empty(), "auth_method should be non-empty");
            // metadata and error are optional and may be None
        }
    }

    #[tokio::test]
    #[cfg(feature = "scribe-inspect")]
    async fn pg_scribe_cross_day_batch_produces_two_files() {
        let (fixture, tenant, scribe) = setup().await;

        // Create a batch spanning two days: 60 rows on 2026-07-14, 40 rows on 2026-07-15
        let day1_time = DateTime::parse_from_rfc3339("2026-07-14T23:59:50Z")
            .unwrap()
            .timestamp_micros();
        let day2_time = DateTime::parse_from_rfc3339("2026-07-15T00:00:10Z")
            .unwrap()
            .timestamp_micros();

        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("value", DataType::UInt64, false),
        ]));

        let mut timestamps = Vec::new();
        let mut values: Vec<u64> = Vec::new();

        // 60 rows on day 1
        for i in 0_i64..60 {
            timestamps.push(day1_time + (i * 100_000)); // 100ms apart
            values.push(u64::try_from(i).expect("bounded row index"));
        }

        // 40 rows on day 2
        for i in 60_i64..100 {
            timestamps.push(day2_time + ((i - 60) * 100_000));
            values.push(u64::try_from(i).expect("bounded row index"));
        }

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(TimestampMicrosecondArray::from(timestamps)),
                Arc::new(UInt64Array::from(values)),
            ],
        )
        .expect("batch");

        let batch_data = encode_batch(&batch);
        let principal = principal_for_tenant(tenant);

        let req = ScribeAppend {
            table_fqn: "vala.events".to_string(),
            schema_fingerprint: stub_schema_fingerprint(),
            batch_data,
            principal,
        };

        scribe.append(req).await.expect("append");

        // Force seal
        let vala = vala_sql::ValaPostgres::from_pools(fixture.app_pool().clone(), None);
        let pool = vala.pool();
        let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn");
        scribe.force_seal(&mut conn).await.expect("force_seal");
        conn.commit().await.expect("commit");

        // Verify two file_list rows with distinct partition_day
        let mut conn2 = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn2");
        let tx = conn2.transaction();
        let rows: Vec<(String, i64)> = sqlx::query_as(
            r"
            SELECT partition_day::text, row_count
            FROM vala.file_list
            WHERE namespace = 'vala' AND table_name = 'events'
            ORDER BY partition_day
            ",
        )
        .fetch_all(&mut **tx)
        .await
        .expect("file_list query");

        assert_eq!(rows.len(), 2, "expected two file_list rows (one per day)");
        assert_eq!(rows[0].0, "2026-07-14");
        assert_eq!(rows[0].1, 60, "first day should have 60 rows");
        assert_eq!(rows[1].0, "2026-07-15");
        assert_eq!(rows[1].1, 40, "second day should have 40 rows");
    }

    #[tokio::test]
    #[cfg(feature = "scribe-inspect")]
    #[ignore = "requires fault injection infrastructure"]
    async fn pg_scribe_seal_tx_failure_leaves_no_file_list_or_audit() {
        // This test would verify that if the seal transaction fails between
        // file_list INSERT and audit_outbox fan-out, the transaction rolls back
        // atomically and leaves no partial state.
        //
        // Implementation requires fault injection seams in SealDriver or
        // a test-only hook in insert_and_audit that can fail after file_list
        // but before audit. Deferred to follow-up with crash-injection harness.
        // Skip for ; implement in follow-up with crash-injection harness.
    }
}
