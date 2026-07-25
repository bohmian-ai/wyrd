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
    use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
    use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
    use vala_bifrost_redux::scribe::ScribeImpl;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;

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
        let mut node_id_bytes = *Uuid::now_v7().as_bytes();
        // The current seal filename seam accepts a PodId string while
        // file_list stores the same value as UUID; use a UUID whose first
        // hexadecimal character also satisfies the PodId grammar.
        node_id_bytes[0] = 0xa0 | (node_id_bytes[0] & 0x0f);
        let node_id = Uuid::from_bytes(node_id_bytes);
        let wal = Arc::new(
            vala_bifrost_redux::scribe::wal::WalWriter::new(
                temp_dir.path(),
                node_id_bytes,
                1,
                vala_bifrost_redux::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );

        // Leak temp_dir to keep WAL files for test lifetime
        std::mem::forget(temp_dir);

        let writer_epoch = 1;
        let scribe = ScribeImpl::new_for_embedded_with_deps(
            operator,
            wal,
            node_id.to_string(),
            writer_epoch,
        );

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

    fn principal_for_tenant(tenant: DataTenantId) -> Principal {
        Principal {
            id: PrincipalId::new(Uuid::now_v7()),
            kind: PrincipalKind::User,
            tenant_id: tenant,
            roles: vec![],
            effective_permissions: PermissionSet::new(),
        }
    }

    fn schema_fingerprint(batch: &RecordBatch) -> SchemaFingerprint {
        SchemaFingerprint::from_arrow_schema(batch.schema().as_ref())
    }

    fn events_table() -> TableRef {
        TableRef::new(BifrostNamespace::Bifrost, "events")
    }

    #[tokio::test]
    async fn pg_scribe_append_seal_file_list() {
        let (fixture, tenant, scribe) = setup().await;
        let binding = TenantTableBinding::resolve((tenant, events_table())).expect("binding");

        // 1. Append 50k rows to trigger seal predicate
        let base_time = DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
            .unwrap()
            .timestamp_micros();
        let batch = make_batch(50_000, base_time);
        let principal = principal_for_tenant(tenant);
        let fingerprint = schema_fingerprint(&batch);

        let req = ScribeAppend {
            principal,
            table: events_table(),
            rows: batch,
            schema_fingerprint: fingerprint,
            request_id: RequestId::now_v7(),
            batch_id: uuid::Uuid::now_v7(),
            measured_wire_bytes: 0,
        };

        scribe.append(req).await.expect("append");

        // 2. Force seal
        let pool = fixture.app_pool();
        let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn");
        let post_commit = scribe.force_seal(&mut conn).await.expect("force_seal");
        conn.commit().await.expect("commit");
        scribe
            .complete_post_commit(post_commit)
            .expect("post_commit");

        // 3. Verify file_list row and its organization-qualified object identity
        let mut conn2 = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn2");
        let tx = conn2.transaction();

        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            Uuid,                  // id
            Uuid,                  // data_tenant_id
            String,                // namespace
            String,                // table_name
            String,                // file_path
            i64,                   // row_count
            i64,                   // file_size
            String,                // partition_day
            i64,                   // wal_lsn_min
            i64,                   // wal_lsn_max
            Uuid,                  // node_id
            i64,                   // writer_epoch
            DateTime<chrono::Utc>, // min_event_time
            DateTime<chrono::Utc>, // max_event_time
        )> = sqlx::query_as(
            r"
            SELECT id, data_tenant_id, namespace, table_name, file_path, row_count, file_size,
                   partition_day::text, wal_lsn_min, wal_lsn_max,
                   node_id, writer_epoch, min_event_time, max_event_time
            FROM vala.file_list
            WHERE namespace = 'vala.bifrost' AND table_name = 'events'
            ",
        )
        .fetch_all(&mut **tx)
        .await
        .expect("file_list query");

        assert_eq!(rows.len(), 1, "expected exactly one file_list row");
        let (
            id,
            data_tenant_id,
            namespace,
            table_name,
            file_path,
            row_count,
            file_size,
            partition_day,
            wal_lsn_min,
            wal_lsn_max,
            node_id,
            writer_epoch,
            min_event_time,
            max_event_time,
        ) = &rows[0];

        assert_ne!(*id, Uuid::nil(), "id should be non-nil UUID");
        assert_eq!(*data_tenant_id, tenant.as_uuid());
        assert_eq!(namespace, BifrostNamespace::Bifrost.as_str());
        assert_eq!(table_name, "events");
        assert!(
            file_path.starts_with(&binding.object_prefix),
            "file_path should start with the tenant-qualified object prefix"
        );
        assert!(
            file_path.ends_with(".parquet"),
            "file_path should end with .parquet"
        );
        assert_eq!(*row_count, 50_000);
        assert!(*file_size > 0, "file_size should be positive");
        assert_eq!(partition_day, "2026-07-14");
        assert!(*wal_lsn_min >= 0, "wal_lsn_min should be non-negative");
        assert!(*wal_lsn_max >= 0, "wal_lsn_max should be non-negative");
        assert!(*wal_lsn_min <= *wal_lsn_max, "LSN range should be valid");
        assert_ne!(*node_id, Uuid::nil(), "node_id should be non-nil");
        assert_eq!(*writer_epoch, 1);
        assert!(
            *min_event_time <= *max_event_time,
            "event time range should be valid"
        );
    }

    #[tokio::test]
    async fn pg_scribe_seal_emits_one_audit_row_per_append() {
        let (fixture, tenant, scribe) = setup().await;
        let base_time = DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
            .unwrap()
            .timestamp_micros();
        for i in 0..3 {
            let batch = make_batch(1000, base_time + (i * 1_000_000));
            let mut principal = principal_for_tenant(tenant);
            principal.id = PrincipalId::new(Uuid::now_v7());
            let fingerprint = schema_fingerprint(&batch);
            let req = ScribeAppend {
                principal,
                table: events_table(),
                rows: batch,
                schema_fingerprint: fingerprint,
                request_id: RequestId::now_v7(),
                batch_id: uuid::Uuid::now_v7(),
                measured_wire_bytes: 0,
            };
            scribe.append(req).await.expect("append");
        }
        let pool = fixture.app_pool();
        let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn");
        let post_commit = scribe.force_seal(&mut conn).await.expect("force_seal");
        conn.commit().await.expect("commit");
        scribe
            .complete_post_commit(post_commit)
            .expect("post_commit");
        let mut conn2 = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn2");
        let tx = conn2.transaction();
        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            Uuid,           // data_tenant_id
            String,         // request_id
            Option<String>, // trace_id
            String,         // operation
            String,         // resource
            Option<String>, // card_ref (JSON)
            Uuid,           // principal_id
            String,         // principal_kind
            String,         // auth_method
            String,         // permission
            String,         // decision
            String,         // result
            String,         // payload_summary
            Option<String>, // detail
        )> = sqlx::query_as(
            r"
            SELECT data_tenant_id, request_id, trace_id, operation, resource, card_ref,
                   principal_id, principal_kind, auth_method, permission,
                   decision, result, payload_summary, detail
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
                data_tenant_id,
                request_id,
                _trace_id,
                operation,
                _resource,
                card_ref,
                principal_id,
                principal_kind,
                auth_method,
                permission,
                decision,
                result,
                payload_summary,
                detail,
            ) = row;

            assert_eq!(*data_tenant_id, tenant.as_uuid());
            assert!(!request_id.is_empty(), "request_id {i} should be non-empty");
            assert_eq!(operation, "bifrost.append");
            assert!(card_ref.is_none(), "card_ref should be None for this test");
            assert_ne!(
                *principal_id,
                Uuid::nil(),
                "principal_id {i} should be non-nil"
            );
            assert_eq!(principal_kind, "user");
            assert_eq!(auth_method, "jwt");
            assert!(!permission.is_empty(), "permission should be non-empty");
            assert_eq!(decision, "allow", "decision should be allow");
            assert_eq!(result, "success");
            assert!(
                !payload_summary.is_empty(),
                "payload_summary should be non-empty"
            );
            assert!(detail.is_none(), "detail should be None for this test");
        }
    }

    #[tokio::test]
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

        let principal = principal_for_tenant(tenant);
        let fingerprint = schema_fingerprint(&batch);

        let req = ScribeAppend {
            principal,
            table: events_table(),
            rows: batch,
            schema_fingerprint: fingerprint,
            request_id: RequestId::now_v7(),
            batch_id: uuid::Uuid::now_v7(),
            measured_wire_bytes: 0,
        };

        scribe.append(req).await.expect("append");

        // Force seal
        let vala = vala_sql::ValaPostgres::from_pool(fixture.app_pool().clone());
        let pool = vala.pool();
        let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn");
        let post_commit = scribe.force_seal(&mut conn).await.expect("force_seal");
        conn.commit().await.expect("commit");
        scribe
            .complete_post_commit(post_commit)
            .expect("post_commit");

        // Verify two file_list rows with distinct partition_day
        let mut conn2 = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn2");
        let tx = conn2.transaction();
        let rows: Vec<(String, i64)> = sqlx::query_as(
            r"
            SELECT partition_day::text, row_count
            FROM vala.file_list
            WHERE namespace = 'vala.bifrost' AND table_name = 'events'
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
    async fn pg_scribe_seal_tx_failure_leaves_no_file_list_or_audit() {
        let (fixture, tenant, scribe) = setup().await;
        let batch = make_batch(
            2,
            DateTime::parse_from_rfc3339("2026-07-14T12:00:00Z")
                .expect("time")
                .timestamp_micros(),
        );
        scribe
            .append(ScribeAppend {
                principal: principal_for_tenant(tenant),
                table: events_table(),
                schema_fingerprint: schema_fingerprint(&batch),
                request_id: RequestId::now_v7(),
                batch_id: uuid::Uuid::now_v7(),
                measured_wire_bytes: 0,
                rows: batch,
            })
            .await
            .expect("append");

        let pool = fixture.app_pool();
        let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant connection");
        let post_commit = scribe.force_seal(&mut conn).await.expect("force seal");
        drop(conn);
        scribe.abort_post_commit(post_commit).expect("abort seal");

        let mut verify = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("verification connection");
        let tx = verify.transaction();
        let file_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id = $1 AND table_name = $2",
        )
        .bind(tenant.as_uuid())
        .bind("events")
        .fetch_one(&mut **tx)
        .await
        .expect("file count");
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND resource LIKE $2",
        )
        .bind(tenant.as_uuid())
        .bind("%events%")
        .fetch_one(&mut **tx)
        .await
        .expect("audit count");
        assert_eq!(file_count, 0);
        assert_eq!(audit_count, 0);
    }
}
