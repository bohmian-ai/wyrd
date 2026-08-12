mod pg_tests {
    //! Postgres-backed contract tests for organization-qualified file-list identity.

    use chrono::{DateTime, NaiveDate, Utc};
    use opendal::services::Memory;
    use secrecy::ExposeSecret;
    use sqlx::types::Uuid;
    use std::sync::Arc;
    use vala_bifrost_redux::catalog::tenant_table::TenantTableBindingError;
    use vala_bifrost_redux::catalog::{BifrostCatalog, BifrostCatalogError, CreateTableRequest};
    use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
    use vala_bifrost_redux::cluster::ClusterRegistry;
    use vala_bifrost_redux::namespaces::BifrostNamespace;
    use vala_bifrost_redux::scribe::ScribeImpl;
    use vala_bifrost_redux::scribe::file_list_writer::{
        FileListInsert, FileListInsertOutcome, PublicationFenceBarrier, insert_and_audit,
        insert_and_audit_fenced, insert_and_audit_fenced_with_barrier,
    };
    use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
    use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
    use vala_sql::queries::file_list::{HotFileCatalog, HotFileCut};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};
    use wyrd_spec::vala::api::{NodeId as ClusterNodeId, ScribeCapabilitiesV1};
    use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};

    fn logical_table() -> TableRef {
        TableRef::new(BifrostNamespace::Traces, "spans")
    }

    async fn setup() -> (PgFixture, DataTenantId, DataTenantId) {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant_a,
                &format!("tenant-a-{}", tenant_a.as_uuid().simple()),
            )
            .await
            .expect("tenant A");
        fixture
            .seed_additional_tenant_with_uuid(
                tenant_b,
                &format!("tenant-b-{}", tenant_b.as_uuid().simple()),
            )
            .await
            .expect("tenant B");
        (fixture, tenant_a, tenant_b)
    }

    async fn register_scribe(fixture: &PgFixture, node: NodeId, epoch: i64) {
        let pool = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query(
            "INSERT INTO vala.cluster_nodes (data_tenant_id,node_id,role,advertise_addr,fencing_token,started_at,heartbeat_at) VALUES ($1,$2,'scribe','127.0.0.1:1',$3,now(),now()) ON CONFLICT (data_tenant_id,node_id,role) DO UPDATE SET fencing_token=EXCLUDED.fencing_token,heartbeat_at=now()",
        )
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .bind(node.as_uuid())
        .bind(epoch)
        .execute(&pool)
        .await
        .expect("register Scribe fence");
    }

    async fn redux_catalog(
        fixture: &PgFixture,
    ) -> (tempfile::TempDir, Arc<StorageHandle>, BifrostCatalog) {
        let warehouse = tempfile::tempdir().expect("warehouse");
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: warehouse.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: std::time::Duration::from_mins(10),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await
        .expect("storage");
        let catalog = BifrostCatalog::new(
            fixture.catalog_dsn().expose_secret(),
            storage.backend_config(),
            fixture.vala_postgres().clone(),
        )
        .await
        .expect("Redux catalog");
        (warehouse, storage, catalog)
    }

    fn catalog_request(table: TableRef, tenant: DataTenantId) -> CreateTableRequest {
        CreateTableRequest {
            table,
            user_fields: vec![arrow::datatypes::Field::new(
                "value",
                arrow::datatypes::DataType::Int64,
                false,
            )],
            tenant,
            audit: None,
        }
    }

    fn insert_row(
        binding: &TenantTableBinding,
        node_id: Uuid,
        writer_epoch: i64,
        wal_lsn_min: i64,
        wal_lsn_max: i64,
    ) -> FileListInsert<'_> {
        let event_time = DateTime::<Utc>::from_timestamp(0, 0).expect("epoch");
        FileListInsert {
            id: Uuid::now_v7(),
            data_tenant_id: binding.tenant,
            namespace: &binding.logical_namespace,
            table_name: &binding.table_name,
            file_path: &binding.object_prefix,
            file_size: 128,
            row_count: 1,
            min_event_time: event_time,
            max_event_time: event_time,
            partition_day: NaiveDate::from_ymd_opt(1970, 1, 1).expect("date"),
            node_id,
            writer_epoch,
            wal_lsn_min,
            wal_lsn_max,
        }
    }

    fn audit_event() -> AuditEvent {
        AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "test.file_list_identity".to_owned(),
            resource: "vala.traces.spans".to_owned(),
            card_ref: None,
            principal_id: PrincipalId::new(Uuid::now_v7()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Internal,
            permission: "test:file_list".to_owned(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "one file".to_owned(),
            detail: None,
        }
    }

    async fn index_shape(pool: &sqlx::PgPool, index_name: &str) -> (Vec<String>, Option<String>) {
        sqlx::query_as(
            r"
            SELECT ARRAY(
                       SELECT a.attname
                         FROM pg_index i2
                         CROSS JOIN LATERAL unnest(i2.indkey) WITH ORDINALITY AS key(attnum, ordinality)
                         JOIN pg_attribute a
                           ON a.attrelid = i2.indrelid
                          AND a.attnum = key.attnum
                        WHERE i2.indexrelid = i.indexrelid
                        ORDER BY key.ordinality
                   ),
                   pg_get_expr(i.indpred, i.indrelid)
              FROM pg_class c
              JOIN pg_index i ON i.indexrelid = c.oid
              JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'vala' AND c.relname = $1
            ",
        )
        .bind(index_name)
        .fetch_one(pool)
        .await
        .expect("index shape")
    }

    /// Reads one committed cut through the production tenant-bound catalog owner.
    async fn read_hot_cut(
        fixture: &PgFixture,
        tenant: DataTenantId,
        catalog: &HotFileCatalog,
        pinned_paths: &std::collections::BTreeSet<String>,
        operation_id: Option<Uuid>,
    ) -> HotFileCut {
        let mut conn = fixture
            .vala_postgres()
            .tenant_conn(tenant)
            .await
            .expect("tenant connection");
        let cut = catalog
            .unresolved_for_cut(&mut conn, pinned_paths, operation_id)
            .await
            .expect("cut-aware manifest");
        conn.commit().await.expect("commit manifest read");
        cut
    }

    #[tokio::test]
    async fn test_migration_matches_tenant_table_contract() {
        let (fixture, _, _) = setup().await;
        let pool = fixture.superuser_pool().await.expect("superuser pool");

        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT column_name FROM information_schema.columns WHERE table_schema = 'vala' AND table_name = 'file_list'",
        )
        .fetch_all(&pool)
        .await
        .expect("file_list columns");
        assert!(!columns.iter().any(|column| column == "scope"));
        let tenant_nullable: String = sqlx::query_scalar(
            "SELECT is_nullable FROM information_schema.columns WHERE table_schema = 'vala' AND table_name = 'file_list' AND column_name = 'data_tenant_id'",
        )
        .fetch_one(&pool)
        .await
        .expect("data_tenant_id nullability");
        assert_eq!(tenant_nullable, "NO");

        let (group_columns, group_predicate) = index_shape(&pool, "file_list_group_idx").await;
        assert_eq!(
            group_columns,
            ["data_tenant_id", "namespace", "table_name", "partition_day"]
        );
        assert!(
            group_predicate
                .as_deref()
                .is_some_and(|predicate| predicate.to_lowercase().contains("not compacted"))
        );

        let (watermark_columns, watermark_predicate) =
            index_shape(&pool, "file_list_live_tail_watermark_idx").await;
        assert_eq!(
            watermark_columns,
            [
                "data_tenant_id",
                "namespace",
                "table_name",
                "node_id",
                "writer_epoch",
                "wal_lsn_max"
            ]
        );
        assert!(watermark_predicate.is_none());

        for index_name in ["file_list_tenant_idx", "file_list_stream_range_uniq"] {
            let exists: (bool,) = sqlx::query_as(
                "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE schemaname = 'vala' AND indexname = $1)",
            )
            .bind(index_name)
            .fetch_one(&pool)
            .await
            .expect("index existence");
            assert!(exists.0, "preserved index {index_name}");
        }
    }

    #[tokio::test]
    async fn test_two_organizations_same_logical_table_are_separate() {
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        let table = logical_table();
        let binding_a = TenantTableBinding::resolve((tenant_a, table.clone())).expect("binding A");
        let binding_b = TenantTableBinding::resolve((tenant_b, table)).expect("binding B");

        assert_eq!(binding_a.table_name, binding_b.table_name);
        assert_eq!(binding_a.logical_namespace, binding_b.logical_namespace);
        assert_ne!(binding_a.iceberg_namespace, binding_b.iceberg_namespace);
        assert_ne!(binding_a.object_prefix, binding_b.object_prefix);
    }

    #[tokio::test]
    async fn test_redux_catalog_registers_same_logical_name_per_tenant() {
        let (fixture, tenant_a, tenant_b) = setup().await;
        let warehouse = tempfile::tempdir().expect("warehouse");
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: warehouse.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: std::time::Duration::from_mins(10),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await
        .expect("storage");
        let catalog = BifrostCatalog::new(
            fixture.catalog_dsn().expose_secret(),
            storage.backend_config(),
            fixture.vala_postgres().clone(),
        )
        .await
        .expect("Redux catalog");
        let table = TableRef::new(BifrostNamespace::Bifrost, "shared_logical_name");
        let user_fields = vec![arrow::datatypes::Field::new(
            "value",
            arrow::datatypes::DataType::Int64,
            false,
        )];

        let uid_a = catalog
            .create_table(CreateTableRequest {
                table: table.clone(),
                user_fields: user_fields.clone(),
                tenant: tenant_a,
                audit: None,
            })
            .await
            .expect("tenant A table");
        let uid_b = catalog
            .create_table(CreateTableRequest {
                table: table.clone(),
                user_fields,
                tenant: tenant_b,
                audit: None,
            })
            .await
            .expect("tenant B table");

        assert_ne!(uid_a, uid_b);
        assert_eq!(
            catalog
                .describe_table(&table, tenant_a)
                .await
                .expect("tenant A description")
                .entry
                .name,
            "shared_logical_name"
        );
        assert_eq!(
            catalog
                .describe_table(&table, tenant_b)
                .await
                .expect("tenant B description")
                .entry
                .name,
            "shared_logical_name"
        );
        let unregistered_tenant = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                unregistered_tenant,
                &format!("unregistered-{}", unregistered_tenant.as_uuid().simple()),
            )
            .await
            .expect("unregistered tenant");
        assert!(matches!(
            catalog
                .table_schema_fingerprint(&table, unregistered_tenant)
                .await,
            Err(BifrostCatalogError::TableNotFound(_))
        ));
    }

    #[tokio::test]
    async fn test_redux_catalog_reconciles_physical_only_state() {
        let (fixture, tenant, _) = setup().await;
        let (_warehouse, _storage, catalog) = redux_catalog(&fixture).await;
        let table = TableRef::new(
            BifrostNamespace::Bifrost,
            format!("physical_only_{}", Uuid::now_v7().simple()),
        );
        let request = catalog_request(table.clone(), tenant);
        let original_uid = catalog.create_table(request).await.expect("initial table");

        let mut conn = fixture
            .vala_postgres()
            .tenant_conn(tenant)
            .await
            .expect("tenant connection");
        vala_sql::queries::olap_catalog::delete_table(&mut conn, &table.fqn())
            .await
            .expect("delete control row");
        conn.commit().await.expect("commit control deletion");

        let binding = TenantTableBinding::resolve((tenant, table.clone())).expect("binding");
        assert!(
            catalog
                .iceberg_catalog()
                .table_exists(&binding.table_ident())
                .await
                .expect("physical table exists")
        );
        let repaired_uid = catalog
            .create_table(catalog_request(table.clone(), tenant))
            .await
            .expect("repair control row");
        assert_ne!(original_uid, repaired_uid);
        catalog
            .table_schema_fingerprint(&table, tenant)
            .await
            .expect("repaired control row");
    }

    #[tokio::test]
    async fn test_redux_catalog_rejects_control_only_state_without_recreation() {
        let (fixture, tenant, _) = setup().await;
        let (_warehouse, _storage, catalog) = redux_catalog(&fixture).await;
        let table = TableRef::new(
            BifrostNamespace::Bifrost,
            format!("control_only_{}", Uuid::now_v7().simple()),
        );
        catalog
            .create_table(catalog_request(table.clone(), tenant))
            .await
            .expect("initial table");

        let binding = TenantTableBinding::resolve((tenant, table.clone())).expect("binding");
        catalog
            .iceberg_catalog()
            .drop_table(&binding.table_ident())
            .await
            .expect("drop physical table");
        assert!(
            !catalog
                .iceberg_catalog()
                .table_exists(&binding.table_ident())
                .await
                .expect("physical table absence")
        );

        let error = catalog
            .create_table(catalog_request(table, tenant))
            .await
            .expect_err("control-only state must fail closed");
        assert!(
            matches!(error, BifrostCatalogError::MetadataMismatch(message) if message.contains("without physical table"))
        );
        assert!(
            !catalog
                .iceberg_catalog()
                .table_exists(&binding.table_ident())
                .await
                .expect("physical table remains absent")
        );
    }

    #[tokio::test]
    async fn test_redux_catalog_three_concurrent_first_creates_reconcile_once() {
        let (fixture, tenant, _) = setup().await;
        let (_warehouse, storage, catalog_a) = redux_catalog(&fixture).await;
        let catalog_b = BifrostCatalog::new(
            fixture.catalog_dsn().expose_secret(),
            storage.backend_config(),
            fixture.vala_postgres().clone(),
        )
        .await
        .expect("second Redux catalog");
        let catalog_c = BifrostCatalog::new(
            fixture.catalog_dsn().expose_secret(),
            storage.backend_config(),
            fixture.vala_postgres().clone(),
        )
        .await
        .expect("third Redux catalog");
        let table = TableRef::new(
            BifrostNamespace::Bifrost,
            format!("concurrent_{}", Uuid::now_v7().simple()),
        );

        let (result_a, result_b, result_c) = tokio::join!(
            catalog_a.create_table(catalog_request(table.clone(), tenant)),
            catalog_b.create_table(catalog_request(table.clone(), tenant)),
            catalog_c.create_table(catalog_request(table.clone(), tenant)),
        );
        let uid_a = result_a.expect("first concurrent registration");
        let uid_b = result_b.expect("second concurrent registration");
        let uid_c = result_c.expect("third concurrent registration");
        assert_eq!(uid_a, uid_b);
        assert_eq!(uid_b, uid_c);

        let mut conn = fixture
            .vala_postgres()
            .tenant_conn(tenant)
            .await
            .expect("tenant connection");
        let rows = vala_sql::queries::olap_catalog::list_tables_for_tenant(&mut conn)
            .await
            .expect("catalog rows");
        conn.commit().await.expect("commit catalog read");
        assert_eq!(rows.iter().filter(|row| row.fqn == table.fqn()).count(), 1);
        let binding = TenantTableBinding::resolve((tenant, table)).expect("binding");
        assert!(
            catalog_a
                .iceberg_catalog()
                .table_exists(&binding.table_ident())
                .await
                .expect("physical table exists")
        );
    }

    #[tokio::test]
    async fn test_file_list_row_uses_exact_tenant_table_identity() {
        let (fixture, tenant_a, _) = setup().await;
        let binding = TenantTableBinding::resolve((tenant_a, logical_table())).expect("binding");
        let node_id = Uuid::now_v7();
        let row = insert_row(&binding, node_id, 1, 100, 100);
        let event = audit_event();
        let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_a)
            .await
            .expect("tenant connection");
        let outcome = insert_and_audit(&mut conn, &row, &[event])
            .await
            .expect("file_list insert");
        conn.commit().await.expect("commit");

        let superuser = fixture.superuser_pool().await.expect("superuser pool");
        let stored: (Uuid, Uuid, String, String, String) = sqlx::query_as(
            "SELECT id, data_tenant_id, namespace, table_name, file_path FROM vala.file_list WHERE id = $1",
        )
        .bind(outcome.id)
        .fetch_one(&superuser)
        .await
        .expect("stored row");

        assert_eq!(stored.0, row.id);
        assert_eq!(stored.1, tenant_a.as_uuid());
        assert_eq!(stored.2, binding.logical_namespace);
        assert_eq!(stored.3, binding.table_name);
        assert!(stored.4.starts_with(&binding.object_prefix));
    }

    /// Oracle sees transition rows so its pinned snapshot performs exact subtraction.
    #[tokio::test]
    async fn oracle_hot_manifest_retains_compacted_transition_rows() {
        let (fixture, tenant, _) = setup().await;
        let binding = TenantTableBinding::resolve((tenant, logical_table())).expect("binding");
        let row = insert_row(&binding, Uuid::now_v7(), 1, 10, 20);
        let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        insert_and_audit(&mut conn, &row, &[audit_event()])
            .await
            .expect("file-list insert");
        conn.commit().await.expect("commit insert");
        let superuser = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query(
            "UPDATE vala.file_list SET compacted = true, committed_snapshot_id = 77 WHERE id = $1",
        )
        .bind(row.id)
        .execute(&superuser)
        .await
        .expect("mark transition row compacted");

        let catalog = HotFileCatalog::new(&binding.logical_namespace, &binding.table_name);
        let mut conn = fixture
            .vala_postgres()
            .tenant_conn(tenant)
            .await
            .expect("tenant connection");
        let rows = catalog
            .unresolved_for_cut(&mut conn, &std::collections::BTreeSet::new(), None)
            .await
            .expect("tenant-scoped Oracle manifest");
        conn.commit().await.expect("commit manifest read");
        let observed = rows
            .sealed_manifest
            .iter()
            .find(|candidate| candidate.id == row.id)
            .expect("transition row remains visible until pinned subtraction");
        assert!(observed.compacted);
        assert_eq!(observed.committed_snapshot_id, Some(77));
    }

    /// Proves every Forge publication phase selects a file through exactly one sealed source.
    #[tokio::test]
    async fn forge_publication_operation_closes_catalog_commit_window() {
        let (fixture, tenant, _) = setup().await;
        let binding = TenantTableBinding::resolve((tenant, logical_table())).expect("binding");
        let row = insert_row(&binding, Uuid::now_v7(), 3, 40, 50);
        let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        insert_and_audit(&mut conn, &row, &[audit_event()])
            .await
            .expect("file-list insert");
        conn.commit().await.expect("commit insert");

        let catalog = HotFileCatalog::new(&binding.logical_namespace, &binding.table_name);
        let operation = Uuid::now_v7();
        let unrelated = Uuid::now_v7();
        let empty_paths = std::collections::BTreeSet::new();
        let initial = read_hot_cut(&fixture, tenant, &catalog, &empty_paths, None).await;
        assert_eq!(
            initial
                .hot_files
                .iter()
                .filter(|file| file.id == row.id)
                .count(),
            1
        );

        let superuser = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::query(
            "UPDATE vala.file_list SET compacted = true, publication_operation_id = $1 WHERE id = $2",
        )
        .bind(operation)
        .bind(row.id)
        .execute(&superuser)
        .await
        .expect("prepare publication");

        let matching =
            read_hot_cut(&fixture, tenant, &catalog, &empty_paths, Some(operation)).await;
        assert!(!matching.hot_files.iter().any(|file| file.id == row.id));
        assert!(
            matching
                .sealed_manifest
                .iter()
                .any(|file| file.id == row.id)
        );
        let unrelated_cut =
            read_hot_cut(&fixture, tenant, &catalog, &empty_paths, Some(unrelated)).await;
        assert_eq!(
            unrelated_cut
                .hot_files
                .iter()
                .filter(|file| file.id == row.id)
                .count(),
            1
        );

        sqlx::query("UPDATE vala.file_list SET committed_snapshot_id = 91 WHERE id = $1")
            .bind(row.id)
            .execute(&superuser)
            .await
            .expect("stamp publication");
        let stamped = read_hot_cut(&fixture, tenant, &catalog, &empty_paths, Some(operation)).await;
        assert!(!stamped.hot_files.iter().any(|file| file.id == row.id));
        assert!(stamped.sealed_manifest.iter().any(|file| file.id == row.id));

        sqlx::query(
            "UPDATE vala.file_list SET compacted = false, committed_snapshot_id = NULL, publication_operation_id = NULL WHERE id = $1",
        )
        .bind(row.id)
        .execute(&superuser)
        .await
        .expect("reset publication");
        let reset = read_hot_cut(&fixture, tenant, &catalog, &empty_paths, Some(operation)).await;
        assert_eq!(
            reset
                .hot_files
                .iter()
                .filter(|file| file.id == row.id)
                .count(),
            1
        );
        assert!(!reset.ambiguous_publication);
    }

    #[tokio::test]
    async fn test_foreign_tenant_write_fails_before_put() {
        let (fixture, tenant_a, tenant_b) = setup().await;
        let operator = Arc::new(
            opendal::Operator::new(Memory::default())
                .expect("memory backend")
                .finish(),
        );
        let temp_dir = tempfile::tempdir().expect("temp WAL dir");
        let node_id = Uuid::now_v7();
        let wal = Arc::new(
            vala_bifrost_redux::scribe::wal::WalWriter::new(
                temp_dir.path(),
                *node_id.as_bytes(),
                1,
                vala_bifrost_redux::scribe::wal::WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        std::mem::forget(temp_dir);
        let scribe =
            ScribeImpl::new_for_embedded_with_deps(operator.clone(), wal, &node_id.to_string(), 1);
        let seal_key = SealKey::new(
            tenant_a,
            logical_table(),
            EventDay::new(NaiveDate::from_ymd_opt(1970, 1, 1).expect("date")),
        );
        let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_b)
            .await
            .expect("tenant connection");

        let error = scribe
            .seal_one(&seal_key, &mut conn)
            .await
            .expect_err("foreign tenant seal must fail");
        assert!(error.to_string().contains("tenant"));
        assert!(operator.list("").await.expect("object list").is_empty());
    }

    #[tokio::test]
    async fn test_replay_conflict_cannot_cross_tenant() {
        let (fixture, tenant_a, tenant_b) = setup().await;
        let table = logical_table();
        let binding_a = TenantTableBinding::resolve((tenant_a, table.clone())).expect("binding A");
        let binding_b = TenantTableBinding::resolve((tenant_b, table)).expect("binding B");
        let node_id = Uuid::now_v7();

        let first = insert_row(&binding_a, node_id, 1, 400, 450);
        let mut conn_a = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_a)
            .await
            .expect("tenant A connection");
        let first_outcome = insert_and_audit(&mut conn_a, &first, &[audit_event()])
            .await
            .expect("first insert");
        conn_a.commit().await.expect("first commit");

        let replay = insert_row(&binding_a, node_id, 1, 400, 450);
        let mut conn_a_replay = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_a)
            .await
            .expect("tenant A replay connection");
        let replay_outcome = insert_and_audit(&mut conn_a_replay, &replay, &[audit_event()])
            .await
            .expect("validated replay");
        conn_a_replay.commit().await.expect("replay commit");
        assert_eq!(
            replay_outcome,
            FileListInsertOutcome {
                id: first_outcome.id,
                commit_key: first.commit_key(),
                replayed: true,
            }
        );

        let foreign = insert_row(&binding_b, node_id, 1, 400, 450);
        let mut conn_b = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant_b)
            .await
            .expect("tenant B connection");
        let foreign_outcome = insert_and_audit(&mut conn_b, &foreign, &[])
            .await
            .expect("tenant-qualified replay identity");
        conn_b.commit().await.expect("tenant B commit");
        assert_eq!(foreign_outcome.id, foreign.id);
        assert!(!foreign_outcome.replayed);
        assert_eq!(foreign_outcome.commit_key.data_tenant_id, tenant_b);

        let superuser = fixture.superuser_pool().await.expect("superuser pool");
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1 AND operation = $2",
        )
        .bind(tenant_a.as_uuid())
        .bind("test.file_list_identity")
        .fetch_one(&superuser)
        .await
        .expect("audit count");
        assert_eq!(
            audit_count, 1,
            "replay must not append a duplicate audit row"
        );
    }

    /// A replacement actor may idempotently replay an already-published source epoch.
    #[tokio::test]
    async fn recovered_publication_replays_idempotently_under_current_actor_fence() {
        let (fixture, tenant, _) = setup().await;
        let binding = TenantTableBinding::resolve((tenant, logical_table())).expect("binding");
        let actor_node = NodeId::generate();
        register_scribe(&fixture, actor_node, 4).await;
        let source_node = Uuid::now_v7();
        let first = insert_row(&binding, source_node, 2, 10, 20);
        let actor = StreamIdentity::new(actor_node, WriterEpoch::new(4));
        let first_outcome =
            insert_and_audit_fenced(fixture.operator_pool(), actor, &first, &[audit_event()])
                .await
                .expect("first fenced publication");
        let replay = insert_row(&binding, source_node, 2, 10, 20);
        let replay_outcome =
            insert_and_audit_fenced(fixture.operator_pool(), actor, &replay, &[audit_event()])
                .await
                .expect("idempotent replay");
        assert_eq!(replay_outcome.id, first_outcome.id);
        assert!(replay_outcome.replayed);
        let pool = fixture.superuser_pool().await.expect("superuser pool");
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM vala.file_list WHERE data_tenant_id=$1 AND node_id=$2 AND writer_epoch=2), (SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1 AND operation='test.file_list_identity')",
        )
        .bind(tenant.as_uuid())
        .bind(source_node)
        .fetch_one(&pool)
        .await
        .expect("durable replay counts");
        assert_eq!(counts, (1, 1));
    }

    /// Losing the actor fence before publication rolls back file-list and audit state.
    #[tokio::test]
    async fn stale_recovery_writer_cannot_publish_or_audit() {
        let (fixture, tenant, _) = setup().await;
        let binding = TenantTableBinding::resolve((tenant, logical_table())).expect("binding");
        let actor_node = NodeId::generate();
        register_scribe(&fixture, actor_node, 5).await;
        let row = insert_row(&binding, Uuid::now_v7(), 3, 30, 40);
        let stale_actor = StreamIdentity::new(actor_node, WriterEpoch::new(4));
        insert_and_audit_fenced(fixture.operator_pool(), stale_actor, &row, &[audit_event()])
            .await
            .expect_err("stale recovery actor must be fenced");
        let pool = fixture.superuser_pool().await.expect("superuser pool");
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM vala.file_list WHERE id=$1), (SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$2 AND operation='test.file_list_identity')",
        )
        .bind(row.id)
        .bind(tenant.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("rolled back counts");
        assert_eq!(counts, (0, 0));
    }

    /// Fence advancement serializes behind an in-flight atomic publication lock.
    #[tokio::test]
    async fn concurrent_fence_advance_never_splits_file_list_and_audit() {
        let (fixture, tenant, _) = setup().await;
        let binding = TenantTableBinding::resolve((tenant, logical_table())).expect("binding");
        let actor_node = NodeId::generate();
        register_scribe(&fixture, actor_node, 4).await;
        let row = insert_row(&binding, Uuid::now_v7(), 3, 30, 40);
        let actor = StreamIdentity::new(actor_node, WriterEpoch::new(4));
        let barrier = PublicationFenceBarrier::new();
        let replacement_started = Arc::new(tokio::sync::Notify::new());
        let replacement_signal = Arc::clone(&replacement_started);
        let replacement_registry = ClusterRegistry::new(
            fixture.vala_postgres().clone(),
            ClusterNodeId::new(actor_node.as_uuid()),
        );
        let replacement = async {
            barrier.wait_until_acquired().await;
            replacement_signal.notify_one();
            replacement_registry
                .reserve_scribe(
                    "127.0.0.1:2",
                    ScribeCapabilitiesV1 {
                        tail_protocol_version:
                            vala_bifrost_redux::scribe::tail_rpc::TAIL_PROTOCOL_VERSION,
                    },
                )
                .await
                .expect("replacement fence advance")
                .fencing_token
        };
        let controller = async {
            replacement_started.notified().await;
            tokio::task::yield_now().await;
            barrier.release();
        };
        let audit_events = [audit_event()];
        let publication = insert_and_audit_fenced_with_barrier(
            fixture.operator_pool(),
            actor,
            &row,
            &audit_events,
            &barrier,
        );
        let (published, replacement_epoch, ()) = tokio::join!(publication, replacement, controller);
        let published = published.expect("old authority publication commits atomically");
        assert!(!published.replayed);
        assert_eq!(replacement_epoch, 5);

        let pool = fixture.superuser_pool().await.expect("superuser pool");
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM vala.file_list WHERE id=$1), (SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$2 AND operation='test.file_list_identity'), (SELECT fencing_token FROM vala.cluster_nodes WHERE data_tenant_id=$3 AND node_id=$4 AND role='scribe')",
        )
        .bind(row.id)
        .bind(tenant.as_uuid())
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .bind(actor_node.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("serialized publication state");
        assert_eq!(counts, (1, 1, 5));
    }

    #[test]
    fn tenant_table_binding_error_remains_typed() {
        let tenant = DataTenantId::new_v7();
        let error = TenantTableBinding::resolve((
            tenant,
            TableRef::new(BifrostNamespace::Traces, "bad/name"),
        ))
        .expect_err("unsafe local table name");
        assert!(matches!(
            error,
            TenantTableBindingError::InvalidTableName { .. }
        ));
    }
}
