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
    use chrono::{DateTime, Utc};
    use opendal::services::Memory;
    use sha2::{Digest, Sha256};
    use sqlx::types::Uuid;
    use std::sync::Arc;
    use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
    use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
    use vala_bifrost_redux::scribe::persistence::PersistenceFaults;
    use vala_bifrost_redux::scribe::seal::ScribeCommitAttempt;
    use vala_bifrost_redux::scribe::{ScribeImpl, ScribePublicationEvent};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;

    /// Starts the roomy production seal fixture used by non-capacity journeys.
    async fn setup() -> (PgFixture, DataTenantId, ScribeImpl, Arc<opendal::Operator>) {
        setup_with_faults_at_memory_limit(PersistenceFaults::default(), 1152 * 1024 * 1024).await
    }

    /// Starts the production seal fixture with caller-selected persistence faults.
    async fn setup_with_faults(
        faults: PersistenceFaults,
    ) -> (PgFixture, DataTenantId, ScribeImpl, Arc<opendal::Operator>) {
        setup_with_faults_at_memory_limit(faults, 1152 * 1024 * 1024).await
    }

    /// Starts the production seal fixture with explicit faults and memory capacity.
    async fn setup_with_faults_at_memory_limit(
        faults: PersistenceFaults,
        memory_limit_bytes: usize,
    ) -> (PgFixture, DataTenantId, ScribeImpl, Arc<opendal::Operator>) {
        setup_with_faults_at_memory_limit_and_identity(faults, memory_limit_bytes, 1, None).await
    }

    /// Starts a direct-seal fixture with a caller-selected durable stream identity.
    async fn setup_with_faults_at_memory_limit_and_identity(
        faults: PersistenceFaults,
        memory_limit_bytes: usize,
        writer_epoch: i64,
        node_id: Option<Uuid>,
    ) -> (PgFixture, DataTenantId, ScribeImpl, Arc<opendal::Operator>) {
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
        let scratch_dir = tempfile::tempdir().expect("temp scratch dir");
        let scribe_output = scratch_dir.path().join("scribe-output");
        let forge_scratch = scratch_dir.path().join("forge");
        let oracle_scratch = scratch_dir.path().join("oracle");
        for root in [&scribe_output, &forge_scratch, &oracle_scratch] {
            std::fs::create_dir(root).expect("test volume root");
        }
        let mut node_id_bytes = *node_id.unwrap_or_else(Uuid::now_v7).as_bytes();
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

        let runtime_resources =
            vala_bifrost_redux::resources::BifrostRuntimeResources::from_snapshot(
                vala_bifrost_redux::resources::SystemResourceSnapshot {
                    memory_limit_bytes,
                    effective_cpu: 4,
                    scratch_capacity_bytes: 1024 * 1024 * 1024,
                    scratch_available_bytes: 1024 * 1024 * 1024,
                    memory_source: vala_bifrost_redux::resources::ResourceSource::Injected,
                    cpu_source: vala_bifrost_redux::resources::ResourceSource::Injected,
                },
                vala_bifrost_redux::resources::BifrostResourcePolicy {
                    roles: std::collections::BTreeSet::from([
                        vala_bifrost_redux::resources::BifrostRole::Scribe,
                    ]),
                    memory_limit_bytes: None,
                    unmanaged_reserve_bytes: None,
                    scratch_limit_bytes: None,
                    effective_cpu: None,
                    scratch_root: scratch_dir.path().to_owned(),
                    volume_roots: Some(vala_bifrost_redux::resources::BifrostVolumeRoots {
                        wal: temp_dir.path().to_owned(),
                        scribe_output_scratch: scribe_output,
                        forge_scratch,
                        oracle_scratch,
                    }),
                },
            )
            .expect("test Bifrost resources");
        let resources = runtime_resources
            .compose_roles()
            .expect("test role resources");
        let scribe_resources = resources.scribe().expect("test Scribe resources");
        let (_, output_scratch) = scribe_resources
            .volume_capabilities()
            .expect("test Scribe volumes");

        // Leak directories to keep WAL and scratch files for test lifetime.
        std::mem::forget(temp_dir);
        std::mem::forget(scratch_dir);

        let pool = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query(
            "INSERT INTO vala.cluster_nodes (data_tenant_id,node_id,role,advertise_addr,fencing_token,started_at,heartbeat_at) VALUES ($1,$2,'scribe','127.0.0.1:1',$3,now(),now()) ON CONFLICT (data_tenant_id,node_id,role) DO UPDATE SET fencing_token=EXCLUDED.fencing_token,heartbeat_at=now()",
        )
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .bind(node_id)
        .bind(writer_epoch)
        .execute(&pool)
        .await
        .expect("register Scribe publication fence");
        let scribe = ScribeImpl::new_for_embedded_with_runtime_config_and_admission_and_memory(
            Arc::clone(&operator),
            wal,
            &node_id.to_string(),
            writer_epoch,
            vala_bifrost_redux::scribe::ScribeEmbeddedConfig {
                lane_config: vala_bifrost_redux::scribe::ScribeLaneConfig::default(),
                admission: vala_bifrost_redux::scribe::admission::AdmissionConfig::default(),
                coordination_runtime: tokio::runtime::Handle::current(),
                persistence: Some(
                    vala_bifrost_redux::scribe::ScribePersistenceConfig::new(
                        Arc::new(fixture.vala_postgres().clone()),
                        16,
                        1,
                    )
                    .with_operator_pool(fixture.operator_pool().clone())
                    .with_output_scratch(output_scratch)
                    .with_test_faults(faults),
                ),
                memory_budget: Some(scribe_resources.memory_governor().scribe_budget()),
                staging_file_publisher: None,
            },
        );

        (fixture, tenant, scribe, operator)
    }

    async fn complete_post_commit(
        scribe: &ScribeImpl,
        batch: Vec<ScribeCommitAttempt>,
        commit_result: &Result<(), vala_sql::SqlError>,
    ) {
        scribe
            .settle_commit_attempts(batch, commit_result)
            .await
            .expect("post_commit");
        assert!(commit_result.is_ok(), "fixture transaction must commit");
    }

    fn make_batch(row_count: usize, base_time_micros: i64) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
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
                Arc::new(TimestampMicrosecondArray::from(timestamps).with_timezone("UTC")),
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

    /// Publishes local seal zero for one selected writer epoch and returns its identity.
    async fn publish_direct_epoch(
        node_id: Uuid,
        writer_epoch: i64,
    ) -> (String, i64, String, Vec<u8>) {
        let (fixture, tenant, scribe, operator) = setup_with_faults_at_memory_limit_and_identity(
            PersistenceFaults::default(),
            1152 * 1024 * 1024,
            writer_epoch,
            Some(node_id),
        )
        .await;
        let day = Utc::now().date_naive();
        let batch = make_batch(
            32,
            day.and_hms_opt(12, 0, 0)
                .expect("current day accepts noon")
                .and_utc()
                .timestamp_micros(),
        );
        scribe
            .append(ScribeAppend {
                principal: principal_for_tenant(tenant),
                table: events_table(),
                schema_fingerprint: schema_fingerprint(&batch),
                rows: batch,
                request_id: RequestId::now_v7(),
                batch_id: Uuid::from_u128(1),
                measured_wire_bytes: 0,
            })
            .await
            .expect("direct epoch append");
        let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        let attempts = scribe
            .force_seal(&mut conn)
            .await
            .expect("direct epoch seal");
        let commit = conn.commit().await;
        complete_post_commit(&scribe, attempts, &commit).await;
        let row: (String, i64, String) = sqlx::query_as(
            "SELECT file_path,file_size,file_checksum FROM vala.file_list WHERE data_tenant_id=$1 AND table_name='events'",
        )
        .bind(tenant.as_uuid())
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("direct epoch catalog row");
        let bytes = operator
            .read(&row.0)
            .await
            .expect("direct epoch object")
            .to_bytes()
            .to_vec();
        (row.0, row.1, row.2, bytes)
    }

    /// Direct sealing reuses local seal zero without overwriting another epoch.
    #[tokio::test]
    async fn direct_seal_driver_artifact_identity_is_cross_epoch_durable() {
        let node_id = Uuid::now_v7();
        let first = publish_direct_epoch(node_id, 1).await;
        let second = publish_direct_epoch(node_id, 2).await;
        assert_ne!(
            first.0.rsplit('/').next(),
            second.0.rsplit('/').next(),
            "writer epoch must distinguish the durable artifact component"
        );
        for (path, size, checksum, bytes) in [&first, &second] {
            assert_eq!(
                i64::try_from(bytes.len()).expect("object size fits i64"),
                *size,
                "catalog length matches {path}"
            );
            assert_eq!(hex::encode(Sha256::digest(bytes)), *checksum);
        }
    }

    /// A lost COMMIT response transfers ownership and reconciles without duplication.
    #[tokio::test]
    async fn caller_commit_error_reconciles_exact_set_and_retires_once() {
        let faults = PersistenceFaults::default();
        faults.fail_next_post_commit_response();
        let (fixture, tenant, scribe, operator) = setup_with_faults(faults).await;
        let day = Utc::now().date_naive();
        let batch = make_batch(
            32,
            day.and_hms_opt(12, 0, 0)
                .expect("current day accepts noon")
                .and_utc()
                .timestamp_micros(),
        );
        scribe
            .append(ScribeAppend {
                principal: principal_for_tenant(tenant),
                table: events_table(),
                schema_fingerprint: schema_fingerprint(&batch),
                rows: batch,
                request_id: RequestId::now_v7(),
                batch_id: Uuid::now_v7(),
                measured_wire_bytes: 0,
            })
            .await
            .expect("append before ambiguous seal");
        let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        let attempts = scribe.force_seal(&mut conn).await.expect("force seal");
        conn.commit().await.expect("server committed transaction");
        let client_error = Err(vala_sql::SqlError::InvariantViolation {
            detail: "test client lost COMMIT response".to_owned(),
        });
        scribe
            .settle_commit_attempts(attempts, &client_error)
            .await
            .expect("runtime-owned reconciliation");

        let file_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND table_name='events'",
        )
        .bind(tenant.as_uuid())
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("file-list count");
        let audit_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1")
                .bind(tenant.as_uuid())
                .fetch_one(&fixture.superuser_pool().await.expect("superuser pool"))
                .await
                .expect("audit count");
        assert_eq!(file_count, 1);
        assert_eq!(audit_count, 1);
        let objects = operator
            .list(&format!("tenants/{tenant}/"))
            .await
            .expect("list exact tenant objects");
        assert_eq!(objects.len(), 1);
    }

    /// Cancelling the caller after COMMIT polling leaves reconciliation owned.
    #[tokio::test]
    async fn cancelled_commit_settlement_continues_under_runtime_owner() {
        let faults = PersistenceFaults::default();
        faults.fail_next_post_commit_response();
        let (fixture, tenant, scribe, _operator) = setup_with_faults(faults).await;
        let scribe = Arc::new(scribe);
        let day = Utc::now().date_naive();
        let batch = make_batch(
            32,
            day.and_hms_opt(12, 0, 0)
                .expect("current day accepts noon")
                .and_utc()
                .timestamp_micros(),
        );
        scribe
            .append(ScribeAppend {
                principal: principal_for_tenant(tenant),
                table: events_table(),
                schema_fingerprint: schema_fingerprint(&batch),
                rows: batch,
                request_id: RequestId::now_v7(),
                batch_id: Uuid::now_v7(),
                measured_wire_bytes: 0,
            })
            .await
            .expect("append before cancellation");
        let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        let attempts = scribe.force_seal(&mut conn).await.expect("force seal");
        conn.commit().await.expect("server commit");
        let observer = scribe.publication_observer_for_test();
        let settlement_owner = Arc::clone(&scribe);
        let settlement = tokio::spawn(async move {
            settlement_owner
                .settle_commit_attempts(
                    attempts,
                    &Err(vala_sql::SqlError::InvariantViolation {
                        detail: "test client lost COMMIT response".to_owned(),
                    }),
                )
                .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        settlement.abort();
        let published = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            observer.wait_for(|event| matches!(event, ScribePublicationEvent::Published { .. })),
        )
        .await
        .expect("runtime reconciliation survives caller cancellation");
        assert!(matches!(
            published,
            ScribePublicationEvent::Published { .. }
        ));
        let audit_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1")
                .bind(tenant.as_uuid())
                .fetch_one(&fixture.superuser_pool().await.expect("superuser pool"))
                .await
                .expect("audit count");
        assert_eq!(audit_count, 1);
    }

    /// Proves the caller-owned seal driver emits writer-v2 at the 832 MiB floor.
    #[tokio::test]
    async fn scribe_seal_driver_writer_v2_is_bounded_at_exact_floor() {
        let (fixture, tenant, scribe, operator) =
            setup_with_faults_at_memory_limit(PersistenceFaults::default(), 832 * 1024 * 1024)
                .await;
        let binding = TenantTableBinding::resolve((tenant, events_table())).expect("binding");
        // 1. Append 50k rows to trigger seal predicate
        let expected_day = Utc::now().date_naive();
        let base_time = expected_day
            .and_hms_opt(12, 0, 0)
            .expect("current day accepts noon")
            .and_utc()
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
        assert_eq!(
            scribe.memory_snapshot().scribe_total_bytes,
            (256 - 8) * 1024 * 1024,
            "the complete owner releases its exact footer child after inspection"
        );
        let commit_result = conn.commit().await;
        complete_post_commit(&scribe, post_commit, &commit_result).await;
        assert_eq!(
            scribe.memory_snapshot().categories
                [vala_bifrost_redux::scribe::memory::MemoryCategory::Persistence as usize],
            0,
            "committed settlement releases the complete producer delta"
        );

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
            i16,                   // file_ordinal
            Option<String>,        // file_checksum
        )> = sqlx::query_as(
            r"
            SELECT id, data_tenant_id, namespace, table_name, file_path, row_count, file_size,
                   partition_day::text, wal_lsn_min, wal_lsn_max,
                   node_id, writer_epoch, min_event_time, max_event_time,
                   file_ordinal, file_checksum
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
            file_ordinal,
            file_checksum,
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
        assert!(file_path.contains("-epoch-1-shard-"));
        assert!(file_path.contains("-wal-"));
        assert_eq!(*row_count, 50_000);
        assert!(*file_size > 0, "file_size should be positive");
        assert_eq!(partition_day, &expected_day.to_string());
        assert!(*wal_lsn_min >= 0, "wal_lsn_min should be non-negative");
        assert!(*wal_lsn_max >= 0, "wal_lsn_max should be non-negative");
        assert!(*wal_lsn_min <= *wal_lsn_max, "LSN range should be valid");
        assert_ne!(*node_id, Uuid::nil(), "node_id should be non-nil");
        assert_eq!(*writer_epoch, 1);
        assert_eq!(*file_ordinal, 0);
        let checksum = file_checksum.as_deref().expect("writer-v2 checksum");
        assert_eq!(checksum.len(), 64);
        assert!(checksum.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(
            *min_event_time <= *max_event_time,
            "event time range should be valid"
        );
        let bytes = operator
            .read(file_path)
            .await
            .expect("read sealed writer-v2 object")
            .to_bytes();
        assert_eq!(
            u64::try_from(bytes.len()).expect("object length fits u64"),
            u64::try_from(*file_size).expect("positive file size fits u64")
        );
        assert_eq!(hex::encode(Sha256::digest(&bytes)), checksum);
        let metadata = parquet::file::metadata::ParquetMetaDataReader::new()
            .parse_and_finish(&bytes)
            .expect("standard Parquet decoder accepts writer-v2 output");
        let decoded_schema =
            parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(bytes.clone())
                .expect("standard Arrow decoder accepts writer-v2 output")
                .schema()
                .clone();
        vala_bifrost_redux::parquet::BifrostParquetMemoryEnvelope::from_footer(
            metadata.file_metadata(),
            decoded_schema.as_ref(),
            file_path,
        )
        .expect("all nine writer-v2 fields round-trip");
        vala_bifrost_redux::parquet::memory::validate_writer_v2_structure(&metadata)
            .expect("sealed output respects all structural caps");
        let trailer = bytes
            .get(bytes.len().saturating_sub(8)..bytes.len().saturating_sub(4))
            .expect("Parquet trailer");
        let footer_bytes = u64::from(u32::from_le_bytes(
            trailer.try_into().expect("four-byte footer length"),
        ));
        assert!(footer_bytes <= 8 * 1024 * 1024);
    }

    #[tokio::test]
    async fn pg_scribe_seal_emits_one_audit_row_per_append() {
        let (fixture, tenant, scribe, _operator) = setup().await;
        let base_time = Utc::now().timestamp_micros();
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
        let commit_result = conn.commit().await;
        complete_post_commit(&scribe, post_commit, &commit_result).await;
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
        let (fixture, tenant, scribe, operator) = setup().await;

        let day1 = Utc::now().date_naive();
        let day2 = day1.succ_opt().expect("current date has a successor");
        let day1_time = day1
            .and_hms_opt(23, 59, 50)
            .expect("current day accepts boundary time")
            .and_utc()
            .timestamp_micros();
        let day2_time = day2
            .and_hms_opt(0, 0, 10)
            .expect("next day accepts boundary time")
            .and_utc()
            .timestamp_micros();

        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
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
                Arc::new(TimestampMicrosecondArray::from(timestamps).with_timezone("UTC")),
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
        let commit_result = conn.commit().await;
        complete_post_commit(&scribe, post_commit, &commit_result).await;

        // Verify two file_list rows with distinct partition_day
        let mut conn2 = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .expect("tenant conn2");
        let tx = conn2.transaction();
        let rows: Vec<(String, i64, String)> = sqlx::query_as(
            r"
            SELECT partition_day::text, row_count, file_path
            FROM vala.file_list
            WHERE namespace = 'vala.bifrost' AND table_name = 'events'
            ORDER BY partition_day
            ",
        )
        .fetch_all(&mut **tx)
        .await
        .expect("file_list query");

        assert_eq!(rows.len(), 2, "expected two file_list rows (one per day)");
        assert_eq!(rows[0].0, day1.to_string());
        assert_eq!(rows[0].1, 60, "first day should have 60 rows");
        assert_eq!(rows[0].2.matches("day=").count(), 1);
        assert!(rows[0].2.contains(&format!("day={day1}/")));
        assert_eq!(rows[1].0, day2.to_string());
        assert_eq!(rows[1].1, 40, "second day should have 40 rows");
        assert_eq!(rows[1].2.matches("day=").count(), 1);
        assert!(rows[1].2.contains(&format!("day={day2}/")));
        let objects = operator
            .list_with("")
            .recursive(true)
            .await
            .expect("object list")
            .into_iter()
            .map(|entry| entry.path().to_owned())
            .collect::<Vec<_>>();
        assert!(rows.iter().all(|row| objects.contains(&row.2)));
    }

    #[tokio::test]
    async fn pg_scribe_seal_tx_failure_leaves_no_file_list_or_audit() {
        let (fixture, tenant, scribe, _operator) = setup().await;
        let batch = make_batch(2, Utc::now().timestamp_micros());
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
        scribe
            .abort_commit_attempts(post_commit)
            .await
            .expect("abort seal");

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
